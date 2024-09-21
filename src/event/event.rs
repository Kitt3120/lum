use crate::service::{BoxedError, PinnedBoxedFutureResult};
use std::{
    any::type_name,
    fmt::{self, Debug, Formatter},
    sync::Arc,
};
use tokio::sync::{mpsc::channel, Mutex};

use super::{
    subscriber::{ReceiverSubscription, Subscription},
    Callback, DispatchError, Subscriber,
};

pub struct Event<T>
where
    T: Send + Sync + 'static,
{
    pub name: String,

    subscribers: Mutex<Vec<Subscriber<T>>>,
}

impl<T> Event<T>
where
    T: Send + Sync + 'static,
{
    pub fn new<S>(name: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            name: name.into(),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub async fn subscriber_count(&self) -> usize {
        let subscribers = self.subscribers.lock().await;
        subscribers.len()
    }

    pub async fn subscribe_channel<S>(
        &self,
        name: S,
        buffer: usize,
        log_on_error: bool,
        remove_on_error: bool,
    ) -> ReceiverSubscription<Arc<T>>
    where
        S: Into<String>,
    {
        let (sender, receiver) = channel(buffer);
        let subscriber = Subscriber::new(name, log_on_error, remove_on_error, Callback::Channel(sender));

        let subscription = Subscription::from(&subscriber);
        let receiver_subscription = ReceiverSubscription::new(subscription, receiver);

        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(subscriber);

        receiver_subscription
    }

    pub async fn subscribe_async_closure<S>(
        &self,
        name: S,
        closure: impl Fn(Arc<T>) -> PinnedBoxedFutureResult<()> + Send + Sync + 'static,
        log_on_error: bool,
        remove_on_error: bool,
    ) -> Subscription
    where
        S: Into<String>,
    {
        let subscriber = Subscriber::new(
            name,
            log_on_error,
            remove_on_error,
            Callback::AsyncClosure(Box::new(closure)),
        );
        let subscription = Subscription::from(&subscriber);

        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(subscriber);

        subscription
    }

    pub async fn subscribe_closure<S>(
        &self,
        name: S,
        closure: impl Fn(Arc<T>) -> Result<(), BoxedError> + Send + Sync + 'static,
        log_on_error: bool,
        remove_on_error: bool,
    ) -> Subscription
    where
        S: Into<String>,
    {
        let subscriber = Subscriber::new(
            name,
            log_on_error,
            remove_on_error,
            Callback::Closure(Box::new(closure)),
        );
        let subscription = Subscription::from(&subscriber);

        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(subscriber);

        subscription
    }

    pub async fn dispatch(&self, data: Arc<T>) -> Result<(), Vec<DispatchError<T>>> {
        let mut errors = Vec::new();
        let mut subscribers_to_remove = Vec::new();

        let mut subscribers = self.subscribers.lock().await;
        for (index, subscriber) in subscribers.iter().enumerate() {
            let data = Arc::clone(&data);

            let result = subscriber.dispatch(data).await;
            if let Err(err) = result {
                if subscriber.log_on_error {
                    log::error!(
                        "Event \"{}\" failed to dispatch data to subscriber {}: {}.",
                        self.name,
                        subscriber.name,
                        err
                    );
                }

                if subscriber.remove_on_error {
                    if subscriber.log_on_error {
                        log::error!("Subscriber will be unregistered from event.");
                    }

                    subscribers_to_remove.push(index);
                }

                errors.push(err);
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

impl<T> Debug for Event<T>
where
    T: Send + Sync + 'static,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct(type_name::<Self>())
            .field("name", &self.name)
            .field("subscribers", &self.subscribers.blocking_lock().len())
            .finish()
    }
}
