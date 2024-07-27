use crate::service::BoxedError;
use std::{
    any::type_name,
    fmt::{self, Debug, Formatter},
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::{
    mpsc::{channel, error::SendError, Receiver},
    Mutex,
};

use super::{
    subscriber::{ReceiverSubscription, Subscription},
    Callback, Subscriber,
};

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

    pub async fn open_channel<S>(
        &self,
        name: S,
        buffer: usize,
        remove_on_error: bool,
    ) -> ReceiverSubscription<Arc<T>>
    where
        S: Into<String>,
    {
        let (sender, receiver) = channel(buffer);
        let subscriber = Subscriber::new(name, remove_on_error, Callback::Channel(sender));

        let subscription = Subscription::from(&subscriber);
        let receiver_subscription = ReceiverSubscription::new(subscription, receiver);

        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(subscriber);

        receiver_subscription
    }

    pub async fn register_callback<S>(
        &self,
        name: S,
        closure: impl Fn(Arc<T>) -> Result<(), BoxedError> + Send + Sync + 'static,
        remove_on_error: bool,
    ) -> Subscription
    where
        S: Into<String>,
    {
        let subscriber = Subscriber::new(name, remove_on_error, Callback::Closure(Box::new(closure)));
        let subscription = Subscription::from(&subscriber);

        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(subscriber);

        subscription
    }

    pub async fn dispatch(&self, data: T) -> Result<(), Vec<EventError<T>>> {
        let data = Arc::new(data);

        let mut errors = Vec::new();
        let mut subscribers_to_remove = Vec::new();

        let mut subscribers = self.subscribers.lock().await;
        for (index, subscriber) in subscribers.iter().enumerate() {
            let data = Arc::clone(&data);

            match &subscriber.callback {
                Callback::Channel(sender) => {
                    let result = sender.send(data).await;

                    if let Err(err) = result {
                        if self.log_on_error {
                            log::error!(
                                "Event \"{}\" failed to dispatch data to Channel callback of subscriber {}: {}.",
                                self.name,
                                subscriber.name,
                                err
                            );
                        }

                        if subscriber.remove_on_error {
                            if self.log_on_error {
                                log::error!("Subscriber will be unregistered from event.");
                            }

                            subscribers_to_remove.push(index);
                        }

                        errors.push(EventError::ChannelSend(err));
                    }
                }

                Callback::Closure(closure) => {
                    let result = closure(data);

                    if let Err(err) = result {
                        if self.log_on_error {
                            log::error!(
                                "Event \"{}\" failed to dispatch data to Closure callback of subscriber {}: {}.",
                                self.name,
                                subscriber.name,
                                err
                            );
                        }

                        if subscriber.remove_on_error {
                            if self.log_on_error {
                                log::error!("Subscriber will be unregistered from event.");
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
