use crate::service::{OverallStatus, Service, ServiceManager, ServiceManagerBuilder};

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

    pub fn with_service(mut self, service: Box<dyn Service>) -> Self {
        self.service_manager = self.service_manager.with_service(service); // The ServiceManagerBuilder itself will warn when adding a service multiple times

        self
    }

    pub fn with_services(mut self, services: Vec<Box<dyn Service>>) -> Self {
        for service in services {
            self.service_manager = self.service_manager.with_service(service);
        }

        self
    }

    pub fn build(self) -> Bot {
        Bot::from(self)
    }
}

pub struct Bot {
    pub name: String,
    pub service_manager: ServiceManager,
}

impl Bot {
    pub fn builder(name: &str) -> BotBuilder {
        BotBuilder::new(name)
    }

    pub async fn start(&mut self) {
        self.service_manager.start_services().await;
    }

    pub async fn stop(&mut self) {
        self.service_manager.stop_services().await;
    }

    pub async fn overall_status(&self) -> OverallStatus {
        self.service_manager.overall_status().await
    }
}

impl From<BotBuilder> for Bot {
    fn from(builder: BotBuilder) -> Self {
        Self {
            name: builder.name,
            service_manager: builder.service_manager.build(),
        }
    }
}
