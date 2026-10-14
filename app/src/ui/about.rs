use std::sync::Arc;

use gpui_kit::component::link::Link;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::{
    InteractiveElement as _, ParentElement as _, Styled as _, StyledImage as _,
};
use gpui_kit::AppContext as _;
use gpui_kit::{
    div, img, point, px, size, AnyWindowHandle, App, Context, Div, FontWeight, Global, Hsla, Image,
    ImageFormat, IntoElement, MouseButton, ObjectFit, Render, Stateful, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};

use super::icon;
use super::theme::{DISPLAY, UI_FONT};

const PUBLISHER: &str = "Boxing Octopus Creative Engineering";
const PUBLISHER_URL: &str = "https://boxingoctop.us";
const LICENSE: &str = "MPL-2.0";
const LICENSE_URL: &str = "https://mozilla.org/MPL/2.0/";
const WEBSITE: &str = "grimoirebooks.app";
const WEBSITE_URL: &str = "https://grimoirebooks.app";
const VERSION_LABEL: &str = concat!("Version ", env!("CARGO_PKG_VERSION"));

struct OpenAbout(AnyWindowHandle);

impl Global for OpenAbout {}

/// Opens the About window, or brings the existing one forward.
pub(crate) fn show(cx: &mut App) {
    if let Some(existing) = cx.try_global::<OpenAbout>().map(|open| open.0) {
        if existing
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
    }

    let Ok((window, _)) =
        gpui_kit::open_window(window_options(cx), cx, |_, cx| cx.new(|_| About::new()))
    else {
        return;
    };
    cx.set_global(OpenAbout(window));
}

fn window_options(cx: &App) -> WindowOptions {
    let mut options = WindowOptions::default();
    let bounds = size(px(440.), px(520.));
    options.window_bounds = Some(WindowBounds::centered(bounds, cx));
    options.window_min_size = Some(bounds);
    options.is_resizable = false;
    options.titlebar = Some(TitlebarOptions {
        title: Some("About Grimoire".into()),
        appears_transparent: true,
        traffic_light_position: Some(point(px(14.), px(18.))),
    });
    options.app_owns_titlebar_drag = true;
    icon::apply(&mut options);
    options
}

struct About {
    dragging: bool,
    logo: Arc<Image>,
}

impl About {
    fn new() -> Self {
        Self {
            dragging: false,
            logo: Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../../../static/grimoire_icon.svg").to_vec(),
            )),
        }
    }
}

impl Render for About {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ink = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().primary;

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(cx.theme().background)
            .text_color(ink)
            .font_family(UI_FONT)
            .text_size(px(15.))
            .px(px(28.))
            .child(
                draggable("about-drag", cx)
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(40.))
                    .child(
                        img(self.logo.clone())
                            .w(px(126.))
                            .h(px(148.))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .mt(px(14.))
                            .flex()
                            .items_end()
                            .font_family(DISPLAY)
                            .text_size(px(32.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Grimoire")
                            .child(div().text_color(accent).child(".")),
                    )
                    .child(
                        div()
                            .mt(px(4.))
                            .text_size(px(13.))
                            .text_color(muted)
                            .child(VERSION_LABEL),
                    ),
            )
            .child(
                div()
                    .mt(px(28.))
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(16.))
                    .child(fact(
                        "Publisher",
                        "about-publisher",
                        PUBLISHER,
                        PUBLISHER_URL,
                        muted,
                    ))
                    .child(fact(
                        "License",
                        "about-license",
                        LICENSE,
                        LICENSE_URL,
                        muted,
                    ))
                    .child(fact(
                        "Website",
                        "about-website",
                        WEBSITE,
                        WEBSITE_URL,
                        muted,
                    )),
            )
            .child(
                draggable("about-drag-rest", cx)
                    .w_full()
                    .flex_1()
                    .pb(px(32.)),
            )
    }
}

fn draggable(id: &'static str, cx: &mut Context<About>) -> Stateful<Div> {
    div()
        .id(id)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, _| this.dragging = true),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, _| this.dragging = false),
        )
        .on_mouse_down_out(cx.listener(|this, _, _, _| this.dragging = false))
        .on_mouse_move(cx.listener(|this, _, window, _| {
            if this.dragging {
                this.dragging = false;
                window.start_window_move();
            }
        }))
}

fn fact(
    caption: &'static str,
    id: &'static str,
    label: &'static str,
    href: &'static str,
    muted: Hsla,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(2.))
        .child(div().text_size(px(12.)).text_color(muted).child(caption))
        .child(Link::new(id).href(href).text_center().child(label))
}

#[cfg(test)]
mod tests {
    use super::{LICENSE, LICENSE_URL, PUBLISHER, PUBLISHER_URL, VERSION_LABEL, WEBSITE_URL};

    #[test]
    fn about_states_publisher_license_and_site() {
        assert_eq!(PUBLISHER, "Boxing Octopus Creative Engineering");
        assert_eq!(PUBLISHER_URL, "https://boxingoctop.us");
        assert_eq!(LICENSE, "MPL-2.0");
        assert_eq!(LICENSE, env!("CARGO_PKG_LICENSE"));
        assert!(VERSION_LABEL.contains(env!("CARGO_PKG_VERSION")));
        assert_eq!(LICENSE_URL, "https://mozilla.org/MPL/2.0/");
        assert_eq!(WEBSITE_URL, "https://grimoirebooks.app");
    }
}
