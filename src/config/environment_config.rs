use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, Clone)]
pub struct EnvironmentConfig {
    pub discord_token: Option<String>,
}

impl Display for EnvironmentConfig {
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
