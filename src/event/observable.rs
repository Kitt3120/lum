use tokio::sync::Mutex;

use super::{Event, EventError};

#[derive(Debug)]
pub enum ObservableResult<T> {
    Unchanged,
    Changed(Result<(), Vec<EventError<T>>>),
}

#[derive(Debug)]
pub struct Observable<T>
where
    T: Clone + PartialEq,
{
    value: Mutex<T>,
    on_change: Event<T>,
}

impl<T> Observable<T>
where
    T: Clone + PartialEq,
{
    pub fn new<I>(value: T, event_name: I) -> Self
    where
        I: Into<String>,
    {
        Self {
            value: Mutex::new(value),
            on_change: Event::from(event_name),
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

        *lock = value;
        let dispatch_result = self.on_change.dispatch(lock.clone()).await;

        match dispatch_result {
            Ok(_) => ObservableResult::Changed(Ok(())),
            Err(errors) => ObservableResult::Changed(Err(errors)),
        }
    }
}
