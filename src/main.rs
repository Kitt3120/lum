use ::log::{debug, warn};
use lum::{bot::Bot, config::ConfigHandler, log, service::ServiceManager};

const BOT_NAME: &str = "Lum";

#[tokio::main]
async fn main() {
    if lum::is_debug() {
        warn!("THIS IS A DEBUG RELEASE!");
    }

    if let Err(error) = log::setup() {
        panic!(
            "Error setting up the Logger: {}\n{} will exit.",
            error, BOT_NAME
        );
    }

    let config_handler = ConfigHandler::new(BOT_NAME.to_lowercase().as_str());
    let config = match config_handler.get_config() {
        Ok(config) => config,
        Err(err) => {
            panic!(
                "Error reading config file: {}\n{} will exit.",
                err, BOT_NAME
            );
        }
    };
    debug!("Using config: {}", config);

    let service_manager = initialize_services();

    let bot = Bot::builder(BOT_NAME)
        .with_services(service_manager.services)
        .build();
    lum::run(bot).await;
}

fn initialize_services() -> ServiceManager {
    //TODO: Add services
    //...

    ServiceManager::builder().build()
}
