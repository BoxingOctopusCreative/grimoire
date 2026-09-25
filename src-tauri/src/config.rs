use crate::error::{err, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub library_path: Option<String>,
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
        let config = parse_config("\u{feff}").expect("BOM-only config should default");
        assert!(config.library_path.is_none());
    }
}
