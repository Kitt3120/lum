use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinHandle};

use super::Event;

pub struct EventRepeater<T>
where
    T: Send + Sync + 'static,
{
    pub event: Event<T>,
    self_arc: Mutex<Option<Arc<Self>>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
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
            self_arc: Mutex::new(None),
            event,
            tasks: Mutex::new(Vec::new()),
        };

        let self_arc = Arc::new(event_repeater);
        let mut lock = self_arc.self_arc.lock().await;
        let self_arc_clone = Arc::clone(&self_arc);
        *lock = Some(self_arc_clone);
        drop(lock);

        self_arc
    }

    pub async fn attach(&self, event: &Event<T>, buffer: usize) {
        let self_arc = match self.self_arc.lock().await.as_ref() {
            Some(arc) => Arc::clone(arc),
            None => panic!("Tried to attach event {} to EventRepeater {} before it was initialized. Did you not use EventRepeater<T>::new()?", event.name, self.event.name),
        };

        let mut receiver = event.open_channel(&self.event.name, buffer, true, true).await;
        let join_handle = tokio::spawn(async move {
            while let Some(value) = receiver.receiver.recv().await {
                let _ = self_arc.event.dispatch(value).await;
            }
        });

        let mut tasks = self.tasks.lock().await;
        tasks.push(join_handle);
    }
}
