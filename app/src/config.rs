use crate::error::{err, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub library_path: Option<String>,
    /// `light`, `dark`, or `system`.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// `publisher`, `serif`, `sans`, or `opendyslexic`.
    #[serde(default = "default_reader_font")]
    pub reader_font: String,
    /// Text size as a percent of the default. Clamped when applied.
    #[serde(default = "default_reader_zoom")]
    pub reader_zoom: u32,
    /// `native`, `ink`, `inverted`, or `paper`.
    #[serde(default = "default_reader_colors")]
    pub reader_colors: String,
    /// Comic page size as a percent of the default. Clamped when applied.
    #[serde(default = "default_comic_zoom")]
    pub comic_zoom: u32,
    /// `single` or `spread`.
    #[serde(default = "default_comic_layout")]
    pub comic_layout: String,
}

fn default_theme() -> String {
    "dark".into()
}

fn default_reader_font() -> String {
    "serif".into()
}

fn default_reader_zoom() -> u32 {
    100
}

fn default_reader_colors() -> String {
    "native".into()
}

fn default_comic_zoom() -> u32 {
    100
}

fn default_comic_layout() -> String {
    "single".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            library_path: None,
            theme: default_theme(),
            reader_font: default_reader_font(),
            reader_zoom: default_reader_zoom(),
            reader_colors: default_reader_colors(),
            comic_zoom: default_comic_zoom(),
            comic_layout: default_comic_layout(),
        }
    }
}

fn config_path() -> AppResult<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| err("Could not resolve config directory"))?
        .join("grimoire");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

pub fn load_config() -> AppResult<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = fs::read_to_string(path)?;
    parse_config(&raw)
}

/// Parse config JSON, tolerating a leading UTF-8 BOM and empty files.
///
/// Windows PowerShell 5.1 `Set-Content -Encoding utf8` writes a BOM. serde_json
/// rejects that with "expected value at line 1 column 1".
fn parse_config(raw: &str) -> AppResult<AppConfig> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    if raw.trim().is_empty() {
        return Ok(AppConfig::default());
    }
    Ok(serde_json::from_str(raw)?)
}

pub fn save_config(config: &AppConfig) -> AppResult<()> {
    let path = config_path()?;
    let raw = serde_json::to_string_pretty(config)?;
    fs::write(path, raw)?;
    Ok(())
}

pub fn set_library_path(path: &Path) -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.library_path = Some(path.to_string_lossy().to_string());
    save_config(&config)?;
    Ok(config)
}

pub fn clear_library_path() -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.library_path = None;
    save_config(&config)?;
    Ok(config)
}

pub fn set_theme_preference(theme: &str) -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.theme = theme.to_string();
    save_config(&config)?;
    Ok(config)
}

pub fn set_reader_font(font: &str) -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.reader_font = font.to_string();
    save_config(&config)?;
    Ok(config)
}

pub fn set_reader_zoom(zoom: u32) -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.reader_zoom = zoom;
    save_config(&config)?;
    Ok(config)
}

pub fn set_reader_colors(colors: &str) -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.reader_colors = colors.to_string();
    save_config(&config)?;
    Ok(config)
}

pub fn set_comic_zoom(zoom: u32) -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.comic_zoom = zoom;
    save_config(&config)?;
    Ok(config)
}

pub fn set_comic_layout(layout: &str) -> AppResult<AppConfig> {
    let mut config = load_config()?;
    config.comic_layout = layout.to_string();
    save_config(&config)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_accepts_utf8_bom() {
        let raw = "\u{feff}{\"library_path\":\"C:\\\\Books\"}";
        let config = parse_config(raw).expect("BOM-prefixed JSON should parse");
        assert_eq!(config.library_path.as_deref(), Some("C:\\Books"));
    }

    #[test]
    fn parse_config_treats_empty_as_default() {
        let config = parse_config("").expect("empty config should default");
        assert!(config.library_path.is_none());
        assert_eq!(config.theme, "dark");
        assert_eq!(config.reader_font, "serif");
        assert_eq!(config.reader_zoom, 100);
        assert_eq!(config.reader_colors, "native");
        assert_eq!(config.comic_zoom, 100);
        assert_eq!(config.comic_layout, "single");
        let config = parse_config("\u{feff}").expect("BOM-only config should default");
        assert!(config.library_path.is_none());
    }

    #[test]
    fn missing_theme_and_reader_font_use_defaults() {
        let config = parse_config(r#"{"library_path":"C:\\Books"}"#).expect("partial config");
        assert_eq!(config.library_path.as_deref(), Some("C:\\Books"));
        assert_eq!(config.theme, "dark");
        assert_eq!(config.reader_font, "serif");
        assert_eq!(config.reader_zoom, 100);
        assert_eq!(config.reader_colors, "native");
        assert_eq!(config.comic_zoom, 100);
        assert_eq!(config.comic_layout, "single");
    }
}
