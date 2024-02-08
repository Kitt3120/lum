use crate::service::BoxedError;
use std::{fmt::Debug, sync::Arc};
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};

pub struct Event<T> {
    receivers: Mutex<Vec<Sender<Arc<T>>>>,
    closures: Mutex<Vec<Box<dyn Fn(Arc<T>) -> Result<(), BoxedError> + Send>>>,
}

impl<T> Event<T> {
    pub fn new() -> Self {
        Self {
            receivers: Mutex::new(Vec::new()),
            closures: Mutex::new(Vec::new()),
        }
    }

    pub async fn get_receiver(&self, buffer: usize) -> Receiver<Arc<T>> {
        let (sender, receiver) = channel(buffer);
        let mut subscribers = self.receivers.lock().await;
        subscribers.push(sender);
        receiver
    }

    pub async fn subscribe(&self, closure: impl Fn(Arc<T>) -> Result<(), BoxedError> + Send + 'static) {
        let mut closures = self.closures.lock().await;
        closures.push(Box::new(closure));
    }

    pub async fn dispatch(&self, data: T) {
        let subscribers = self.receivers.lock().await;
        let data = Arc::new(data);

        for subscriber in subscribers.iter() {
            let data = Arc::clone(&data);
            let result = subscriber.send(data).await;
            if let Err(err) = result {
                log::error!("Event failed to dispatch data to one of its receivers: {}", err);
            }
        }

        let closures = self.closures.lock().await;
        for closure in closures.iter() {
            let data = Arc::clone(&data);
            let result = closure(data);
            if let Err(err) = result {
                log::error!("Event failed to dispatch data to one of its closures: {}", err);
            }
        }
    }
}

impl<T> Default for Event<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Debug for Event<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(format!("Event of type {}", std::any::type_name::<T>()).as_str())
            .finish()
    }
}
