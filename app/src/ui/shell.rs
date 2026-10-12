use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Selectable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::AppContext as _;
use gpui_kit::{
    Anchor, App, Context, Entity, ExternalPaths, FontWeight, Image, ImageFormat,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render, Styled as _,
    Subscription, Window, div, img, px,
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

pub struct Shell {
    pub state: AppState,
    pub route: Route,
    pub book_id: Option<i64>,
    pub device_id: Option<String>,
    pub theme_pref: String,
    pub reader_font: String,
    pub devices: Vec<EbookReaderDevice>,
    pub notice: Option<String>,
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
    titlebar_drag: bool,
    _subs: Vec<Subscription>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search title, author, or tag")
        });
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
        let device_query = cx.new(|cx| InputState::new(window, cx).placeholder("Filter device books"));

        let search_sub = cx.subscribe_in(&search, window, |this, input, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                this.queue_search(query, window, cx);
            }
        });

        let config = backend::config().unwrap_or_default();
        let theme_pref = config.theme.clone();
        let reader_font = config.reader_font.clone();

        let shell = Self {
            state: AppState::new(),
            route: Route::Library,
            book_id: None,
            device_id: None,
            theme_pref,
            reader_font,
            devices: Vec::new(),
            notice: None,
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
            let mut seen: Vec<String> = Vec::new();
            loop {
                let next = runtime.spawn(async { backend::readers().await }).await;
                let _ = this.update(cx, |this, cx| {
                    if let Ok(Ok(devices)) = next {
                        let ids: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();
                        if !seen.is_empty() {
                            for device in &devices {
                                if !seen.iter().any(|id| id == &device.id) {
                                    this.notice = Some(format!("{} connected.", device.product));
                                }
                            }
                            for id in &seen {
                                if !ids.iter().any(|next| next == id) {
                                    let name = this
                                        .devices
                                        .iter()
                                        .find(|device| &device.id == id)
                                        .map(|device| device.product.clone())
                                        .unwrap_or_else(|| "eReader".into());
                                    this.notice = Some(format!("{name} disconnected."));
                                }
                            }
                        }
                        seen = ids;
                        this.devices = devices;
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
            .child(self.nav_button("nav-library", IconName::BookOpen, "Library", self.route == Route::Library && self.book_id.is_none(), Route::Library, cx))
            .child(self.nav_button("nav-devices", IconName::HardDrive, "Devices", self.route == Route::Devices, Route::Devices, cx))
            .child(self.nav_button("nav-settings", IconName::Settings, "Settings", self.route == Route::Settings, Route::Settings, cx))
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
        div().relative().child(
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
        ).when(count > 0, |this| {
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
