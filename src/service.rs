pub mod discord;
// TODO: Used for downcast_rs. Maybe this can be removed when updating the crate.
#[allow(clippy::multiple_bound_locations)]
pub mod service; // Will be fixed when lum gets seperated into multiple workspaces
pub mod service_manager;
pub mod types;
pub mod watchdog;

pub use service::{Service, ServiceInfo};
pub use service_manager::{ServiceManager, ServiceManagerBuilder};
pub use types::{
    BoxedError, BoxedFuture, BoxedFutureResult, OverallStatus, PinnedBoxedFuture, PinnedBoxedFutureResult,
    Priority, ShutdownError, StartupError, Status,
};
pub use watchdog::Watchdog;
