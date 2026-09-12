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
    Ok(serde_json::from_str(&raw)?)
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
