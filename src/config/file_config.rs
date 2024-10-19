use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use super::{EnvironmentConfig, Merge};

#[derive(Debug, PartialEq, PartialOrd, Serialize, Deserialize, Clone)]
pub struct FileConfig {
    #[serde(rename = "discordToken")]
    pub discord_token: String,
}

impl Merge<EnvironmentConfig> for FileConfig {
    fn merge(&self, other: &EnvironmentConfig) -> Self {
        let discord_token = other
            .discord_token
            .clone()
            .unwrap_or(self.discord_token.clone());

        FileConfig { discord_token }
    }
}

impl Default for FileConfig {
    fn default() -> Self {
        FileConfig {
            discord_token: String::from("Please provide a token"),
        }
    }
}

impl Display for FileConfig {
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
