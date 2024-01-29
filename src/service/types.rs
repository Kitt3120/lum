use std::{error::Error, fmt::Display, future::Future, pin::Pin};

use thiserror::Error;

pub type BoxedError = Box<dyn Error + Send + Sync>;

pub type BoxedFuture<'a, T> = Box<dyn Future<Output = T> + Send + 'a>;
pub type BoxedFutureResult<'a, T> = BoxedFuture<'a, Result<T, BoxedError>>;

pub type PinnedBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type PinnedBoxedFutureResult<'a, T> = PinnedBoxedFuture<'a, Result<T, BoxedError>>;

#[derive(Debug, Clone)]
pub enum Status {
    Started,
    Stopped,
    Starting,
    Stopping,
    FailedToStart(String),
    FailedToStop(String),
    RuntimeError(String),
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

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("Service {0} is not managed by this Service Manager")]
    ServiceNotManaged(String),
    #[error("Service {0} already has a background task running")]
    BackgroundTaskAlreadyRunning(String),
    #[error("Service {0} is not stopped")]
    ServiceNotStopped(String),
    #[error("Service {0} failed to start")]
    FailedToStartService(String),
}

#[derive(Debug, Error)]
pub enum ShutdownError {
    #[error("Service {0} is not managed by this Service Manager")]
    ServiceNotManaged(String),
    #[error("Service {0} is not started")]
    ServiceNotStarted(String),
    #[error("Service {0} failed to stop")]
    FailedToStopService(String),
}
