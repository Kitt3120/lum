use std::time::SystemTime;

use ::log::{error, info};
use bot::Bot;

use crate::service::OverallStatus;

pub mod bot;
pub mod config;
pub mod log;
pub mod service;

pub fn is_debug() -> bool {
    cfg!(debug_assertions)
}

pub async fn run(mut bot: Bot) {
    if !log::is_set_up() {
        eprintln!(
            "Logger has not been set up! {} will not initialize.",
            bot.name
        );

        return;
    }

    let now = SystemTime::now();

    bot.start().await;

    match now.elapsed() {
        Ok(elapsed) => info!("Startup took {}ms", elapsed.as_millis()),
        Err(error) => {
            error!(
                "Error getting elapsed startup time: {}\n{} will exit.",
                error, bot.name
            );

            return;
        }
    };

    if bot.overall_status().await != OverallStatus::Healthy {
        error!("{} is not healthy! Some essential services did not start up successfully. Please check the logs.\n{} will exit.", bot.name, bot.name);
        return;
    }

    info!("{} is alive!", bot.name,);

    //TODO: Add CLI commands
    match tokio::signal::ctrl_c().await {
        Ok(_) => {
            info!("Received SIGINT, shutting down...");
        }
        Err(error) => {
            panic!("Error receiving SIGINT: {}\n{} will exit.", error, bot.name);
        }
    }
}
