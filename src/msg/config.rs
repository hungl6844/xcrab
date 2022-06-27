use crate::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize)]
// Dummy struct for deserializing the message config - we're using the same file for both binaries
pub struct XcrabConfig {
    pub msg: Option<XcrabMsgConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct XcrabMsgConfig {
    pub socket_path: PathBuf,
}

pub fn load_file() -> Result<XcrabConfig> {
    let home_dir = std::env::var("HOME")?;

    let contents = std::fs::read_to_string(format!("{}/.config/xcrab/config.toml", home_dir))?;

    let config: XcrabConfig = toml::from_str(&contents)?;

    Ok(config)
}
