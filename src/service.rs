use std::{collections::HashMap, error::Error, fmt::Display, future::Future, pin::Pin, sync::Arc};

use log::{error, info, warn};
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum Status {
    Started,
    Stopped,
    Starting,
    Stopping,
    FailedStarting(Box<dyn Error + Send + Sync>),
    FailedStopping(Box<dyn Error + Send + Sync>),
    RuntimeError(Box<dyn Error + Send + Sync>),
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Started => write!(f, "Started"),
            Status::Stopped => write!(f, "Stopped"),
            Status::Starting => write!(f, "Starting"),
            Status::Stopping => write!(f, "Stopping"),
            Status::FailedStarting(error) => write!(f, "Failed to start: {}", error),
            Status::FailedStopping(error) => write!(f, "Failed to stop: {}", error),
            Status::RuntimeError(error) => write!(f, "Runtime error: {}", error),
        }
    }
}

impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Status::Started, Status::Started)
                | (Status::Stopped, Status::Stopped)
                | (Status::Starting, Status::Starting)
                | (Status::Stopping, Status::Stopping)
                | (Status::FailedStarting(_), Status::FailedStarting(_))
                | (Status::FailedStopping(_), Status::FailedStopping(_))
                | (Status::RuntimeError(_), Status::RuntimeError(_))
        )
    }
}

impl Eq for Status {}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverallStatus {
    Healthy,
    Unhealthy,
}

impl Display for OverallStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverallStatus::Healthy => write!(f, "Healthy"),
            OverallStatus::Unhealthy => write!(f, "Unhealthy"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Essential,
    Optional,
}

impl Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Essential => write!(f, "Essential"),
            Priority::Optional => write!(f, "Optional"),
        }
    }
}

#[derive(Debug)]
pub struct ServiceInfo {
    pub name: String,
    pub priority: Priority,

    pub status: Arc<Mutex<Status>>,
}

impl ServiceInfo {
    pub fn new(name: &str, priority: Priority) -> Self {
        Self {
            name: name.to_string(),
            priority,
            status: Arc::new(Mutex::new(Status::Stopped)),
        }
    }
}

//TODO: When Rust allows async trait methods to be object-safe, refactor this to use async instead of returning a future
pub trait ServiceInternals {
    fn start(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + '_>>;
    fn stop(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + '_>>;
}

//TODO: When Rust allows async trait methods to be object-safe, refactor this to use async instead of returning a future
pub trait Service: ServiceInternals {
    fn info(&self) -> &ServiceInfo;

    fn wrapped_start(&mut self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async move {
            let mut lock = self.info().status.lock().await;

            if !matches!(&*lock, Status::Started) {
                warn!(
                    "Tried to start service {} while it was in state {}. Ignoring start request.",
                    self.info().name,
                    lock
                );
                return;
            }

            *lock = Status::Starting;
            drop(lock);

            match self.start().await {
                Ok(()) => {
                    let mut lock = self.info().status.lock().await;
                    *lock = Status::Started;
                    info!("Started service: {}", self.info().name);
                }
                Err(error) => {
                    let mut lock = self.info().status.lock().await;
                    *lock = Status::FailedStarting(error);
                    error!("Failed to start service: {}", self.info().name);
                }
            }
        })
    }

    fn wrapped_stop(&mut self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async move {
            let mut lock = self.info().status.lock().await;

            if matches!(&*lock, Status::Started) {
                warn!(
                    "Tried to stop service {} while it was in state {}. Ignoring stop request.",
                    self.info().name,
                    lock
                );
                return;
            }

            *lock = Status::Stopping;
            drop(lock);

            match ServiceInternals::stop(self).await {
                Ok(()) => {
                    let mut lock = self.info().status.lock().await;
                    *lock = Status::Stopped;
                }
                Err(error) => {
                    let mut lock = self.info().status.lock().await;
                    *lock = Status::FailedStopping(error);
                }
            }
        })
    }

    fn is_available(&self) -> Pin<Box<dyn Future<Output = bool> + '_>> {
        Box::pin(async move {
            let lock = self.info().status.lock().await;
            matches!(&*lock, Status::Started)
        })
    }
}

impl PartialEq for dyn Service {
    fn eq(&self, other: &Self) -> bool {
        self.info().name == other.info().name
    }
}

impl Eq for dyn Service {}

impl std::hash::Hash for dyn Service {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.info().name.hash(state);
    }
}

#[derive(Default)]
pub struct ServiceManagerBuilder {
    services: Vec<Box<dyn Service>>,
}

impl ServiceManagerBuilder {
    pub fn new() -> Self {
        Self { services: vec![] }
    }

    pub fn with_service(mut self, service: Box<dyn Service>) -> Self {
        let service_exists = self
            .services
            .iter()
            .any(|s| s.info().name == service.info().name); // Can't use *s == service here because value would be moved

        if service_exists {
            warn!(
                "Tried to add service {} multiple times. Ignoring.",
                service.info().name
            );

            return self;
        }

        self.services.push(service);

        self
    }

    pub fn build(self) -> ServiceManager {
        ServiceManager::from(self)
    }
}

pub struct ServiceManager {
    pub services: Vec<Box<dyn Service>>,
}

impl ServiceManager {
    pub fn builder() -> ServiceManagerBuilder {
        ServiceManagerBuilder::new()
    }

    pub async fn start_services(&mut self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async move {
            for service in &mut self.services {
                info!("Starting service: {}", service.info().name);
                service.wrapped_start().await;
            }
        })
    }

    pub fn stop_services(&mut self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async move {
            for service in &mut self.services {
                info!("Stopping service: {}", service.info().name);
                service.wrapped_stop().await;
            }
        })
    }

    pub fn status_map(
        &self,
    ) -> Pin<Box<dyn Future<Output = HashMap<&Box<dyn Service>, String>> + '_>> {
        Box::pin(async move {
            let mut map = HashMap::new();

            for service in &self.services {
                let status = service.info().status.lock().await;

                let status = status.to_string();
                map.insert(service, status.to_string());
            }

            map
        })
    }

    pub fn overall_status(&self) -> Pin<Box<dyn Future<Output = OverallStatus> + '_>> {
        Box::pin(async move {
            for service in self.services.iter() {
                let status = service.info().status.lock().await;

                if !matches!(&*status, Status::Started) {
                    return OverallStatus::Unhealthy;
                }
            }

            OverallStatus::Healthy
        })
    }
}

impl Display for ServiceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Services: ")?;

        if self.services.is_empty() {
            write!(f, "None")?;
            return Ok(());
        }

        let mut services = self.services.iter().peekable();
        while let Some(service) = services.next() {
            write!(f, "{}", service.info().name)?;
            if services.peek().is_some() {
                write!(f, ", ")?;
            }
        }
        Ok(())
    }
}

impl From<ServiceManagerBuilder> for ServiceManager {
    fn from(builder: ServiceManagerBuilder) -> Self {
        Self {
            services: builder.services,
        }
    }
}
