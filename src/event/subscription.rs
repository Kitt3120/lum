use tokio::sync::mpsc::Receiver;
use uuid::Uuid;

use super::Subscriber;

#[derive(Debug, PartialEq, Eq)]
pub struct Subscription {
    pub uuid: Uuid,
}

impl<T> From<Subscriber<T>> for Subscription
where
    T: Send + Sync + 'static,
{
    fn from(subscriber: Subscriber<T>) -> Self {
        Self {
            uuid: subscriber.uuid,
        }
    }
}

impl<T> From<&Subscriber<T>> for Subscription
where
    T: Send + Sync + 'static,
{
    fn from(subscriber: &Subscriber<T>) -> Self {
        Self {
            uuid: subscriber.uuid,
        }
    }
}

pub struct ReceiverSubscription<T>
where
    T: Send + Sync + 'static,
{
    pub subscription: Subscription,
    pub receiver: Receiver<T>,
}

impl<T> ReceiverSubscription<T>
where
    T: Send + Sync + 'static,
{
    pub fn new(subscription: Subscription, receiver: Receiver<T>) -> Self {
        Self {
            subscription,
            receiver,
        }
    }
}

impl<T> PartialEq for ReceiverSubscription<T>
where
    T: Send + Sync + 'static,
{
    fn eq(&self, other: &Self) -> bool {
        self.subscription == other.subscription
    }
}

impl<T> Eq for ReceiverSubscription<T> where T: Send + Sync {}

impl<T> AsRef<Subscription> for ReceiverSubscription<T>
where
    T: Send + Sync + 'static,
{
    fn as_ref(&self) -> &Subscription {
        &self.subscription
    }
}
