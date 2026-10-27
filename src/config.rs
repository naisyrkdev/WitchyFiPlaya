use crate::prompts::Prompts;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => "Repeat off",
            Repeat::All => "Repeat all",
            Repeat::One => "Repeat one",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub music_folder: Option<PathBuf>,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub prompts: Prompts,
    pub button_images: bool,
    pub background: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Config { music_folder: None, volume: 0.7, shuffle: false, repeat: Repeat::All, prompts: Prompts::Auto, button_images: true, background: None }
    }
}

impl Config {
    fn dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("YASAFF_APP_DATA") {
            return PathBuf::from(dir);
        }
        directories::ProjectDirs::from("", "", "witchy-fi-playa")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::dir().join("config.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = Self::dir();
        let result = fs::create_dir_all(&dir)
            .and_then(|()| fs::write(dir.join("config.json"), serde_json::to_string_pretty(self).unwrap_or_default()));
        if let Err(e) = result {
            eprintln!("could not save settings: {e}");
        }
    }

    pub fn music_folder(&self) -> PathBuf {
        self.music_folder
            .clone()
            .or_else(|| directories::UserDirs::new().and_then(|u| u.audio_dir().map(|p| p.to_path_buf())))
            .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}
