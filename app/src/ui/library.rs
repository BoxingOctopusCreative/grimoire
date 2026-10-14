use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, WindowExt as _,
};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _,
};
use gpui_kit::AppContext as _;
use gpui_kit::{
    div, img, px, AnyElement, Context, IntoElement, ObjectFit, ParentElement as _, Window,
};

use super::backend::{self, LibraryStatus};
use super::shell::{FormatKind, ImportProgress, ReadingStatus, Route, Shell};
use super::theme::DISPLAY;
use crate::library::{BookBulkPatch, BookSummary};

impl Shell {
    pub fn library_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let shell = cx.entity().clone();
        if self.loading && self.books.is_empty() && self.library_open {
            return super::shell::muted_page("Opening library…", cx).into_any_element();
        }
        if !self.library_open {
            return self.closed_library(window, cx).into_any_element();
        }

        let filtered = self.filtered_books();
        let mut reading = Vec::new();
        let mut to_read = Vec::new();
        let mut read = Vec::new();
        for book in &filtered {
            match book_status(book) {
                ReadingStatus::Reading => reading.push(book.clone()),
                ReadingStatus::ToRead => to_read.push(book.clone()),
                ReadingStatus::Read => read.push(book.clone()),
            }
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.library_bar(filtered.is_empty(), window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .overflow_hidden()
                    .when(self.filters_open, |this| {
                        let books = self.books.clone();
                        this.child(self.filters(&books, cx))
                    })
                    .child(self.library_grid(
                        &reading,
                        &to_read,
                        &read,
                        filtered.is_empty(),
                        shell,
                        window,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn closed_library(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::Div {
        div().p(px(28.)).child(
            div()
                .max_w(px(560.))
                .p(px(22.))
                .rounded(px(10.))
                .bg(cx.theme().popover)
                .border_1()
                .border_color(cx.theme().border)
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .font_family(DISPLAY)
                        .text_size(px(26.))
                        .child("Your library starts here"),
                )
                .child(
                    div().text_color(cx.theme().muted_foreground).child(
                        "Set a library folder. Grimoire stores metadata.db and your book files locally, Calibre-style.",
                    ),
                )
                .child(
                    Button::new("choose-library")
                        .primary()
                        .label(if self.busy {
                            "Working…"
                        } else {
                            "Choose library folder"
                        })
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| this.pick_library(window, cx))),
                )
                .child(
                    Button::new("open-settings")
                        .ghost()
                        .label("Open Settings")
                        .on_click(cx.listener(|this, _, _, cx| this.go(Route::Settings, cx))),
                )
                .when(!self.error.is_empty(), |this| {
                    this.child(div().text_color(cx.theme().danger).child(self.error.clone()))
                }),
        )
    }

    fn library_bar(
        &mut self,
        empty: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(16.))
            .py(px(10.))
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex_1()
                    .min_w(px(180.))
                    .child(Input::new(&self.search)),
            )
            .child(
                Button::new("toggle-filters")
                    .ghost()
                    .compact()
                    .icon(IconName::PanelLeft)
                    .selected(self.filters_open)
                    .tooltip(if self.filters_open {
                        "Hide filters"
                    } else {
                        "Show filters"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.filters_open = !this.filters_open;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("select-books")
                    .ghost()
                    .compact()
                    .icon(if self.select_mode {
                        IconName::Check
                    } else {
                        IconName::Menu
                    })
                    .selected(self.select_mode)
                    .disabled(self.busy || empty)
                    .tooltip(if self.select_mode {
                        "Done selecting"
                    } else {
                        "Select books"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.select_mode = !this.select_mode;
                        if !this.select_mode {
                            this.selected.clear();
                            this.bulk_open = false;
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("import-books")
                    .primary()
                    .compact()
                    .icon(IconName::Plus)
                    .tooltip("Import books")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.import_files(window, cx))),
            )
            .child(
                Button::new("switch-library")
                    .ghost()
                    .compact()
                    .icon(IconName::FolderOpen)
                    .tooltip("Switch library")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.pick_library(window, cx))),
            )
    }

    fn filters(&mut self, books: &[BookSummary], cx: &mut Context<Self>) -> impl IntoElement {
        let mut genres: HashMap<String, usize> = HashMap::new();
        let mut authors: HashMap<String, usize> = HashMap::new();
        let mut status_counts = [0usize; 3];
        let mut format_counts = [0usize; 2];
        for book in books {
            match book_status(book) {
                ReadingStatus::Reading => status_counts[0] += 1,
                ReadingStatus::ToRead => status_counts[1] += 1,
                ReadingStatus::Read => status_counts[2] += 1,
            }
            match format_kind(book) {
                FormatKind::Novel => format_counts[0] += 1,
                FormatKind::Comic => format_counts[1] += 1,
            }
            for tag in &book.tags {
                let tag = tag.trim();
                if !tag.is_empty() {
                    *genres.entry(tag.to_string()).or_default() += 1;
                }
            }
            for author in author_labels(book) {
                *authors.entry(author).or_default() += 1;
            }
        }
        let mut genre_list: Vec<_> = genres.into_iter().collect();
        genre_list.sort_by(|a, b| a.0.cmp(&b.0));
        let mut author_list: Vec<_> = authors.into_iter().collect();
        author_list.sort_by(|a, b| a.0.cmp(&b.0));

        let active = !self.filter_status.is_empty()
            || !self.filter_genres.is_empty()
            || !self.filter_formats.is_empty()
            || !self.filter_authors.is_empty();

        div()
            .id("library-filters")
            .w(px(230.))
            .h_full()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(cx.theme().border)
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(filter_heading("Status"))
            .children(status_row(
                "reading",
                "Reading",
                status_counts[0],
                self.filter_status.contains(&ReadingStatus::Reading),
                cx,
            ))
            .children(status_row(
                "to-read",
                "To Read",
                status_counts[1],
                self.filter_status.contains(&ReadingStatus::ToRead),
                cx,
            ))
            .children(status_row(
                "read",
                "Read",
                status_counts[2],
                self.filter_status.contains(&ReadingStatus::Read),
                cx,
            ))
            .child(filter_heading("Format"))
            .children(format_row(
                "novel",
                "Novel",
                format_counts[0],
                self.filter_formats.contains(&FormatKind::Novel),
                cx,
            ))
            .children(format_row(
                "comic",
                "Comic Book",
                format_counts[1],
                self.filter_formats.contains(&FormatKind::Comic),
                cx,
            ))
            .when(!genre_list.is_empty(), |this| {
                let rows = genre_list.into_iter().map(|(name, count)| {
                    let checked = self.filter_genres.contains(&name);
                    let id = format!("genre-{name}");
                    let label = format!("{name}  {count}");
                    let key = name.clone();
                    Checkbox::new(id)
                        .label(label)
                        .checked(checked)
                        .on_change(cx.listener(move |this, checked, _, cx| {
                            if *checked {
                                this.filter_genres.insert(key.clone());
                            } else {
                                this.filter_genres.remove(&key);
                            }
                            cx.notify();
                        }))
                });
                this.child(filter_heading("Genres")).children(rows)
            })
            .when(!author_list.is_empty(), |this| {
                let rows = author_list.into_iter().take(40).map(|(name, count)| {
                    let checked = self.filter_authors.contains(&name);
                    let id = format!("author-{name}");
                    let label = format!("{name}  {count}");
                    let key = name.clone();
                    Checkbox::new(id)
                        .label(label)
                        .checked(checked)
                        .on_change(cx.listener(move |this, checked, _, cx| {
                            if *checked {
                                this.filter_authors.insert(key.clone());
                            } else {
                                this.filter_authors.remove(&key);
                            }
                            cx.notify();
                        }))
                });
                this.child(filter_heading("Authors")).children(rows)
            })
            .when(active, |this| {
                this.child(
                    Button::new("clear-filters")
                        .ghost()
                        .label("Clear filters")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.filter_status.clear();
                            this.filter_genres.clear();
                            this.filter_formats.clear();
                            this.filter_authors.clear();
                            cx.notify();
                        })),
                )
            })
    }

    fn library_grid(
        &mut self,
        reading: &[BookSummary],
        to_read: &[BookSummary],
        read: &[BookSummary],
        none: bool,
        shell: gpui_kit::Entity<Shell>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_count = self.selected.len();
        div()
            .id("library-grid")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .when(!self.message.is_empty(), |this| {
                this.child(div().text_color(cx.theme().primary).child(self.message.clone()))
            })
            .when(!self.error.is_empty(), |this| {
                this.child(div().text_color(cx.theme().danger).child(self.error.clone()))
            })
            .when(self.select_mode && selected_count > 0, |this| {
                this.child(self.selection_bar(cx))
            })
            .when(self.bulk_open, |this| this.child(self.bulk_panel(cx)))
            .when(self.books.is_empty(), |this| {
                this.child(empty_panel(
                    "No books yet",
                    "Import EPUB, PDF, TXT, CBZ/CBR comics, or DRM-free MOBI to read in Grimoire. AZW3 can be imported for Kindle send.",
                    cx,
                ))
            })
            .when(!self.books.is_empty() && none, |this| {
                this.child(empty_panel(
                    "No books match these filters",
                    "Try clearing some filters, or adjust your search.",
                    cx,
                ))
            })
            .child(book_section("Reading", reading, &self.covers, self.select_mode, &self.selected, shell.clone(), cx))
            .child(book_section("To Read", to_read, &self.covers, self.select_mode, &self.selected, shell.clone(), cx))
            .child(book_section("Read", read, &self.covers, self.select_mode, &self.selected, shell, cx))
    }

    fn selection_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.selected.len();
        let can_send = self.devices.iter().any(|device| device.supports_send);
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(10.))
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .child(format!("{count} selected"))
            .child(
                Button::new("select-all")
                    .outline()
                    .label("Select all")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.selected = this
                            .filtered_books()
                            .into_iter()
                            .map(|book| book.id)
                            .collect();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("clear-selection")
                    .outline()
                    .label("Clear")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.selected.clear();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("bulk-fetch")
                    .outline()
                    .label("Fetch metadata")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.bulk_fetch(window, cx))),
            )
            .child(
                Button::new("bulk-edit")
                    .outline()
                    .label("Edit metadata")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.bulk_open = true;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("bulk-send")
                    .outline()
                    .label("Send to device")
                    .disabled(self.busy || !can_send)
                    .on_click(cx.listener(|this, _, window, cx| this.bulk_send(window, cx))),
            )
            .child(
                Button::new("bulk-delete")
                    .danger()
                    .label("Delete")
                    .disabled(self.busy)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.confirm_bulk_delete(window, cx)),
                    ),
            )
    }

    fn bulk_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(14.))
            .rounded(px(10.))
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_col()
            .gap(px(8.))
            .child("Only checked fields are applied. Tags are added, not replaced.")
            .child(Input::new(&self.bulk_tags))
            .child(
                Checkbox::new("bulk-series")
                    .label("Apply series")
                    .checked(self.bulk_series)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.bulk_series = *checked;
                        cx.notify();
                    })),
            )
            .child(Input::new(&self.bulk_series_input))
            .child(
                Checkbox::new("bulk-index")
                    .label("Apply series index")
                    .checked(self.bulk_index)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.bulk_index = *checked;
                        cx.notify();
                    })),
            )
            .child(Input::new(&self.bulk_index_input))
            .child(
                Checkbox::new("bulk-notes")
                    .label("Replace notes")
                    .checked(self.bulk_notes)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.bulk_notes = *checked;
                        cx.notify();
                    })),
            )
            .child(gpui_kit::component::input::Textarea::new(
                &self.bulk_notes_input,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        Button::new("bulk-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.bulk_open = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("bulk-apply")
                            .primary()
                            .label("Apply")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.apply_bulk(window, cx)),
                            ),
                    ),
            )
    }

    pub fn filtered_books(&self) -> Vec<BookSummary> {
        self.books
            .iter()
            .filter(|book| book_matches(book, self))
            .cloned()
            .collect()
    }

    pub fn reload_books(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.library_open {
            self.books.clear();
            self.covers.clear();
            self.loading = false;
            cx.notify();
            return;
        }
        let state = self.state.clone();
        let query = self.search.read(cx).value().to_string();
        self.loading = true;
        self.spawn_io(
            window,
            cx,
            move || {
                let query = query.trim();
                let books =
                    backend::list(&state, if query.is_empty() { None } else { Some(query) })
                        .map_err(|e| e.to_string())?;
                let covers = backend::covers_for(&state, &books);
                Ok((books, covers))
            },
            |this, result, _, cx| {
                this.loading = false;
                match result {
                    Ok((books, covers)) => {
                        this.books = books;
                        this.covers = covers;
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    pub fn pick_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose or create a Grimoire library folder")
            .pick_folder()
        else {
            return;
        };
        match backend::create_or_open(&self.state, &path) {
            Ok(status) => {
                self.apply_status(status);
                self.error.clear();
                self.message =
                    "Library path saved. Grimoire will open it on the next launch too.".into();
                self.reload_books(window, cx);
            }
            Err(error) => self.error = error.to_string(),
        }
        cx.notify();
    }

    pub fn import_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Import eBooks")
            .add_filter("eBooks", backend::SUPPORTED_EXT)
            .pick_files()
        else {
            return;
        };
        self.import_paths(paths, window, cx);
    }

    pub fn import_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error.clear();
        self.message.clear();
        self.import_progress = Some(ImportProgress {
            done: 0,
            total: 0,
            name: "Scanning for books…".into(),
        });
        let library_open = self.library_open;
        let state = self.state.clone();
        let runtime = self.runtime(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let scanned = runtime
                .spawn_blocking(move || backend::collect_ebook_paths(&paths))
                .await;
            let ebook_paths = match scanned {
                Ok(paths) => paths,
                Err(error) => {
                    this.update_in(cx, |this, _, cx| {
                        this.busy = false;
                        this.import_progress = None;
                        this.error = error.to_string();
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            if ebook_paths.is_empty() {
                this.update_in(cx, |this, _, cx| {
                    this.busy = false;
                    this.import_progress = None;
                    this.error = "No supported eBooks in that drop. Use EPUB, PDF, AZW3, MOBI, TXT, CBZ, or CBR.".into();
                    cx.notify();
                })
                .ok();
                return;
            }
            if !library_open {
                this.update_in(cx, |this, _, cx| {
                    this.busy = false;
                    this.import_progress = None;
                    this.error = "Open a library folder before importing books.".into();
                    cx.notify();
                })
                .ok();
                return;
            }

            let total = ebook_paths.len();
            let mut imported = 0usize;
            let mut failures = Vec::new();
            for (index, path) in ebook_paths.into_iter().enumerate() {
                let name = path
                    .file_name()
                    .and_then(|file_name| file_name.to_str())
                    .unwrap_or("book")
                    .to_string();
                this.update(cx, |this, cx| {
                    this.import_progress = Some(ImportProgress {
                        done: index,
                        total,
                        name: name.clone(),
                    });
                    cx.notify();
                })
                .ok();

                let state = state.clone();
                let outcome = runtime
                    .spawn(async move {
                        match backend::import(&state, &path) {
                            Ok(book) => {
                                if needs_enrichment(&book) {
                                    let _ = backend::enrich(&state, book.id).await;
                                }
                                Ok(())
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    })
                    .await;
                match outcome {
                    Ok(Ok(())) => imported += 1,
                    Ok(Err(error)) => failures.push(format!("{name}: {error}")),
                    Err(error) => failures.push(format!("{name}: {error}")),
                }
            }

            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.import_progress = None;
                if imported > 0 {
                    this.message = if imported == 1 {
                        "Imported 1 book.".into()
                    } else {
                        format!("Imported {imported} books.")
                    };
                }
                if !failures.is_empty() {
                    this.error = failures.into_iter().take(3).collect::<Vec<_>>().join(" · ");
                }
                cx.notify();
                this.reload_books(window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn apply_bulk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<i64> = self.selected.iter().copied().collect();
        let tags = self
            .bulk_tags
            .read(cx)
            .value()
            .split(',')
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect::<Vec<_>>();
        let series = self.bulk_series_input.read(cx).value().to_string();
        let index = self.bulk_index_input.read(cx).value().to_string();
        let notes = self.bulk_notes_input.read(cx).value().to_string();
        if tags.is_empty() && !self.bulk_series && !self.bulk_index && !self.bulk_notes {
            self.error = "Choose at least one field to update.".into();
            cx.notify();
            return;
        }
        let patch = BookBulkPatch {
            add_tags: tags,
            set_series: self.bulk_series,
            series: if self.bulk_series {
                let series = series.trim().to_string();
                if series.is_empty() {
                    None
                } else {
                    Some(series)
                }
            } else {
                None
            },
            set_series_index: self.bulk_index,
            series_index: if self.bulk_index {
                Some(index.trim().parse().unwrap_or(1.0))
            } else {
                None
            },
            set_comment: self.bulk_notes,
            comment: notes,
        };
        self.busy = true;
        let state = self.state.clone();
        self.spawn_io(
            window,
            cx,
            move || backend::patch_books(&state, &ids, patch).map_err(|e| e.to_string()),
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        this.message = format!("Updated {} books.", result.updated);
                        this.bulk_open = false;
                        if !result.failed.is_empty() {
                            this.error = format!("{} failed.", result.failed.len());
                        }
                    }
                    Err(error) => this.error = error,
                }
                this.reload_books(window, cx);
            },
        );
    }

    fn bulk_fetch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<i64> = self.selected.iter().copied().collect();
        let state = self.state.clone();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move {
                let mut updated = 0;
                for id in ids {
                    if let Ok(result) = backend::enrich(&state, id).await {
                        if !result.updated_fields.is_empty() {
                            updated += 1;
                        }
                    }
                }
                Ok(updated)
            },
            |this, result, window, cx| {
                this.busy = false;
                if let Ok(updated) = result {
                    this.message = format!("Fetched metadata for {updated} books.");
                }
                this.reload_books(window, cx);
            },
        );
    }

    fn bulk_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(device) = self
            .devices
            .iter()
            .find(|device| device.supports_send)
            .cloned()
        else {
            self.error =
                "No Kindle connected. Connect over MTP / USB file transfer, then try again.".into();
            cx.notify();
            return;
        };
        let ids: Vec<i64> = self.selected.iter().copied().collect();
        let state = self.state.clone();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move {
                let mut sent = 0;
                for id in ids {
                    if backend::send(&state, id, &device.id).await.is_ok() {
                        sent += 1;
                    }
                }
                Ok((sent, device.product))
            },
            |this, result, _, cx| {
                this.busy = false;
                if let Ok((sent, product)) = result {
                    this.message = format!("Sent {sent} books to {product}.");
                }
                cx.notify();
            },
        );
    }

    fn confirm_bulk_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.selected.len();
        let shell = cx.entity().clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title(format!("Delete {count} books"))
                .description("Delete the selected books from the library?")
                .on_ok({
                    let shell = shell.clone();
                    move |_, window, cx| {
                        shell.update(cx, |this, cx| this.bulk_delete(window, cx));
                        true
                    }
                })
        });
    }

    fn bulk_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<i64> = self.selected.iter().copied().collect();
        let state = self.state.clone();
        self.busy = true;
        self.spawn_io(
            window,
            cx,
            move || backend::remove_books(&state, &ids).map_err(|e| e.to_string()),
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        this.message = format!("Deleted {} books.", result.updated);
                        this.selected.clear();
                    }
                    Err(error) => this.error = error,
                }
                this.reload_books(window, cx);
            },
        );
    }

    pub fn show_book(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.book_id = Some(id);
        self.load_book(id, window, cx);
    }

    pub fn open_reader_for(
        &mut self,
        book: &BookSummary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if backend::can_read(&book.formats) {
            self.open_reader(book.id, cx);
            return;
        }
        let title = book.title.clone();
        let formats = if book.formats.is_empty() {
            "none".into()
        } else {
            book.formats.join(", ")
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert.title("Cannot open book").description(format!(
                "\"{title}\" can't be opened in the reader. Available formats: {formats}. Grimoire can open EPUB, PDF, TXT, MOBI, CBZ, and CBR."
            ))
        });
    }

    pub fn open_reader(&mut self, id: i64, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let Ok((handle, _)) = gpui_kit::open_window(
            super::reader_window_options("Grimoire", cx),
            cx,
            |window, cx| cx.new(|cx| super::reader::ReaderWindow::new(state, id, window, cx)),
        ) else {
            return;
        };
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
}

fn book_matches(book: &BookSummary, shell: &Shell) -> bool {
    if !shell.filter_status.is_empty() && !shell.filter_status.contains(&book_status(book)) {
        return false;
    }
    if !shell.filter_formats.is_empty() && !shell.filter_formats.contains(&format_kind(book)) {
        return false;
    }
    if !shell.filter_genres.is_empty()
        && !book
            .tags
            .iter()
            .any(|tag| shell.filter_genres.contains(tag.trim()))
    {
        return false;
    }
    if !shell.filter_authors.is_empty()
        && !author_labels(book)
            .iter()
            .any(|author| shell.filter_authors.contains(author))
    {
        return false;
    }
    true
}

fn book_status(book: &BookSummary) -> ReadingStatus {
    match book.progress_percent {
        None => ReadingStatus::ToRead,
        Some(percent) if percent >= 100.0 => ReadingStatus::Read,
        Some(_) => ReadingStatus::Reading,
    }
}

fn format_kind(book: &BookSummary) -> FormatKind {
    if book
        .formats
        .iter()
        .any(|format| format.eq_ignore_ascii_case("cbz") || format.eq_ignore_ascii_case("cbr"))
    {
        FormatKind::Comic
    } else {
        FormatKind::Novel
    }
}

fn author_labels(book: &BookSummary) -> Vec<String> {
    let labels = book
        .authors
        .iter()
        .map(|author| author.trim().to_string())
        .filter(|author| !author.is_empty())
        .collect::<Vec<_>>();
    if labels.is_empty() {
        vec!["Unknown".into()]
    } else {
        labels
    }
}

fn needs_enrichment(book: &crate::library::BookDetail) -> bool {
    let authors_missing = book.authors.is_empty()
        || book
            .authors
            .iter()
            .all(|author| author.trim().eq_ignore_ascii_case("unknown"));
    !book.has_cover || authors_missing
}

fn status_row(
    id: &str,
    label: &str,
    count: usize,
    checked: bool,
    cx: &mut Context<Shell>,
) -> Option<impl IntoElement> {
    let status = match id {
        "reading" => ReadingStatus::Reading,
        "read" => ReadingStatus::Read,
        _ => ReadingStatus::ToRead,
    };
    Some(
        Checkbox::new(id.to_string())
            .label(format!("{label}  {count}"))
            .checked(checked)
            .on_change(cx.listener(move |this, checked, _, cx| {
                if *checked {
                    this.filter_status.insert(status);
                } else {
                    this.filter_status.remove(&status);
                }
                cx.notify();
            })),
    )
}

fn format_row(
    id: &str,
    label: &str,
    count: usize,
    checked: bool,
    cx: &mut Context<Shell>,
) -> Option<impl IntoElement> {
    let kind = if id == "comic" {
        FormatKind::Comic
    } else {
        FormatKind::Novel
    };
    Some(
        Checkbox::new(id.to_string())
            .label(format!("{label}  {count}"))
            .checked(checked)
            .on_change(cx.listener(move |this, checked, _, cx| {
                if *checked {
                    this.filter_formats.insert(kind);
                } else {
                    this.filter_formats.remove(&kind);
                }
                cx.notify();
            })),
    )
}

fn filter_heading(text: &str) -> impl IntoElement {
    div()
        .text_size(px(12.))
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .child(text.to_string())
}

fn empty_panel(title: &str, body: &str, cx: &gpui_kit::App) -> impl IntoElement {
    div()
        .p(px(22.))
        .rounded(px(10.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .font_family(DISPLAY)
                .text_size(px(22.))
                .child(title.to_string()),
        )
        .child(
            div()
                .mt(px(6.))
                .text_color(cx.theme().muted_foreground)
                .child(body.to_string()),
        )
}

fn book_section(
    title: &str,
    books: &[BookSummary],
    covers: &HashMap<i64, PathBuf>,
    select_mode: bool,
    selected: &HashSet<i64>,
    shell: gpui_kit::Entity<Shell>,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    if books.is_empty() {
        return div();
    }
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(super::shell::section_title(title, Some(books.len())))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.))
                .children(books.iter().map(|book| {
                    book_card(
                        book,
                        covers.get(&book.id),
                        select_mode,
                        selected.contains(&book.id),
                        shell.clone(),
                        cx,
                    )
                })),
        )
}

fn book_card(
    book: &BookSummary,
    cover: Option<&PathBuf>,
    select_mode: bool,
    is_selected: bool,
    shell: gpui_kit::Entity<Shell>,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let id = book.id;
    let title = book.title.clone();
    let authors = author_labels(book).join(", ");
    let progress = book.progress_percent.filter(|percent| *percent < 100.0);
    let menu_shell = shell.clone();
    let card_title = title.clone();
    let menu_title = title.clone();

    div()
        .id(format!("book-card-{id}"))
        .w(px(148.))
        .cursor_pointer()
        .when(is_selected, |this| {
            this.border_1()
                .border_color(cx.theme().primary)
                .rounded(px(8.))
                .p(px(4.))
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            if this.select_mode {
                if this.selected.contains(&id) {
                    this.selected.remove(&id);
                } else {
                    this.selected.insert(id);
                }
                cx.notify();
            } else if let Some(book) = this.books.iter().find(|book| book.id == id).cloned() {
                this.open_reader_for(&book, window, cx);
            }
        }))
        .context_menu(move |menu, _, _| {
            let info = menu_shell.clone();
            let read = menu_shell.clone();
            let fetch = menu_shell.clone();
            let send = menu_shell.clone();
            let delete = menu_shell.clone();
            let title_for_delete = card_title.clone();
            let send_title = menu_title.clone();
            menu.item(
                PopupMenuItem::element(|_, _| {
                    div().font_weight(gpui_kit::FontWeight::BOLD).child("Read")
                })
                .on_click(move |_, window, cx| {
                    read.update(cx, |this, cx| {
                        if let Some(book) = this.books.iter().find(|book| book.id == id).cloned() {
                            this.open_reader_for(&book, window, cx);
                        }
                    });
                }),
            )
            .separator()
            .item(PopupMenuItem::new("Info").on_click(move |_, window, cx| {
                info.update(cx, |this, cx| this.show_book(id, window, cx));
            }))
            .item(PopupMenuItem::new("Edit metadata").on_click({
                let info = menu_shell.clone();
                move |_, window, cx| {
                    info.update(cx, |this, cx| this.show_book(id, window, cx));
                }
            }))
            .item(
                PopupMenuItem::new("Fetch metadata").on_click(move |_, window, cx| {
                    fetch.update(cx, |this, cx| this.fetch_one(id, window, cx));
                }),
            )
            .item(PopupMenuItem::new("Send to device").on_click({
                let send_title = send_title.clone();
                move |_, window, cx| {
                    send.update(cx, |this, cx| this.send_one(id, &send_title, window, cx));
                }
            }))
            .separator()
            .item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                let delete = delete.clone();
                let title = title_for_delete.clone();
                window.open_alert_dialog(cx, move |alert, _, _| {
                    alert
                        .confirm()
                        .title("Delete book")
                        .description(format!("Delete \"{title}\" from the library?"))
                        .on_ok({
                            let delete = delete.clone();
                            move |_, window, cx| {
                                delete.update(cx, |this, cx| this.delete_one(id, window, cx));
                                true
                            }
                        })
                });
            }))
        })
        .child(cover_box(cover, &title, progress, cx))
        .child(
            div()
                .mt(px(8.))
                .font_family(DISPLAY)
                .text_size(px(15.))
                .line_height(px(19.))
                .child(title.clone()),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(cx.theme().muted_foreground)
                .child(authors),
        )
        .when(select_mode, |this| {
            this.child(if is_selected { "Selected" } else { "" })
        })
}

fn cover_box(
    cover: Option<&PathBuf>,
    title: &str,
    progress: Option<f64>,
    cx: &gpui_kit::App,
) -> impl IntoElement {
    let letter = title.chars().next().unwrap_or('?').to_string();
    div()
        .relative()
        .w(px(140.))
        .h(px(210.))
        .rounded(px(6.))
        .overflow_hidden()
        .bg(cx.theme().muted)
        .child(match cover {
            Some(path) => img(path.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .font_family(DISPLAY)
                .text_size(px(36.))
                .text_color(cx.theme().muted_foreground)
                .child(letter)
                .into_any_element(),
        })
        .when_some(progress, |this, percent| {
            this.child(
                div()
                    .absolute()
                    .bottom(px(6.))
                    .right(px(6.))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(99.))
                    .text_size(px(11.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(format!("{}%", percent.round() as i64)),
            )
        })
}

impl Shell {
    fn fetch_one(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
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
                    }
                    Err(error) => this.error = error,
                }
                this.reload_books(window, cx);
            },
        );
    }

    fn send_one(&mut self, id: i64, title: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(device) = self
            .devices
            .iter()
            .find(|device| device.supports_send)
            .cloned()
        else {
            self.error =
                "No Kindle connected. Connect over MTP / USB file transfer, then try again.".into();
            cx.notify();
            return;
        };
        let state = self.state.clone();
        let title = title.to_string();
        self.busy = true;
        self.spawn_task(
            window,
            cx,
            async move {
                backend::send(&state, id, &device.id)
                    .await
                    .map(|result| (result, device.product))
                    .map_err(|e| e.to_string())
            },
            move |this, result, _, cx| {
                this.busy = false;
                match result {
                    Ok((result, product)) => {
                        this.message = format!(
                            "Sent \"{title}\" ({}) to {product} as {}.",
                            result.format, result.remote_path
                        );
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            },
        );
    }

    fn delete_one(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.clone();
        self.spawn_io(
            window,
            cx,
            move || backend::remove_book(&state, id).map_err(|e| e.to_string()),
            |this, result, window, cx| {
                if let Err(error) = result {
                    this.error = error;
                }
                this.reload_books(window, cx);
            },
        );
    }
}

#[allow(dead_code)]
fn _status_used(status: LibraryStatus) -> bool {
    status.open
}
