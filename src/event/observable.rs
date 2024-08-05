use std::sync::Arc;

use tokio::sync::Mutex;

use super::{DispatchError, Event};

#[derive(Debug)]
pub enum ObservableResult<T>
where
    T: Send + Sync + 'static,
{
    Unchanged,
    Changed(Result<(), Vec<DispatchError<T>>>),
}

#[derive(Debug)]
pub struct Observable<T>
where
    T: Send + Sync + 'static + Clone + PartialEq,
{
    value: Mutex<T>,
    on_change: Event<T>,
}

impl<T> Observable<T>
where
    T: Send + Sync + 'static + Clone + PartialEq,
{
    pub fn new<I>(value: T, event_name: I) -> Self
    where
        I: Into<String>,
    {
        Self {
            value: Mutex::new(value),
            on_change: Event::new(event_name),
        }
    }

    pub async fn get(&self) -> T {
        let lock = self.value.lock().await;
        lock.clone()
    }

    pub async fn set(&self, value: T) -> ObservableResult<T> {
        let mut lock = self.value.lock().await;
        let current_value = lock.clone();

        if current_value == value {
            return ObservableResult::Unchanged;
        }

        *lock = value.clone();

        let value = Arc::new(value);
        let dispatch_result = self.on_change.dispatch(value).await;

        match dispatch_result {
            Ok(_) => ObservableResult::Changed(Ok(())),
            Err(errors) => ObservableResult::Changed(Err(errors)),
        }
    }
}
