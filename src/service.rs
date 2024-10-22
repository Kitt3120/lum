pub mod discord;
pub mod service; // Will be fixed when lum gets seperated into multiple workspaces
pub mod service_manager;
pub mod taskchain;
pub mod types;

pub use service::{Service, ServiceInfo};
pub use service_manager::{ServiceManager, ServiceManagerBuilder};
pub use taskchain::Taskchain;
pub use types::{
    BoxedError, LifetimedPinnedBoxedFuture, LifetimedPinnedBoxedFutureResult, OverallStatus,
    PinnedBoxedFuture, PinnedBoxedFutureResult, Priority, ShutdownError, StartupError, Status,
};
