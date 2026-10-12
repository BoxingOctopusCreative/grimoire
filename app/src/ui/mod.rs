mod backend;
mod book;
mod devices;
mod document;
mod icon;
mod library;
mod reader;
mod settings;
mod shell;
mod theme;

use gpui_kit::AppContext as _;
use gpui_kit::{
    App, TitlebarOptions, WindowBounds, WindowOptions, application, point, px, size,
};
use std::future::pending;

pub use shell::Shell;

pub fn launch() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let handle = runtime.handle().clone();
    std::thread::Builder::new()
        .name("grimoire-tokio".into())
        .spawn(move || runtime.block_on(pending::<()>()))
        .expect("tokio thread");

    application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_global(backend::TokioRt(handle));
            theme::apply_saved(cx);
            icon::install();
            gpui_kit::open_window(main_window_options(cx), cx, |window, cx| {
                cx.new(|cx| Shell::new(window, cx))
            })
            .expect("failed to open Grimoire");
        });
}

fn main_window_options(cx: &App) -> WindowOptions {
    let mut options = WindowOptions::default();
    options.window_bounds = Some(WindowBounds::centered(size(px(1180.), px(780.)), cx));
    options.window_min_size = Some(size(px(880.), px(560.)));
    options.titlebar = Some(TitlebarOptions {
        title: Some("Grimoire".into()),
        appears_transparent: true,
        traffic_light_position: Some(point(px(14.), px(18.))),
    });
    options.app_owns_titlebar_drag = true;
    icon::apply(&mut options);
    options
}

pub(crate) fn reader_window_options(title: &str, cx: &App) -> WindowOptions {
    let mut options = WindowOptions::default();
    options.window_bounds = Some(WindowBounds::centered(size(px(920.), px(760.)), cx));
    options.window_min_size = Some(size(px(640.), px(480.)));
    options.titlebar = Some(TitlebarOptions {
        title: Some(title.to_string().into()),
        appears_transparent: true,
        traffic_light_position: Some(point(px(14.), px(16.))),
    });
    options.app_owns_titlebar_drag = true;
    icon::apply(&mut options);
    options
}
