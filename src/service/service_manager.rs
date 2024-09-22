use super::{
    service::Service,
    types::{LifetimedPinnedBoxedFuture, OverallStatus, Priority, ShutdownError, StartupError, Status},
};
use crate::{
    event::EventRepeater, service::Watchdog, setlock::{SetLock, SetLockError}
};
use log::{error, info, warn};
use std::{collections::HashMap, fmt::Display, mem, sync::Arc, time::Duration};
use tokio::{
    spawn,
    sync::{Mutex, MutexGuard},
    task::JoinHandle,
    time::timeout,
};

#[derive(Default)]
pub struct ServiceManagerBuilder {
    services: Vec<Arc<Mutex<dyn Service>>>,
}

impl ServiceManagerBuilder {
pub fn new() -> Self {
        Self { services: Vec::new() }
    }

    //TODO: When Rust allows async closures, refactor this to use iterator methods instead of for loop
    pub async fn with_service(mut self, service: Arc<Mutex<dyn Service>>) -> Self {
        let lock = service.lock().await;

        let mut found = false;
        for registered_service in self.services.iter() {
            let registered_service = registered_service.lock().await;

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
            arc: Mutex::new(SetLock::new()),
            services: self.services,
            background_tasks: Mutex::new(HashMap::new()),
            on_status_change: EventRepeater::new("service_manager_on_status_change").await,
        };

        let self_arc = Arc::new(service_manager);
        let self_arc_clone = Arc::clone(&self_arc);

        let result = self_arc_clone.arc.lock().await.set(Arc::clone(&self_arc_clone));

        if let Err(err) = result {
            match err {
                SetLockError::AlreadySet => {
                    unreachable!("Unable to set ServiceManager's self-arc in ServiceManagerBuilder because it was already set. This should never happen. How did you...?");
                }
            }
        }

        self_arc
    }
}

pub struct ServiceManager {
    arc: Mutex<SetLock<Arc<Self>>>,
    services: Vec<Arc<Mutex<dyn Service>>>,
    background_tasks: Mutex<HashMap<String, JoinHandle<()>>>,

    pub on_status_change: Arc<EventRepeater<Status>>,
}

impl ServiceManager {
    pub fn builder() -> ServiceManagerBuilder {
        ServiceManagerBuilder::new()
    }

    pub async fn manages_service(&self, service_id: &str) -> bool 
    {
        for service in self.services.iter() {
            let service_lock = service.lock().await;

            if service_lock.info().id == service_id {
                return true;
            }
        }

        false
    }

    pub async fn start_service(&self, service: Arc<Mutex<dyn Service>>) -> Result<(), StartupError> {
        let service_id = service.lock().await.info().id.clone();
        if !self.manages_service(&service_id).await {
            return Err(StartupError::ServiceNotManaged(service_id.clone()));
        }

        let mut service_lock = service.lock().await;

        let status = service_lock.info().status.get().await;
        if !matches!(status, Status::Stopped) {
            return Err(StartupError::ServiceNotStopped(service_id.clone()));
        }

        if self.has_background_task_registered(&service_id).await {
            return Err(StartupError::BackgroundTaskAlreadyRunning(service_id.clone()));
        }

        let service_status_event = service_lock.info().status.as_ref();
        let attachment_result = self.on_status_change.attach(service_status_event, 2).await;
        if let Err(err) = attachment_result {
            return Err(StartupError::StatusAttachmentFailed(service_id.clone(), err));
        }

        service_lock.info().status.set(Status::Starting).await;
        self.init_service(&mut service_lock).await?;
        self.start_background_task(&service_lock, Arc::clone(&service))
            .await;

        info!("Started service {}", service_lock.info().name);

        Ok(())
    }

    //TODO: Clean up
    pub async fn stop_service(&self, service: Arc<Mutex<dyn Service>>) -> Result<(), ShutdownError> {
        let service_id = service.lock().await.info().id.clone();
        if !(self.manages_service(&service_id).await) {
            return Err(ShutdownError::ServiceNotManaged(service_id.clone()));
        }

        let mut service_lock = service.lock().await;

        let status = service_lock.info().status.get().await;
        if !matches!(status, Status::Started) {
            return Err(ShutdownError::ServiceNotStarted(service_id.clone()));
        }

        self.stop_background_task(&service_lock).await;

        service_lock.info().status.set(Status::Stopping).await;

        self.shutdown_service(&mut service_lock).await?;

        let service_status_event = service_lock.info().status.as_ref();
        let detach_result = self.on_status_change.detach(service_status_event).await;
        if let Err(err) = detach_result {
            return Err(ShutdownError::StatusDetachmentFailed(service_id.clone(), err));
        }

        info!("Stopped service {}", service_lock.info().name);

        Ok(())
    }

    pub async fn start_services(&self) -> Vec<Result<(), StartupError>> {
        let mut results = Vec::new();

        for service in &self.services {
            let service_arc_clone = Arc::clone(service);
            let result = self.start_service(service_arc_clone).await;

            results.push(result);
        }

        results
    }

    pub async fn stop_services(&self) -> Vec<Result<(), ShutdownError>> {
        let mut results = Vec::new();

        for service in &self.services {
            let service_arc_clone = Arc::clone(service);
            let result = self.stop_service(service_arc_clone).await;

            results.push(result);
        }

        results
    }

    pub async fn get_service<T>(&self) -> Option<Arc<Mutex<T>>>
    where
        T: Service,
    {
        for service in self.services.iter() {
            let lock = service.lock().await;
            let is_t = lock.as_any().is::<T>();

            if is_t {
                let arc_clone = Arc::clone(service);
                let service_ptr: *const Arc<Mutex<dyn Service>> = &arc_clone;

                /*
                    I tried to do this in safe rust for 3 days, but I couldn't figure it out
                    Should you come up with a way to do this in safe rust, please make a PR! :)
                    Anyways, this should never cause any issues, since we checked if the service is of type T
                */
                unsafe {
                    let t_ptr: *const Arc<Mutex<T>> = mem::transmute(service_ptr);
                    return Some(Arc::clone(&*t_ptr));
                }
            }
        }

        None
    }

    //TODO: When Rust allows async closures, refactor this to use iterator methods instead of for loop
    pub fn overall_status(&self) -> LifetimedPinnedBoxedFuture<'_, OverallStatus> {
        Box::pin(async move {
            for service in self.services.iter() {
                let service = service.lock().await;
                let status = service.info().status.get().await;

                if !matches!(status, Status::Started) {
                    return OverallStatus::Unhealthy;
                }
            }

            OverallStatus::Healthy
        })
    }

    //TODO: When Rust allows async closures, refactor this to use iterator methods instead of for loop
    pub fn status_tree(&self) -> LifetimedPinnedBoxedFuture<'_, String> {
        Box::pin(async move {
            let mut text_buffer = String::new();

            let mut failed_essentials = String::new();
            let mut failed_optionals = String::new();
            let mut non_failed_essentials = String::new();
            let mut non_failed_optionals = String::new();
            let mut others = String::new();

            for service in self.services.iter() {
                let service = service.lock().await;
                let info = service.info();
                let priority = &info.priority;
                let status = info.status.get().await;

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

    async fn init_service(
        &self,
        service: &mut MutexGuard<'_, dyn Service>,
    ) -> Result<(), StartupError> {
        let service_manager = Arc::clone(self.arc.lock().await.unwrap());

        //TODO: Add to config instead of hardcoding duration
        let start = service.start(service_manager);
        let timeout_result = timeout(Duration::from_secs(10), start).await;

        match timeout_result {
            Ok(start_result) => match start_result {
                Ok(()) => {
                    service.info().status.set(Status::Started).await;
                }
                Err(error) => {
                    service
                        .info()
                        .status
                        .set(Status::FailedToStart(error.to_string()))
                        .await;
                    return Err(StartupError::FailedToStartService(service.info().id.clone()));
                }
            },
            Err(error) => {
                service
                    .info()
                    .status
                    .set(Status::FailedToStart(error.to_string()))
                    .await;
                return Err(StartupError::FailedToStartService(service.info().id.clone()));
            }
        }

        Ok(())
    }

    async fn shutdown_service(
        &self,
        service: &mut MutexGuard<'_, dyn Service>,
    ) -> Result<(), ShutdownError> {
        //TODO: Add to config instead of hardcoding duration
        let stop = service.stop();
        let timeout_result = timeout(Duration::from_secs(10), stop).await;

        match timeout_result {
            Ok(stop_result) => match stop_result {
                Ok(()) => {
                    service.info().status.set(Status::Stopped).await;
                }
                Err(error) => {
                    service
                        .info()
                        .status
                        .set(Status::FailedToStop(error.to_string()))
                        .await;
                    return Err(ShutdownError::FailedToStopService(service.info().id.clone()));
                }
            },
            Err(error) => {
                service
                    .info()
                    .status
                    .set(Status::FailedToStop(error.to_string()))
                    .await;
                return Err(ShutdownError::FailedToStopService(service.info().id.clone()));
            }
        }

        Ok(())
    }

    async fn has_background_task_registered(&self, service_id: &str) -> bool {
        let tasks = self.background_tasks.lock().await;
        tasks.contains_key(service_id)
    }

    async fn start_background_task(
        &self,
        service_lock: &MutexGuard<'_, dyn Service>,
        service: Arc<Mutex<dyn Service>>,
    ) {
        if self.has_background_task_registered(&service_lock.info().id).await {
            return;
        }

        let task = service_lock.task();
        if let Some(task) = task {
            let mut watchdog = Watchdog::new(task);

            watchdog.append(|result| async move {
                /*
                    We technically only need a read lock here, but we want to immediately stop
                    other services from accessing the service, so we acquire a write lock instead.
                */
                let service = service.lock().await;

                match result {
                    Ok(()) => {
                        error!(
                            "Background task of service {} ended unexpectedly! Service will be marked as failed.",
                            service.info().name
                        );
                
                        service
                            .info()
                            .status
                            .set(Status::RuntimeError("Background task ended unexpectedly!".to_string()))
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
                            .status
                            .set(Status::RuntimeError(
                                format!("Background task ended with error: {}", error),
                            ))
                            .await;
                    }
                }
                Ok(())
            });

            let join_handle = spawn(watchdog.run());

            self.background_tasks
                .lock()
                .await
                .insert(service_lock.info().id.clone(), join_handle);
        }
    }

    async fn stop_background_task(&self, service_lock: &MutexGuard<'_, dyn Service>) {
        if !self.has_background_task_registered(&service_lock.info().id).await {
            return;
        }

        let mut tasks_lock = self.background_tasks.lock().await;
        let task = tasks_lock.get(&service_lock.info().id).unwrap();
        task.abort();
        tasks_lock.remove(&service_lock.info().id);
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
            let service = service.blocking_lock();
            write!(f, "{} ({})", service.info().name, service.info().id)?;
            if services.peek().is_some() {
                write!(f, ", ")?;
            }
        }
        Ok(())
    }
}
