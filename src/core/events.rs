use tokio::sync::broadcast;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationType {
    Created,
    Updated,
    Deleted,
    Transitioned,
}

#[derive(Clone, Debug)]
pub struct ContentEvent {
    pub content_type: String,
    pub id: String,
    pub mutation_type: MutationType,
    /// Unfiltered snapshot of the item (as produced by `content_item_to_json`); `None` only for `Deleted`.
    pub item: Option<serde_json::Value>,
}

/// Broadcasts content mutation events to any number of GraphQL subscribers.
pub struct EventBus {
    tx: broadcast::Sender<ContentEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, event: ContentEvent) {
        // No receivers is not an error - it just means nobody is subscribed right now.
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ContentEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
