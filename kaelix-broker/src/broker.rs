//! # Message Broker
//!
//! The broker module implements the core message brokering functionality,
//! handling message routing, storage, and delivery coordination.

use dashmap::DashMap;
use kaelix_core::{Message, Result, Topic};
use parking_lot::RwLock;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Broker state information
#[derive(Debug)]
pub struct BrokerState {
    /// Whether the broker is currently running
    pub running: bool,
    /// Total number of messages processed
    pub messages_processed: u64,
    /// Number of active subscriptions
    pub active_subscriptions: usize,
}

/// Core message broker implementation
#[derive(Debug)]
pub struct MessageBroker {
    /// Current broker state
    state: Arc<RwLock<BrokerState>>,
    /// Topic to subscription mapping
    subscriptions: Arc<DashMap<Topic, Vec<broadcast::Sender<Message>>>>,
    /// Message processing metrics
    message_counter: AtomicU64,
}

impl MessageBroker {
    /// Create a new message broker instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(BrokerState {
                running: false,
                messages_processed: 0,
                active_subscriptions: 0,
            })),
            subscriptions: Arc::new(DashMap::new()),
            message_counter: AtomicU64::new(0),
        }
    }

    /// Start the broker and begin processing messages
    /// # Errors
    /// Returns an error if broker is already running or cannot start
    pub fn start(&self) -> Result<()> {
        info!("Starting message broker");
        let mut state = self.state.write();
        if state.running {
            return Err(kaelix_core::Error::Internal("Broker is already running".to_string()));
        }

        state.running = true;
        drop(state);

        info!("Message broker started successfully");
        Ok(())
    }

    /// Stop the broker gracefully
    /// # Errors
    /// Returns an error if broker is not running or cannot stop
    #[allow(clippy::cognitive_complexity)]
    pub fn stop(&self) -> Result<()> {
        info!("Stopping message broker");
        let mut state = self.state.write();
        if !state.running {
            warn!("Broker is not running");
            return Ok(());
        }

        state.running = false;
        drop(state);

        // Clean up resources
        self.subscriptions.clear();

        info!("Message broker stopped successfully");
        Ok(())
    }

    /// Publish a message to a topic
    /// # Errors
    /// Returns an error if broker is not running or publishing fails
    #[allow(clippy::cognitive_complexity)]
    pub fn publish(&self, topic: &Topic, message: &Message) -> Result<()> {
        let state = self.state.read();
        if !state.running {
            return Err(kaelix_core::Error::Internal("Broker is not running".to_string()));
        }
        drop(state);

        debug!(topic = %topic, "Publishing message");

        if let Some(senders) = self.subscriptions.get(topic) {
            for sender in senders.value() {
                if let Err(e) = sender.send(message.clone()) {
                    warn!(error = %e, "Failed to send message to subscriber");
                }
            }
        }

        // Update metrics
        self.message_counter.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Subscribe to a topic and receive messages
    /// # Errors
    /// Returns an error if broker is not running or subscription fails
    pub fn subscribe(&self, topic: &Topic) -> Result<broadcast::Receiver<Message>> {
        let state = self.state.read();
        if !state.running {
            return Err(kaelix_core::Error::Internal("Broker is not running".to_string()));
        }
        drop(state);

        debug!(topic = %topic, "Creating subscription");

        let (sender, receiver) = broadcast::channel(1000);

        self.subscriptions.entry(topic.clone()).or_default().push(sender);

        Ok(receiver)
    }

    /// Get the current broker state
    #[must_use]
    pub fn state(&self) -> BrokerState {
        let state = self.state.read();
        let message_count = self.message_counter.load(Ordering::Relaxed);
        let subscription_count = self.subscriptions.len();

        BrokerState {
            running: state.running,
            messages_processed: message_count,
            active_subscriptions: subscription_count,
        }
    }

    /// Get total number of messages processed
    #[must_use]
    pub fn messages_processed(&self) -> u64 {
        self.message_counter.load(Ordering::Relaxed)
    }

    /// Get number of active subscriptions
    #[must_use]
    pub fn active_subscriptions(&self) -> usize {
        self.subscriptions.len()
    }
}

impl Default for MessageBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaelix_core::Topic;
    use bytes::Bytes;

    #[tokio::test]
    async fn test_broker_lifecycle() {
        let broker = MessageBroker::new();

        assert!(!broker.state().running);

        broker.start().expect("Failed to start broker");
        assert!(broker.state().running);

        broker.stop().expect("Failed to stop broker");
        assert!(!broker.state().running);
    }

    #[tokio::test]
    async fn test_publish_subscribe() {
        let broker = MessageBroker::new();
        broker.start().expect("Failed to start broker");

        let topic = Topic::new("test.topic").expect("Failed to create topic");
        let mut receiver = broker.subscribe(&topic).expect("Failed to subscribe");

        let message = Message::new("test.topic", Bytes::from("test message")).expect("Failed to create message");
        broker.publish(&topic, &message).expect("Failed to publish");

        let received = receiver.recv().await.expect("Failed to receive message");
        assert_eq!(received.payload, message.payload);

        broker.stop().expect("Failed to stop broker");
    }
}