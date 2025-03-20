pub mod stream {
    use futures::channel::mpsc::Sender;
    use futures::SinkExt;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tonic::Status;

    // Type alias for our gRPC stream sender
    type MessageSender<T> = Sender<Result<T, Status>>;

    // Stream manager to store and handle streams by ID
    #[derive(Debug, Clone)]
    pub struct StreamManager<T: Clone + Send + 'static + prost::Message> {
        streams: Arc<Mutex<HashMap<String, MessageSender<T>>>>,
    }

    impl<T: Clone + Send + 'static + prost::Message> StreamManager<T> {
        // Create a new stream manager
        pub fn new() -> Self {
            Self {
                streams: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        // Store an existing stream with the given ID
        pub async fn store_stream(&self, id: String, sender: MessageSender<T>) {
            let mut streams = self.streams.lock().await;
            streams.insert(id, sender);
        }

        // Remove a stream by ID
        pub async fn remove_stream(&self, id: &str) -> bool {
            let mut streams = self.streams.lock().await;
            streams.remove(id).is_some()
        }

        // Send a message to a specific stream
        pub async fn send_message(&self, id: &str, message: T) -> Result<(), Status> {
            let streams = self.streams.lock().await;

            if let Some(sender) = streams.get(id) {
                sender.clone().send(Ok(message)).await.map_err(|_| {
                    Status::internal(format!("Failed to send message to stream {}", id))
                })
            } else {
                Err(Status::not_found(format!(
                    "Stream with ID {} not found",
                    id
                )))
            }
        }

        // Check if a stream exists
        pub async fn has_stream(&self, id: &str) -> bool {
            let streams = self.streams.lock().await;
            streams.contains_key(id)
        }

        // Get the number of active streams
        pub async fn count(&self) -> usize {
            let streams = self.streams.lock().await;
            streams.len()
        }

        // Get a stream by ID
        pub async fn get_stream(&self, id: &str) -> Option<MessageSender<T>> {
            let streams = self.streams.lock().await;
            streams.get(id).cloned()
        }
    }

    // Default implementation to make it easier to create a new instance
    impl<T: Clone + Send + 'static + prost::Message> Default for StreamManager<T> {
        fn default() -> Self {
            Self::new()
        }
    }
}
