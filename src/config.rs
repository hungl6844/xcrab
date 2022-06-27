#![allow(dead_code, clippy::module_name_repetitions)]

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize)]
pub struct XcrabConfig {
    border_color: Option<u32>,
    border_size: Option<u16>,
    gap_size: Option<u16>,
    pub msg: Option<XcrabMsgConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct XcrabMsgConfig {
    socket_path: PathBuf,
}

const DEFAULT_BORDER_COLOR: u32 = 0xff_00_00; // red
const DEFAULT_BORDER_SIZE: u16 = 5;
const DEFAULT_GAP_SIZE: u16 = 10;

impl Default for XcrabConfig {
    fn default() -> Self {
        Self {
            border_color: Some(DEFAULT_BORDER_COLOR),
            border_size: Some(DEFAULT_BORDER_SIZE),
            gap_size: Some(DEFAULT_GAP_SIZE),
            msg: None, // TODO: use a default socket path
        }
    }
}

impl XcrabConfig {
    pub fn border_color(&self) -> u32 {
        self.border_color.unwrap_or(DEFAULT_BORDER_COLOR)
    }

    pub fn border_size(&self) -> u16 {
        self.border_size.unwrap_or(DEFAULT_BORDER_SIZE)
    }

    pub fn gap_size(&self) -> u16 {
        self.gap_size.unwrap_or(DEFAULT_GAP_SIZE)
    }
}

pub fn load_file() -> Result<XcrabConfig, crate::XcrabError> {
    let home_dir = std::env::var("HOME")?;

    let contents = std::fs::read_to_string(format!("{}/.config/xcrab/config.toml", home_dir))?;

    let config: XcrabConfig = toml::from_str(&contents)?;

    Ok(config)
}
