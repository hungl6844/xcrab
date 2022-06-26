use serde::{Deserialize,Serialize};
use std::fs;
use std::path::Path;
use toml::de::from_slice;


#[derive(Serialize, Deserialize, Debug)]
pub struct XcrabConfig {
    border_color: String,
    border_size: i32,
    gap_width: i32,
}


impl XcrabConfig{
    pub fn load_config() -> XcrabConfig{
        if Path::new("~/.config/xcrab").is_dir(){
            if Path::new("~/.config/xcrab/config.toml").is_file(){
                let mut config_slice = fs::read("~/.config/xcrab/config.toml");
                let de_config:XcrabConfig = from_slice(&config_slice.unwrap()).unwrap(); //got that shiz
                return de_config
            }
            else{
                fs::write("~/.config/xcrab/config.toml", "border_color = 'ffffff'\nborder_size = 1\ngap_width = 1");
                //add stuff
                let mut config_slice = fs::read("~/.config/xcrab/config.toml");
                let de_config:XcrabConfig = from_slice(&config_slice.unwrap()).unwrap(); //got that shiz
                return de_config
            }
        }else{
            fs::create_dir_all("~/.config/xcrab");
            fs::write("~/.config/xcrab/config.toml", "border_color = 'ffffff'\nborder_size = 1\ngap_width = 1");
            //add stuff
            let mut config_slice = fs::read("~/.config/xcrab/config.toml");
            let de_config:XcrabConfig = from_slice(&config_slice.unwrap()).unwrap(); //got that shiz
            return de_config
        }
    }
}
