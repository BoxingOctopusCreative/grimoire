use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _,
};
use gpui_kit::{
    div, img, px, rgb, Context, FontWeight, Hsla, Image, ImageFormat, IntoElement, MouseButton,
    ObjectFit, ParentElement as _, Pixels, Render, Size, Window,
};

use super::backend;
use super::document::{self, Page};
use super::settings::reader_family;
use super::theme::DISPLAY;
use crate::reader::ReadingProgressUpdate;
use crate::state::AppState;

pub struct ReaderWindow {
    state: AppState,
    book_id: i64,
    title: String,
    authors: String,
    format: String,
    path: PathBuf,
    cover: Option<PathBuf>,
    on_cover: bool,
    error: String,
    loading: bool,
    chapters: Vec<super::document::Chapter>,
    pages: Vec<Page>,
    page: usize,
    anchor_chapter: usize,
    anchor_offset: usize,
    layout_key: (u32, u32, u32),
    zoom: u32,
    colors: TextColors,
    comic_zoom: u32,
    comic_spread: bool,
    comic_count: usize,
    comic_index: usize,
    comic_images: Vec<ComicFrame>,
    comic_load: u64,
    percent: f64,
    titlebar_drag: bool,
    /// `Some(true)` is the right side, `Some(false)` the left. `None` when the pointer is in the middle.
    page_edge: Option<bool>,
}

impl ReaderWindow {
    pub fn new(state: AppState, book_id: i64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = backend::config().ok();
        let zoom = clamp_zoom(
            saved
                .as_ref()
                .map(|config| config.reader_zoom)
                .unwrap_or(100),
        );
        let colors = TextColors::parse(
            saved
                .as_ref()
                .map(|config| config.reader_colors.as_str())
                .unwrap_or("native"),
        );
        let comic_zoom = clamp_comic_zoom(
            saved
                .as_ref()
                .map(|config| config.comic_zoom)
                .unwrap_or(100),
        );
        let comic_spread = saved
            .as_ref()
            .is_some_and(|config| comic_layout_spread(&config.comic_layout));
        let reader = Self {
            state,
            book_id,
            title: "Reading".into(),
            authors: String::new(),
            format: String::new(),
            path: PathBuf::new(),
            cover: None,
            on_cover: false,
            error: String::new(),
            loading: true,
            chapters: Vec::new(),
            pages: Vec::new(),
            page: 0,
            anchor_chapter: 0,
            anchor_offset: 0,
            layout_key: (0, 0, 0),
            zoom,
            colors,
            comic_zoom,
            comic_spread,
            comic_count: 0,
            comic_index: 0,
            comic_images: Vec::new(),
            comic_load: 0,
            percent: 0.0,
            titlebar_drag: false,
            page_edge: None,
        };
        reader.load(window, cx);
        reader
    }

    fn runtime(&self, cx: &gpui_kit::App) -> tokio::runtime::Handle {
        cx.global::<super::backend::TokioRt>().0.clone()
    }

    fn load(&self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let book_id = self.book_id;
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let loaded = runtime
                .spawn_blocking(move || {
                    let source =
                        backend::reader_source(&state, book_id).map_err(|e| e.to_string())?;
                    let cover = backend::book_cover(&state, book_id).ok().flatten();
                    let saved = backend::progress(&state, book_id).ok().flatten();
                    Ok::<_, String>((source, cover, saved))
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match loaded {
                    Ok((source, cover, saved)) => {
                        this.title = source.title.clone();
                        this.authors = source.authors.join(", ");
                        this.format = source.format.clone();
                        this.path = PathBuf::from(&source.path);
                        this.cover = cover;
                        if let Some(saved) = saved {
                            this.percent = saved.percent;
                            if saved.location == "cover" {
                                this.on_cover = this.cover.is_some();
                            } else if let Some(rest) = saved.location.strip_prefix("loc:") {
                                let mut parts = rest.split(':');
                                this.anchor_chapter =
                                    parts.next().unwrap_or("0").parse().unwrap_or(0);
                                this.anchor_offset =
                                    parts.next().unwrap_or("0").parse().unwrap_or(0);
                            } else if let Some(index) = saved.location.strip_prefix("chapter:") {
                                this.anchor_chapter = index.parse().unwrap_or(0);
                            } else if let Some(index) = saved.location.strip_prefix("page:") {
                                this.comic_index = index.parse().unwrap_or(0);
                            }
                        } else {
                            this.on_cover = this.cover.is_some();
                        }
                        this.open_format(window, cx);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let format = self.format.to_ascii_uppercase();
        if format == "PDF" {
            return;
        }
        if format == "CBZ" || format == "CBR" {
            if self.comic_spread {
                self.comic_index = align_comic_spread(self.comic_index);
            }
            self.load_comic_page(window, cx);
            return;
        }
        let path = self.path.clone();
        let format = format.clone();
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let chapters = runtime
                .spawn_blocking(move || document::load_chapters(&path, &format))
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            this.update(cx, |this, cx| {
                match chapters {
                    Ok(chapters) => {
                        if this.anchor_chapter >= chapters.len() {
                            this.anchor_chapter = 0;
                            this.anchor_offset = 0;
                        }
                        this.chapters = chapters;
                        this.pages.clear();
                        this.layout_key = (0, 0, 0);
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_comic_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.path.clone();
        let index = self.comic_index;
        let spread = self.comic_spread && !self.on_cover;
        self.comic_load = self.comic_load.wrapping_add(1);
        let generation = self.comic_load;
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let page = runtime
                .spawn_blocking(move || {
                    let (_name, data_url, count) =
                        backend::comic_page(&path, index).map_err(|e| e.to_string())?;
                    let next = if spread && index + 1 < count {
                        let (_name, next_url, _) =
                            backend::comic_page(&path, index + 1).map_err(|e| e.to_string())?;
                        Some(next_url)
                    } else {
                        None
                    };
                    Ok::<_, String>((data_url, next, count))
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            this.update(cx, |this, cx| {
                if this.comic_load != generation {
                    return;
                }
                match page {
                    Ok((data_url, next, count)) => {
                        this.comic_count = count;
                        let mut images = Vec::new();
                        if let Some(image) = image_from_data_url(&data_url) {
                            images.push(image);
                        }
                        if let Some(next) = next {
                            if let Some(image) = image_from_data_url(&next) {
                                images.push(image);
                            }
                        }
                        this.comic_images = images;
                        if this.on_cover {
                            this.percent = slot_percent(true, 0, this.cover.is_some(), count);
                        } else {
                            this.remember("page", this.comic_index, cx);
                        }
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remember(&mut self, kind: &str, index: usize, cx: &mut Context<Self>) {
        let body_len = if kind == "page" {
            self.comic_count
        } else {
            self.chapters.len()
        };
        let percent = slot_percent(
            false,
            index,
            self.cover.is_some() && kind == "page",
            body_len,
        );
        self.percent = percent;
        let state = self.state.clone();
        let book_id = self.book_id;
        let format = self.format.clone();
        let location = format!("{kind}:{index}");
        let runtime = self.runtime(cx);
        cx.spawn(async move |_this, _cx| {
            let _ = runtime
                .spawn_blocking(move || {
                    backend::save_progress(
                        &state,
                        book_id,
                        ReadingProgressUpdate {
                            format,
                            location,
                            percent,
                        },
                    )
                })
                .await;
        })
        .detach();
    }

    fn ensure_pages(&mut self, window: &Window) {
        if self.chapters.is_empty() {
            return;
        }
        let size = window.viewport_size();
        let key = (
            (size.width.as_f32() / 8.0) as u32,
            (size.height.as_f32() / 8.0) as u32,
            self.zoom,
        );
        if key == self.layout_key && !self.pages.is_empty() {
            return;
        }
        let (lines, cols) = page_capacity(size, self.zoom);
        self.pages = document::paginate(&self.chapters, lines, cols);
        self.layout_key = key;
        if !self.on_cover {
            self.page =
                document::page_index_at(&self.pages, self.anchor_chapter, self.anchor_offset);
        }
        self.percent = slot_percent(
            self.on_cover,
            self.page,
            self.cover.is_some(),
            self.pages.len(),
        );
    }

    fn turn(&mut self, forward: bool) -> bool {
        if self.is_comic() {
            let Some((on_cover, index)) = step_comic(
                self.on_cover,
                self.comic_index,
                self.cover.is_some(),
                self.comic_count,
                self.comic_spread,
                forward,
            ) else {
                return false;
            };
            self.on_cover = on_cover;
            self.comic_index = index;
            return true;
        }
        let Some((on_cover, index)) = step_place(
            self.on_cover,
            self.body_index(),
            self.cover.is_some(),
            self.body_len(),
            forward,
        ) else {
            return false;
        };
        self.on_cover = on_cover;
        self.set_body_index(index);
        true
    }

    fn body_len(&self) -> usize {
        if self.is_comic() {
            self.comic_count
        } else {
            self.pages.len()
        }
    }

    fn body_index(&self) -> usize {
        if self.is_comic() {
            self.comic_index
        } else {
            self.page
        }
    }

    fn set_body_index(&mut self, index: usize) {
        if self.is_comic() {
            self.comic_index = index;
        } else {
            self.page = index;
        }
    }

    fn is_comic(&self) -> bool {
        let format = self.format.to_ascii_uppercase();
        format == "CBZ" || format == "CBR"
    }

    fn remember_page(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.pages.get(self.page) else {
            return;
        };
        self.anchor_chapter = page.chapter;
        self.anchor_offset = page.offset;
        self.percent = slot_percent(false, self.page, self.cover.is_some(), self.pages.len());
        let state = self.state.clone();
        let book_id = self.book_id;
        let format = self.format.clone();
        let location = format!("loc:{}:{}", page.chapter, page.offset);
        let percent = self.percent;
        let runtime = self.runtime(cx);
        cx.spawn(async move |_this, _cx| {
            let _ = runtime
                .spawn_blocking(move || {
                    backend::save_progress(
                        &state,
                        book_id,
                        ReadingProgressUpdate {
                            format,
                            location,
                            percent,
                        },
                    )
                })
                .await;
        })
        .detach();
    }

    fn remember_cover(&mut self, cx: &mut Context<Self>) {
        self.percent = slot_percent(true, 0, true, self.body_len());
        let state = self.state.clone();
        let book_id = self.book_id;
        let format = self.format.clone();
        let percent = self.percent;
        let runtime = self.runtime(cx);
        cx.spawn(async move |_this, _cx| {
            let _ = runtime
                .spawn_blocking(move || {
                    backend::save_progress(
                        &state,
                        book_id,
                        ReadingProgressUpdate {
                            format,
                            location: "cover".into(),
                            percent,
                        },
                    )
                })
                .await;
        })
        .detach();
    }

    fn set_zoom(&mut self, zoom: u32, cx: &mut Context<Self>) {
        if let Some(page) = self.pages.get(self.page) {
            self.anchor_chapter = page.chapter;
            self.anchor_offset = page.offset;
        }
        self.zoom = clamp_zoom(zoom);
        self.layout_key = (0, 0, 0);
        let _ = backend::save_reader_zoom(self.zoom);
        cx.notify();
    }

    fn set_comic_zoom(&mut self, zoom: u32, cx: &mut Context<Self>) {
        self.comic_zoom = clamp_comic_zoom(zoom);
        let _ = backend::save_comic_zoom(self.comic_zoom);
        cx.notify();
    }

    fn set_comic_spread(&mut self, spread: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.comic_spread == spread {
            return;
        }
        self.comic_spread = spread;
        let _ = backend::save_comic_layout(comic_layout_id(spread));
        if spread && !self.on_cover {
            self.comic_index = align_comic_spread(self.comic_index);
        }
        if self.on_cover {
            cx.notify();
            return;
        }
        self.load_comic_page(window, cx);
    }

    fn set_colors(&mut self, colors: TextColors, cx: &mut Context<Self>) {
        self.colors = colors;
        let _ = backend::save_reader_colors(colors.id());
        cx.notify();
    }

    fn hover_page_edge(&mut self, edge: Option<bool>, cx: &mut Context<Self>) {
        if self.page_edge == edge {
            return;
        }
        self.page_edge = edge;
        cx.notify();
    }

    /// Side band over the page. Clicks turn the page. The chevron shows while the pointer is in the band.
    fn page_edge(
        &mut self,
        forward: bool,
        at_start: bool,
        at_end: bool,
        width: f32,
        ink: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let blocked = if forward { at_end } else { at_start };
        let hinted = visible_page_edge(self.page_edge, at_start, at_end) == Some(forward);
        div()
            .absolute()
            .top(px(0.))
            .bottom(px(0.))
            .w(px(width))
            .flex()
            .items_center()
            .when(forward, |zone| zone.right(px(0.)).justify_end().pr(px(16.)))
            .when(!forward, |zone| {
                zone.left(px(0.)).justify_start().pl(px(16.))
            })
            .when(!blocked, |zone| zone.cursor_pointer())
            .when(hinted, |zone| zone.child(page_chevron(forward, ink)))
            .block_mouse_except_scroll()
            .id(if forward { "turn-forward" } else { "turn-back" })
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                this.hover_page_edge(Some(forward), cx);
            }))
            .on_mouse_exit(cx.listener(move |this, _, _, cx| {
                if this.page_edge == Some(forward) {
                    this.hover_page_edge(None, cx);
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.turn(forward) {
                    this.finish_turn(window, cx);
                }
            }))
    }
}

impl Render for ReaderWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = backend::config()
            .map(|config| config.reader_font)
            .unwrap_or_else(|_| "serif".into());
        let family = reader_family(&font);
        let ink = cx.theme().foreground;

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(ink)
            .child(self.chrome(window, cx))
            .child(self.content(family, window, cx))
    }
}

impl ReaderWindow {
    fn chrome(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(48.))
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(10.))
            .id("reader-titlebar")
            .border_b_1()
            .border_color(cx.theme().border)
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
                div()
                    .pl(px(72.))
                    .font_family(DISPLAY)
                    .text_size(px(16.))
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{} · {:.0}%", self.format, self.percent)),
            )
    }

    fn content(
        &mut self,
        family: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if self.loading {
            return div().p(px(24.)).child("Opening book…").into_any_element();
        }
        if !self.error.is_empty() {
            return div()
                .p(px(24.))
                .text_color(cx.theme().danger)
                .child(self.error.clone())
                .into_any_element();
        }
        let format = self.format.to_ascii_uppercase();
        if format == "PDF" {
            let path = self.path.clone();
            return div()
                .p(px(28.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .when_some(self.cover.clone(), |this, path| {
                    this.child(
                        img(path)
                            .max_h(px(360.))
                            .max_w(px(240.))
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .child(
                    div()
                        .font_family(DISPLAY)
                        .text_size(px(24.))
                        .child(self.title.clone()),
                )
                .child(
                    "PDF opens in your system viewer. EPUB, TXT, and comics stay inside Grimoire.",
                )
                .child(
                    Button::new("open-pdf")
                        .primary()
                        .label("Open PDF")
                        .on_click(move |_, _, _| open_path(&path)),
                )
                .into_any_element();
        }
        if format == "CBZ" || format == "CBR" {
            return self.comic_view(window, cx).into_any_element();
        }
        self.text_view(family, window, cx).into_any_element()
    }

    fn text_view(
        &mut self,
        family: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_pages(window);
        let page = self.pages.get(self.page).cloned();
        let count = self.pages.len();
        let index = self.page;
        let has_cover = self.cover.is_some();
        let on_cover = self.on_cover;
        let at_start = step_place(on_cover, index, has_cover, count, false).is_none();
        let at_end = step_place(on_cover, index, has_cover, count, true).is_none();
        let shown = reading_slot(on_cover, index, has_cover) + 1;
        let total = reading_len(count, has_cover).max(1);
        let (paper, ink) = self
            .colors
            .surface(cx.theme().popover, cx.theme().foreground);
        let margin = if self.colors == TextColors::Native {
            cx.theme().background
        } else {
            paper
        };
        let scale = self.zoom as f32 / 100.0;
        let edge_width = page_edge_width(window.viewport_size().width.as_f32());
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(20.))
                    .py(px(8.))
                    .child(
                        Button::new("prev-page")
                            .ghost()
                            .label("Previous")
                            .disabled(at_start)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.turn(false) {
                                    this.finish_turn(window, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{shown} / {total}")),
                    )
                    .child(
                        Button::new("next-page")
                            .ghost()
                            .label("Next")
                            .disabled(at_end)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.turn(true) {
                                    this.finish_turn(window, cx);
                                }
                            })),
                    ),
            )
            .child(self.reading_controls(cx))
            .child(
                div()
                    .id("reader-page")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .px(px(28.))
                    .py(px(18.))
                    .flex()
                    .flex_col()
                    .bg(margin)
                    .child(if on_cover {
                        self.cover_frame(paper).into_any_element()
                    } else {
                        div()
                            .w_full()
                            .p(px(22.))
                            .rounded(px(10.))
                            .bg(paper)
                            .text_color(ink)
                            .font_family(family)
                            .text_size(px(18. * scale))
                            .line_height(px(30. * scale))
                            .child(match page {
                                Some(page) => div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(14. * scale))
                                    .children(page.blocks.into_iter().map(move |block| {
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(14. * scale))
                                            .when_some(block.heading, |this, title| {
                                                this.child(
                                                    div()
                                                        .font_family(DISPLAY)
                                                        .text_size(px(26. * scale))
                                                        .child(title),
                                                )
                                            })
                                            .child(block.text)
                                    }))
                                    .into_any_element(),
                                None => div().child("No pages in this book.").into_any_element(),
                            })
                            .into_any_element()
                    })
                    .child(self.page_edge(false, at_start, at_end, edge_width, ink, cx))
                    .child(self.page_edge(true, at_start, at_end, edge_width, ink, cx)),
            )
    }

    fn reading_controls(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.zoom;
        let colors = self.colors;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .px(px(20.))
            .pb(px(4.))
            .child(
                Button::new("zoom-out")
                    .ghost()
                    .label("A-")
                    .disabled(zoom <= ZOOM_MIN)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_zoom(this.zoom.saturating_sub(ZOOM_STEP), cx);
                    })),
            )
            .child(
                div()
                    .min_w(px(52.))
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{zoom}%")),
            )
            .child(
                Button::new("zoom-in")
                    .ghost()
                    .label("A+")
                    .disabled(zoom >= ZOOM_MAX)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_zoom(this.zoom.saturating_add(ZOOM_STEP), cx);
                    })),
            )
            .children(TextColors::CHOICES.iter().copied().map(|choice| {
                let selected = choice == colors;
                Button::new(format!("colors-{}", choice.id()))
                    .when(selected, |button| button.primary())
                    .when(!selected, |button| button.outline())
                    .label(choice.label())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_colors(choice, cx)))
            }))
    }

    fn cover_frame(&self, paper: Hsla) -> impl IntoElement {
        let image = self.cover.clone().map(|path| {
            img(path)
                .max_w_full()
                .max_h_full()
                .object_fit(ObjectFit::Contain)
        });
        div()
            .flex_1()
            .min_h(px(0.))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(22.))
            .rounded(px(10.))
            .bg(paper)
            .when_some(image, |this, image| this.child(image))
    }

    fn finish_turn(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.on_cover {
            self.remember_cover(cx);
            cx.notify();
        } else if self.is_comic() {
            self.load_comic_page(window, cx);
        } else {
            self.remember_page(cx);
            cx.notify();
        }
    }

    fn comic_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let images = self.comic_images.clone();
        let index = self.comic_index;
        let count = self.comic_count;
        let has_cover = self.cover.is_some();
        let on_cover = self.on_cover;
        let spread = self.comic_spread;
        let at_start = step_comic(on_cover, index, has_cover, count, spread, false).is_none();
        let at_end = step_comic(on_cover, index, has_cover, count, spread, true).is_none();
        let place = comic_place_label(on_cover, index, has_cover, count, spread);
        let paper = self
            .colors
            .surface(cx.theme().popover, cx.theme().foreground)
            .0;
        let pages_across = if spread && !on_cover && images.len() > 1 {
            2
        } else {
            1
        };
        let page_width = comic_page_width(window.viewport_size(), pages_across);
        let zoom = self.comic_zoom;
        let sizes: Vec<(f32, f32)> = images
            .iter()
            .map(|frame| fitted_page_size(page_width, frame.width, frame.height, zoom))
            .collect();
        let content_width = sizes.iter().map(|(width, _)| *width).sum::<f32>()
            + COMIC_SPREAD_GAP * sizes.len().saturating_sub(1) as f32;
        let edge_width = page_edge_width(window.viewport_size().width.as_f32());
        let ink = cx.theme().foreground;
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .py(px(8.))
                    .child(
                        Button::new("prev-page")
                            .ghost()
                            .label("Previous")
                            .disabled(at_start)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.turn(false) {
                                    this.finish_turn(window, cx);
                                }
                            })),
                    )
                    .child(place)
                    .child(
                        Button::new("next-page")
                            .ghost()
                            .label("Next")
                            .disabled(at_end)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.turn(true) {
                                    this.finish_turn(window, cx);
                                }
                            })),
                    ),
            )
            .child(self.comic_controls(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("comic-scroll")
                            .flex_1()
                            .min_h(px(0.))
                            .w_full()
                            .overflow_scroll()
                            .flex()
                            .flex_col()
                            .items_start()
                            .child(if on_cover {
                                self.cover_frame(paper).into_any_element()
                            } else if images.is_empty() {
                                div().p(px(12.)).child("Loading page…").into_any_element()
                            } else {
                                div()
                                    .flex_shrink_0()
                                    .w(px(content_width))
                                    .flex()
                                    .flex_row()
                                    .items_start()
                                    .justify_start()
                                    .gap(px(COMIC_SPREAD_GAP))
                                    .children(images.into_iter().zip(sizes).map(
                                        |(frame, (width, height))| {
                                            img(frame.image)
                                                .flex_shrink_0()
                                                .w(px(width))
                                                .h(px(height))
                                                .object_fit(ObjectFit::Contain)
                                        },
                                    ))
                                    .into_any_element()
                            }),
                    )
                    .child(self.page_edge(false, at_start, at_end, edge_width, ink, cx))
                    .child(self.page_edge(true, at_start, at_end, edge_width, ink, cx)),
            )
    }

    fn comic_controls(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.comic_zoom;
        let spread = self.comic_spread;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .px(px(20.))
            .pb(px(4.))
            .child(
                Button::new("comic-zoom-out")
                    .ghost()
                    .label("-")
                    .disabled(zoom <= COMIC_ZOOM_MIN)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_comic_zoom(this.comic_zoom.saturating_sub(COMIC_ZOOM_STEP), cx);
                    })),
            )
            .child(
                div()
                    .min_w(px(52.))
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{zoom}%")),
            )
            .child(
                Button::new("comic-zoom-in")
                    .ghost()
                    .label("+")
                    .disabled(zoom >= COMIC_ZOOM_MAX)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_comic_zoom(this.comic_zoom.saturating_add(COMIC_ZOOM_STEP), cx);
                    })),
            )
            .child(
                Button::new("comic-single")
                    .when(!spread, |button| button.primary())
                    .when(spread, |button| button.outline())
                    .label("Single page")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_comic_spread(false, window, cx);
                    })),
            )
            .child(
                Button::new("comic-spread")
                    .when(spread, |button| button.primary())
                    .when(!spread, |button| button.outline())
                    .label("Two pages")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_comic_spread(true, window, cx);
                    })),
            )
    }
}

pub(crate) const ZOOM_MIN: u32 = 80;
pub(crate) const ZOOM_MAX: u32 = 200;
pub(crate) const ZOOM_STEP: u32 = 10;
pub(crate) const COMIC_ZOOM_MIN: u32 = 50;
pub(crate) const COMIC_ZOOM_MAX: u32 = 200;
pub(crate) const COMIC_ZOOM_STEP: u32 = 10;
const COMIC_SPREAD_GAP: f32 = 12.0;

pub(crate) fn clamp_zoom(zoom: u32) -> u32 {
    zoom.clamp(ZOOM_MIN, ZOOM_MAX)
}

pub(crate) fn clamp_comic_zoom(zoom: u32) -> u32 {
    zoom.clamp(COMIC_ZOOM_MIN, COMIC_ZOOM_MAX)
}

pub(crate) fn comic_layout_spread(layout: &str) -> bool {
    layout.eq_ignore_ascii_case("spread")
}

pub(crate) fn comic_layout_id(spread: bool) -> &'static str {
    if spread {
        "spread"
    } else {
        "single"
    }
}

fn align_comic_spread(index: usize) -> usize {
    index - (index % 2)
}

const PAGE_EDGE_FRACTION: f32 = 0.28;
const PAGE_EDGE_MIN: f32 = 128.0;

/// Width of each clickable side of the page. The two bands stay apart so a middle strip remains.
fn page_edge_width(viewport_width: f32) -> f32 {
    let width = viewport_width.max(1.0);
    let cap = (width * 0.42).max(1.0);
    let band = width * PAGE_EDGE_FRACTION;
    band.clamp(PAGE_EDGE_MIN.min(cap), cap)
}

/// `Some(true)` is the right-hand chevron. A side at the end of the book stays hidden.
fn visible_page_edge(hover: Option<bool>, at_start: bool, at_end: bool) -> Option<bool> {
    match hover {
        Some(false) if !at_start => Some(false),
        Some(true) if !at_end => Some(true),
        _ => None,
    }
}

fn page_chevron(forward: bool, ink: Hsla) -> impl IntoElement {
    div()
        .w(px(88.))
        .h(px(88.))
        .rounded(px(44.))
        .flex()
        .items_center()
        .justify_center()
        .bg(ink.opacity(0.2))
        .text_color(ink.opacity(0.82))
        .text_size(px(64.))
        .line_height(px(64.))
        .font_weight(FontWeight::BOLD)
        .child(if forward { "❯" } else { "❮" })
}

/// Width of one page so the visible pages span the reader window. `pages` is 1 or 2.
fn comic_page_width(viewport: Size<Pixels>, pages: usize) -> f32 {
    let pages = pages.clamp(1, 2);
    let gap = if pages > 1 { COMIC_SPREAD_GAP } else { 0.0 };
    let width = viewport.width.as_f32().max(160.0);
    ((width - gap) / pages as f32).max(80.0)
}

/// Width fills the slot. Height follows the page aspect, so a tall page scrolls.
fn fitted_page_size(slot_w: f32, image_w: f32, image_h: f32, zoom: u32) -> (f32, f32) {
    let image_w = if image_w > 0.0 { image_w } else { 2.0 };
    let image_h = if image_h > 0.0 { image_h } else { 3.0 };
    let width = slot_w * zoom as f32 / 100.0;
    (width, width * (image_h / image_w))
}

fn reading_len(body_len: usize, has_cover: bool) -> usize {
    body_len + usize::from(has_cover)
}

fn reading_slot(on_cover: bool, body_index: usize, has_cover: bool) -> usize {
    if on_cover {
        0
    } else {
        body_index + usize::from(has_cover)
    }
}

fn slot_percent(on_cover: bool, body_index: usize, has_cover: bool, body_len: usize) -> f64 {
    let total = reading_len(body_len, has_cover).max(1);
    let index = reading_slot(on_cover, body_index, has_cover);
    ((index as f64 + 1.0) / total as f64) * 100.0
}

/// The metadata cover sits in front of the body. `None` means that direction has no page.
fn step_place(
    on_cover: bool,
    body_index: usize,
    has_cover: bool,
    body_len: usize,
    forward: bool,
) -> Option<(bool, usize)> {
    if forward {
        if on_cover {
            if body_len == 0 {
                None
            } else {
                Some((false, 0))
            }
        } else if body_index + 1 < body_len {
            Some((false, body_index + 1))
        } else {
            None
        }
    } else if on_cover {
        None
    } else if body_index > 0 {
        Some((false, body_index - 1))
    } else if has_cover {
        Some((true, 0))
    } else {
        None
    }
}

fn step_comic(
    on_cover: bool,
    index: usize,
    has_cover: bool,
    count: usize,
    spread: bool,
    forward: bool,
) -> Option<(bool, usize)> {
    let step = if spread { 2 } else { 1 };
    if forward {
        if on_cover {
            if count == 0 {
                None
            } else {
                Some((false, 0))
            }
        } else {
            let next = index.saturating_add(step);
            if next < count {
                Some((false, next))
            } else {
                None
            }
        }
    } else if on_cover {
        None
    } else if index == 0 {
        if has_cover {
            Some((true, 0))
        } else {
            None
        }
    } else {
        Some((false, index.saturating_sub(step)))
    }
}

fn comic_place_label(
    on_cover: bool,
    index: usize,
    has_cover: bool,
    count: usize,
    spread: bool,
) -> String {
    let total = reading_len(count, has_cover).max(1);
    if on_cover || !spread || count == 0 {
        let shown = reading_slot(on_cover, index, has_cover) + 1;
        return format!("{shown} / {total}");
    }
    let left = reading_slot(false, index, has_cover) + 1;
    let right_index = (index + 1).min(count - 1);
    let right = reading_slot(false, right_index, has_cover) + 1;
    if right == left {
        format!("{left} / {total}")
    } else {
        format!("{left}-{right} / {total}")
    }
}

fn page_capacity(size: Size<Pixels>, zoom: u32) -> (usize, usize) {
    let zoom = clamp_zoom(zoom) as f32 / 100.0;
    let font = 18.0 * zoom;
    let line = 30.0 * zoom;
    let text_width = (size.width.as_f32() - 28.0 * 2.0 - 22.0 * 2.0).max(240.0);
    let chars = (text_width / (font * 0.55)).floor() as usize;
    let text_height = (size.height.as_f32() - 48.0 - 52.0 - 40.0 - 36.0 - 44.0).max(line * 6.0);
    let lines = (text_height / line).floor() as usize;
    (lines.saturating_sub(1).max(4), chars.max(24))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextColors {
    Native,
    BlackAndWhite,
    Inverted,
    Paper,
}

impl TextColors {
    pub(crate) const CHOICES: [Self; 4] = [
        Self::Native,
        Self::BlackAndWhite,
        Self::Inverted,
        Self::Paper,
    ];

    pub(crate) fn parse(id: &str) -> Self {
        match id {
            "ink" => Self::BlackAndWhite,
            "inverted" => Self::Inverted,
            "paper" => Self::Paper,
            _ => Self::Native,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::BlackAndWhite => "ink",
            Self::Inverted => "inverted",
            Self::Paper => "paper",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Native => "Native",
            Self::BlackAndWhite => "Black and white",
            Self::Inverted => "Inverted",
            Self::Paper => "Paper",
        }
    }

    fn surface(self, theme_paper: Hsla, theme_ink: Hsla) -> (Hsla, Hsla) {
        match self {
            Self::Native => (theme_paper, theme_ink),
            Self::BlackAndWhite => (solid(0xffffff), solid(0x000000)),
            Self::Inverted => (solid(0x000000), solid(0xffffff)),
            Self::Paper => (solid(0xf4efe4), solid(0x1c1814)),
        }
    }
}

fn solid(hex: u32) -> Hsla {
    rgb(hex).into()
}

#[cfg(test)]
mod tests {
    use super::{page_capacity, TextColors};
    use gpui_kit::{px, rgb, size, Hsla};

    #[test]
    fn wide_window_fits_more_text() {
        let narrow = page_capacity(size(px(800.), px(700.)), 100);
        let wide = page_capacity(size(px(1600.), px(1100.)), 100);
        assert!(wide.0 > narrow.0);
        assert!(wide.1 > narrow.1);
    }

    #[test]
    fn larger_type_fits_less_on_a_page() {
        let size = size(px(1200.), px(900.));
        let normal = page_capacity(size, 100);
        let large = page_capacity(size, 200);
        assert!(large.0 < normal.0);
        assert!(large.1 < normal.1);
    }

    #[test]
    fn text_colors_cover_the_four_reading_surfaces() {
        let theme_paper: Hsla = rgb(0x112233).into();
        let theme_ink: Hsla = rgb(0xaabbcc).into();
        assert_eq!(TextColors::parse("native"), TextColors::Native);
        assert_eq!(TextColors::parse("ink"), TextColors::BlackAndWhite);
        assert_eq!(TextColors::parse("inverted"), TextColors::Inverted);
        assert_eq!(TextColors::parse("paper"), TextColors::Paper);
        assert_eq!(TextColors::parse("other"), TextColors::Native);

        let (paper, ink) = TextColors::Native.surface(theme_paper, theme_ink);
        assert_eq!(paper, theme_paper);
        assert_eq!(ink, theme_ink);

        let (paper, ink) = TextColors::BlackAndWhite.surface(theme_paper, theme_ink);
        assert_eq!(paper, rgb(0xffffff).into());
        assert_eq!(ink, rgb(0x000000).into());

        let (paper, ink) = TextColors::Inverted.surface(theme_paper, theme_ink);
        assert_eq!(paper, rgb(0x000000).into());
        assert_eq!(ink, rgb(0xffffff).into());

        let (paper, ink) = TextColors::Paper.surface(theme_paper, theme_ink);
        assert_eq!(paper, rgb(0xf4efe4).into());
        assert_eq!(ink, rgb(0x1c1814).into());
        assert_ne!(paper, rgb(0xffffff).into());
    }

    #[test]
    fn metadata_cover_is_the_first_page() {
        assert_eq!(super::reading_len(10, true), 11);
        assert_eq!(super::reading_len(10, false), 10);
        assert_eq!(super::reading_slot(true, 4, true), 0);
        assert_eq!(super::reading_slot(false, 0, true), 1);
        assert_eq!(super::reading_slot(false, 4, true), 5);
        assert_eq!(super::reading_slot(false, 0, false), 0);

        assert_eq!(super::step_place(true, 0, true, 10, false), None);
        assert_eq!(super::step_place(true, 0, true, 10, true), Some((false, 0)));
        assert_eq!(
            super::step_place(false, 0, true, 10, false),
            Some((true, 0))
        );
        assert_eq!(super::step_place(false, 0, false, 10, false), None);
        assert_eq!(super::step_place(false, 9, true, 10, true), None);
        assert!((super::slot_percent(true, 0, true, 9) - 10.0).abs() < 0.01);
    }

    #[test]
    fn comic_spread_turns_two_pages_and_keeps_the_cover_single() {
        assert_eq!(
            super::step_comic(true, 0, true, 5, true, true),
            Some((false, 0))
        );
        assert_eq!(
            super::step_comic(false, 0, true, 5, true, true),
            Some((false, 2))
        );
        assert_eq!(
            super::step_comic(false, 2, true, 5, true, true),
            Some((false, 4))
        );
        assert_eq!(super::step_comic(false, 4, true, 5, true, true), None);
        assert_eq!(
            super::step_comic(false, 4, true, 5, true, false),
            Some((false, 2))
        );
        assert_eq!(
            super::step_comic(false, 0, true, 5, true, false),
            Some((true, 0))
        );
        assert_eq!(super::step_comic(true, 0, true, 5, true, false), None);
        assert_eq!(super::step_comic(false, 0, false, 5, false, false), None);
        assert_eq!(
            super::step_comic(false, 1, false, 5, false, true),
            Some((false, 2))
        );
        assert_eq!(super::align_comic_spread(3), 2);
        assert_eq!(super::align_comic_spread(4), 4);
        assert_eq!(super::comic_place_label(false, 0, true, 5, true), "2-3 / 6");
        assert_eq!(super::comic_place_label(true, 0, true, 5, true), "1 / 6");
        assert_eq!(super::comic_place_label(false, 4, false, 5, true), "5 / 5");
        assert!(super::comic_layout_spread("spread"));
        assert!(!super::comic_layout_spread("single"));
        assert_eq!(super::comic_layout_id(true), "spread");
        assert_eq!(super::comic_layout_id(false), "single");
        assert_eq!(super::clamp_comic_zoom(10), super::COMIC_ZOOM_MIN);
        assert_eq!(super::clamp_comic_zoom(400), super::COMIC_ZOOM_MAX);
    }

    #[test]
    fn comic_page_uses_the_full_window_width() {
        let (width, height) = super::fitted_page_size(800.0, 1000.0, 1500.0, 100);
        assert!((width - 800.0).abs() < 0.1);
        assert!((height - 1200.0).abs() < 0.1);

        let (zoomed_w, zoomed_h) = super::fitted_page_size(800.0, 1000.0, 1500.0, 200);
        assert!((zoomed_w - 1600.0).abs() < 0.1);
        assert!((zoomed_h - 2400.0).abs() < 0.1);

        let maximized = size(px(1600.), px(1000.));
        let windowed = size(px(920.), px(760.));
        let max_w = super::comic_page_width(maximized, 1);
        let win_w = super::comic_page_width(windowed, 1);
        assert!((max_w - 1600.0).abs() < 0.1);
        assert!((win_w - 920.0).abs() < 0.1);
        let spread = super::comic_page_width(maximized, 2);
        assert!((spread * 2.0 + super::COMIC_SPREAD_GAP - 1600.0).abs() < 0.1);
    }

    #[test]
    fn page_edges_leave_a_middle_and_hide_at_the_ends() {
        let band = super::page_edge_width(920.0);
        assert!(band >= 128.0);
        assert!(band * 2.0 < 920.0);
        let wide = super::page_edge_width(1600.0);
        assert!((wide - 1600.0 * 0.28).abs() < 0.1);
        assert!(wide * 2.0 < 1600.0);

        assert_eq!(
            super::visible_page_edge(Some(true), false, false),
            Some(true)
        );
        assert_eq!(
            super::visible_page_edge(Some(false), false, false),
            Some(false)
        );
        assert_eq!(super::visible_page_edge(Some(false), true, false), None);
        assert_eq!(super::visible_page_edge(Some(true), false, true), None);
        assert_eq!(super::visible_page_edge(None, false, false), None);
    }
}

#[derive(Clone)]
struct ComicFrame {
    image: Arc<Image>,
    width: f32,
    height: f32,
}

fn image_from_data_url(data_url: &str) -> Option<ComicFrame> {
    let encoded = data_url.split_once("base64,")?.1;
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).ok()?;
    let format = image::guess_format(&bytes).ok()?;
    let gpui_format = match format {
        image::ImageFormat::Png => ImageFormat::Png,
        image::ImageFormat::Jpeg => ImageFormat::Jpeg,
        image::ImageFormat::WebP => ImageFormat::Webp,
        image::ImageFormat::Gif => ImageFormat::Gif,
        image::ImageFormat::Bmp => ImageFormat::Bmp,
        _ => return None,
    };
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .unwrap_or((2, 3));
    Some(ComicFrame {
        image: Arc::new(Image::from_bytes(gpui_format, bytes)),
        width: width as f32,
        height: height as f32,
    })
}

fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path.display().to_string()])
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
