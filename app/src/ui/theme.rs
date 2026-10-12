use gpui_kit::component::theme::{Theme, ThemeColor, ThemeMode};
use gpui_kit::{App, Hsla, WindowAppearance, px, rgb};

use crate::config::{load_config, set_theme_preference};

pub const DISPLAY: &str = "Iowan Old Style";
pub const UI_FONT: &str = ".SystemUIFont";

pub fn apply_saved(cx: &mut App) {
    let pref = load_config()
        .map(|config| config.theme)
        .unwrap_or_else(|_| "dark".into());
    apply(&pref, cx);
}

pub fn label(pref: &str) -> &'static str {
    match pref {
        "light" => "Light",
        "system" => "Follow System",
        _ => "Dark",
    }
}

pub fn persist_and_apply(pref: &str, cx: &mut App) {
    let _ = set_theme_preference(pref);
    apply(pref, cx);
}

pub fn apply(pref: &str, cx: &mut App) {
    let dark = match pref {
        "light" => false,
        "system" => matches!(
            cx.window_appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
        _ => true,
    };
    if pref == "system" {
        cx.set_window_appearance(None);
    } else if dark {
        cx.set_window_appearance(Some(WindowAppearance::Dark));
    } else {
        cx.set_window_appearance(Some(WindowAppearance::Light));
    }
    Theme::change(
        if dark { ThemeMode::Dark } else { ThemeMode::Light },
        None,
        cx,
    );
    Theme::update(cx, |theme| {
        theme.colors = if dark { dark_palette() } else { light_palette() };
        theme.font_family = UI_FONT.into();
        theme.font_size = px(15.);
        theme.radius = px(10.);
        theme.radius_lg = px(14.);
    });
}

fn paint(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn paint_a(hex: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(hex).into();
    color.a = alpha;
    color
}

fn light_palette() -> ThemeColor {
    let mut color = *ThemeColor::light();
    let ink = paint(0x1c1814);
    let muted = paint(0x5c534a);
    let paper = paint(0xf3ece0);
    let paper_deep = paint(0xe6dccb);
    let panel = paint(0xfaf6ee);
    let accent = paint(0x2f5d50);
    let accent_hover = paint(0x244a40);
    let danger = paint(0x8b3a2f);
    let line = paint_a(0x1c1814, 0.12);
    let control = paint(0xfffdf8);
    let btn = paint(0xf8f5ef);
    fill(
        &mut color,
        ink,
        muted,
        paper,
        paper_deep,
        panel,
        accent,
        accent_hover,
        danger,
        line,
        control,
        btn,
        paint(0xe4efe9),
    );
    color
}

fn dark_palette() -> ThemeColor {
    let mut color = *ThemeColor::dark();
    let ink = paint(0xefe6d8);
    let muted = paint(0xb0a493);
    let paper = paint(0x1a1714);
    let paper_deep = paint(0x12100e);
    let panel = paint(0x24201c);
    let accent = paint(0x7fb5a4);
    let accent_hover = paint(0x9ac9ba);
    let danger = paint(0xd88476);
    let line = paint_a(0xefe6d8, 0.12);
    let control = paint(0x2c2823);
    let btn = paint(0x1a1714);
    fill(
        &mut color,
        ink,
        muted,
        paper,
        paper_deep,
        panel,
        accent,
        accent_hover,
        danger,
        line,
        control,
        btn,
        paint(0x2c3834),
    );
    color
}

fn fill(
    color: &mut ThemeColor,
    ink: Hsla,
    muted: Hsla,
    paper: Hsla,
    paper_deep: Hsla,
    panel: Hsla,
    accent: Hsla,
    accent_hover: Hsla,
    danger: Hsla,
    line: Hsla,
    control: Hsla,
    btn: Hsla,
    accent_wash: Hsla,
) {
    color.background = paper;
    color.foreground = ink;
    color.muted = paper_deep;
    color.muted_foreground = muted;
    color.border = line;
    color.popover = panel;
    color.popover_foreground = ink;
    color.group_box = panel;
    color.group_box_foreground = ink;
    color.primary = accent;
    color.primary_hover = accent_hover;
    color.primary_active = accent_hover;
    color.primary_foreground = btn;
    color.accent = accent;
    color.accent_foreground = btn;
    color.ring = accent;
    color.danger = danger;
    color.danger_hover = danger;
    color.danger_active = danger;
    color.danger_foreground = paint(0xfff8f5);
    color.input = control;
    color.caret = accent;
    color.title_bar = panel;
    color.title_bar_border = line;
    color.sidebar = paper_deep;
    color.sidebar_foreground = ink;
    color.sidebar_border = line;
    color.sidebar_accent = accent_wash;
    color.sidebar_accent_foreground = accent;
    color.sidebar_primary = accent;
    color.sidebar_primary_foreground = btn;
    color.list = panel;
    color.list_hover = accent_wash;
    color.list_active = accent_wash;
    color.list_active_border = accent;
    color.selection = accent_wash;
    color.link = accent;
    color.link_hover = accent_hover;
    color.link_active = accent_hover;
    color.button_primary = accent;
    color.button_primary_hover = accent_hover;
    color.button_primary_active = accent_hover;
    color.button_primary_foreground = btn;
    color.button_danger = danger;
    color.button_danger_hover = danger;
    color.button_danger_active = danger;
    color.button_danger_foreground = paint(0xfff8f5);
    color.tab_active = accent;
    color.tab_active_foreground = btn;
    color.scrollbar_thumb = line;
    color.window_border = line;
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn labels_known_preferences() {
        assert_eq!(label("light"), "Light");
        assert_eq!(label("dark"), "Dark");
        assert_eq!(label("system"), "Follow System");
    }

    #[test]
    fn unknown_preference_labels_as_dark() {
        assert_eq!(label("neon"), "Dark");
    }
}
