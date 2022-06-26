#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct XcrabConfig {
    border_color: Option<u32>,
    border_size: Option<u16>,
    gap_width: Option<u16>,
}

impl Default for XcrabConfig {
    fn default() -> Self {
        Self {
            border_color: Some(0xff_00_00), // red
            border_size: Some(5),
            gap_width: Some(10),
        }
    }
}

impl XcrabConfig {
    pub fn border_color(&self) -> u32 {
        self.border_color.unwrap_or(0xff_00_00)
    }

    pub fn border_size(&self) -> u16 {
        self.border_size.unwrap_or(5)
    }

    pub fn gap_width(&self) -> u16 {
        self.gap_width.unwrap_or(10)
    }
}

pub fn load_file() -> Result<XcrabConfig, crate::XcrabError> {
    let home_dir = std::env::var("HOME")?;

    let contents = std::fs::read_to_string(format!("{}/.config/xcrab/config.toml", home_dir))?;

    let config: XcrabConfig = toml::from_str(&contents)?;

    Ok(config)
}
