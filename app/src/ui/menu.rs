#[cfg(target_os = "macos")]
use gpui_kit::base::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{AnyWindowHandle, App, Entity, KeyBinding};
#[cfg(target_os = "macos")]
use gpui_kit::{Menu, MenuItem, OsAction, SystemMenuType};

use super::shell::{Route, Shell};

gpui_kit::actions!(
    grimoire,
    [
        About,
        OpenSettings,
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        CloseWindow,
        Minimize,
        Zoom
    ]
);

/// Installs quit, close, and settings shortcuts, plus the macOS menu bar.
/// `secondary` is Command on macOS and Control elsewhere.
/// Closing the library window quits Grimoire. A reader window can close on its own.
pub fn install(main_window: AnyWindowHandle, shell: Entity<Shell>, cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-w", CloseWindow, None),
        KeyBinding::new("alt-f4", CloseWindow, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
    ]);
    #[cfg(target_os = "macos")]
    {
        cx.bind_keys([
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
            KeyBinding::new("cmd-m", Minimize, None),
        ]);
        cx.set_menus(menus());
    }

    let main_id = main_window.window_id();
    cx.on_window_closed(move |cx, closed| {
        if closed == main_id {
            cx.quit();
        }
    })
    .detach();

    let shell = shell.downgrade();
    cx.on_action(move |_: &OpenSettings, cx| {
        let _ = shell.update(cx, |shell, cx| shell.go(Route::Settings, cx));
        // Menu actions run inside the active window update, so window calls wait a turn.
        cx.defer(move |cx| {
            let _ = main_window.update(cx, |_, window, _| window.activate_window());
        });
    });
    cx.on_action(|_: &About, cx| {
        cx.defer(|cx| super::about::show(cx));
    });
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &CloseWindow, cx| close_active(cx));
    cx.on_action(|_: &Minimize, cx| {
        with_active(cx, |window| window.minimize_window());
    });
    cx.on_action(|_: &Zoom, cx| {
        with_active(cx, |window| window.zoom_window());
    });
}

#[cfg(target_os = "macos")]
fn menus() -> Vec<Menu> {
    vec![
        Menu::new("Grimoire").items([
            MenuItem::action("About Grimoire", About),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide Grimoire", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Grimoire", Quit),
        ]),
        Menu::new("File").items([MenuItem::action("Close", CloseWindow)]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
        ]),
    ]
}

fn close_active(cx: &mut App) {
    with_active(cx, |window| window.remove_window());
}

fn with_active(cx: &mut App, f: impl FnOnce(&mut gpui_kit::Window) + 'static) {
    let Some(handle) = cx.active_window() else {
        return;
    };
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| f(window));
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn quit_and_close_shortcuts_parse() {
        for shortcut in ["secondary-q", "secondary-w", "alt-f4", "secondary-,"] {
            gpui_kit::Keystroke::parse(shortcut)
                .unwrap_or_else(|_| panic!("{shortcut} is not a valid shortcut"));
        }
    }
}
