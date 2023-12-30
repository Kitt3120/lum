use std::{
    error::Error,
    fmt::Display,
    io,
    sync::{self, Arc, PoisonError},
};

use tokio::sync::{Mutex, MutexGuard};

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

#[derive(Debug)]
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

#[derive(Debug)]
struct IoError(io::Error);

impl From<sync::PoisonError<MutexGuard<'_, Status>>> for IoError {
    fn from(error: PoisonError<MutexGuard<Status>>) -> Self {
        Self(io::Error::new(io::ErrorKind::Other, format!("{}", error)))
    }
}

impl Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

unsafe impl Send for IoError {}
unsafe impl Sync for IoError {}
impl Error for IoError {}

pub trait ServiceInternals {
    async fn start(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn stop(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
}

pub trait Service: ServiceInternals {
    fn info(&self) -> &ServiceInfo;

    async fn start(&mut self) {
        let mut lock = self.info().status.lock().await;
        *lock = Status::Starting;
        drop(lock);

        match ServiceInternals::start(self).await {
            Ok(()) => {
                let mut lock = self.info().status.lock().await;
                *lock = Status::Started;
            }
            Err(error) => {
                let mut lock = self.info().status.lock().await;
                *lock = Status::FailedStarting(error);
            }
        }
    }
    async fn stop(&mut self) {
        let mut lock = self.info().status.lock().await;
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
    }

    async fn is_available(&self) -> bool {
        let lock = self.info().status.lock().await;
        matches!(&*lock, Status::Started)
    }
}
