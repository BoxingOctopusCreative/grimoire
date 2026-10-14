use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _,
};
use gpui_kit::{div, img, px, Context, IntoElement, ObjectFit, ParentElement as _, Window};

use super::backend;
use super::shell::Shell;
use super::theme::DISPLAY;
use crate::library::{BookDetail, BookUpdate};

impl Shell {
    pub fn book_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let Some(id) = self.book_id else {
            return div().child("No book selected.").into_any_element();
        };
        if self.book.as_ref().map(|book| book.id) != Some(id) && !self.busy && self.error.is_empty()
        {
            self.load_book(id, window, cx);
        }
        let title = self
            .book
            .as_ref()
            .map(|book| book.title.clone())
            .unwrap_or_else(|| "Book".into());
        let cover = self.book_cover.clone();
        let formats = self
            .book
            .as_ref()
            .map(|book| {
                book.formats
                    .iter()
                    .map(|format| format.format.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let can_read = self
            .book
            .as_ref()
            .map(|book| {
                backend::can_read(
                    &book
                        .formats
                        .iter()
                        .map(|format| format.format.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or(false);
        let targets = self.convert_targets.clone();
        let devices = self.devices.clone();

        div()
            .id("book-page")
            .size_full()
            .overflow_y_scroll()
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                Button::new("back-library")
                    .ghost()
                    .label("Back to library")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.book_id = None;
                        this.book = None;
                        this.reload_books(window, cx);
                    })),
            )
            .child(div().font_family(DISPLAY).text_size(px(28.)).child(title))
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
            .child(
                div()
                    .flex()
                    .gap(px(20.))
                    .child(book_cover(cover, cx))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(field("Title", Input::new(&self.title_input)))
                            .child(field("Authors", Input::new(&self.authors_input)))
                            .child(field("Tags", Input::new(&self.tags_input)))
                            .child(field("Series", Input::new(&self.series_input)))
                            .child(field("Series index", Input::new(&self.series_index_input)))
                            .child(field("ISBN", Input::new(&self.isbn_input)))
                            .child(field(
                                "Notes",
                                Textarea::new(&self.comment_input).h(px(90.)),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        Button::new("save-book")
                            .primary()
                            .label(if self.busy { "Working…" } else { "Save" })
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save_book_form(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("fetch-ol")
                            .outline()
                            .label("Fetch from Open Library")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.fetch_book(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("choose-cover")
                            .outline()
                            .label("Choose cover")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.choose_cover(window, cx)),
                            ),
                    )
                    .when(can_read, |this| {
                        this.child(Button::new("read-book").outline().label("Read").on_click(
                            cx.listener(|this, _, _, cx| {
                                if let Some(id) = this.book_id {
                                    this.open_reader(id, cx);
                                }
                            }),
                        ))
                    })
                    .child(
                        Button::new("delete-book")
                            .danger()
                            .label("Delete")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_delete_book(window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Formats: {formats}")),
            )
            .when(!targets.is_empty(), |this| {
                let buttons = targets.into_iter().map(|target| {
                    let label = format!("Convert to {target}");
                    let target_format = target.clone();
                    Button::new(SharedId(format!("convert-{target}")))
                        .outline()
                        .label(label)
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.convert_to(target_format.clone(), window, cx);
                        }))
                });
                this.child(div().flex().flex_wrap().gap(px(8.)).children(buttons))
            })
            .child(
                div()
                    .font_family(DISPLAY)
                    .text_size(px(18.))
                    .child("Send to device"),
            )
            .children(devices.into_iter().map(|device| {
                let id = device.id.clone();
                let label = format!(
                    "{} · {}",
                    device.product,
                    if device.supports_send {
                        "Send"
                    } else {
                        "Detected"
                    }
                );
                Button::new(SharedId(format!("send-{}", device.id)))
                    .outline()
                    .label(label)
                    .disabled(self.busy || !device.supports_send)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.send_open_book(id.clone(), window, cx);
                    }))
            }))
            .into_any_element()
    }

    pub fn load_book(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let state = self.state.clone();
        self.busy = true;
        self.error.clear();
        self.spawn_io(
            window,
            cx,
            move || {
                let book = backend::book(&state, id).map_err(|e| e.to_string())?;
                let cover = state
                    .with_conn(|conn, root| crate::library::cover_path(conn, root, id))
                    .ok()
                    .flatten();
                let targets = backend::convert_targets(&state, id).unwrap_or_default();
                Ok((book, cover, targets))
            },
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok((book, cover, targets)) => {
                        this.sync_form(&book, window, cx);
                        this.book_cover = cover;
                        this.convert_targets = targets;
                        this.book = Some(book);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn sync_form(&mut self, book: &BookDetail, window: &mut Window, cx: &mut Context<Self>) {
        let title = book.title.clone();
        let authors = book.authors.join(", ");
        let tags = book.tags.join(", ");
        let series = book.series.clone().unwrap_or_default();
        let index = book
            .series_index
            .map(|value| value.to_string())
            .unwrap_or_default();
        let isbn = backend::isbn_of(book).unwrap_or_default();
        let comment = book.comment.clone();
        self.title_input
            .update(cx, |input, cx| input.set_value(title, window, cx));
        self.authors_input
            .update(cx, |input, cx| input.set_value(authors, window, cx));
        self.tags_input
            .update(cx, |input, cx| input.set_value(tags, window, cx));
        self.series_input
            .update(cx, |input, cx| input.set_value(series, window, cx));
        self.series_index_input
            .update(cx, |input, cx| input.set_value(index, window, cx));
        self.isbn_input
            .update(cx, |input, cx| input.set_value(isbn, window, cx));
        self.comment_input
            .update(cx, |input, cx| input.set_value(comment, window, cx));
    }

    fn save_book_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = self.book.clone() else {
            return;
        };
        let title = self.title_input.read(cx).value().to_string();
        let authors = split_list(&self.authors_input.read(cx).value());
        let tags = split_list(&self.tags_input.read(cx).value());
        let series = self.series_input.read(cx).value().to_string();
        let index = self.series_index_input.read(cx).value().to_string();
        let isbn = self.isbn_input.read(cx).value().to_string();
        let comment = self.comment_input.read(cx).value().to_string();
        let update = BookUpdate {
            title: if title.trim().is_empty() {
                "Untitled".into()
            } else {
                title.trim().to_string()
            },
            authors,
            tags,
            series: {
                let series = series.trim().to_string();
                if series.is_empty() {
                    None
                } else {
                    Some(series)
                }
            },
            series_index: index.trim().parse().ok(),
            comment,
            identifiers: backend::with_isbn(&book, &isbn),
        };
        let state = self.state.clone();
        self.busy = true;
        self.spawn_io(
            window,
            cx,
            move || backend::save_book(&state, book.id, update).map_err(|e| e.to_string()),
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(book) => {
                        this.sync_form(&book, window, cx);
                        this.book = Some(book);
                        this.message = "Saved.".into();
                        this.error.clear();
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn fetch_book(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.book_id else {
            return;
        };
        let state = self.state.clone();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move { backend::enrich(&state, id).await.map_err(|e| e.to_string()) },
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        this.message = if result.updated_fields.is_empty() {
                            "No missing fields to fill.".into()
                        } else {
                            format!("Updated: {}", result.updated_fields.join(", "))
                        };
                        this.sync_form(&result.book, window, cx);
                        let id = result.book.id;
                        this.book = Some(result.book);
                        if let Ok(cover) = this
                            .state
                            .with_conn(|conn, root| crate::library::cover_path(conn, root, id))
                        {
                            this.book_cover = cover;
                        }
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn choose_cover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.book_id else {
            return;
        };
        let state = self.state.clone();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move {
                backend::cover_candidates(&state, id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(candidates) => this.open_cover_picker(candidates, window, cx),
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn open_cover_picker(
        &mut self,
        candidates: Vec<crate::openlibrary::CoverCandidate>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shell = cx.entity().clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let mut dialog = dialog.title("Choose cover");
            for candidate in &candidates {
                let urls = candidate.download_urls.clone();
                let shell = shell.clone();
                let label = format!("{} · {}", candidate.title, candidate.authors);
                dialog = dialog.child(
                    Button::new(SharedId(format!("cover-{}", candidate.id)))
                        .outline()
                        .label(label)
                        .on_click(move |_, window, cx| {
                            let urls = urls.clone();
                            shell.update(cx, |this, cx| this.apply_picked_cover(urls, window, cx));
                        }),
                );
            }
            dialog
        });
    }

    fn apply_picked_cover(
        &mut self,
        urls: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.book_id else {
            return;
        };
        let state = self.state.clone();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move {
                backend::apply_cover(&state, id, &urls)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(book) => {
                        this.message = "Cover updated.".into();
                        let id = book.id;
                        this.sync_form(&book, window, cx);
                        this.book = Some(book);
                        if let Ok(cover) = this
                            .state
                            .with_conn(|conn, root| crate::library::cover_path(conn, root, id))
                        {
                            this.book_cover = cover;
                        }
                        window.close_dialog(cx);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn convert_to(&mut self, target: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.book_id else {
            return;
        };
        let state = self.state.clone();
        self.busy = true;
        self.spawn_io(
            window,
            cx,
            move || backend::convert(&state, id, &target).map_err(|e| e.to_string()),
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        this.message = if result.converted {
                            format!("Converted to {}.", result.format)
                        } else {
                            format!("{} already available.", result.format)
                        };
                        if let Some(id) = this.book_id {
                            this.busy = false;
                            this.load_book(id, window, cx);
                        }
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn send_open_book(&mut self, device_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.book_id else {
            return;
        };
        let state = self.state.clone();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move {
                backend::send(&state, id, &device_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, _, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        this.message = format!("Sent {} as {}.", result.format, result.remote_path);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn confirm_delete_book(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title("Delete book")
                .description("Delete this book from the library?")
                .on_ok({
                    let shell = shell.clone();
                    move |_, window, cx| {
                        shell.update(cx, |this, cx| {
                            if let Some(id) = this.book_id {
                                let state = this.state.clone();
                                this.spawn_io(
                                    window,
                                    cx,
                                    move || {
                                        backend::remove_book(&state, id).map_err(|e| e.to_string())
                                    },
                                    |this, result, window, cx| {
                                        if let Err(error) = result {
                                            this.error = error;
                                            cx.notify();
                                            return;
                                        }
                                        this.book_id = None;
                                        this.book = None;
                                        this.reload_books(window, cx);
                                    },
                                );
                            }
                        });
                        true
                    }
                })
        });
    }
}

fn field(label: &str, control: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(div().text_size(px(12.)).child(label.to_string()))
        .child(control)
}

fn book_cover(path: Option<std::path::PathBuf>, cx: &gpui_kit::App) -> impl IntoElement {
    div()
        .w(px(160.))
        .h(px(240.))
        .rounded(px(6.))
        .overflow_hidden()
        .bg(cx.theme().muted)
        .child(match path {
            Some(path) => img(path)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => div().size_full().into_any_element(),
        })
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// ElementId from an owned string. GPUI accepts `Into<ElementId>`.
struct SharedId(String);

impl From<SharedId> for gpui_kit::ElementId {
    fn from(value: SharedId) -> Self {
        value.0.into()
    }
}
