use crate::setlock::SetLock;
use log::{error, info, warn};
use serenity::FutureExt;
use std::{collections::HashMap, fmt::Display, mem, sync::Arc, time::Duration};
use tokio::{spawn, sync::RwLock, task::JoinHandle, time::timeout};

use super::{
    service::Service,
    types::{OverallStatus, PinnedBoxedFuture, Priority, StartupError, Status},
};

#[derive(Default)]
pub struct ServiceManagerBuilder {
    services: Vec<Arc<RwLock<dyn Service>>>,
}

impl ServiceManagerBuilder {
    pub fn new() -> Self {
        Self { services: Vec::new() }
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
                lock.info().name,
                lock.info().id
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
            tasks: RwLock::new(HashMap::new()),
        };

        let self_arc = Arc::new(service_manager);

        match self_arc.arc.write().await.set(Arc::clone(&self_arc)) {
            Ok(()) => {}
            Err(err) => {
                panic!("Failed to set ServiceManager in SetLock for self_arc: {}", err);
            }
        }

        self_arc
    }
}

pub struct ServiceManager {
    services: Vec<Arc<RwLock<dyn Service>>>,
    arc: RwLock<SetLock<Arc<Self>>>,
    tasks: RwLock<HashMap<String, JoinHandle<()>>>,
}

impl ServiceManager {
    pub fn builder() -> ServiceManagerBuilder {
        ServiceManagerBuilder::new()
    }

    pub async fn manages_service(&self, service: Arc<RwLock<dyn Service>>) -> bool {
        let service_id = service.read().await.info().id.clone();
        for registered_service in self.services.iter() {
            if registered_service.read().await.info().id == service_id {
                return true;
            }
        }

        false
    }

    pub async fn start_service(&self, service: Arc<RwLock<dyn Service>>) -> Result<(), StartupError> {
        let service_lock = service.read().await;

        // Check if the service is managed by this Service Manager
        if !self.manages_service(Arc::clone(&service)).await {
            return Err(StartupError::ServiceNotManaged(service_lock.info().id.clone()));
        }

        // Check if the service already has a background task running
        if self
            .tasks
            .read()
            .await
            .contains_key(service_lock.info().id.as_str())
        {
            return Err(StartupError::BackgroundTaskAlreadyRunning(
                service_lock.info().id.clone(),
            ));
        }

        // Upgrade the read lock to a write lock
        drop(service_lock);
        let mut service_lock = service.write().await;

        // Check if the service is already running and cancel the start request if it is
        let mut status = service_lock.info().status.write().await;
        if !matches!(&*status, Status::Stopped) {
            return Err(StartupError::ServiceAlreadyRunning(
                service_lock.info().id.clone(),
            ));
        }

        // Set the status to Starting
        *status = Status::Starting;
        drop(status);

        // Start the service
        let service_manager = Arc::clone(self.arc.read().await.unwrap());
        let start = service_lock.start(service_manager);
        let timeout_duration = Duration::from_secs(10); //TODO: Add to config instead of hardcoding
        let timeout_result = timeout(timeout_duration, start).await;
        match timeout_result {
            Ok(start_result) => match start_result {
                Ok(()) => {
                    service_lock.info().set_status(Status::Started).await;
                }
                Err(error) => {
                    service_lock.info().set_status(Status::FailedToStart(error)).await;
                    return Err(StartupError::FailedToStartService(service_lock.info().id.clone()));
                }
            },
            Err(error) => {
                service_lock
                    .info()
                    .set_status(Status::FailedToStart(Box::new(error)))
                    .await;
                return Err(StartupError::FailedToStartService(service_lock.info().id.clone()));
            }
        }

        // Start the background task if one is defined
        let task = service_lock.task();
        drop(service_lock);

        if let Some(task) = task {
            let service_clone = Arc::clone(&service);
            let task_with_watchdog = task.then(|result| async move {
                let service = service_clone;

                /*
                    We technically only need a read lock here, but we want to immediately stop other
                    services from accessing the service, so we acquire a write lock instead.
                */
                let service = service.write().await;

                //TODO: Better handling of this. For example, send a message to a channel and let ServiceManager know.
                match result {
                    Ok(()) => {
                        error!(
                            "Background task of service {} ended unexpectedly! Service will be marked as failed.",
                            service.info().name
                        );
                        service
                            .info()
                            .set_status(Status::RuntimeError("Background task ended unexpectedly!".into()))
                            .await;
                    }
                    Err(error) => {
                        error!(
                            "Background task of service {} ended with error: {}! Service will be marked as failed.",
                            service.info().name,
                            error
                        );
                        service
                            .info()
                            .set_status(Status::RuntimeError(
                                format!("Background task ended with error: {}", error).into(),
                            ))
                            .await;
                    }
                }
            });

            let join_handle = spawn(task_with_watchdog);
            self.tasks
                .write()
                .await
                .insert(service.read().await.info().id.clone(), join_handle);
            info!(
                "Started background task for service {}",
                service.read().await.info().name
            );
        }

        Ok(())
    }

    pub async fn stop_service(&self, service: Arc<RwLock<dyn Service>>) {
        if !self.manages_service(Arc::clone(&service)).await {
            let service = service.read().await;
            warn!(
                "Tried to stop service {} ({}), but it's not managed by this Service Manager. Ignoring stop request.",
                service.info().name,
                service.info().id
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

        let stop = service.stop();

        let duration = Duration::from_secs(10); //TODO: Add to config instead of hardcoding
        let timeout_result = timeout(duration, stop);

        match timeout_result.await {
            Ok(stop_result) => match stop_result {
                Ok(()) => {
                    info!("Stopped service: {}", service.info().name);
                    service.info().set_status(Status::Stopped).await;
                }
                Err(error) => {
                    error!("Failed to stop service {}: {}", service.info().name, error);
                    service.info().set_status(Status::FailedToStop(error)).await;
                }
            },
            Err(error) => {
                error!(
                    "Failed to stop service {}: Timeout of {} seconds reached.",
                    service.info().name,
                    duration.as_secs()
                );
                service
                    .info()
                    .set_status(Status::FailedToStop(Box::new(error)))
                    .await;
            }
        }
    }

    pub async fn start_services(&self) -> Vec<Result<(), StartupError>> {
        let mut results = Vec::new();

        for service in &self.services {
            results.push(self.start_service(Arc::clone(service)).await);
        }

        results
    }

    pub async fn stop_services(&self) {
        for service in &self.services {
            self.stop_service(Arc::clone(service)).await;
        }
    }

    pub async fn get_service<T>(&self) -> Option<Arc<RwLock<T>>>
    where
        T: Service,
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
                            non_failed_essentials.push_str(&format!(" - {}: {}\n", info.name, status));
                        }
                        Priority::Optional => {
                            non_failed_optionals.push_str(&format!(" - {}: {}\n", info.name, status));
                        }
                    },
                    Status::FailedToStart(_) | Status::FailedToStop(_) | Status::RuntimeError(_) => {
                        match priority {
                            Priority::Essential => {
                                failed_essentials.push_str(&format!(" - {}: {}\n", info.name, status));
                            }
                            Priority::Optional => {
                                failed_optionals.push_str(&format!(" - {}: {}\n", info.name, status));
                            }
                        }
                    }
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
