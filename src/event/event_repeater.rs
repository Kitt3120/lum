use log::error;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, Weak},
};
use thiserror::Error;
use tokio::{sync::Mutex, task::JoinHandle};
use uuid::Uuid;

use super::{Event, Subscription};

#[derive(Debug, Error)]
pub enum AttachError {
    #[error("Tried to attach event {event_name} to EventRepeater {repeater_name} while it was uninitialized. Did you not use EventRepeater<T>::new()?")]
    NotInitialized {
        event_name: String,
        repeater_name: String,
    },

    #[error(
        "Tried to attach event {event_name} to EventRepeater {repeater_name}, which was already attached to it."
    )]
    AlreadyAttached {
        event_name: String,
        repeater_name: String,
    },
}

#[derive(Debug, Error)]
pub enum DetachError {
    #[error(
        "Tried to detach event {event_name} from EventRepeater {repeater_name}, which was not attached."
    )]
    NotAttached {
        event_name: String,
        repeater_name: String,
    },
}

#[derive(Error)]
pub enum CloseError<T>
where
    T: Send + Sync + 'static,
{
    #[error("EventRepeater still has attached events. Detach all events before closing.")]
    AttachedEvents(EventRepeater<T>),
}

pub struct EventRepeater<T>
where
    T: Send + Sync + 'static,
{
    pub event: Event<T>,
    weak: OnceLock<Weak<Self>>,
    subscriptions: Mutex<HashMap<Uuid, (Subscription, JoinHandle<()>)>>,
}

impl<T> EventRepeater<T>
where
    T: Send + Sync + 'static,
{
    pub async fn new<S>(name: S) -> Arc<Self>
    where
        T: 'static,
        S: Into<String>,
    {
        let event = Event::new(name);
        let event_repeater = Self {
            weak: OnceLock::new(),
            event,
            subscriptions: Mutex::new(HashMap::new()),
        };

        let arc = Arc::new(event_repeater);
        let weak = Arc::downgrade(&arc);

        let result = arc.weak.set(weak);
        if result.is_err() {
            error!("Failed to set EventRepeater {}'s Weak self-reference because it was already set. This should never happen. Shutting down ungracefully to prevent further undefined behavior.", arc.event.name);
            unreachable!(
                "Unable to set EventRepeater {}'s Weak self-reference because it was already set.",
                arc.event.name
            );
        }

        arc
    }

    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.lock().await.len()
    }

    pub async fn attach(&self, event: &Event<T>, buffer: usize) -> Result<(), AttachError> {
        let weak = match self.weak.get() {
            Some(weak) => weak,
            None => {
                return Err(AttachError::NotInitialized {
                    event_name: event.name.clone(),
                    repeater_name: self.event.name.clone(),
                })
            }
        };

        // This can't fail because the Arc is guaranteed to be valid as long as &self is valid.
        let arc = match weak.upgrade() {
            Some(arc) => arc,
            None => {
                error!("EventRepeater {}'s Weak self-reference could not be upgraded to Arc while attaching event {}. This should never happen. Shutting down ungracefully to prevent further undefined behavior.", self.event.name, event.name);
                unreachable!("EventRepeater {}'s Weak self-reference could not be upgraded to Arc while attaching event {}.", self.event.name, event.name);
            }
        };

        let mut subscriptions = self.subscriptions.lock().await;
        if subscriptions.contains_key(&event.uuid) {
            return Err(AttachError::AlreadyAttached {
                event_name: event.name.clone(),
                repeater_name: self.event.name.clone(),
            });
        }

        let receiver_subscription = event
            .subscribe_channel(&self.event.name, buffer, true, true)
            .await;

        let subscription = receiver_subscription.subscription;
        let mut receiver = receiver_subscription.receiver;

        let join_handle = tokio::spawn(async move {
            while let Some(value) = receiver.recv().await {
                let _ = arc.event.dispatch(value).await;
            }
        });
        subscriptions.insert(event.uuid, (subscription, join_handle));

        Ok(())
    }

    pub async fn detach(&self, event: &Event<T>) -> Result<(), DetachError> {
        let mut subscriptions = self.subscriptions.lock().await;

        let subscription = match subscriptions.remove(&event.uuid) {
            Some(subscription) => subscription,
            None => {
                return Err(DetachError::NotAttached {
                    event_name: event.name.clone(),
                    repeater_name: self.event.name.clone(),
                })
            }
        };
        subscription.1.abort();

        Ok(())
    }

    pub async fn close(self) -> Result<(), CloseError<T>> {
        let subscription_count = self.subscription_count().await;

        if subscription_count > 0 {
            return Err(CloseError::AttachedEvents(self));
        }

        Ok(())
    }
}

impl<T> AsRef<Event<T>> for EventRepeater<T>
where
    T: Send + Sync + 'static,
{
    fn as_ref(&self) -> &Event<T> {
        &self.event
    }
}
