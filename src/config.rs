use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct XcrabConfig {
    pub border_color: Option<u32>,
    pub border_size: Option<u16>,
    pub gap_width: Option<u16>,
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

pub fn load_file() -> Arc<XcrabConfig> {
    Arc::new(load_file_inner().unwrap_or_default())
}

fn load_file_inner() -> Result<XcrabConfig, crate::XcrabError> {
    let contents = std::fs::read_to_string("~/.config/xcrab/config.toml")?;

    let config: XcrabConfig = toml::from_str(&contents)?;

    Ok(config)
}
