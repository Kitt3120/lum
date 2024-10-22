use core::mem;
use std::future::Future;

use super::LifetimedPinnedBoxedFuture;

pub struct Taskchain<'a, T: Send + Sync + 'static> {
    task: LifetimedPinnedBoxedFuture<'a, T>,
}

impl<'a, T: Send + Sync + 'static> Taskchain<'a, T> {
    pub fn new(task: LifetimedPinnedBoxedFuture<'a, T>) -> Self {
        Self { task }
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

    pub async fn run(self) -> T {
        self.task.await
    }
}
