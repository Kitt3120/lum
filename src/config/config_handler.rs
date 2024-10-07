use std::{fs, io, marker::PhantomData, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub trait Merge<T> {
    fn merge(&self, other: &T) -> Self;
}

#[derive(Debug, Error)]
pub enum ConfigPathError {
    #[error("Unable to get OS-specific config directory")]
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
pub enum ConfigSaveError {
    #[error("Unable to get config path: {0}")]
    Path(#[from] ConfigPathError),

    #[error("Unable to init config: {0}")]
    Init(#[from] ConfigInitError),

    #[error("Unable to serialize config: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    IO(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum FileConfigParseError {
    #[error("Unable to get config path: {0}")]
    Path(#[from] ConfigPathError),

    #[error("Unable to initialize config: {0}")]
    Init(#[from] ConfigInitError),

    #[error("Unable to save config: {0}")]
    Save(#[from] ConfigSaveError),

    #[error("I/O error: {0}")]
    IO(#[from] io::Error),

    #[error("Unable to serialize or deserialize config: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum EnvironmentConfigParseError {
    #[error("Unable to parse environment variables: {0}")]
    Envy(#[from] serde_env::Error),
}

#[derive(Debug, Error)]
pub enum ConfigParseError {
    #[error("Unable to parse config from file: {0}")]
    File(#[from] FileConfigParseError),

    #[error("Unable to parse config from environment: {0}")]
    Env(#[from] EnvironmentConfigParseError),
}

#[derive(Debug)]
pub struct ConfigHandler<FILE, ENV>
where
    FILE: Serialize + for<'de> Deserialize<'de> + Merge<ENV>,
    ENV: Serialize + for<'de> Deserialize<'de>,
{
    pub app_name: String,
    _phantom_file: PhantomData<FILE>,
    _phantom_env: PhantomData<ENV>,
}

impl<FILE, ENV> ConfigHandler<FILE, ENV>
where
    FILE: Serialize + for<'de> Deserialize<'de> + Merge<ENV>,
    ENV: Serialize + for<'de> Deserialize<'de>,
{
    pub fn new(app_name: &str) -> Self {
        ConfigHandler {
            app_name: app_name.to_string(),
            _phantom_file: PhantomData,
            _phantom_env: PhantomData,
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

    pub fn save_config(&self, config: &FILE) -> Result<(), ConfigSaveError> {
        let path = self.get_config_file_path()?;
        if !path.exists() {
            self.create_config_dir_path()?;
        }

        let config_json = serde_json::to_string_pretty(config)?;
        fs::write(path, config_json)?;

        Ok(())
    }

    pub fn load_config_from_file(&self) -> Result<FILE, FileConfigParseError> {
        let path = self.get_config_file_path()?;
        if !path.exists() {
            self.create_config_dir_path()?;
            fs::write(&path, "{}")?;
        }

        let config_json = fs::read_to_string(path)?;
        let config = serde_json::from_str(&config_json)?;
        self.save_config(&config)?; // In case the config file was missing some fields which serde used the defaults for

        Ok(config)
    }

    pub fn load_config_from_env(&self) -> Result<ENV, EnvironmentConfigParseError> {
        let prefix = self.app_name.to_uppercase();
        let config = serde_env::from_env_with_prefix(&prefix)?;

        Ok(config)
    }

    pub fn load_config(&self) -> Result<FILE, ConfigParseError> {
        let env_config = self.load_config_from_env()?;
        let file_config = self.load_config_from_file()?;
        let merged_config = ConfigHandler::merge_configs(env_config, file_config);

        Ok(merged_config)
    }

    pub fn merge_configs(prioritized_config: ENV, secondary_config: FILE) -> FILE {
        secondary_config.merge(&prioritized_config)
    }
}
