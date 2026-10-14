use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Selectable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::AppContext as _;
use gpui_kit::{
    div, img, px, relative, Anchor, App, Context, Entity, ExternalPaths, FontWeight, Image,
    ImageFormat, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render,
    Styled as _, Subscription, Window,
};

use super::backend::{self, LibraryStatus, TokioRt};
use super::theme::{self, DISPLAY};
use crate::kindle::{DeviceLibrary, EbookReaderDevice, SyncRecord};
use crate::library::{BookDetail, BookSummary};
use crate::state::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Library,
    Devices,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadingStatus {
    Reading,
    ToRead,
    Read,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatKind {
    Novel,
    Comic,
}

#[derive(Clone, Debug)]
pub struct ImportProgress {
    pub done: usize,
    pub total: usize,
    pub name: String,
}

pub(crate) fn import_status_label(progress: &ImportProgress) -> String {
    if progress.total == 0 {
        return progress.name.clone();
    }
    let current = (progress.done + 1).min(progress.total);
    format!("Importing {current} of {}", progress.total)
}

fn import_fraction(done: usize, total: usize) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (done as f32 / total as f32).clamp(0.0, 1.0)
}

pub struct Shell {
    pub state: AppState,
    pub route: Route,
    pub book_id: Option<i64>,
    pub device_id: Option<String>,
    pub theme_pref: String,
    pub reader_font: String,
    pub reader_zoom: u32,
    pub reader_colors: String,
    pub comic_zoom: u32,
    pub comic_spread: bool,
    pub devices: Vec<EbookReaderDevice>,
    pub notice: Option<String>,
    notice_gen: u64,
    pub(crate) device_io: u32,
    pub(crate) device_missing_polls: u8,
    pub booting: bool,
    pub boot_error: String,
    pub library_open: bool,
    pub library_path: Option<String>,
    pub kindle_convert: bool,
    pub books: Vec<BookSummary>,
    pub covers: HashMap<i64, PathBuf>,
    pub search: Entity<InputState>,
    pub search_gen: u64,
    pub loading: bool,
    pub busy: bool,
    pub import_progress: Option<ImportProgress>,
    pub error: String,
    pub message: String,
    pub filters_open: bool,
    pub select_mode: bool,
    pub selected: HashSet<i64>,
    pub filter_status: HashSet<ReadingStatus>,
    pub filter_genres: HashSet<String>,
    pub filter_formats: HashSet<FormatKind>,
    pub filter_authors: HashSet<String>,
    pub bulk_open: bool,
    pub bulk_series: bool,
    pub bulk_index: bool,
    pub bulk_notes: bool,
    pub bulk_tags: Entity<InputState>,
    pub bulk_series_input: Entity<InputState>,
    pub bulk_index_input: Entity<InputState>,
    pub bulk_notes_input: Entity<TextareaState>,
    pub book: Option<BookDetail>,
    pub book_cover: Option<PathBuf>,
    pub convert_targets: Vec<String>,
    pub title_input: Entity<InputState>,
    pub authors_input: Entity<InputState>,
    pub tags_input: Entity<InputState>,
    pub series_input: Entity<InputState>,
    pub series_index_input: Entity<InputState>,
    pub isbn_input: Entity<InputState>,
    pub comment_input: Entity<TextareaState>,
    pub sync_history: Vec<SyncRecord>,
    pub devices_loading: bool,
    pub device_library: Option<DeviceLibrary>,
    pub device_query: Entity<InputState>,
    pub device_selected: HashSet<String>,
    pub device_select_mode: bool,
    pub(crate) device_prompt_open: bool,
    titlebar_drag: bool,
    _subs: Vec<Subscription>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search title, author, or tag"));
        let bulk_tags = cx.new(|cx| InputState::new(window, cx).placeholder("Tags to add"));
        let bulk_series_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Series name (blank clears)"));
        let bulk_index_input = cx.new(|cx| InputState::new(window, cx).placeholder("Index"));
        let bulk_notes_input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Notes"));
        let title_input = cx.new(|cx| InputState::new(window, cx));
        let authors_input = cx.new(|cx| InputState::new(window, cx).placeholder("Author, Author"));
        let tags_input = cx.new(|cx| InputState::new(window, cx).placeholder("tag, tag"));
        let series_input = cx.new(|cx| InputState::new(window, cx));
        let series_index_input = cx.new(|cx| InputState::new(window, cx));
        let isbn_input = cx.new(|cx| InputState::new(window, cx));
        let comment_input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Notes"));
        let device_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter device books"));

        let search_sub = cx.subscribe_in(&search, window, |this, input, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                this.queue_search(query, window, cx);
            }
        });

        let config = backend::config().unwrap_or_default();
        let theme_pref = config.theme.clone();
        let reader_font = config.reader_font.clone();
        let reader_zoom = super::reader::clamp_zoom(config.reader_zoom);
        let reader_colors = super::reader::TextColors::parse(&config.reader_colors)
            .id()
            .to_string();
        let comic_zoom = super::reader::clamp_comic_zoom(config.comic_zoom);
        let comic_spread = super::reader::comic_layout_spread(&config.comic_layout);

        let shell = Self {
            state: AppState::new(),
            route: Route::Library,
            book_id: None,
            device_id: None,
            theme_pref,
            reader_font,
            reader_zoom,
            reader_colors,
            comic_zoom,
            comic_spread,
            devices: Vec::new(),
            notice: None,
            notice_gen: 0,
            device_io: 0,
            device_missing_polls: 0,
            booting: true,
            boot_error: String::new(),
            library_open: false,
            library_path: None,
            kindle_convert: true,
            books: Vec::new(),
            covers: HashMap::new(),
            search,
            search_gen: 0,
            loading: true,
            busy: false,
            import_progress: None,
            error: String::new(),
            message: String::new(),
            filters_open: true,
            select_mode: false,
            selected: HashSet::new(),
            filter_status: HashSet::new(),
            filter_genres: HashSet::new(),
            filter_formats: HashSet::new(),
            filter_authors: HashSet::new(),
            bulk_open: false,
            bulk_series: false,
            bulk_index: false,
            bulk_notes: false,
            bulk_tags,
            bulk_series_input,
            bulk_index_input,
            bulk_notes_input,
            book: None,
            book_cover: None,
            convert_targets: Vec::new(),
            title_input,
            authors_input,
            tags_input,
            series_input,
            series_index_input,
            isbn_input,
            comment_input,
            sync_history: Vec::new(),
            devices_loading: false,
            device_library: None,
            device_query,
            device_selected: HashSet::new(),
            device_select_mode: false,
            device_prompt_open: false,
            titlebar_drag: false,
            _subs: vec![search_sub],
        };
        shell.boot(window, cx);
        shell.watch_devices(window, cx);
        shell
    }

    pub fn runtime(&self, cx: &App) -> tokio::runtime::Handle {
        cx.global::<TokioRt>().0.clone()
    }

    pub fn spawn_io<T, Done>(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
        done: Done,
    ) where
        T: Send + 'static,
        Done: FnOnce(&mut Self, Result<T, String>, &mut Window, &mut Context<Self>) + 'static,
    {
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = match runtime.spawn_blocking(work).await {
                Ok(result) => result,
                Err(error) => Err(error.to_string()),
            };
            this.update_in(cx, |this, window, cx| done(this, result, window, cx))
                .ok();
        })
        .detach();
    }

    pub fn spawn_task<T, Fut, Done>(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        work: Fut,
        done: Done,
    ) where
        T: Send + 'static,
        Fut: std::future::Future<Output = Result<T, String>> + Send + 'static,
        Done: FnOnce(&mut Self, Result<T, String>, &mut Window, &mut Context<Self>) + 'static,
    {
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = match runtime.spawn(work).await {
                Ok(result) => result,
                Err(error) => Err(error.to_string()),
            };
            this.update_in(cx, |this, window, cx| done(this, result, window, cx))
                .ok();
        })
        .detach();
    }

    fn boot(&self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.clone();
        self.spawn_io(
            window,
            cx,
            move || backend::open_saved(&state).map_err(|e| e.to_string()),
            |this, result, window, cx| {
                this.booting = false;
                match result {
                    Ok(status) => {
                        this.apply_status(status);
                        if this.library_open {
                            this.reload_books(window, cx);
                        } else {
                            this.loading = false;
                        }
                    }
                    Err(error) => {
                        this.boot_error = error;
                        this.loading = false;
                    }
                }
                cx.notify();
            },
        );
    }

    fn watch_devices(&self, window: &mut Window, cx: &mut Context<Self>) {
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let mut seen: HashSet<String> = HashSet::new();
            let mut prompted: HashSet<String> = HashSet::new();
            let mut chimed: HashSet<String> = HashSet::new();
            let mut primed = false;
            loop {
                let next = runtime.spawn(async { backend::readers().await }).await;
                let _ = this.update_in(cx, |this, window, cx| {
                    if let Ok(Ok(devices)) = next {
                        let ids: HashSet<String> =
                            devices.iter().map(|device| device.id.clone()).collect();
                        if primed {
                            let mut left = false;
                            for id in &seen {
                                if !ids.contains(id) {
                                    left = true;
                                    let name = this
                                        .devices
                                        .iter()
                                        .find(|device| &device.id == id)
                                        .map(|device| device.product.clone())
                                        .unwrap_or_else(|| "eReader".into());
                                    this.show_notice(format!("{name} disconnected."), window, cx);
                                }
                            }
                            if left {
                                super::chime::play_disconnect_chime();
                            }
                        }
                        primed = true;
                        prompted.retain(|id| ids.contains(id));
                        chimed.retain(|id| ids.contains(id));
                        let offer: Vec<EbookReaderDevice> = devices
                            .iter()
                            .filter(|device| !prompted.contains(&device.id))
                            .cloned()
                            .collect();
                        let offer_ids: Vec<String> =
                            offer.iter().map(|device| device.id.clone()).collect();
                        let fresh: Vec<String> = offer_ids
                            .iter()
                            .filter(|id| !chimed.contains(*id))
                            .cloned()
                            .collect();
                        for id in &fresh {
                            chimed.insert(id.clone());
                        }
                        let will_ask = !this.device_prompt_open
                            && super::devices::offer_manage(&offer_ids, this.device_id.as_deref());
                        if !fresh.is_empty() && !will_ask {
                            super::chime::play_connect_chime();
                        }
                        this.note_open_device(&ids);
                        seen = ids;
                        this.devices = devices;
                        if this.device_prompt_open {
                            // Ask on a later pass, once the current question is closed.
                        } else if super::devices::offer_manage(
                            &offer_ids,
                            this.device_id.as_deref(),
                        ) {
                            for id in &offer_ids {
                                prompted.insert(id.clone());
                            }
                            this.device_prompt_open = true;
                            this.ask_to_manage(offer, window, cx);
                        } else {
                            for id in offer_ids {
                                prompted.insert(id);
                            }
                        }
                        cx.notify();
                    }
                });
                runtime
                    .spawn(async { tokio::time::sleep(Duration::from_secs(4)).await })
                    .await
                    .ok();
            }
        })
        .detach();
    }

    fn show_notice(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.notice_gen = self.notice_gen.wrapping_add(1);
        let generation = self.notice_gen;
        self.notice = Some(text);
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let _ = runtime
                .spawn(async {
                    tokio::time::sleep(Duration::from_secs(super::devices::NOTICE_SECS)).await;
                })
                .await;
            let _ = this.update_in(cx, |this, _window, cx| {
                if this.notice_gen == generation {
                    this.notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn apply_status(&mut self, status: LibraryStatus) {
        self.library_open = status.open;
        self.library_path = status.path;
        self.kindle_convert = status.kindle_convert_available;
    }

    pub fn go(&mut self, route: Route, cx: &mut Context<Self>) {
        self.route = route;
        if !matches!(route, Route::Library) {
            self.book_id = None;
        }
        cx.notify();
    }

    pub fn queue_search(&mut self, _query: String, window: &mut Window, cx: &mut Context<Self>) {
        self.search_gen = self.search_gen.wrapping_add(1);
        let generation = self.search_gen;
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            runtime
                .spawn(async { tokio::time::sleep(Duration::from_millis(220)).await })
                .await
                .ok();
            this.update_in(cx, |this, window, cx| {
                if this.search_gen == generation {
                    this.reload_books(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn set_theme(&mut self, pref: &str, cx: &mut Context<Self>) {
        self.theme_pref = pref.to_string();
        theme::persist_and_apply(pref, cx);
        cx.notify();
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paper = cx.theme().background;
        let ink = cx.theme().foreground;
        let line = cx.theme().border;
        let panel = cx.theme().popover;

        div()
            .id("grimoire-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(paper)
            .text_color(ink)
            .font_family(super::theme::UI_FONT)
            .text_size(px(15.))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                let files = paths.paths().to_vec();
                this.import_paths(files, window, cx);
            }))
            .child(self.topbar(window, cx))
            .when_some(self.import_progress.clone(), |this, progress| {
                this.child(import_progress_bar(&progress, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(self.body(window, cx)),
            )
            .when_some(self.notice.clone(), |this, notice| {
                this.child(notice_toast(notice, panel, line, cx))
            })
    }
}

impl Shell {
    fn topbar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let line = cx.theme().border;
        let bar = cx.theme().title_bar;
        #[cfg(target_os = "macos")]
        let leading = px(78.);
        #[cfg(not(target_os = "macos"))]
        let leading = px(16.);

        div()
            .h(px(52.))
            .flex()
            .items_center()
            .px(px(12.))
            .gap(px(8.))
            .bg(bar)
            .border_b_1()
            .border_color(line)
            .child(
                div()
                    .id("titlebar-drag")
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(leading)
                    .on_double_click(cx.listener(|this, _, window, _| {
                        this.titlebar_drag = false;
                        window.zoom_window();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.titlebar_drag = true),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.titlebar_drag = false),
                    )
                    .on_mouse_down_out(cx.listener(|this, _, _, _| this.titlebar_drag = false))
                    .on_mouse_move(cx.listener(|this, _, window, _| {
                        if this.titlebar_drag {
                            this.titlebar_drag = false;
                            window.start_window_move();
                        }
                    }))
                    .child(
                        img(std::sync::Arc::new(Image::from_bytes(
                            ImageFormat::Svg,
                            include_bytes!("../../../static/grimoire_icon.svg").to_vec(),
                        )))
                        .size(px(28.)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .font_family(DISPLAY)
                            .text_size(px(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Grimoire")
                            .child(div().text_color(accent).child(".")),
                    ),
            )
            .child(
                div()
                    .id("titlebar-spacer")
                    .flex_1()
                    .h_full()
                    .on_double_click(cx.listener(|this, _, window, _| {
                        this.titlebar_drag = false;
                        window.zoom_window();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.titlebar_drag = true),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.titlebar_drag = false),
                    )
                    .on_mouse_down_out(cx.listener(|this, _, _, _| this.titlebar_drag = false))
                    .on_mouse_move(cx.listener(|this, _, window, _| {
                        if this.titlebar_drag {
                            this.titlebar_drag = false;
                            window.start_window_move();
                        }
                    })),
            )
            .child(self.nav_button(
                "nav-library",
                IconName::BookOpen,
                "Library",
                self.route == Route::Library && self.book_id.is_none(),
                Route::Library,
                cx,
            ))
            .child(self.nav_button(
                "nav-devices",
                IconName::HardDrive,
                "Devices",
                self.route == Route::Devices,
                Route::Devices,
                cx,
            ))
            .child(self.nav_button(
                "nav-settings",
                IconName::Settings,
                "Settings",
                self.route == Route::Settings,
                Route::Settings,
                cx,
            ))
            .child(self.theme_menu(cx))
            .child(div().w(px(4.)).text_color(muted))
    }

    fn theme_menu(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.theme_pref.clone();
        let shell = cx.entity();
        Button::new("theme")
            .ghost()
            .compact()
            .icon(if cx.theme().is_dark() {
                IconName::Sun
            } else {
                IconName::Moon
            })
            .tooltip(format!("Theme: {}", theme::label(&current)))
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                menu.item(theme_choice("Dark", "dark", &current, shell.clone()))
                    .item(theme_choice("Light", "light", &current, shell.clone()))
                    .item(theme_choice(
                        "Follow System",
                        "system",
                        &current,
                        shell.clone(),
                    ))
            })
    }

    fn nav_button(
        &self,
        id: &'static str,
        icon: IconName,
        tip: &'static str,
        active: bool,
        route: Route,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let count = if id == "nav-devices" {
            self.devices.len()
        } else {
            0
        };
        div()
            .relative()
            .child(
                Button::new(id)
                    .ghost()
                    .compact()
                    .icon(icon)
                    .selected(active)
                    .tooltip(tip)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.book_id = None;
                        this.device_id = None;
                        this.route = route;
                        if route == Route::Devices {
                            this.refresh_devices(window, cx);
                        }
                        cx.notify();
                    })),
            )
            .when(count > 0, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(0.))
                        .right(px(0.))
                        .px(px(4.))
                        .rounded(px(99.))
                        .text_size(px(10.))
                        .bg(cx.theme().primary)
                        .text_color(cx.theme().primary_foreground)
                        .child(count.to_string()),
                )
            })
    }

    fn body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        if self.booting {
            return muted_page("Opening library…", cx).into_any_element();
        }
        if !self.boot_error.is_empty() && !self.library_open && self.route == Route::Library {
            return muted_page(&self.boot_error, cx).into_any_element();
        }
        if self.book_id.is_some() {
            return self.book_page(window, cx).into_any_element();
        }
        match self.route {
            Route::Library => self.library_page(window, cx).into_any_element(),
            Route::Devices => {
                if self.device_id.is_some() {
                    self.device_library_page(window, cx).into_any_element()
                } else {
                    self.devices_page(window, cx).into_any_element()
                }
            }
            Route::Settings => self.settings_page(window, cx).into_any_element(),
        }
    }
}

fn notice_toast(
    notice: String,
    panel: gpui_kit::Hsla,
    line: gpui_kit::Hsla,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    div()
        .absolute()
        .bottom(px(18.))
        .right(px(18.))
        .max_w(px(360.))
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(14.))
        .py(px(10.))
        .rounded(px(10.))
        .bg(panel)
        .border_1()
        .border_color(line)
        .child(div().flex_1().child(notice))
        .child(
            Button::new("dismiss-notice")
                .ghost()
                .compact()
                .icon(IconName::Close)
                .tooltip("Dismiss")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                })),
        )
}

pub fn muted_page(text: &str, cx: &App) -> impl IntoElement {
    div()
        .p(px(28.))
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}

fn import_progress_bar(progress: &ImportProgress, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let show_name = progress.total > 0;
    div()
        .px(px(16.))
        .pt(px(8.))
        .pb(px(10.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .bg(cx.theme().sidebar)
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .text_size(px(13.))
                .child(import_status_label(progress))
                .when(show_name, |this| {
                    this.child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(muted)
                            .child(progress.name.clone()),
                    )
                }),
        )
        .child(
            div()
                .w_full()
                .h(px(4.))
                .rounded(px(99.))
                .overflow_hidden()
                .bg(cx.theme().muted)
                .child(
                    div()
                        .h_full()
                        .w(relative(import_fraction(progress.done, progress.total)))
                        .bg(cx.theme().primary),
                ),
        )
}

pub fn section_title(text: &str, count: Option<usize>) -> impl IntoElement {
    div()
        .flex()
        .items_end()
        .gap(px(8.))
        .child(
            div()
                .font_family(DISPLAY)
                .text_size(px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text.to_string()),
        )
        .when_some(count, |this, count| {
            this.child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(count.to_string()),
            )
        })
}

fn theme_choice(
    label: &'static str,
    pref: &'static str,
    current: &str,
    shell: Entity<Shell>,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .checked(current == pref)
        .on_click(move |_, _, cx| {
            shell.update(cx, |this, cx| this.set_theme(pref, cx));
        })
}

#[cfg(test)]
mod tests {
    use super::{import_fraction, import_status_label, ImportProgress};

    #[test]
    fn import_status_counts_the_book_in_progress() {
        assert_eq!(
            import_status_label(&ImportProgress {
                done: 0,
                total: 0,
                name: "Scanning for books…".into(),
            }),
            "Scanning for books…"
        );
        assert_eq!(
            import_status_label(&ImportProgress {
                done: 0,
                total: 1,
                name: "Tale.epub".into(),
            }),
            "Importing 1 of 1"
        );
        assert_eq!(
            import_status_label(&ImportProgress {
                done: 2,
                total: 10,
                name: "Next.epub".into(),
            }),
            "Importing 3 of 10"
        );
        assert_eq!(import_fraction(0, 0), 0.0);
        assert_eq!(import_fraction(2, 10), 0.2);
        assert_eq!(import_fraction(10, 10), 1.0);
    }
}
