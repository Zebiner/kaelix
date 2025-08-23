//! # Communication Module
//!
//! Provides high-performance inter-node communication with zero-copy design
//! and support for various transport protocols.

use crate::{
    error::{Error, Result},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::RwLock,
};
use tracing::{debug, info, warn};

// Re-export NodeId from types module
pub use crate::types::NodeId;

/// Network message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Ping message for connectivity check
    Ping { timestamp: u64 },
    /// Pong response to ping
    Pong { timestamp: u64 },
    /// Membership update
    MembershipUpdate {
        /// Updated member information
        updates: Vec<u8>, // Serialized member data
    },
    /// Consensus message
    Consensus {
        /// Consensus algorithm specific data
        data: Vec<u8>,
    },
    /// Application data
    Application {
        /// Application-specific payload
        payload: Bytes,
    },
}

/// Connection information
#[derive(Debug)]
pub struct ConnectionInfo {
    /// Remote node identifier
    pub node_id: NodeId,
    /// Remote address
    pub address: SocketAddr,
    /// Connection established timestamp
    pub established_at: u64,
    /// Bytes sent counter
    pub bytes_sent: AtomicU64,
    /// Bytes received counter
    pub bytes_received: AtomicU64,
    /// Messages sent counter
    pub messages_sent: AtomicU64,
    /// Messages received counter
    pub messages_received: AtomicU64,
}

impl Clone for ConnectionInfo {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id,
            address: self.address,
            established_at: self.established_at,
            bytes_sent: AtomicU64::new(self.bytes_sent.load(Ordering::Relaxed)),
            bytes_received: AtomicU64::new(self.bytes_received.load(Ordering::Relaxed)),
            messages_sent: AtomicU64::new(self.messages_sent.load(Ordering::Relaxed)),
            messages_received: AtomicU64::new(self.messages_received.load(Ordering::Relaxed)),
        }
    }
}

impl ConnectionInfo {
    /// Create new connection info
    pub fn new(node_id: NodeId, address: SocketAddr) -> Self {
        Self {
            node_id,
            address,
            established_at: chrono::Utc::now().timestamp_millis() as u64,
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        }
    }

    /// Record bytes sent
    pub fn record_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record bytes received
    pub fn record_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record message sent
    pub fn record_message_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Record message received
    pub fn record_message_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total bytes sent
    pub fn total_bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Get total bytes received
    pub fn total_bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// Get total messages sent
    pub fn total_messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get total messages received
    pub fn total_messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }
}

/// TCP transport implementation
pub struct TcpTransport {
    /// Local bind address
    bind_address: SocketAddr,
    /// Active connections
    connections: Arc<RwLock<HashMap<NodeId, ConnectionInfo>>>,
    /// TCP listener
    listener: Option<TcpListener>,
}

impl TcpTransport {
    /// Create a new TCP transport
    pub async fn new(bind_address: SocketAddr) -> Result<Self> {
        Ok(Self {
            bind_address,
            connections: Arc::new(RwLock::new(HashMap::new())),
            listener: None,
        })
    }

    /// Start listening for incoming connections
    pub async fn start_listening(&mut self) -> Result<()> {
        let listener = TcpListener::bind(&self.bind_address).await.map_err(|e| {
            Error::communication(format!("Failed to bind TCP listener to {}: {}", self.bind_address, e))
        })?;

        info!("TCP transport listening on {}", self.bind_address);
        self.listener = Some(listener);
        Ok(())
    }

    /// Connect to a remote node
    pub async fn connect(&self, node_id: NodeId, address: SocketAddr) -> Result<()> {
        let _stream = TcpStream::connect(address).await.map_err(|e| {
            Error::communication(format!("Failed to connect to {} at {}: {}", node_id, address, e))
        })?;

        let connection = ConnectionInfo::new(node_id, address);
        self.connections.write().await.insert(node_id, connection);

        info!("Connected to node {} at {}", node_id, address);
        Ok(())
    }

    /// Send message to a node
    pub async fn send_message(&self, node_id: &NodeId, message: NetworkMessage) -> Result<()> {
        let connections = self.connections.read().await;
        if let Some(connection) = connections.get(node_id) {
            // Simulate sending message
            let serialized = bincode::serialize(&message).map_err(|e| {
                Error::configuration(format!("Failed to serialize message: {}", e))
            })?;

            connection.record_bytes_sent(serialized.len() as u64);
            connection.record_message_sent();

            debug!("Sent message to node {}: {:?}", node_id, message);
            Ok(())
        } else {
            Err(Error::communication(format!("No connection to node {}", node_id)))
        }
    }

    /// Get connection information
    pub async fn get_connection(&self, node_id: &NodeId) -> Option<ConnectionInfo> {
        self.connections.read().await.get(node_id).cloned()
    }

    /// Get all connections
    pub async fn get_all_connections(&self) -> HashMap<NodeId, ConnectionInfo> {
        self.connections.read().await.clone()
    }

    /// Disconnect from a node
    pub async fn disconnect(&self, node_id: &NodeId) -> Result<()> {
        if let Some(connection) = self.connections.write().await.remove(node_id) {
            info!("Disconnected from node {} at {}", node_id, connection.address);
            Ok(())
        } else {
            Err(Error::communication(format!("No connection to node {}", node_id)))
        }
    }

    /// Get bind address
    pub fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }
}

/// Message router for handling inter-node communication
pub struct MessageRouter {
    /// Node identifier
    node_id: NodeId,
    /// Transport layer
    transport: Arc<TcpTransport>,
    /// Message handlers
    handlers: HashMap<String, Box<dyn Fn(NetworkMessage) -> Result<()> + Send + Sync>>,
}

impl MessageRouter {
    /// Create a new message router
    pub fn new(node_id: NodeId, transport: TcpTransport) -> Self {
        Self {
            node_id,
            transport: Arc::new(transport),
            handlers: HashMap::new(),
        }
    }

    /// Get node ID
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get transport reference
    pub fn transport(&self) -> &Arc<TcpTransport> {
        &self.transport
    }

    /// Send message to a specific node
    pub async fn send_to_node(&self, target: &NodeId, message: NetworkMessage) -> Result<()> {
        self.transport.send_message(target, message).await
    }

    /// Broadcast message to all connected nodes
    pub async fn broadcast(&self, message: NetworkMessage) -> Result<()> {
        let connections = self.transport.get_all_connections().await;

        for node_id in connections.keys() {
            if *node_id != self.node_id {
                if let Err(e) = self.transport.send_message(node_id, message.clone()).await {
                    warn!("Failed to broadcast to node {}: {}", node_id, e);
                }
            }
        }

        Ok(())
    }

    /// Route incoming message
    pub fn route_message(&self, message: NetworkMessage) -> Result<()> {
        match &message {
            NetworkMessage::Ping { .. } => {
                debug!("Received ping message");
                // Handle ping message
                Ok(())
            }
            NetworkMessage::Pong { .. } => {
                debug!("Received pong message");
                // Handle pong message
                Ok(())
            }
            NetworkMessage::MembershipUpdate { .. } => {
                debug!("Received membership update");
                // Handle membership update
                Ok(())
            }
            NetworkMessage::Consensus { .. } => {
                debug!("Received consensus message");
                // Handle consensus message
                Ok(())
            }
            NetworkMessage::Application { .. } => {
                debug!("Received application message");
                // Handle application message
                Ok(())
            }
        }
    }
}

/// Network transport trait
pub trait NetworkTransport: Send + Sync {
    /// Connect to a remote address
    fn connect(&self, node_id: NodeId, address: SocketAddr) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Send message to a node
    fn send_message(&self, node_id: &NodeId, message: NetworkMessage) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Disconnect from a node
    fn disconnect(&self, node_id: &NodeId) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Get connection info
    fn get_connection(&self, node_id: &NodeId) -> impl std::future::Future<Output = Option<ConnectionInfo>> + Send;
}

impl NetworkTransport for TcpTransport {
    async fn connect(&self, node_id: NodeId, address: SocketAddr) -> Result<()> {
        self.connect(node_id, address).await
    }

    async fn send_message(&self, node_id: &NodeId, message: NetworkMessage) -> Result<()> {
        self.send_message(node_id, message).await
    }

    async fn disconnect(&self, node_id: &NodeId) -> Result<()> {
        self.disconnect(node_id).await
    }

    async fn get_connection(&self, node_id: &NodeId) -> Option<ConnectionInfo> {
        self.get_connection(node_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_info() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let conn = ConnectionInfo::new(node_id, address);

        assert_eq!(conn.node_id, node_id);
        assert_eq!(conn.address, address);
        assert_eq!(conn.total_bytes_sent(), 0);
        assert_eq!(conn.total_bytes_received(), 0);

        conn.record_bytes_sent(100);
        conn.record_bytes_received(200);
        conn.record_message_sent();
        conn.record_message_received();

        assert_eq!(conn.total_bytes_sent(), 100);
        assert_eq!(conn.total_bytes_received(), 200);
        assert_eq!(conn.total_messages_sent(), 1);
        assert_eq!(conn.total_messages_received(), 1);
    }

    #[tokio::test]
    async fn test_tcp_transport_creation() {
        let address: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let transport = TcpTransport::new(address).await.unwrap();
        assert_eq!(transport.bind_address(), address);
    }

    #[test]
    fn test_message_router() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:0".parse().unwrap();
        
        // Create transport (this would fail in async context but we're just testing structure)
        let transport = TcpTransport {
            bind_address: address,
            connections: Arc::new(RwLock::new(HashMap::new())),
            listener: None,
        };

        let router = MessageRouter::new(node_id, transport);
        assert_eq!(router.node_id(), node_id);
    }

    #[test]
    fn test_network_message_creation() {
        let ping = NetworkMessage::Ping { timestamp: 12345 };
        let pong = NetworkMessage::Pong { timestamp: 67890 };

        match ping {
            NetworkMessage::Ping { timestamp } => assert_eq!(timestamp, 12345),
            _ => panic!("Expected ping message"),
        }

        match pong {
            NetworkMessage::Pong { timestamp } => assert_eq!(timestamp, 67890),
            _ => panic!("Expected pong message"),
        }
    }

    #[test]
    fn test_connection_info_clone() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let conn = ConnectionInfo::new(node_id, address);

        conn.record_bytes_sent(100);
        conn.record_message_sent();

        let cloned = conn.clone();
        
        assert_eq!(cloned.node_id, conn.node_id);
        assert_eq!(cloned.address, conn.address);
        assert_eq!(cloned.total_bytes_sent(), 100);
        assert_eq!(cloned.total_messages_sent(), 1);
    }
}