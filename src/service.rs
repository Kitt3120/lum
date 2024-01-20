pub mod discord;
pub mod service;
pub mod service_manager;
pub mod types;

pub use service::{Service, ServiceInfo};
pub use service_manager::{ServiceManager, ServiceManagerBuilder};
pub use types::{
    BoxedError, BoxedFuture, BoxedFutureResult, OverallStatus, PinnedBoxedFuture, PinnedBoxedFutureResult,
    Priority, StartupError, Status,
};
