//! Two-note chimes for an eReader connecting and disconnecting.
//! Connect rises from C4 to E4. Disconnect falls from E4 to C4.

const RATE: u32 = 22_050;
const SECONDS: f32 = 0.72;
const C4: f32 = 261.63;
const E4: f32 = 329.63;

pub fn play_connect_chime() {
    play_wav(&chime_wav(true));
}

pub fn play_disconnect_chime() {
    play_wav(&chime_wav(false));
}

fn chime_wav(rising: bool) -> Vec<u8> {
    let notes = if rising {
        [(C4, 0.0_f32), (E4, 0.14)]
    } else {
        [(E4, 0.0), (C4, 0.14)]
    };
    let count = (RATE as f32 * SECONDS) as usize;
    let mut samples = vec![0.0_f32; count];
    for (freq, start) in notes {
        let offset = (start * RATE as f32) as usize;
        for (index, sample) in samples.iter_mut().enumerate().skip(offset) {
            let t = (index - offset) as f32 / RATE as f32;
            let attack = 1.0 - (-t * 90.0).exp();
            let decay = (-t * 6.2).exp();
            let angle = t * freq * std::f32::consts::TAU;
            *sample += (angle.sin() + 0.28 * (angle * 2.0).sin()) * attack * decay;
        }
    }
    let pcm: Vec<i16> = samples
        .into_iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * 0.269 * i16::MAX as f32) as i16)
        .collect();
    wav_bytes(&pcm, RATE)
}

fn wav_bytes(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    push_u32(&mut out, (36 + data_len) as u32);
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    push_u32(&mut out, 16);
    push_u16(&mut out, 1);
    push_u16(&mut out, 1);
    push_u32(&mut out, rate);
    push_u32(&mut out, rate * 2);
    push_u16(&mut out, 2);
    push_u16(&mut out, 16);
    out.extend_from_slice(b"data");
    push_u32(&mut out, data_len as u32);
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(target_os = "macos")]
fn play_wav(bytes: &[u8]) {
    use std::cell::RefCell;

    use objc2::AnyThread;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;

    thread_local! {
        static PLAYING: RefCell<Option<objc2::rc::Retained<NSSound>>> = RefCell::new(None);
    }

    let data = NSData::with_bytes(bytes);
    let Some(sound) = NSSound::initWithData(NSSound::alloc(), &data) else {
        return;
    };
    if !sound.play() {
        return;
    }
    // Dropping the sound stops it, so keep this one until the next chime.
    PLAYING.with(|playing| *playing.borrow_mut() = Some(sound));
}

#[cfg(target_os = "windows")]
fn play_wav(bytes: &[u8]) {
    use std::sync::Mutex;

    static CLIP: Mutex<Option<Vec<u8>>> = Mutex::new(None);
    const ASYNC: u32 = 0x0001;
    const NO_DEFAULT: u32 = 0x0002;
    const MEMORY: u32 = 0x0004;

    #[link(name = "winmm")]
    extern "system" {
        fn PlaySoundA(name: *const u8, module: *mut core::ffi::c_void, flags: u32) -> i32;
    }

    let mut clip = CLIP.lock().unwrap_or_else(|poison| poison.into_inner());
    *clip = Some(bytes.to_vec());
    let ptr = clip
        .as_ref()
        .map(|wav| wav.as_ptr())
        .unwrap_or(std::ptr::null());
    unsafe {
        PlaySoundA(ptr, std::ptr::null_mut(), ASYNC | NO_DEFAULT | MEMORY);
    }
}

/// Linux has no audio crate in this app. `paplay` or `aplay` plays the same clip when one is installed.
#[cfg(all(unix, not(target_os = "macos")))]
fn play_wav(bytes: &[u8]) {
    let path = std::env::temp_dir().join(format!(
        "grimoire-chime-{}-{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|time| time.as_nanos())
            .unwrap_or(0)
    ));
    if std::fs::write(&path, bytes).is_err() {
        return;
    }
    std::thread::spawn(move || {
        for program in ["paplay", "aplay"] {
            let played = std::process::Command::new(program)
                .arg(&path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if played.map(|status| status.success()).unwrap_or(false) {
                break;
            }
        }
        let _ = std::fs::remove_file(&path);
    });
}

#[cfg(test)]
mod tests {
    use super::{chime_wav, RATE};

    #[test]
    fn connect_chime_is_a_short_quiet_wav() {
        assert_quiet_chime(&chime_wav(true));
    }

    #[test]
    fn disconnect_chime_falls_where_connect_rises() {
        let rising = chime_wav(true);
        let falling = chime_wav(false);
        assert_quiet_chime(&falling);
        assert_ne!(rising, falling);
        let opening = (RATE as f32 * 0.12) as usize;
        assert!(
            zero_crossings(&falling, opening) > zero_crossings(&rising, opening),
            "disconnect should open on the higher note"
        );
    }

    fn assert_quiet_chime(wav: &[u8]) {
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(wav.len(), 44 + data_len);
        let mut peak = 0_i16;
        let mut audible = 0_usize;
        for frame in wav[44..].chunks_exact(2) {
            let sample = i16::from_le_bytes(frame.try_into().unwrap());
            peak = peak.max(sample.abs());
            if sample.abs() > 400 {
                audible += 1;
            }
        }
        assert!(peak > 1000, "chime was silent");
        assert!(peak < 20_000, "chime was too loud");
        assert!(audible > 1000);
        assert!(wav.len() < 40_000, "chime was longer than a short chime");
    }

    fn zero_crossings(wav: &[u8], samples: usize) -> usize {
        let mut previous = 0_i16;
        let mut crossings = 0_usize;
        for frame in wav[44..].chunks_exact(2).take(samples) {
            let sample = i16::from_le_bytes(frame.try_into().unwrap());
            if previous.signum() != 0
                && sample.signum() != 0
                && previous.signum() != sample.signum()
            {
                crossings += 1;
            }
            if sample != 0 {
                previous = sample;
            }
        }
        crossings
    }
}
