use std::{error::Error, future::Future, pin::Pin};

use ::log::{debug, error, warn};
use lum::{
    bot::Bot,
    config::{Config, ConfigHandler, ConfigParseError},
    log,
    service::{Priority, Service, ServiceInfo, ServiceInternals},
};

const BOT_NAME: &str = "Lum";

#[tokio::main]
async fn main() {
    setup_logger();

    if lum::is_debug() {
        warn!("THIS IS A DEBUG RELEASE!");
    }

    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(
                "Error reading config file: {}\n{} will exit.",
                err, BOT_NAME
            );

            return;
        }
    };
    debug!("Using config: {}", config);

    let bot = Bot::builder(BOT_NAME)
        .with_services(initialize_services())
        .build();

    lum::run(bot).await;
}

fn setup_logger() {
    if let Err(error) = log::setup() {
        panic!(
            "Error setting up the Logger: {}\n{} will exit.",
            error, BOT_NAME
        );
    }
}

fn get_config() -> Result<Config, ConfigParseError> {
    let config_handler = ConfigHandler::new(BOT_NAME.to_lowercase().as_str());

    config_handler.get_config()
}

fn initialize_services() -> Vec<Box<dyn Service>> {
    //TODO: Add services
    //...

    let crash_service = CrashService {
        info: ServiceInfo::new("CrashService", Priority::Essential),
    };

    vec![Box::new(crash_service)]
}

struct CrashService {
    info: ServiceInfo,
}

impl ServiceInternals for CrashService {
    fn start(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn stop(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + '_>> {
        Box::pin(async move { Ok(()) })
    }
}

impl Service for CrashService {
    fn info(&self) -> &ServiceInfo {
        &self.info
    }
}
