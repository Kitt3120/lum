use crate::{service::Watchdog, setlock::SetLock};
use log::{error, info, warn};
use std::{collections::HashMap, fmt::Display, mem, sync::Arc, time::Duration};
use tokio::{
    spawn,
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
    task::JoinHandle,
    time::timeout,
};
use super::{
    service::Service,
    types::{OverallStatus, PinnedBoxedFuture, Priority, ShutdownError, StartupError, Status},
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
            arc: RwLock::new(SetLock::new()),
            services: self.services,
            background_tasks: RwLock::new(HashMap::new()),
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
    arc: RwLock<SetLock<Arc<Self>>>,
    services: Vec<Arc<RwLock<dyn Service>>>,
    background_tasks: RwLock<HashMap<String, JoinHandle<()>>>,
}

impl ServiceManager {
    pub fn builder() -> ServiceManagerBuilder {
        ServiceManagerBuilder::new()
    }

    pub async fn manages_service(&self, service_id: &str) -> bool {
        for registered_service in self.services.iter() {
            if registered_service.read().await.info().id == service_id {
                return true;
            }
        }

        false
    }

    pub async fn start_service(&self, service: Arc<RwLock<dyn Service>>) -> Result<(), StartupError> {
        let service_lock = service.read().await;

        let service_id = service_lock.info().id.clone();

        if !self.manages_service(&service_id).await {
            return Err(StartupError::ServiceNotManaged(service_id));
        }

        if !self.is_service_stopped(&service_lock).await {
            return Err(StartupError::ServiceNotStopped(service_id));
        }

        if self.has_background_task_running(&service_lock).await {
            return Err(StartupError::BackgroundTaskAlreadyRunning(service_id));
        }
        
        drop(service_lock);
        let mut service_lock = service.write().await;

        service_lock.info().set_status(Status::Starting).await;
        self.init_service(&mut service_lock).await?;
        self.start_background_task(&service_lock, Arc::clone(&service)).await;

        info!("Started service {}", service_lock.info().name);
        
        Ok(())
    }

    pub async fn stop_service(&self, service: Arc<RwLock<dyn Service>>) -> Result<(), ShutdownError> {
        let service_lock = service.read().await;

        if !(self.manages_service(service_lock.info().id.as_str()).await) {
            return Err(ShutdownError::ServiceNotManaged(service_lock.info().id.clone()));
        }

        if !self.is_service_started(&service_lock).await {
            return Err(ShutdownError::ServiceNotStarted(service_lock.info().id.clone()));
        }

        self.stop_background_task(&service_lock).await;

        drop(service_lock);
        let mut service_lock = service.write().await;

        service_lock.info().set_status(Status::Stopping).await;
        self.shutdown_service(&mut service_lock).await?;

        info!("Stopped service {}", service_lock.info().name);
       
        Ok(())
    }

    pub async fn start_services(&self) -> Vec<Result<(), StartupError>> {
        let mut results = Vec::new();

        for service in &self.services {
            results.push(self.start_service(Arc::clone(service)).await);
        }

        results
    }

    pub async fn stop_services(&self) -> Vec<Result<(), ShutdownError>> {
        let mut results = Vec::new();

        for service in &self.services {
            results.push(self.stop_service(Arc::clone(service)).await);
        }

        results
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
                let status = service.info().get_status().await;

                if !matches!(status, Status::Started) {
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
                let status = info.get_status().await;

                match status {
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

    // Helper methods for start_service and stop_service

    async fn has_background_task_running(
        &self,
        service: &RwLockReadGuard<'_, dyn Service>,
    ) -> bool {
        let tasks = self.background_tasks.read().await;
        tasks.contains_key(service.info().id.as_str())
    }

    async fn is_service_started(
        &self,
        service: &RwLockReadGuard<'_, dyn Service>,
    ) -> bool {
        let status = service.info().get_status().await;
        matches!(status, Status::Started)
    }

    async fn is_service_stopped(
        &self,
        service: &RwLockReadGuard<'_, dyn Service>,
    ) -> bool {
        let status = service.info().get_status().await;
        matches!(status, Status::Stopped)
    }

    async fn init_service(
        &self,
        service: &mut RwLockWriteGuard<'_, dyn Service>,
    ) -> Result<(), StartupError> {
        let service_manager = Arc::clone(self.arc.read().await.unwrap());

        //TODO: Add to config instead of hardcoding duration
        let start = service.start(service_manager);
        let timeout_result = timeout(Duration::from_secs(10), start).await;

        match timeout_result {
            Ok(start_result) => match start_result {
                Ok(()) => {
                    service.info().set_status(Status::Started).await;
                }
                Err(error) => {
                    service.info().set_status(Status::FailedToStart(error.to_string())).await;
                    return Err(StartupError::FailedToStartService(service.info().id.clone()));
                }
            },
            Err(error) => {
                service
                    .info()
                    .set_status(Status::FailedToStart(error.to_string()))
                    .await;
                return Err(StartupError::FailedToStartService(service.info().id.clone()));
            }
        }

        Ok(())
    }

    async fn shutdown_service(
        &self,
        service: &mut RwLockWriteGuard<'_, dyn Service>,
    ) -> Result<(), ShutdownError> {
        //TODO: Add to config instead of hardcoding duration
        let stop = service.stop();
        let timeout_result = timeout(Duration::from_secs(10), stop).await;

        match timeout_result {
            Ok(stop_result) => match stop_result {
                Ok(()) => {
                    service.info().set_status(Status::Stopped).await;
                }
                Err(error) => {
                    service.info().set_status(Status::FailedToStop(error.to_string())).await;
                    return Err(ShutdownError::FailedToStopService(service.info().id.clone()));
                }
            },
            Err(error) => {
                service
                    .info()
                    .set_status(Status::FailedToStop(error.to_string()))
                    .await;
                return Err(ShutdownError::FailedToStopService(service.info().id.clone()));
            }
        }

        Ok(())
    }

    async fn start_background_task(&self, service_lock: &RwLockWriteGuard<'_, dyn Service>, service: Arc<RwLock<dyn Service>>) {
        let task = service_lock.task();
        if let Some(task) = task {
            let mut watchdog = Watchdog::new(task);
    
            watchdog.append(|result| async move {
                let service = service;

                /*
                    We technically only need a read lock here, but we want to immediately stop
                    other services from accessing the service, so we acquire a write lock instead.
                */
                let service = service.write().await;

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
                            "Background task of service {} ended with error: {}. Service will be marked as failed.",
                            service.info().name,
                            error
                        );

                        service
                            .info()
                            .set_status(Status::RuntimeError(
                                format!("Background task ended with error: {}", error),
                            ))
                            .await;
                    }
                }
                Ok(())
            });

            let join_handle = spawn(watchdog.run());

            self.background_tasks
                .write()
                .await
                .insert(service_lock.info().id.clone(), join_handle);
        }
    }

    async fn stop_background_task(&self, service_lock: &RwLockReadGuard<'_, dyn Service>) {
        if self.has_background_task_running(service_lock).await {
            let mut tasks_lock = self.background_tasks.write().await;
            let task = tasks_lock.get(service_lock.info().id.as_str()).unwrap();
            task.abort();
            tasks_lock.remove(service_lock.info().id.as_str());
        }
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
