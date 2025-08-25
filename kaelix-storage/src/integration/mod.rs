//! Integration layer for connecting storage with other system components
//!
//! Provides bridges and adapters for seamless integration between the storage
//! engine and other components like message brokers, streams, and external systems.
//!
//! # Features
//!
//! - **Broker Bridge**: Seamless integration with message brokers for persistent streaming
//! - **Message Conversion**: Zero-copy conversion between message types
//! - **Transaction Coordination**: Atomic operations across storage and messaging
//! - **Replay Management**: Efficient message replay for new subscribers
//! - **Subscription Tracking**: Track which messages need persistence
//! - **Performance Optimization**: Minimal overhead integration patterns

pub mod broker_bridge;

pub use broker_bridge::{
    BridgeConfig, BridgeMetrics, BrokerBridge, ConversionError, ReplayCursor, 
    SubscriptionInfo, Transaction, TransactionOperation, TransactionStatus,
};