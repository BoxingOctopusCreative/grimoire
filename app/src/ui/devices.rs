use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, WindowExt as _};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _, Styled as _,
};
use gpui_kit::{div, px, Context, IntoElement, ParentElement as _, Window};

use super::backend;
use super::shell::Shell;
use super::theme::DISPLAY;
use crate::kindle::{ConnectionKind, DeviceBook, DeviceBookTarget};

impl Shell {
    pub fn devices_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let devices = self.devices.clone();
        let history = self.sync_history.clone();
        div()
            .id("devices-page")
            .size_full()
            .overflow_y_scroll()
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().flex().flex_col().gap(px(4.)).child(
                        div()
                            .font_family(DISPLAY)
                            .text_size(px(28.))
                            .child("Devices"),
                    ).child(
                        div().text_color(cx.theme().muted_foreground).child(
                            "Detects Kindles, Kobos, BOOX, PocketBook, reMarkable, Tolino, and Nook over USB/MTP or mounted volumes.",
                        ),
                    ))
                    .child(
                        Button::new("refresh-devices")
                            .outline()
                            .label(if self.devices_loading { "Scanning…" } else { "Refresh" })
                            .disabled(self.devices_loading)
                            .on_click(cx.listener(|this, _, window, cx| this.refresh_devices(window, cx))),
                    ),
            )
            .when(!self.error.is_empty(), |this| {
                this.child(div().text_color(cx.theme().danger).child(self.error.clone()))
            })
            .child(panel_title("Connected eReaders", cx))
            .when(devices.is_empty(), |this| {
                this.child(
                    div().text_color(cx.theme().muted_foreground).child(
                        "No eReaders found yet. Grimoire checks USB/MTP and mounted volumes every few seconds. On macOS, quit Android File Transfer if an MTP device stays locked.",
                    ),
                )
            })
            .children(devices.into_iter().map(|device| {
                let id = device.id.clone();
                let send = if device.supports_send { "Send ready" } else { "Detected" };
                let connection = match device.connection {
                    ConnectionKind::Mtp => "USB/MTP",
                    ConnectionKind::Mount => "Mounted volume",
                };
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(device.product.clone())
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} · {} · {} · prefers {}",
                                        device.family_label,
                                        connection,
                                        format_bytes(device.free_space_bytes),
                                        device.preferred_formats.join(", ")
                                    )),
                            ),
                    )
                    .child(
                        div().flex().items_center().gap(px(8.)).child(
                            Button::new(Id(format!("view-{id}")))
                                .outline()
                                .label("View books")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.device_id = Some(id.clone());
                                    this.device_library = None;
                                    this.scan_device(window, cx);
                                })),
                        ).child(send),
                    )
            }))
            .child(panel_title("Recent sends", cx))
            .when(history.is_empty(), |this| {
                this.child(div().text_color(cx.theme().muted_foreground).child(
                    "No transfers recorded yet.",
                ))
            })
            .children(history.into_iter().take(20).map(|record| {
                div().text_size(px(14.)).child(format!(
                    "Book {} · {} · {} · {}",
                    record.book_id, record.format, record.remote_path, record.last_sent_at
                ))
            }))
    }

    pub fn ask_to_manage(
        &mut self,
        devices: Vec<crate::kindle::EbookReaderDevice>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        cx.activate(true);
        window.activate_window();
        // Activation stops a chime that is already playing, so start this one after the window is front.
        self.chime_after_activation(window, cx);
        let products: Vec<String> = devices
            .iter()
            .map(|device| device.product.clone())
            .collect();
        let ids: Vec<String> = devices.iter().map(|device| device.id.clone()).collect();
        let (title, description) = manage_prompt(&products);
        let shell = cx.entity().clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title(title.clone())
                .description(description.clone())
                .ok_text("Manage")
                .cancel_text("Not now")
                .on_ok({
                    let shell = shell.clone();
                    let ids = ids.clone();
                    move |_, window, cx| {
                        shell.update(cx, |this, cx| {
                            this.open_connected_devices(ids.clone(), window, cx);
                        });
                        true
                    }
                })
                .on_close({
                    let shell = shell.clone();
                    move |_, _, cx| {
                        shell.update(cx, |this, _cx| this.device_prompt_open = false);
                    }
                })
        });
    }

    fn chime_after_activation(&self, window: &mut Window, cx: &mut Context<Self>) {
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |_this, cx| {
            let _ = runtime
                .spawn(async {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                })
                .await;
            let _ = cx.update(|_window, _cx| super::chime::play_connect_chime());
        })
        .detach();
    }

    fn open_connected_devices(
        &mut self,
        ids: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.book_id = None;
        self.route = super::shell::Route::Devices;
        self.device_selected.clear();
        self.device_select_mode = false;
        if let [id] = ids.as_slice() {
            self.device_id = Some(id.clone());
            self.device_library = None;
            self.scan_device(window, cx);
        } else {
            self.device_id = None;
            self.refresh_devices(window, cx);
        }
        cx.notify();
    }

    pub fn refresh_devices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.devices_loading = true;
        let state = self.state.clone();
        self.spawn_task(
            window,
            cx,
            async move {
                let devices = backend::readers().await.map_err(|e| e.to_string())?;
                let history = if state.is_open() {
                    backend::sync_history(&state).map_err(|e| e.to_string())?
                } else {
                    Vec::new()
                };
                Ok((devices, history))
            },
            |this, result, _, cx| {
                this.devices_loading = false;
                match result {
                    Ok((devices, history)) => {
                        this.devices = devices;
                        this.sync_history = history;
                        this.error.clear();
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    pub fn device_library_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let books = self
            .device_library
            .as_ref()
            .map(|library| library.books.clone())
            .unwrap_or_default();
        let query = self
            .device_query
            .read(cx)
            .value()
            .to_string()
            .to_lowercase();
        let filtered: Vec<DeviceBook> = books
            .into_iter()
            .filter(|book| {
                query.is_empty()
                    || book.title.to_lowercase().contains(&query)
                    || book.filename.to_lowercase().contains(&query)
                    || book.format.to_lowercase().contains(&query)
            })
            .collect();
        let name = self
            .device_library
            .as_ref()
            .map(|library| library.device_name.clone())
            .unwrap_or_else(|| "Device".into());
        let selected = self.device_selected.len();

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(16.))
                    .py(px(10.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("back-devices")
                            .ghost()
                            .label("Devices")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.device_id = None;
                                this.device_library = None;
                                cx.notify();
                            })),
                    )
                    .child(div().font_family(DISPLAY).text_size(px(22.)).child(name))
                    .child(div().flex_1().child(Input::new(&self.device_query)))
                    .child(
                        Button::new("device-select")
                            .ghost()
                            .label(if self.device_select_mode {
                                "Done"
                            } else {
                                "Select"
                            })
                            .selected(self.device_select_mode)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.device_select_mode = !this.device_select_mode;
                                if !this.device_select_mode {
                                    this.device_selected.clear();
                                }
                                cx.notify();
                            })),
                    )
                    .when(self.device_select_mode && selected > 0, |this| {
                        this.child(
                            Button::new("device-delete")
                                .danger()
                                .label(format!("Delete {selected}"))
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_device_delete(window, cx)
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .id("device-books")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .when(self.device_library.is_none(), |this| {
                        this.child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child("Scanning device…"),
                        )
                    })
                    .when(!self.message.is_empty(), |this| {
                        this.child(
                            div()
                                .text_color(cx.theme().primary)
                                .child(self.message.clone()),
                        )
                    })
                    .when(!self.error.is_empty(), |this| {
                        this.child(
                            div()
                                .text_color(cx.theme().danger)
                                .child(self.error.clone()),
                        )
                    })
                    .children(filtered.into_iter().map(|book| {
                        let key = book_key(&book);
                        let checked = self.device_selected.contains(&key);
                        let select_mode = self.device_select_mode;
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(8.))
                            .rounded(px(8.))
                            .bg(cx.theme().popover)
                            .border_1()
                            .border_color(cx.theme().border)
                            .when(select_mode, |row| {
                                let key = key.clone();
                                row.child(
                                    Checkbox::new(Id(format!("dev-{key}")))
                                        .checked(checked)
                                        .on_change(cx.listener(move |this, checked, _, cx| {
                                            if *checked {
                                                this.device_selected.insert(key.clone());
                                            } else {
                                                this.device_selected.remove(&key);
                                            }
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .child(book.title.clone())
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "{} · {} · {}",
                                                book.format,
                                                book.filename,
                                                format_size(book.size)
                                            )),
                                    ),
                            )
                    })),
            )
    }

    pub(crate) fn begin_device_io(&mut self) {
        self.device_io = self.device_io.saturating_add(1);
    }

    pub(crate) fn end_device_io(&mut self) {
        self.device_io = self.device_io.saturating_sub(1);
    }

    pub(crate) fn note_open_device(&mut self, connected: &std::collections::HashSet<String>) {
        let missing = self.route == super::shell::Route::Devices
            && self
                .device_id
                .as_ref()
                .is_some_and(|id| !connected.contains(id));
        self.device_missing_polls =
            note_device_presence(missing, self.device_io, self.device_missing_polls);
        if unplugged_by_user(self.device_missing_polls, self.device_io) {
            self.device_id = None;
            self.device_library = None;
            self.device_selected.clear();
            self.device_select_mode = false;
            self.route = super::shell::Route::Devices;
            self.error.clear();
            self.message.clear();
            self.device_missing_polls = 0;
        }
    }

    pub fn scan_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.device_id.clone() else {
            return;
        };
        self.begin_device_io();
        self.busy = true;
        self.error.clear();
        self.spawn_task(
            window,
            cx,
            async move { backend::scan_device(id).await.map_err(|e| e.to_string()) },
            |this, result, _, cx| {
                this.end_device_io();
                this.busy = false;
                match result {
                    Ok(library) => this.device_library = Some(library),
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn confirm_device_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.device_selected.len();
        let shell = cx.entity().clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title(format!("Delete {count} books"))
                .description("Remove the selected files from the device?")
                .on_ok({
                    let shell = shell.clone();
                    move |_, window, cx| {
                        shell.update(cx, |this, cx| this.delete_device_selection(window, cx));
                        true
                    }
                })
        });
    }

    fn delete_device_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(device_id) = self.device_id.clone() else {
            return;
        };
        let Some(library) = self.device_library.clone() else {
            return;
        };
        let targets = library
            .books
            .iter()
            .filter(|book| self.device_selected.contains(&book_key(book)))
            .map(|book| DeviceBookTarget {
                path: book.path.clone(),
                filename: book.filename.clone(),
                handle: book.handle,
            })
            .collect::<Vec<_>>();
        self.begin_device_io();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move {
                backend::delete_on_device(device_id, targets)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, window, cx| {
                this.end_device_io();
                this.busy = false;
                match result {
                    Ok(result) => {
                        this.message = format!("Removed {} books from the device.", result.removed);
                        this.device_selected.clear();
                        this.scan_device(window, cx);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }
}

pub(crate) const NOTICE_SECS: u64 = 5;
pub(crate) const DEVICE_UNPLUG_POLLS: u8 = 2;

pub(crate) fn note_device_presence(missing: bool, device_io: u32, polls: u8) -> u8 {
    if device_io > 0 || !missing {
        0
    } else {
        polls.saturating_add(1)
    }
}

pub(crate) fn unplugged_by_user(polls: u8, device_io: u32) -> bool {
    device_io == 0 && polls >= DEVICE_UNPLUG_POLLS
}

pub(crate) fn offer_manage(ids: &[String], open_device: Option<&str>) -> bool {
    match ids {
        [] => false,
        [id] => open_device != Some(id.as_str()),
        _ => true,
    }
}

pub(crate) fn manage_prompt(products: &[String]) -> (String, String) {
    if products.len() == 1 {
        (
            "eReader connected".into(),
            format!("{} is connected. Manage it with Grimoire?", products[0]),
        )
    } else {
        (
            "eReaders connected".into(),
            format!(
                "{} are connected. Manage them with Grimoire?",
                join_names(products)
            ),
        )
    }
}

fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        many => {
            let (last, rest) = many.split_last().unwrap();
            format!("{}, and {last}", rest.join(", "))
        }
    }
}

fn book_key(book: &DeviceBook) -> String {
    format!(
        "{}|{}|{}",
        book.path,
        book.handle.unwrap_or(0),
        book.filename
    )
}

fn panel_title(text: &str, _cx: &gpui_kit::App) -> impl IntoElement {
    div()
        .font_family(DISPLAY)
        .text_size(px(20.))
        .child(text.to_string())
}

fn format_bytes(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "Unknown free space".into();
    };
    let gb = bytes as f64 / 1e9;
    if gb >= 1.0 {
        format!("{gb:.1} GB free")
    } else {
        format!("{:.0} MB free", bytes as f64 / 1e6)
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        manage_prompt, note_device_presence, offer_manage, unplugged_by_user, NOTICE_SECS,
    };

    #[test]
    fn a_connected_reader_asks_to_manage() {
        let (title, description) = manage_prompt(&["Kindle Paperwhite".into()]);
        assert_eq!(title, "eReader connected");
        assert_eq!(
            description,
            "Kindle Paperwhite is connected. Manage it with Grimoire?"
        );
        assert!(offer_manage(&["k1".into()], None));
        assert!(!offer_manage(&["k1".into()], Some("k1")));
        assert!(offer_manage(&["k1".into(), "k2".into()], Some("k1")));
        assert!(!offer_manage(&[], None));
    }

    #[test]
    fn unplug_returns_after_the_device_stays_gone() {
        assert_eq!(NOTICE_SECS, 5);
        assert_eq!(note_device_presence(true, 0, 0), 1);
        assert!(!unplugged_by_user(1, 0));
        assert_eq!(note_device_presence(true, 0, 1), 2);
        assert!(unplugged_by_user(2, 0));
        assert_eq!(note_device_presence(true, 1, 1), 0);
        assert!(!unplugged_by_user(0, 1));
        assert_eq!(note_device_presence(false, 0, 2), 0);
    }

    #[test]
    fn several_readers_are_named_together() {
        let (title, description) = manage_prompt(&["Kindle".into(), "Kobo Clara".into()]);
        assert_eq!(title, "eReaders connected");
        assert_eq!(
            description,
            "Kindle and Kobo Clara are connected. Manage them with Grimoire?"
        );
        let (_, description) =
            manage_prompt(&["Kindle".into(), "Kobo Clara".into(), "BOOX".into()]);
        assert_eq!(
            description,
            "Kindle, Kobo Clara, and BOOX are connected. Manage them with Grimoire?"
        );
    }
}

struct Id(String);

impl From<Id> for gpui_kit::ElementId {
    fn from(value: Id) -> Self {
        value.0.into()
    }
}
