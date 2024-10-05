use std::{
    error::Error,
    fmt::{self, Display},
    future::Future,
    pin::Pin,
};

use thiserror::Error;

use crate::event::event_repeater::{AttachError, DetachError};

pub type BoxedError = Box<dyn Error + Send + Sync>;

pub type PinnedBoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send + Sync>>;
pub type PinnedBoxedFutureResult<T> = PinnedBoxedFuture<Result<T, BoxedError>>;

pub type LifetimedPinnedBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + Sync + 'a>>;
pub type LifetimedPinnedBoxedFutureResult<'a, T> =
    LifetimedPinnedBoxedFuture<'a, Result<T, BoxedError>>;

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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    #[error("Service {0} is not stopped")]
    ServiceNotStopped(String),

    #[error("Service {0} already has a background task running")]
    BackgroundTaskAlreadyRunning(String),

    #[error(
        "Failed to attach Service Manager's status_change EventRepeater to {0}'s status_change Event: {1}"
    )]
    StatusAttachmentFailed(String, AttachError),

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

    #[error(
        "Failed to detach Service Manager's status_change EventRepeater from {0}'s status_change Event: {1}"
    )]
    StatusDetachmentFailed(String, DetachError),
}
