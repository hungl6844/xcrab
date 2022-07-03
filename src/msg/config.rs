// Copyright (C) 2022 Infoshock Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use crate::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Deserialize)]
// Dummy struct for deserializing the message config - we're using the same file for both binaries
pub struct XcrabConfig {
    pub msg: XcrabMsgConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct XcrabMsgConfig {
    pub socket_path: PathBuf,
}

impl Default for XcrabMsgConfig {
    fn default() -> Self {
        Self {
            socket_path: format!("{}/.config/xcrab/msg.sock", get_home()).into(),
        }
    }
}

fn load_file_inner() -> Result<XcrabConfig> {
    let home_dir = get_home();

    let contents = std::fs::read_to_string(format!("{}/.config/xcrab/config.toml", home_dir))?;

    let config: XcrabConfig = toml::from_str(&contents)?;

    Ok(config)
}

pub fn load_file() -> XcrabConfig {
    load_file_inner().unwrap_or_default()
}

fn get_home() -> String {
    std::env::var("HOME").expect("Error: $HOME variable was not set")
}
