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

#![allow(dead_code, clippy::module_name_repetitions)]

use crate::Result;
use breadx::auto::xproto::KeyButMask;
use serde::{
    de::{Deserializer, Visitor},
    Deserialize,
};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize)]
pub struct XcrabConfig {
    border_color: Option<u32>,
    focused_color: Option<u32>,
    border_size: Option<u16>,
    gap_size: Option<u16>,
    outer_gap_size: Option<u16>,
    pub msg: Option<XcrabMsgConfig>,
    #[serde(default)]
    pub binds: HashMap<Keybind, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct XcrabMsgConfig {
    pub socket_path: PathBuf,
}

const DEFAULT_BORDER_COLOR: u32 = 0xff_00_00; // red
const DEFAULT_FOCUSED_COLOR: u32 = 0x00_00_ff; // blue
const DEFAULT_BORDER_SIZE: u16 = 5;
const DEFAULT_GAP_SIZE: u16 = 20;

impl Default for XcrabConfig {
    fn default() -> Self {
        Self {
            border_color: Some(DEFAULT_BORDER_COLOR),
            focused_color: Some(DEFAULT_FOCUSED_COLOR),
            border_size: Some(DEFAULT_BORDER_SIZE),
            gap_size: Some(DEFAULT_GAP_SIZE),
            outer_gap_size: None,
            msg: Some(XcrabMsgConfig::default()),
            binds: HashMap::new(),
        }
    }
}

impl Default for XcrabMsgConfig {
    fn default() -> Self {
        let home_dir = get_home().expect("Error: $HOME variable not set");
        Self {
            socket_path: format!("{}/.config/xcrab/msg.sock", home_dir).into(),
        }
    }
}

impl XcrabConfig {
    pub fn border_color(&self) -> u32 {
        self.border_color.unwrap_or(DEFAULT_BORDER_COLOR)
    }

    pub fn focused_color(&self) -> u32 {
        self.focused_color.unwrap_or(DEFAULT_FOCUSED_COLOR)
    }

    pub fn border_size(&self) -> u16 {
        self.border_size.unwrap_or(DEFAULT_BORDER_SIZE)
    }

    pub fn gap_size(&self) -> u16 {
        self.gap_size.unwrap_or(DEFAULT_GAP_SIZE)
    }

    pub fn outer_gap_size(&self) -> u16 {
        self.outer_gap_size.unwrap_or_else(|| self.gap_size())
    }
}

pub fn load_file() -> Result<XcrabConfig> {
    let home_dir = get_home()?;

    let contents = std::fs::read_to_string(format!("{}/.config/xcrab/config.toml", home_dir))?;

    let config: XcrabConfig = toml::from_str(&contents)?;

    Ok(config)
}

fn get_home() -> Result<String> {
    Ok(std::env::var("HOME")?)
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Keybind {
    pub key: char,
    pub mods: KeyButMask,
}

struct KeybindVisitor;
impl<'de> Visitor<'de> for KeybindVisitor {
    type Value = Keybind;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a keybind in the form of 'M-x'")
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
        let mut mask = KeyButMask::default();

        for part in value.split('-') {
            if part.len() > 1 {
                return Err(E::custom("parts may only contain one character"));
            }

            let c = part
                .chars()
                .next()
                .ok_or_else(|| E::custom("parts must contain at least one character"))?;

            if c.is_ascii_uppercase() {
                // FIXME: add more as required
                match c {
                    'C' => mask.set_control(true),
                    'S' => mask.set_shift(true),
                    'A' => mask.set_mod1(true), // alt key
                    'W' => mask.set_mod4(true), // super key, 'w' for windows because S is taken
                    _ => return Err(E::custom(format!("no such modifier: {}", c))),
                };
            }
        }

        // ignores extraneous keys
        let c = value
            .split('-')
            .flat_map(str::chars)
            .find(char::is_ascii_lowercase)
            .ok_or_else(|| E::custom("must specify one normal key"))?
            .to_ascii_uppercase();

        Ok(Keybind { key: c, mods: mask })
    }
}

impl<'de> Deserialize<'de> for Keybind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_str(KeybindVisitor)
    }
}
