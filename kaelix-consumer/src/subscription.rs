//! Subscription management for consumers.

use kaelix_core::Topic;

/// Subscription manager for handling topic subscriptions.
#[derive(Debug)]
pub struct SubscriptionManager {
    subscribed_topics: Vec<Topic>,
}

impl SubscriptionManager {
    /// Create a new subscription manager.
    #[must_use]
    pub const fn new() -> Self {
        Self { subscribed_topics: Vec::new() }
    }

    /// Subscribe to a topic.
    pub fn subscribe(&mut self, topic: Topic) {
        if !self.subscribed_topics.contains(&topic) {
            self.subscribed_topics.push(topic);
        }
    }

    /// Unsubscribe from a topic.
    pub fn unsubscribe(&mut self, topic: &Topic) {
        self.subscribed_topics.retain(|t| t != topic);
    }

    /// Get all subscribed topics.
    #[must_use]
    pub fn subscribed_topics(&self) -> &[Topic] {
        &self.subscribed_topics
    }

    /// Check if subscribed to a topic.
    #[must_use]
    pub fn is_subscribed(&self, topic: &Topic) -> bool {
        self.subscribed_topics.contains(topic)
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}
