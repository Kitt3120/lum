use core::fmt;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter},
    fs, io,
    path::PathBuf,
    time::Duration,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigPathError {
    #[error("Unable to get OS config directory")]
    UnknownBasePath,
}

#[derive(Debug, Error)]
pub enum ConfigInitError {
    #[error("Unable to get config path: {0}")]
    Path(#[from] ConfigPathError),
    #[error("I/O error: {0}")]
    IO(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum ConfigParseError {
    #[error("Unable to get config path: {0}")]
    Path(#[from] ConfigPathError),
    #[error("Unable to initialize config: {0}")]
    Init(#[from] ConfigInitError),
    #[error("Unable to serialize or deserialize config: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    IO(#[from] io::Error),
}

fn discord_token_default() -> String {
    String::from("Please provide a token")
}

fn discord_timeout_default() -> Duration {
    Duration::from_secs(10)
}

#[derive(Debug, PartialEq, PartialOrd, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(rename = "discordToken", default = "discord_token_default")]
    pub discord_token: String,
    #[serde(rename = "discordTimeout", default = "discord_timeout_default")]
    pub discord_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            discord_token: discord_token_default(),
            discord_timeout: discord_timeout_default(),
        }
    }
}

impl Display for Config {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let content = match serde_json::to_string(self) {
            Ok(content) => content,
            Err(error) => {
                return write!(f, "Unable to serialize config: {}", error);
            }
        };

        write!(f, "{}", content)
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
        fs::create_dir_all(path)?;
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

    pub fn load_config(&self) -> Result<Config, ConfigParseError> {
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
