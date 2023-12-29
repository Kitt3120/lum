use core::fmt;
use std::{
    error::Error,
    fmt::{Display, Formatter},
    fs, io,
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

//TODO: Use thiserror

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigPathError {
    UnknownBasePath,
}

impl Error for ConfigPathError {}

impl Display for ConfigPathError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            ConfigPathError::UnknownBasePath => write!(f, "Unable to get OS config directory"),
        }
    }
}

#[derive(Debug)]
pub enum ConfigInitError {
    PathError(ConfigPathError),
    IOError(io::Error),
}

impl Error for ConfigInitError {}

impl Display for ConfigInitError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            ConfigInitError::PathError(e) => write!(f, "Path error: {}", e),
            ConfigInitError::IOError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl From<ConfigPathError> for ConfigInitError {
    fn from(e: ConfigPathError) -> Self {
        ConfigInitError::PathError(e)
    }
}

impl From<std::io::Error> for ConfigInitError {
    fn from(e: std::io::Error) -> Self {
        ConfigInitError::IOError(e)
    }
}

#[derive(Debug)]
pub enum ConfigParseError {
    PathError(ConfigPathError),
    InitError(ConfigInitError),
    SerdeError(serde_json::Error),
    IoError(io::Error),
}

impl Error for ConfigParseError {}

impl Display for ConfigParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            ConfigParseError::PathError(e) => write!(f, "Path error: {}", e),
            ConfigParseError::InitError(e) => write!(f, "Init error: {}", e),
            ConfigParseError::SerdeError(e) => write!(f, "De-/Serialization error: {}", e),
            ConfigParseError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl From<ConfigPathError> for ConfigParseError {
    fn from(e: ConfigPathError) -> Self {
        ConfigParseError::PathError(e)
    }
}

impl From<ConfigInitError> for ConfigParseError {
    fn from(e: ConfigInitError) -> Self {
        ConfigParseError::InitError(e)
    }
}

impl From<serde_json::Error> for ConfigParseError {
    fn from(e: serde_json::Error) -> Self {
        ConfigParseError::SerdeError(e)
    }
}

impl From<io::Error> for ConfigParseError {
    fn from(e: io::Error) -> Self {
        ConfigParseError::IoError(e)
    }
}

fn discord_token_default() -> String {
    String::from("Please provide a token")
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "discordToken", default = "discord_token_default")]
    pub discord_token: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            discord_token: discord_token_default(),
        }
    }
}

impl Display for Config {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "discord_token: {}", self.discord_token)
    }
}

#[derive(Debug)]
pub struct ConfigHandler {
    pub app_name: String,
}

impl ConfigHandler {
    pub fn new(app_name: &str) -> Self {
        ConfigHandler {
            app_name: app_name.to_string(),
        }
    }

    pub fn get_config_dir_path(&self) -> Result<PathBuf, ConfigPathError> {
        let mut path = match dirs::config_dir() {
            Some(path) => path,
            None => return Err(ConfigPathError::UnknownBasePath),
        };

        path.push(&self.app_name);
        Ok(path)
    }

    pub fn create_config_dir_path(&self) -> Result<(), ConfigInitError> {
        let path = self.get_config_dir_path()?;
        std::fs::create_dir_all(path)?;
        Ok(())
    }

    pub fn get_config_file_path(&self) -> Result<PathBuf, ConfigPathError> {
        let mut path = self.get_config_dir_path()?;
        path.push("config.json");
        Ok(path)
    }

    pub fn save_config(&self, config: &Config) -> Result<(), ConfigParseError> {
        let path = self.get_config_file_path()?;

        if !path.exists() {
            self.create_config_dir_path()?;
        }

        let config_json = serde_json::to_string_pretty(config)?;

        fs::write(path, config_json)?;

        Ok(())
    }

    pub fn get_config(&self) -> Result<Config, ConfigParseError> {
        let path = self.get_config_file_path()?;
        if !path.exists() {
            self.create_config_dir_path()?;
            fs::write(&path, "{}")?;
        }

        let config_json = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&config_json)?;

        self.save_config(&config)?; // In case the config file was missing some fields which serde used the defaults for

        Ok(config)
    }
}
