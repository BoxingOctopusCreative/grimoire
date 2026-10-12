use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::{ActiveTheme as _, Disableable as _};
use gpui_kit::prelude::{
    InteractiveElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
};
use gpui_kit::{
    Context, Image, ImageFormat, IntoElement, MouseButton, ObjectFit, ParentElement as _, Render,
    ScrollHandle, Window, div, img, px,
};

use super::backend;
use super::document::{self, Chapter};
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
    error: String,
    loading: bool,
    chapters: Vec<Chapter>,
    chapter: usize,
    comic_count: usize,
    comic_index: usize,
    comic_image: Option<Arc<Image>>,
    scroll: ScrollHandle,
    percent: f64,
    titlebar_drag: bool,
}

impl ReaderWindow {
    pub fn new(state: AppState, book_id: i64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let reader = Self {
            state,
            book_id,
            title: "Reading".into(),
            authors: String::new(),
            format: String::new(),
            path: PathBuf::new(),
            error: String::new(),
            loading: true,
            chapters: Vec::new(),
            chapter: 0,
            comic_count: 0,
            comic_index: 0,
            comic_image: None,
            scroll: ScrollHandle::new(),
            percent: 0.0,
            titlebar_drag: false,
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
                    let source = backend::reader_source(&state, book_id).map_err(|e| e.to_string())?;
                    let saved = backend::progress(&state, book_id).ok().flatten();
                    Ok::<_, String>((source, saved))
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match loaded {
                    Ok((source, saved)) => {
                        this.title = source.title.clone();
                        this.authors = source.authors.join(", ");
                        this.format = source.format.clone();
                        this.path = PathBuf::from(&source.path);
                        if let Some(saved) = saved {
                            this.percent = saved.percent;
                            if let Some(index) = saved.location.strip_prefix("chapter:") {
                                this.chapter = index.parse().unwrap_or(0);
                            }
                            if let Some(index) = saved.location.strip_prefix("page:") {
                                this.comic_index = index.parse().unwrap_or(0);
                            }
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
                        if this.chapter >= chapters.len() {
                            this.chapter = 0;
                        }
                        this.chapters = chapters;
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
        let runtime = self.runtime(cx);
        cx.spawn_in(window, async move |this, cx| {
            let page = runtime
                .spawn_blocking(move || backend::comic_page(&path, index).map_err(|e| e.to_string()))
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            this.update(cx, |this, cx| {
                match page {
                    Ok((name, data_url, count)) => {
                        this.comic_count = count;
                        this.comic_image = image_from_data_url(&data_url);
                        let _ = name;
                        this.remember("page", this.comic_index, cx);
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
        let total = if kind == "page" {
            self.comic_count.max(1)
        } else {
            self.chapters.len().max(1)
        };
        let percent = ((index as f64 + 1.0) / total as f64) * 100.0;
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

    fn note_scroll(&mut self, cx: &mut Context<Self>) {
        let max = self.scroll.max_offset().y.as_f32();
        if max <= 1.0 || self.chapters.is_empty() {
            return;
        }
        let offset = self.scroll.offset().y.as_f32().max(0.0);
        let within = (offset / max).clamp(0.0, 1.0) as f64;
        let total = self.chapters.len() as f64;
        let percent = ((self.chapter as f64 + within) / total) * 100.0;
        if (percent - self.percent).abs() < 1.0 {
            return;
        }
        self.percent = percent;
        let state = self.state.clone();
        let book_id = self.book_id;
        let format = self.format.clone();
        let location = format!("chapter:{}", self.chapter);
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
}

impl Render for ReaderWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = backend::config()
            .map(|config| config.reader_font)
            .unwrap_or_else(|_| "serif".into());
        let family = reader_family(&font);
        let paper = cx.theme().popover;
        let ink = cx.theme().foreground;

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(ink)
            .child(self.chrome(window, cx))
            .child(self.content(family, paper, window, cx))
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
        paper: gpui_kit::Hsla,
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
                .child(div().font_family(DISPLAY).text_size(px(24.)).child(self.title.clone()))
                .child("PDF opens in your system viewer. EPUB, TXT, and comics stay inside Grimoire.")
                .child(
                    Button::new("open-pdf")
                        .primary()
                        .label("Open PDF")
                        .on_click(move |_, _, _| open_path(&path)),
                )
                .into_any_element();
        }
        if format == "CBZ" || format == "CBR" {
            return self.comic_view(cx).into_any_element();
        }
        self.text_view(family, paper, window, cx).into_any_element()
    }

    fn text_view(
        &mut self,
        family: &str,
        paper: gpui_kit::Hsla,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let chapter = self.chapters.get(self.chapter).cloned();
        let count = self.chapters.len();
        let index = self.chapter;
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
                        Button::new("prev-chapter")
                            .ghost()
                            .label("Previous")
                            .disabled(index == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.chapter > 0 {
                                    this.chapter -= 1;
                                    this.scroll.set_offset(gpui_kit::point(px(0.), px(0.)));
                                    this.remember("chapter", this.chapter, cx);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div().text_color(cx.theme().muted_foreground).child(format!(
                            "{} / {}",
                            index + 1,
                            count.max(1)
                        )),
                    )
                    .child(
                        Button::new("next-chapter")
                            .ghost()
                            .label("Next")
                            .disabled(count == 0 || index + 1 >= count)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.chapter + 1 < this.chapters.len() {
                                    this.chapter += 1;
                                    this.scroll.set_offset(gpui_kit::point(px(0.), px(0.)));
                                    this.remember("chapter", this.chapter, cx);
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .id("reader-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .on_scroll_wheel(cx.listener(|this, _, _, cx| this.note_scroll(cx)))
                    .px(px(28.))
                    .py(px(18.))
                    .child(
                        div()
                            .max_w(px(680.))
                            .mx_auto()
                            .p(px(22.))
                            .rounded(px(10.))
                            .bg(paper)
                            .font_family(family)
                            .text_size(px(18.))
                            .line_height(px(30.))
                            .child(match chapter {
                                Some(Chapter { title, body }) => div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(14.))
                                    .child(
                                        div()
                                            .font_family(DISPLAY)
                                            .text_size(px(26.))
                                            .child(title),
                                    )
                                    .child(body)
                                    .into_any_element(),
                                None => div().child("No chapter text.").into_any_element(),
                            }),
                    ),
            )
    }

    fn comic_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.comic_image.clone();
        let index = self.comic_index;
        let count = self.comic_count;
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
                            .disabled(index == 0)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.comic_index > 0 {
                                    this.comic_index -= 1;
                                    this.load_comic_page(window, cx);
                                }
                            })),
                    )
                    .child(format!("{} / {}", index + 1, count.max(1)))
                    .child(
                        Button::new("next-page")
                            .ghost()
                            .label("Next")
                            .disabled(count == 0 || index + 1 >= count)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.comic_index + 1 < this.comic_count {
                                    this.comic_index += 1;
                                    this.load_comic_page(window, cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .id("comic-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    .p(px(12.))
                    .child(match image {
                        Some(image) => img(image).max_w(px(900.)).object_fit(ObjectFit::Contain).into_any_element(),
                        None => div().child("Loading page…").into_any_element(),
                    }),
            )
    }
}

fn image_from_data_url(data_url: &str) -> Option<Arc<Image>> {
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
    Some(Arc::new(Image::from_bytes(gpui_format, bytes)))
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
