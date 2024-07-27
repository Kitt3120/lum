use crate::service::BoxedError;
use std::{
    any::type_name,
    fmt::{self, Debug, Formatter},
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::{
    mpsc::{channel, error::SendError, Receiver, Sender},
    Mutex,
};

pub enum Subscriber<T> {
    Channel(Sender<Arc<T>>, bool),
    Closure(Box<dyn Fn(Arc<T>) -> Result<(), BoxedError> + Send + Sync>, bool),
}

#[derive(Debug, Error)]
pub enum EventError<T> {
    ChannelSend(SendError<Arc<T>>),
    Closure(BoxedError),
}

pub struct Event<T> {
    pub name: String,

    log_on_error: bool,

    subscribers: Mutex<Vec<Subscriber<T>>>,
}

impl<T> Event<T> {
    pub fn new(name: impl Into<String>, log_on_error: bool) -> Self {
        Self {
            name: name.into(),
            log_on_error,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub async fn subscriber_count(&self) -> usize {
        let subscribers = self.subscribers.lock().await;
        subscribers.len()
    }

    pub async fn open_channel(&self, buffer: usize, remove_on_error: bool) -> Receiver<Arc<T>> {
        let (sender, receiver) = channel(buffer);
        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(Subscriber::Channel(sender, remove_on_error));

        receiver
    }

    pub async fn subscribe(
        &self,
        closure: impl Fn(Arc<T>) -> Result<(), BoxedError> + Send + Sync + 'static,
        remove_on_error: bool,
    ) {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(Subscriber::Closure(Box::new(closure), remove_on_error));
    }

    pub async fn dispatch(&self, data: T) -> Result<(), Vec<EventError<T>>> {
        let data = Arc::new(data);

        let mut errors = Vec::new();
        let mut subscribers_to_remove = Vec::new();

        let mut subscribers = self.subscribers.lock().await;
        for (index, subscriber) in subscribers.iter().enumerate() {
            let data = Arc::clone(&data);

            match subscriber {
                Subscriber::Channel(sender, remove_on_error) => {
                    let result = sender.send(data).await;

                    if let Err(err) = result {
                        if self.log_on_error {
                            log::error!(
                                "Event \"{}\" failed to dispatch data to receiver {}: {}.",
                                self.name,
                                index,
                                err
                            );
                        }

                        if *remove_on_error {
                            if self.log_on_error {
                                log::error!("Receiver will be unregistered from event.");
                            }

                            subscribers_to_remove.push(index);
                        }

                        errors.push(EventError::ChannelSend(err));
                    }
                }

                Subscriber::Closure(closure, remove_on_error) => {
                    let result = closure(data);

                    if let Err(err) = result {
                        if self.log_on_error {
                            log::error!(
                                "Event \"{}\" failed to dispatch data to closure {}: {}.",
                                self.name,
                                index,
                                err
                            );
                        }

                        if *remove_on_error {
                            if self.log_on_error {
                                log::error!("Closure will be unregistered from event.");
                            }

                            subscribers_to_remove.push(index);
                        }

                        errors.push(EventError::Closure(err));
                    }
                }
            }
        }

        for index in subscribers_to_remove.into_iter().rev() {
            subscribers.remove(index);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl<T> Default for Event<T> {
    fn default() -> Self {
        Self::new("Unnamed Event", true)
    }
}

impl<T, S> From<S> for Event<T>
where
    S: Into<String>,
{
    fn from(name: S) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
}

impl<T> Debug for Event<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct(type_name::<Self>())
            .field("name", &self.name)
            .field("log_on_error", &self.log_on_error)
            .field("subscribers", &self.subscribers.blocking_lock().len())
            .finish()
    }
}
