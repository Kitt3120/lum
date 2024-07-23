use crate::service::OverallStatus;
use ::log::{error, info};
use bot::Bot;
use std::time::SystemTime;

pub mod bot;
pub mod config;
pub mod log;
pub mod service;
pub mod setlock;

pub fn is_debug() -> bool {
    cfg!(debug_assertions)
}

pub async fn run(mut bot: Bot) {
    if !log::is_set_up() {
        eprintln!("Logger has not been set up!\n{} will exit.", bot.name);

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

    if bot.service_manager.overall_status().await != OverallStatus::Healthy {
        let status_tree = bot.service_manager.status_tree().await;

        error!("{} is not healthy! Some essential services did not start up successfully. Please check the logs.\nService status tree:\n{}\n{} will exit.",
        bot.name,
        status_tree,
        bot.name);
        return;
    }

    info!("{} is alive", bot.name,);

    //TODO: Add CLI commands
    match tokio::signal::ctrl_c().await {
        Ok(_) => {
            info!("Received SIGINT, {} will now shut down", bot.name);
        }
        Err(error) => {
            panic!("Error receiving SIGINT: {}\n{} will exit.", error, bot.name);
        }
    }

    bot.stop().await;

    info!("{} has shut down", bot.name);
}
