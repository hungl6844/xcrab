use serde::{Serialize, Deserialize};
use std::fs::{read,File};
use std::path::Path;
use toml::de::from_slice

#[derive(Serialize, Deserialize, Debug)]
pub struct XcrabConfig {
    border-color: String,
    border-size: i32,
    gap-width: i32,
}

impl XcrabConfig{
    pub fn load_config () ->  {
        if Path::new("~/.config/xcrab").is_dir(){
            if Path::new("~/.config/xcrab/config.toml").is_file(){
                let mut config_slice = read("~/.config/xcrab/config.toml");
                let de_config = from_slice(config_slice);
            }else{
                unimplemented!;
            }
        }else{
            fs::create_dir_all("~/.config/xcrab");
        }
    }
}
