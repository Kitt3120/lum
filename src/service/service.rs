use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    sync::Arc,
};

use downcast_rs::{impl_downcast, DowncastSync};

use crate::event::Observable;

use super::{
    service_manager::ServiceManager,
    types::{PinnedBoxedFuture, PinnedBoxedFutureResult, Priority, Status},
};

#[derive(Debug)]
pub struct ServiceInfo {
    pub id: String,
    pub name: String,
    pub priority: Priority,

    pub status: Observable<Status>,
}

impl ServiceInfo {
    pub fn new(id: &str, name: &str, priority: Priority) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            priority,
            status: Observable::new(Status::Stopped, format!("{}_status_change", id)),
        }
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
    fn task<'a>(&self) -> Option<PinnedBoxedFutureResult<'a, ()>> {
        None
    }

    fn is_available(&self) -> PinnedBoxedFuture<'_, bool> {
        Box::pin(async move { matches!(self.info().status.get().await, Status::Started) })
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
