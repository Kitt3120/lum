use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use tokio::sync::Mutex;

use super::{Event, ObservableResult};

#[derive(Debug)]
pub struct ArcObservable<T>
where
    T: Send + 'static + Hash,
{
    value: Arc<Mutex<T>>,
    on_change: Event<Mutex<T>>,
}

impl<T> ArcObservable<T>
where
    T: Send + 'static + Hash,
{
    pub fn new(value: T, event_name: impl Into<String>) -> Self {
        Self {
            value: Arc::new(Mutex::new(value)),
            on_change: Event::new(event_name),
        }
    }

    pub async fn get(&self) -> Arc<Mutex<T>> {
        Arc::clone(&self.value)
    }

    pub async fn set(&self, value: T) -> ObservableResult<Mutex<T>> {
        let mut lock = self.value.lock().await;

        let mut hasher = DefaultHasher::new();
        (*lock).hash(&mut hasher);
        let current_value = hasher.finish();

        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        let new_value = hasher.finish();

        if current_value == new_value {
            return ObservableResult::Unchanged;
        }

        *lock = value;
        drop(lock);

        let value = Arc::clone(&self.value);
        let dispatch_result = self.on_change.dispatch(value).await;

        match dispatch_result {
            Ok(_) => ObservableResult::Changed(Ok(())),
            Err(errors) => ObservableResult::Changed(Err(errors)),
        }
    }
}
