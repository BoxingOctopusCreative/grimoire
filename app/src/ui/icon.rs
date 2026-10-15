use std::sync::Arc;

use gpui_kit::WindowOptions;

const ICON_PNG: &[u8] = include_bytes!("../../icons/icon.png");

pub fn apply(options: &mut WindowOptions) {
    options.icon = Some(raster());
    // Matches the Linux desktop file id (Wayland app id and X11 WM_CLASS).
    #[cfg(target_os = "linux")]
    {
        options.app_id = Some("com.boxingoctopus.grimoire".into());
    }
}

/// Dock and Cmd-Tab icon. The Windows executable icon is `icons/icon.ico`, embedded as resource 1.
pub fn install() {
    #[cfg(target_os = "macos")]
    install_macos();
}

fn raster() -> Arc<image::RgbaImage> {
    Arc::new(
        image::load_from_memory(ICON_PNG)
            .expect("Grimoire icon.png")
            .into_rgba8(),
    )
}

#[cfg(target_os = "macos")]
fn install_macos() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../../icons/icon.icns"));
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    let app = NSApplication::sharedApplication(marker);
    unsafe {
        app.setApplicationIconImage(Some(&image));
    }
}
