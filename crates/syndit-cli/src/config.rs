use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserConfig {
    pub user_id: String,
    pub key_path: String,
}

pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not determine config directory")?;
    Ok(base.join("syndit"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("user.json"))
}

pub fn save(config: &UserConfig) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn load() -> Result<UserConfig> {
    let path = config_path()?;
    if !path.exists() {
        bail!(
            "No user registered. Run `syndit register` first.\n  Expected config at: {}",
            path.display()
        );
    }
    let data = std::fs::read_to_string(&path)?;
    let config: UserConfig = serde_json::from_str(&data)?;
    Ok(config)
}
