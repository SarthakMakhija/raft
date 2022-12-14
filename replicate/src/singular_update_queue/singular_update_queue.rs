use std::future::Future;
use std::pin::Pin;

use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::mpsc::error::SendError;
use tokio::task::JoinHandle;

pub(crate) struct Task {
    block: Pin<Box<dyn Future<Output=()> + Send>>,
}

pub(crate) struct SingularUpdateQueue {
    sender: Sender<Task>,
    single_thread_pool: Runtime,
    task_submission_pool: Runtime,
}

impl SingularUpdateQueue {
    pub(crate) fn new() -> SingularUpdateQueue {
        let single_thread_pool = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();

        //TODO: make 100 configurable
        let (sender, receiver) = mpsc::channel::<Task>(100);
        Self::spin(&single_thread_pool, receiver);

        return SingularUpdateQueue {
            sender,
            single_thread_pool,
            task_submission_pool: Builder::new_multi_thread()
                .worker_threads(10)//TODO: make 10 configurable
                .enable_all()
                .build()
                .unwrap(),
        };
    }

    pub(crate) async fn add_async<F>(&self, handler: F) -> Result<(), SendError<Task>>
        where
            F: Future<Output=()> + Send + 'static {
        let block = Box::pin(handler);
        return self.sender.clone().send(Task { block }).await;
    }

    pub(crate) fn add_spawn<F>(&self, handler: F) -> JoinHandle<Result<(), SendError<Task>>>
        where
            F: Future<Output=()> + Send + 'static {
        let block = Box::pin(handler);
        let sender = self.sender.clone();

        return self.task_submission_pool.spawn(async move {
            return sender.send(Task { block }).await;
        });
    }

    pub(crate) fn shutdown(self) {
        let _ = self.single_thread_pool.shutdown_background();
        let _ = self.task_submission_pool.shutdown_background();
    }

    fn spin(thread_pool: &Runtime, mut receiver: Receiver<Task>) {
        thread_pool.spawn(async move {
            while let Some(task) = receiver.recv().await {
                task.block.await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use tokio::sync::mpsc;

    use crate::singular_update_queue::singular_update_queue::SingularUpdateQueue;

    #[tokio::test]
    async fn get_with_insert_by_a_single_task() {
        let storage = Arc::new(RwLock::new(HashMap::new()));
        let readable_storage = storage.clone();
        let singular_update_queue = SingularUpdateQueue::new();

        let (sender, mut receiver) = mpsc::channel(1);
        let _ = singular_update_queue.add_async(async move {
            storage.write().unwrap().insert("WAL".to_string(), "write-ahead log".to_string());
            sender.send(()).await.unwrap();
        }).await;

        let _ = receiver.recv().await.unwrap();
        let read_storage = readable_storage.read().unwrap();

        assert_eq!("write-ahead log", read_storage.get("WAL").unwrap());

        singular_update_queue.shutdown();
    }

    #[tokio::test]
    async fn get_with_insert_by_multiple_tasks() {
        let storage = Arc::new(RwLock::new(HashMap::new()));
        let writable_storage = storage.clone();
        let readable_storage = storage.clone();
        let singular_update_queue = SingularUpdateQueue::new();

        let (sender_one, mut receiver_one) = mpsc::channel(1);
        let (sender_other, mut receiver_other) = mpsc::channel(1);

        let _ = singular_update_queue.add_async(async move {
            writable_storage.write().unwrap().insert("WAL".to_string(), "write-ahead log".to_string());
            sender_one.clone().send(()).await.unwrap();
        }).await;

        let _ = singular_update_queue.add_async(async move {
            storage.write().unwrap().insert("RAFT".to_string(), "consensus".to_string());
            sender_other.clone().send(()).await.unwrap();
        }).await;

        let _ = receiver_one.recv().await.unwrap();
        let _ = receiver_other.recv().await.unwrap();

        let read_storage = readable_storage.read().unwrap();

        assert_eq!("write-ahead log", read_storage.get("WAL").unwrap());
        assert_eq!("consensus", read_storage.get("RAFT").unwrap());

        singular_update_queue.shutdown();
    }

    #[tokio::test]
    async fn add_single_task() {
        let storage = Arc::new(RwLock::new(HashMap::new()));
        let singular_update_queue = SingularUpdateQueue::new();

        let (sender_one, mut receiver_one) = mpsc::channel(1);
        let _ = singular_update_queue.add_spawn(async move {
            storage.write().unwrap().insert("WAL".to_string(), "write-ahead log".to_string());
            let _ = sender_one.send(("WAL".to_string(), "write-ahead log".to_string())).await;
        });

        let (key, value) = receiver_one.recv().await.unwrap();
        assert_eq!("WAL", key);
        assert_eq!("write-ahead log", value);

        singular_update_queue.shutdown();
    }

    #[tokio::test]
    async fn submit_multiple_task_and_confirm_their_execution_in_order() {
        let storage = Arc::new(RwLock::new(Vec::new()));
        let writable_storage_one = storage.clone();
        let writable_storage_two = storage.clone();
        let readable_storage = storage.clone();
        let singular_update_queue = SingularUpdateQueue::new();

        let (sender_one, mut receiver_one) = mpsc::channel(1);
        let (sender_other, mut receiver_other) = mpsc::channel(1);

        let _ = singular_update_queue.add_async(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            writable_storage_one.write().unwrap().push("WAL".to_string());
            sender_one.clone().send(()).await.unwrap();
        }).await;

        let _ = singular_update_queue.add_async(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            writable_storage_two.write().unwrap().push("consensus".to_string());
            sender_other.clone().send(()).await.unwrap();
        }).await;

        let _ = receiver_one.recv().await.unwrap();
        let _ = receiver_other.recv().await.unwrap();

        let read_storage = readable_storage.read().unwrap();
        assert_eq!(vec!["WAL".to_string(), "consensus".to_string()], *read_storage);

        singular_update_queue.shutdown();
    }
}
