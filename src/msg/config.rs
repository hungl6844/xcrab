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

pub fn load_file() -> XcrabConfig {
    let home_dir = std::env::var("HOME").expect("Error: $HOME variable was not set");

    let contents = std::fs::read_to_string(format!("{}/.config/xcrab/config.toml", home_dir))
        .unwrap_or_else(|_| {
            panic!(
                "Error: file {}/.config/xcrab/config.toml was not found",
                home_dir
            )
        });

    let config: XcrabConfig = toml::from_str(&contents)
        .expect("Error: config file was not parseable. Is it properly formatted?");

    config
}
