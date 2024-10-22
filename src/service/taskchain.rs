use core::mem;
use log::error;
use std::{future::Future, sync::Arc};
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};

use super::LifetimedPinnedBoxedFuture;

//TODO: Use Event<T> instead of manual subscriber handling
pub struct Taskchain<'a, T: Send> {
    task: LifetimedPinnedBoxedFuture<'a, T>,
    subscribers: Arc<Mutex<Vec<Sender<Arc<T>>>>>,
}

impl<'a, T: 'a + Send> Taskchain<'a, T> {
    pub fn new(task: LifetimedPinnedBoxedFuture<'a, T>) -> Self {
        Self {
            task,
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn append<FN, FUT>(&mut self, task: FN)
    where
        FN: FnOnce(T) -> FUT + Send + Sync + 'a,
        FUT: Future<Output = T> + Send + Sync + 'a,
    {
        let previous_task = mem::replace(
            &mut self.task,
            Box::pin(async { unreachable!("Undefined Taskchain task") }),
        );

        let task = async move {
            let result = previous_task.await;
            task(result).await
        };

        self.task = Box::pin(task);
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
                    "Failed to send a Taskchain task result to one of its subscribers: {}",
                    e
                );
            }
        }
    }
}
