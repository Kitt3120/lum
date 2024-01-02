use ::log::{error, warn};
use lum::{bot::Bot, config::ConfigHandler, log, service::Service};

const BOT_NAME: &str = "Lum";

#[tokio::main]
async fn main() {
    setup_logger();

    if lum::is_debug() {
        warn!("THIS IS A DEBUG RELEASE!");
    }

    let config_handler = ConfigHandler::new(BOT_NAME.to_lowercase().as_str());
    let _config = match config_handler.load_config() {
        Ok(config) => config,
        Err(err) => {
            error!(
                "Error reading config file: {}\n{} will exit.",
                err, BOT_NAME
            );

            return;
        }
    };

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

fn initialize_services() -> Vec<Box<dyn Service>> {
    //TODO: Add services
    //...

    vec![]
}
