use std::sync::Arc;

use tokio::{spawn, task::JoinHandle};

use crate::service::{BoxedError, PinnedBoxedFutureResult};

use super::{
    subscriber::{ReceiverSubscription, Subscription},
    Event,
};

pub struct EventRepeater<T> {
    pub name: String,

    channel_task: JoinHandle<()>,

    subscription_channel: ReceiverSubscription<Arc<T>>,
    subscription_async_closure: Subscription,
    subscription_closure: Subscription,
}

impl<T> EventRepeater<T> {
    pub async fn new<S>(name: S, event: &Event<T>, buffer: usize) -> Self
    where
        T: 'static,
        S: Into<String>,
    {
        let name = name.into();

        let subscription_channel = event.open_channel(name.clone(), buffer, false).await;

        let subscription_async_closure = event.subscribe_async(name.clone(), async_closure, false).await;
        let subscription_closure = event.subscribe(name.clone(), closure, false).await;

        let channel_task = spawn(run_channel_task());

        Self {
            name,

            channel_task,

            subscription_channel,
            subscription_async_closure,
            subscription_closure,
        }
    }
}

async fn run_channel_task() {}

fn async_closure<T>(data: Arc<T>) -> PinnedBoxedFutureResult<()> {
    Box::pin(async move { Ok(()) })
}

fn closure<T>(data: Arc<T>) -> Result<(), BoxedError> {
    Ok(())
}
