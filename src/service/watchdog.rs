use log::error;
use serenity::FutureExt;
use std::{mem::replace, sync::Arc};
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};

use super::PinnedBoxedFuture;

pub struct Watchdog<'a, T: Send> {
    task: PinnedBoxedFuture<'a, T>,
    subscribers: Arc<Mutex<Vec<Sender<Arc<T>>>>>,
}

impl<'a, T: 'a + Send> Watchdog<'a, T> {
    pub fn new(task: PinnedBoxedFuture<'a, T>) -> Self {
        Self {
            task,
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn append<F>(&mut self, new_task: F)
    where
        F: FnOnce(T) -> PinnedBoxedFuture<'a, T> + 'a + Send,
    {
        let previous_task = replace(
            &mut self.task,
            Box::pin(async { unreachable!("Undefined watchdog task") }),
        );

        self.task = Box::pin(previous_task.then(new_task));
    }

    pub async fn subscribe(&self) -> Receiver<Arc<T>> {
        let (tx, rx) = channel(1);
        self.subscribers.lock().await.push(tx);
        rx
    }

    pub async fn run(self) {
        let result = self.task.await;
        let result = Arc::new(result);
        for subscriber in self.subscribers.lock().await.iter() {
            let send_result = subscriber.send(Arc::clone(&result)).await;

            if let Err(e) = send_result {
                error!(
                    "Failed to send watchdog task result to one of the subscribers: {}",
                    e
                );
            }
        }
    }
}
