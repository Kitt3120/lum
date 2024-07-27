use std::sync::Arc;

use tokio::sync::mpsc::{Receiver, Sender};
use uuid::Uuid;

use crate::service::BoxedError;

pub enum Callback<T> {
    Channel(Sender<Arc<T>>),
    Closure(Box<dyn Fn(Arc<T>) -> Result<(), BoxedError> + Send + Sync>),
}

pub struct Subscriber<T> {
    pub name: String,
    pub remove_on_error: bool,
    pub callback: Callback<T>,

    pub uuid: Uuid,
}

impl<T> Subscriber<T> {
    pub fn new<S>(name: S, remove_on_error: bool, callback: Callback<T>) -> Self
    where
        S: Into<String>,
    {
        Self {
            name: name.into(),
            remove_on_error,
            callback,
            uuid: Uuid::new_v4(),
        }
    }
}

impl<T> PartialEq for Subscriber<T> {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl<T> Eq for Subscriber<T> {}

#[derive(Debug, PartialEq, Eq)]
pub struct Subscription {
    pub uuid: Uuid,
}

impl<T> From<Subscriber<T>> for Subscription {
    fn from(subscriber: Subscriber<T>) -> Self {
        Self {
            uuid: subscriber.uuid,
        }
    }
}

impl<T> From<&Subscriber<T>> for Subscription {
    fn from(subscriber: &Subscriber<T>) -> Self {
        Self {
            uuid: subscriber.uuid,
        }
    }
}

impl AsRef<Uuid> for Subscription {
    fn as_ref(&self) -> &Uuid {
        &self.uuid
    }
}

pub struct ReceiverSubscription<T> {
    pub subscription: Subscription,
    pub receiver: Receiver<T>,
}

impl<T> ReceiverSubscription<T> {
    pub fn new(subscription: Subscription, receiver: Receiver<T>) -> Self {
        Self {
            subscription,
            receiver,
        }
    }
}

impl<T> PartialEq for ReceiverSubscription<T> {
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

impl<T> Eq for ReceiverSubscription<T> {}

impl<T> AsRef<Subscription> for ReceiverSubscription<T> {
    fn as_ref(&self) -> &Subscription {
        &self.subscription
    }
}

impl<T> AsRef<Uuid> for ReceiverSubscription<T> {
    fn as_ref(&self) -> &Uuid {
        self.subscription.as_ref()
    }
}
