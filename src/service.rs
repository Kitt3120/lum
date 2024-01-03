use log::{error, info, warn};
use std::{
    any::{Any, TypeId},
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    fmt::Display,
    future::Future,
    hash::{Hash, Hasher},
    pin::Pin,
};
use tokio::sync::Mutex;

pub mod discord;

pub type PinnedBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

pub type PinnedBoxedFutureResult<'a, T> =
    PinnedBoxedFuture<'a, Result<T, Box<dyn Error + Send + Sync>>>;

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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
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
    pub id: String,
    pub name: String,
    pub priority: Priority,

    pub status: Mutex<Status>,
}

impl ServiceInfo {
    pub fn new(id: &str, name: &str, priority: Priority) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            priority,
            status: Mutex::new(Status::Stopped),
        }
    }

    pub async fn set_status(&self, status: Status) {
        let mut lock = self.status.lock().await;
        *lock = status;
    }
}

impl PartialEq for ServiceInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ServiceInfo {}

impl Ord for ServiceInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for ServiceInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for ServiceInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

//TODO: When Rust allows async trait methods to be object-safe, refactor this to use async instead of returning a PinnedBoxedFutureResult
pub trait ServiceInternals {
    fn start(&mut self) -> PinnedBoxedFutureResult<'_, ()>;
    fn stop(&mut self) -> PinnedBoxedFutureResult<'_, ()>;
}

//TODO: When Rust allows async trait methods to be object-safe, refactor this to use async instead of returning a PinnedBoxedFutureResult
pub trait Service: ServiceInternals {
    fn info(&self) -> &ServiceInfo;

    // Used for downcasting in get_service method of ServiceManager
    fn as_any(&self) -> &dyn Any;

    fn wrapped_start(&mut self) -> PinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            let mut status = self.info().status.lock().await;

            if !matches!(&*status, Status::Stopped) {
                warn!(
                    "Tried to start service {} while it was in state {}. Ignoring start request.",
                    self.info().name,
                    status
                );
                return;
            }

            *status = Status::Starting;
            drop(status);

            match self.start().await {
                Ok(()) => {
                    info!("Started service: {}", self.info().name);
                    self.info().set_status(Status::Started).await;
                }
                Err(error) => {
                    error!("Failed to start service {}: {}", self.info().name, error);
                    self.info().set_status(Status::FailedStarting(error)).await;
                }
            }
        })
    }

    fn wrapped_stop(&mut self) -> PinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            let mut status = self.info().status.lock().await;

            if !matches!(&*status, Status::Started) {
                warn!(
                    "Tried to stop service {} while it was in state {}. Ignoring stop request.",
                    self.info().name,
                    status
                );
                return;
            }

            *status = Status::Stopping;
            drop(status);

            match ServiceInternals::stop(self).await {
                Ok(()) => {
                    info!("Stopped service: {}", self.info().name);
                    self.info().set_status(Status::Stopped).await;
                }
                Err(error) => {
                    error!("Failed to stop service {}: {}", self.info().name, error);
                    self.info().set_status(Status::FailedStopping(error)).await;
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

impl Eq for dyn Service {}

impl PartialEq for dyn Service {
    fn eq(&self, other: &Self) -> bool {
        self.info() == other.info()
    }
}

impl Ord for dyn Service {
    fn cmp(&self, other: &Self) -> Ordering {
        self.info().cmp(other.info())
    }
}

impl PartialOrd for dyn Service {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for dyn Service {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.info().hash(state);
    }
}

#[derive(Default)]
pub struct ServiceManagerBuilder {
    services: Vec<Box<dyn Service>>,
}

impl ServiceManagerBuilder {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub fn with_service(mut self, service: Box<dyn Service>) -> Self {
        let service_exists = self.services.iter().any(|s| s.info() == service.info());

        if service_exists {
            warn!(
                "Tried to add service {} ({}), but a service with that ID already exists. Ignoring.",
                service.info().name, service.info().id
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

    pub fn start_services(&mut self) -> PinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            for service in &mut self.services {
                service.wrapped_start().await;
            }
        })
    }

    pub fn stop_services(&mut self) -> PinnedBoxedFuture<'_, ()> {
        Box::pin(async move {
            for service in &mut self.services {
                service.wrapped_stop().await;
            }
        })
    }

    pub fn get_service<T>(&self) -> Option<&T>
    where
        T: Service + 'static,
    {
        self.services
            .iter()
            .find(|s| TypeId::of::<T>() == s.as_any().type_id())
            .and_then(|s| s.as_any().downcast_ref::<T>())
    }

    pub fn status_map(&self) -> PinnedBoxedFuture<'_, HashMap<&dyn Service, &Mutex<Status>>> {
        Box::pin(async move {
            let mut status_map = HashMap::new();

            for service in self.services.iter() {
                status_map.insert(&**service, &service.info().status);
            }

            status_map
        })
    }

    //TODO: When Rust allows async closures, refactor this to use iterator methods instead of for loop
    pub fn overall_status(&self) -> PinnedBoxedFuture<'_, OverallStatus> {
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

    //TODO: When Rust allows async closures, refactor this to use iterator methods instead of for loop
    pub fn status_tree(&self) -> PinnedBoxedFuture<'_, String> {
        Box::pin(async move {
            let status_map = self.status_map().await;

            let mut text_buffer = String::new();

            let mut failed_essentials = String::new();
            let mut failed_optionals = String::new();
            let mut non_failed_essentials = String::new();
            let mut non_failed_optionals = String::new();
            let mut others = String::new();

            for (service, status) in status_map.into_iter() {
                let info = service.info();
                let priority = &info.priority;
                let status = status.lock().await;

                match *status {
                    Status::Started | Status::Stopped => match priority {
                        Priority::Essential => {
                            non_failed_essentials
                                .push_str(&format!(" - {}: {}\n", info.name, status));
                        }
                        Priority::Optional => {
                            non_failed_optionals
                                .push_str(&format!(" - {}: {}\n", info.name, status));
                        }
                    },
                    Status::FailedStarting(_)
                    | Status::FailedStopping(_)
                    | Status::RuntimeError(_) => match priority {
                        Priority::Essential => {
                            failed_essentials.push_str(&format!(" - {}: {}\n", info.name, status));
                        }
                        Priority::Optional => {
                            failed_optionals.push_str(&format!(" - {}: {}\n", info.name, status));
                        }
                    },
                    _ => {
                        others.push_str(&format!(" - {}: {}\n", info.name, status));
                    }
                }
            }

            if !failed_essentials.is_empty() {
                text_buffer.push_str(&format!("- {}:\n", "Failed essential services"));
                text_buffer.push_str(&failed_essentials);
            }

            if !failed_optionals.is_empty() {
                text_buffer.push_str(&format!("- {}:\n", "Failed optional services"));
                text_buffer.push_str(&failed_optionals);
            }

            if !non_failed_essentials.is_empty() {
                text_buffer.push_str(&format!("- {}:\n", "Essential services"));
                text_buffer.push_str(&non_failed_essentials);
            }

            if !non_failed_optionals.is_empty() {
                text_buffer.push_str(&format!("- {}:\n", "Optional services"));
                text_buffer.push_str(&non_failed_optionals);
            }

            if !others.is_empty() {
                text_buffer.push_str(&format!("- {}:\n", "Other services"));
                text_buffer.push_str(&others);
            }

            text_buffer
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
            write!(f, "{} ({})", service.info().name, service.info().id)?;
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
