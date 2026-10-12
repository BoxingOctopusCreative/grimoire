use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _, Styled as _,
};
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement as _, Window, div, px};

use super::backend;
use super::shell::Shell;
use super::theme::DISPLAY;

const FONTS: &[(&str, &str, &str)] = &[
    (
        "publisher",
        "Publisher default",
        "Keep the ebook's own fonts when the file supplies them.",
    ),
    (
        "serif",
        "Serif",
        "A reading serif, Iowan Old Style or Georgia.",
    ),
    (
        "sans",
        "Sans",
        "A clean sans for long stretches of text.",
    ),
    (
        "opendyslexic",
        "OpenDyslexic",
        "Used when OpenDyslexic is installed on this computer.",
    ),
];

impl Shell {
    pub fn settings_page(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self
            .library_path
            .clone()
            .unwrap_or_else(|| "No library path saved yet.".into());
        let open = if self.library_open {
            "Open and ready"
        } else if self.library_path.is_some() {
            "Saved, but not open right now"
        } else {
            ""
        };
        let font = self.reader_font.clone();

        div()
            .id("settings-page")
            .size_full()
            .overflow_y_scroll()
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(div().font_family(DISPLAY).text_size(px(28.)).child("Settings"))
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("Choose a library folder to open automatically when Grimoire starts."),
            )
            .when(!self.message.is_empty(), |this| {
                this.child(div().text_color(cx.theme().primary).child(self.message.clone()))
            })
            .when(!self.error.is_empty(), |this| {
                this.child(div().text_color(cx.theme().danger).child(self.error.clone()))
            })
            .child(settings_block(
                "Library location",
                "Grimoire stores metadata.db and book files in this folder. The path is saved in your OS config directory.",
                cx,
            ).child(div().mt(px(8.)).child(path)).when(!open.is_empty(), |this| {
                this.child(div().text_color(cx.theme().muted_foreground).child(open))
            }).child(
                div().mt(px(10.)).flex().gap(px(8.)).child(
                    Button::new("settings-choose")
                        .primary()
                        .label(if self.library_path.is_some() {
                            "Change library folder"
                        } else {
                            "Choose library folder"
                        })
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| this.pick_library(window, cx))),
                ).when(self.library_path.is_some(), |this| {
                    this.child(
                        Button::new("settings-clear")
                            .outline()
                            .label("Clear saved path")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| this.confirm_clear(window, cx))),
                    )
                }),
            ))
            .child({
                let mut block = settings_block(
                    "Reader font",
                    "Applies to EPUB and TXT reading.",
                    cx,
                );
                for (id, label, description) in FONTS {
                    let selected = font == *id;
                    let font_id = (*id).to_string();
                    block = block.child(
                        div()
                            .mt(px(8.))
                            .child(
                                Button::new(format!("font-{id}"))
                                    .when(selected, |button| button.primary())
                                    .when(!selected, |button| button.outline())
                                    .label(*label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.reader_font = font_id.clone();
                                        let _ = backend::save_reader_font(&this.reader_font);
                                        this.message = format!("Reader font set to {label}.");
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(*description),
                            ),
                    );
                }
                block.child(
                    div()
                        .mt(px(10.))
                        .font_family(reader_family(&font))
                        .child("The quick brown fox jumps over the lazy dog."),
                )
            })
    }

    fn confirm_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .confirm()
                .title("Clear library path")
                .description("Grimoire will not open a library until you choose one again.")
                .on_ok({
                    let shell = shell.clone();
                    move |_, _, cx| {
                    shell
                        .downgrade().update_in(cx, |this, _, cx| {
                            match backend::clear_library(&this.state) {
                                Ok(status) => {
                                    this.apply_status(status);
                                    this.books.clear();
                                    this.message = "Saved library path cleared.".into();
                                }
                                Err(error) => this.error = error.to_string(),
                            }
                            cx.notify();
                        })
                        .ok();
                    true
                    }
                })
        });
    }
}

fn settings_block(title: &str, body: &str, cx: &gpui_kit::App) -> gpui_kit::Div {
    div()
        .max_w(px(640.))
        .p(px(16.))
        .rounded(px(10.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .font_family(DISPLAY)
                .text_size(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .mt(px(6.))
                .text_color(cx.theme().muted_foreground)
                .child(body.to_string()),
        )
}

pub fn reader_family(id: &str) -> &'static str {
    match id {
        "sans" => ".SystemUIFont",
        "opendyslexic" => "OpenDyslexic",
        _ => DISPLAY,
    }
}

#[cfg(test)]
mod tests {
    use super::{reader_family, DISPLAY, FONTS};

    #[test]
    fn settings_offers_the_reader_font_choices() {
        let ids: Vec<_> = FONTS.iter().map(|font| font.0).collect();
        assert_eq!(ids, ["publisher", "serif", "sans", "opendyslexic"]);
    }

    #[test]
    fn reader_family_maps_each_choice() {
        assert_eq!(reader_family("publisher"), DISPLAY);
        assert_eq!(reader_family("serif"), DISPLAY);
        assert_eq!(reader_family("sans"), ".SystemUIFont");
        assert_eq!(reader_family("opendyslexic"), "OpenDyslexic");
    }

    #[test]
    fn unknown_reader_font_uses_the_reading_serif() {
        assert_eq!(reader_family("comic-sans"), DISPLAY);
    }
}
