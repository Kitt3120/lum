use core::fmt;
use std::{fmt::Display, sync::Arc};

use log::error;
use tokio::{signal, sync::Mutex};

use crate::service::{
    types::LifetimedPinnedBoxedFuture, OverallStatus, Service, ServiceManager, ServiceManagerBuilder,
};

#[derive(Debug, Clone, Copy)]
pub enum ExitReason {
    SIGINT,
    EssentialServiceFailed,
}

impl Display for ExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SIGINT => write!(f, "SIGINT"),
            Self::EssentialServiceFailed => write!(f, "Essential Service Failed"),
        }
    }
}

pub struct BotBuilder {
    name: String,
    service_manager: ServiceManagerBuilder,
}

impl BotBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            service_manager: ServiceManager::builder(),
        }
    }

    pub async fn with_service(mut self, service: Arc<Mutex<dyn Service>>) -> Self {
        self.service_manager = self.service_manager.with_service(service).await; // The ServiceManagerBuilder itself will warn when adding a service multiple times

        self
    }

    pub async fn with_services(mut self, services: Vec<Arc<Mutex<dyn Service>>>) -> Self {
        for service in services {
            self.service_manager = self.service_manager.with_service(service).await;
        }

        self
    }

    pub async fn build(self) -> Bot {
        Bot {
            name: self.name,
            service_manager: self.service_manager.build().await,
        }
    }
}

pub struct Bot {
    pub name: String,
    pub service_manager: Arc<ServiceManager>,
}

impl Bot {
    pub fn builder(name: &str) -> BotBuilder {
        BotBuilder::new(name)
    }

    //TODO: When Rust allows async trait methods to be object-safe, refactor this to use async instead of returning a future
    pub fn start(&mut self) -> LifetimedPinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            self.service_manager.start_services().await;
            //TODO: Potential for further initialization here, like modules
        })
    }

    //TODO: When Rust allows async trait methods to be object-safe, refactor this to use async instead of returning a future
    pub fn stop(&mut self) -> LifetimedPinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            self.service_manager.stop_services().await;
            //TODO: Potential for further deinitialization here, like modules
        })
    }

    pub async fn join(&self) -> ExitReason {
        let name_clone = self.name.clone();
        let signal_task = tokio::spawn(async move {
            let name = name_clone;

            let result = signal::ctrl_c().await;
            if let Err(error) = result {
                error!(
                    "Error receiving SIGINT: {}. {} will exit ungracefully immediately to prevent undefined behavior.",
                    error, name
                );
                panic!("Error receiving SIGINT: {}", error);
            }
        });

        let service_manager_clone = self.service_manager.clone();
        let mut receiver = self
            .service_manager
            .on_status_change
            .event
            .subscribe_channel("t", 2, true, true)
            .await;
        let status_task = tokio::spawn(async move {
            let service_manager = service_manager_clone;
            while (receiver.receiver.recv().await).is_some() {
                let overall_status = service_manager.overall_status().await;
                if overall_status == OverallStatus::Unhealthy {
                    return;
                }
            }
        });

        tokio::select! {
            _ = signal_task => ExitReason::SIGINT,
            _ = status_task => ExitReason::EssentialServiceFailed,
        }
    }
}
