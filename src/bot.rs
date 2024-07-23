use std::sync::Arc;

use tokio::sync::RwLock;

use crate::service::{PinnedBoxedFuture, Service, ServiceManager, ServiceManagerBuilder};

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

    pub async fn with_service(mut self, service: Arc<RwLock<dyn Service>>) -> Self {
        self.service_manager = self.service_manager.with_service(service).await; // The ServiceManagerBuilder itself will warn when adding a service multiple times

        self
    }

    pub async fn with_services(mut self, services: Vec<Arc<RwLock<dyn Service>>>) -> Self {
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
    pub fn start(&mut self) -> PinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            self.service_manager.start_services().await;
            //TODO: Potential for further initialization here, like modules
        })
    }

    //TODO: When Rust allows async trait methods to be object-safe, refactor this to use async instead of returning a future
    pub fn stop(&mut self) -> PinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            self.service_manager.stop_services().await;
            //TODO: Potential for further deinitialization here, like modules
        })
    }
}
