use std::time::SystemTime;

use ::log::info;
use bot::Bot;

pub mod bot;
pub mod config;
pub mod log;
pub mod service;

pub fn is_debug() -> bool {
    cfg!(debug_assertions)
}

pub async fn run(mut bot: Bot) {
    let now = SystemTime::now();

    if !log::is_set_up() {
        eprintln!(
            "Logger has not been set up! {} will not initialize.",
            bot.name
        );
        return;
    }

    bot.init().await;

    let elapsed = match now.elapsed() {
        Ok(elapsed) => elapsed,
        Err(error) => {
            panic!(
                "Error getting elapsed startup time: {}\n{} will exit.",
                error, bot.name
            );
        }
    };

    info!(
        "{} is alive! Startup took {}ms",
        bot.name,
        elapsed.as_millis()
    );

    //TODO: Add CLI commands
    while match tokio::signal::ctrl_c().await {
        Ok(_) => {
            info!("Received SIGINT, shutting down...");
            false
        }
        Err(error) => {
            panic!("Error receiving SIGINT: {}\n{} will exit.", error, bot.name);
        }
    } {}
}
