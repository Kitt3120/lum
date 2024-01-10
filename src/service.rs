use downcast_rs::{impl_downcast, DowncastSync};
use log::{error, info, warn};
use std::{
    any::Any,
    cmp::Ordering,
    error::Error,
    fmt::Display,
    future::Future,
    hash::{Hash, Hasher},
    mem,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::RwLock;

use crate::setlock::SetLock;

pub mod discord;

pub type BoxedFuture<'a, T> = Box<dyn Future<Output = T> + 'a>;
pub type BoxedFutureResult<'a, T> = BoxedFuture<'a, Result<T, Box<dyn Error + Send + Sync>>>;

pub type PinnedBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
pub type PinnedBoxedFutureResult<'a, T> =
    PinnedBoxedFuture<'a, Result<T, Box<dyn Error + Send + Sync>>>;

#[derive(Debug)]
pub enum Status {
    Started,
    Stopped,
    Starting,
    Stopping,
    FailedToStart(Box<dyn Error + Send + Sync>), //TODO: Test out if it'd be better to use a String instead
    FailedToStop(Box<dyn Error + Send + Sync>),
    RuntimeError(Box<dyn Error + Send + Sync>),
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Started => write!(f, "Started"),
            Status::Stopped => write!(f, "Stopped"),
            Status::Starting => write!(f, "Starting"),
            Status::Stopping => write!(f, "Stopping"),
            Status::FailedToStart(error) => write!(f, "Failed to start: {}", error),
            Status::FailedToStop(error) => write!(f, "Failed to stop: {}", error),
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
                | (Status::FailedToStart(_), Status::FailedToStart(_))
                | (Status::FailedToStop(_), Status::FailedToStop(_))
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
    id: String,
    pub name: String,
    pub priority: Priority,

    pub status: Arc<RwLock<Status>>,
}

impl ServiceInfo {
    pub fn new(id: &str, name: &str, priority: Priority) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            priority,
            status: Arc::new(RwLock::new(Status::Stopped)),
        }
    }

    pub async fn set_status(&self, status: Status) {
        *(self.status.write().await) = status
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
pub trait Service: DowncastSync {
    fn info(&self) -> &ServiceInfo;
    fn start(&mut self, service_manager: Arc<ServiceManager>) -> PinnedBoxedFutureResult<'_, ()>;
    fn stop(&mut self) -> PinnedBoxedFutureResult<'_, ()>;

    fn is_available(&self) -> PinnedBoxedFuture<'_, bool> {
        Box::pin(async move { matches!(&*(self.info().status.read().await), Status::Started) })
    }
}

impl_downcast!(sync Service);

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
    services: Vec<Arc<RwLock<dyn Service>>>,
}

impl ServiceManagerBuilder {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    //TODO: When Rust allows async closures, refactor this to use iterator methods instead of for loop
    pub async fn with_service(mut self, service: Arc<RwLock<dyn Service>>) -> Self {
        let lock = service.read().await;

        let mut found = false;
        for registered_service in self.services.iter() {
            let registered_service = registered_service.read().await;

            if registered_service.info().id == lock.info().id {
                found = true;
            }
        }

        if found {
            warn!(
                    "Tried to add service {} ({}), but a service with that ID already exists. Ignoring.",
                    lock.info().name, lock.info().id
                );
            return self;
        }

        drop(lock);

        self.services.push(service);
        self
    }

    pub async fn build(self) -> Arc<ServiceManager> {
        let service_manager = ServiceManager {
            services: self.services,
            arc: RwLock::new(SetLock::new()),
        };

        let self_arc = Arc::new(service_manager);

        match self_arc.arc.write().await.set(Arc::clone(&self_arc)) {
            Ok(()) => {}
            Err(err) => {
                panic!(
                    "Failed to set ServiceManager in SetLock for self_arc: {}",
                    err
                );
            }
        }

        self_arc
    }
}

pub struct ServiceManager {
    pub services: Vec<Arc<RwLock<dyn Service>>>,
    arc: RwLock<SetLock<Arc<Self>>>,
}

impl ServiceManager {
    pub fn builder() -> ServiceManagerBuilder {
        ServiceManagerBuilder::new()
    }

    pub async fn manages_service(&self, service: &Arc<RwLock<dyn Service>>) -> bool {
        for registered_service in self.services.iter() {
            if registered_service.read().await.info().id == service.read().await.info().id {
                return true;
            }
        }

        false
    }

    pub async fn start_service(&self, service: Arc<RwLock<dyn Service>>) {
        if !self.manages_service(&service).await {
            warn!(
                "Tried to start service {} ({}), but it's not managed by this Service Manager. Ignoring start request.",
                service.read().await.info().name,
                service.read().await.info().id
            );
            return;
        }

        let mut service = service.write().await;

        let mut status = service.info().status.write().await;
        if !matches!(&*status, Status::Stopped) {
            warn!(
                "Tried to start service {} while it was in state {}. Ignoring start request.",
                service.info().name,
                status
            );
            return;
        }
        *status = Status::Starting;
        drop(status);

        let service_manager = Arc::clone(self.arc.read().await.unwrap());
        match service.start(service_manager).await {
            Ok(()) => {
                info!("Started service: {}", service.info().name);
                service.info().set_status(Status::Started).await;
            }
            Err(error) => {
                error!("Failed to start service {}: {}", service.info().name, error);
                service
                    .info()
                    .set_status(Status::FailedToStart(error))
                    .await;
            }
        }
    }

    pub async fn stop_service(&self, service: Arc<RwLock<dyn Service>>) {
        if !self.manages_service(&service).await {
            warn!(
                "Tried to stop service {} ({}), but it's not managed by this Service Manager. Ignoring stop request.",
                service.read().await.info().name,
                service.read().await.info().id
            );
            return;
        }

        let mut service = service.write().await;

        let mut status = service.info().status.write().await;
        if !matches!(&*status, Status::Started) {
            warn!(
                "Tried to stop service {} while it was in state {}. Ignoring stop request.",
                service.info().name,
                status
            );
            return;
        }
        *status = Status::Stopping;
        drop(status);

        match service.stop().await {
            Ok(()) => {
                info!("Stopped service: {}", service.info().name);
                service.info().set_status(Status::Stopped).await;
            }
            Err(error) => {
                error!("Failed to stop service {}: {}", service.info().name, error);
                service.info().set_status(Status::FailedToStop(error)).await;
            }
        }
    }

    pub async fn start_services(&self) {
        for service in &self.services {
            self.start_service(Arc::clone(service)).await;
        }
    }

    pub async fn stop_services(&self) {
        for service in &self.services {
            self.stop_service(Arc::clone(service)).await;
        }
    }

    pub async fn get_service<T>(&self) -> Option<Arc<RwLock<T>>>
    where
        T: Service + Any + Send + Sync + 'static,
    {
        for service in self.services.iter() {
            let lock = service.read().await;
            let is_t = lock.as_any().is::<T>();
            drop(lock);

            if is_t {
                let arc_clone = Arc::clone(service);
                let service_ptr: *const Arc<RwLock<dyn Service>> = &arc_clone;

                /*
                    I tried to do this in safe rust for 3 days, but I couldn't figure it out
                    Should you come up with a way to do this in safe rust, please make a PR! :)
                    Anyways, this should never cause any issues, since we checked if the service is of type T
                */
                unsafe {
                    let t_ptr: *const Arc<RwLock<T>> = mem::transmute(service_ptr);
                    return Some(Arc::clone(&*t_ptr));
                }
            }
        }

        None
    }

    //TODO: When Rust allows async closures, refactor this to use iterator methods instead of for loop
    pub fn overall_status(&self) -> PinnedBoxedFuture<'_, OverallStatus> {
        Box::pin(async move {
            for service in self.services.iter() {
                let service = service.read().await;
                let status = service.info().status.read().await;

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
            let mut text_buffer = String::new();

            let mut failed_essentials = String::new();
            let mut failed_optionals = String::new();
            let mut non_failed_essentials = String::new();
            let mut non_failed_optionals = String::new();
            let mut others = String::new();

            for service in self.services.iter() {
                let service = service.read().await;
                let info = service.info();
                let priority = &info.priority;
                let status = info.status.read().await;

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
                    Status::FailedToStart(_)
                    | Status::FailedToStop(_)
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
            let service = service.blocking_read();
            write!(f, "{} ({})", service.info().name, service.info().id)?;
            if services.peek().is_some() {
                write!(f, ", ")?;
            }
        }
        Ok(())
    }
}
