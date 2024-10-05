use crate::service::OverallStatus;
use ::log::{error, info};
use bot::Bot;
use std::time::SystemTime;

pub mod bot;
pub mod config;
pub mod event;
pub mod log;
pub mod service;

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
        let status_overview = bot.service_manager.status_overview().await;

        error!("{} is not healthy! Some essential services did not start up successfully. {} will now exit ungracefully.\n\n{}",
        bot.name,
        bot.name,
        status_overview);
        return;
    }

    info!("{} is alive", bot.name,);

    //TODO: Add CLI commands

    let exit_reason = bot.join().await;
    match exit_reason {
        bot::ExitReason::SIGINT => info!(
            "{} received a SIGINT signal! Attempting to shut down gracefully.",
            bot.name
        ),
        bot::ExitReason::EssentialServiceFailed => {
            let status_overview = bot.service_manager.status_overview().await;
            error!(
                "An essential service failed! Attempting to shut down gracefully.\n{}",
                status_overview
            );
        }
    }

    bot.stop().await;
    info!("Oyasumi 💤");
}
