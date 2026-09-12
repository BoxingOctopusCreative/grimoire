use crate::kindle::{
    classify_reader, list_ebook_readers, list_mounted_readers, EbookReaderDevice,
};
use futures::StreamExt;
use mtp_rs::mtp::{watch_devices, HotplugEvent, MtpDeviceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::time::{interval, sleep};

pub const EREADER_PRESENCE_EVENT: &str = "ereader-presence";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresenceChange {
    Arrived,
    Left,
    Snapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct EreaderPresenceEvent {
    pub change: PresenceChange,
    pub label: Option<String>,
    pub device_id: Option<String>,
    pub devices: Vec<EbookReaderDevice>,
}

pub fn start_device_watcher(app: AppHandle) {
    let app_mtp = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = watch_mtp_presence(app_mtp).await {
            eprintln!("Grimoire MTP device watch stopped: {e}");
        }
    });

    tauri::async_runtime::spawn(async move {
        watch_mount_presence(app).await;
    });
}

async fn watch_mtp_presence(app: AppHandle) -> Result<(), String> {
    let mut watch = watch_devices().map_err(|e| e.to_string())?;
    // Devices already connected are reported as Arrived when the stream starts.
    // Treat the first second as bootstrap so those do not toast.
    let bootstrap_until = Instant::now() + Duration::from_millis(1000);
    let mut emitted_bootstrap = false;

    while let Some(event) = watch.next().await {
        let (arrived, info) = match event {
            HotplugEvent::Arrived(info) => (true, info),
            HotplugEvent::Left(info) => (false, info),
        };

        if !mtp_info_is_ereader(&info) {
            continue;
        }

        let in_bootstrap = Instant::now() < bootstrap_until;
        if in_bootstrap {
            if !emitted_bootstrap {
                sleep(Duration::from_millis(450)).await;
                emit_presence(&app, PresenceChange::Snapshot, None, None).await;
                emitted_bootstrap = true;
            }
            continue;
        }

        // Give the device a moment to finish enumerating before we open it.
        sleep(Duration::from_millis(450)).await;

        let label = device_label_from_mtp(&info);
        let change = if arrived {
            PresenceChange::Arrived
        } else {
            PresenceChange::Left
        };

        emit_presence(&app, change, Some(label), None).await;
    }

    Ok(())
}

async fn watch_mount_presence(app: AppHandle) {
    let mut known: HashMap<String, String> = HashMap::new();
    let mut primed = false;
    let mut tick = interval(Duration::from_secs(2));

    loop {
        tick.tick().await;
        let mounts = match list_mounted_readers() {
            Ok(list) => list,
            Err(_) => continue,
        };
        let current: HashMap<String, String> = mounts
            .iter()
            .map(|d| (d.id.clone(), d.product.clone()))
            .collect();

        if !primed {
            let had_mounts = !current.is_empty();
            known = current;
            primed = true;
            if had_mounts {
                emit_presence(&app, PresenceChange::Snapshot, None, None).await;
            }
            continue;
        }

        let arrived: Vec<(String, String)> = current
            .iter()
            .filter(|(id, _)| !known.contains_key(*id))
            .map(|(id, product)| (id.clone(), product.clone()))
            .collect();
        let left: Vec<(String, String)> = known
            .iter()
            .filter(|(id, _)| !current.contains_key(*id))
            .map(|(id, product)| (id.clone(), product.clone()))
            .collect();

        for (id, product) in arrived {
            emit_presence(&app, PresenceChange::Arrived, Some(product), Some(id)).await;
        }

        for (id, product) in left {
            emit_presence(&app, PresenceChange::Left, Some(product), Some(id)).await;
        }

        known = current;
    }
}

fn mtp_info_is_ereader(info: &MtpDeviceInfo) -> bool {
    let manufacturer = info.manufacturer.clone().unwrap_or_default();
    let product = info.product.clone().unwrap_or_default();
    classify_reader(&manufacturer, &product).is_some()
}

fn device_label_from_mtp(info: &MtpDeviceInfo) -> String {
    info.product
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "eReader".to_string())
}

async fn emit_presence(
    app: &AppHandle,
    change: PresenceChange,
    label: Option<String>,
    device_id: Option<String>,
) {
    let devices = match list_ebook_readers().await {
        Ok(list) => list,
        Err(e) => {
            eprintln!("Grimoire could not refresh eReaders after presence change: {e}");
            Vec::new()
        }
    };

    let resolved_id = device_id.or_else(|| {
        label.as_ref().and_then(|name| {
            devices
                .iter()
                .find(|d| d.product.eq_ignore_ascii_case(name))
                .map(|d| d.id.clone())
        })
    });

    let payload = EreaderPresenceEvent {
        change,
        label,
        device_id: resolved_id,
        devices,
    };

    if let Err(e) = app.emit(EREADER_PRESENCE_EVENT, payload) {
        eprintln!("Grimoire failed to emit eReader presence event: {e}");
    }
}
