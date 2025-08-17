//! Network handling for client connections.
//!
//! This module provides the TCP server foundation for `MemoryStreamer`, designed to handle
//! 10K+ concurrent connections with ultra-low latency (<1ms connection acceptance).
//!
//! ## Key Components
//! - [`TcpServer`] - Main TCP server with async connection handling
//! - [`Connection`] - Individual client connection management
//! - [`TcpListener`] - Async listener with graceful shutdown capabilities
//!
//! ## Performance Characteristics
//! - Connection acceptance latency: <1ms
//! - Concurrent connections: 10K+
//! - Memory efficient connection pooling
//! - Zero unsafe code for maximum safety
//!
//! ## Examples
//!
//! ```rust
//! use kaelix_broker::network::TcpServer;
//! use kaelix_broker::config::NetworkConfig;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = NetworkConfig::default();
//! let mut server = TcpServer::new(config)?;
//! let handle = server.start().await?;
//! handle.shutdown()?;
//! # Ok(())
//! # }
//! ```

pub mod connection;
pub mod listener;
pub mod server;

pub use connection::{
    Connection, ConnectionId, ConnectionManager, ConnectionState, ConnectionStats,
};
pub use listener::TcpListener;
pub use server::{TcpServer, TcpServerBuilder};

use kaelix_core::Result;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Network server handle for graceful shutdown and management.
#[derive(Debug)]
pub struct NetworkHandle {
    shutdown_tx: oneshot::Sender<()>,
}

impl NetworkHandle {
    /// Create a new network handle.
    #[must_use]
    pub const fn new(shutdown_tx: oneshot::Sender<()>) -> Self {
        Self { shutdown_tx }
    }

    /// Initiate graceful shutdown of the network server.
    ///
    /// # Errors
    /// Returns an error if the shutdown signal cannot be sent.
    pub fn shutdown(self) -> Result<()> {
        self.shutdown_tx.send(()).map_err(|()| {
            kaelix_core::Error::NetworkError("Failed to send shutdown signal".into())
        })?;
        Ok(())
    }
}

/// Connection pool for managing active client connections.
#[derive(Debug)]
pub struct ConnectionPool {
    connections: Arc<dashmap::DashMap<ConnectionId, Connection>>,
    max_connections: usize,
}

impl ConnectionPool {
    /// Create a new connection pool with specified capacity.
    #[must_use]
    pub fn new(max_connections: usize) -> Self {
        Self { connections: Arc::new(dashmap::DashMap::new()), max_connections }
    }

    /// Add a new connection to the pool.
    ///
    /// # Errors
    /// Returns an error if the pool is at capacity.
    pub fn add_connection(&self, connection: Connection) -> Result<()> {
        if self.connections.len() >= self.max_connections {
            return Err(kaelix_core::Error::NetworkError("Connection pool at capacity".into()));
        }

        let id = connection.id();
        self.connections.insert(id, connection);
        Ok(())
    }

    /// Remove a connection from the pool.
    #[must_use]
    pub fn remove_connection(&self, id: ConnectionId) -> Option<Connection> {
        self.connections.remove(&id).map(|(_, conn)| conn)
    }

    /// Get the current number of active connections.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Check if the pool has capacity for new connections.
    #[must_use]
    pub fn has_capacity(&self) -> bool {
        self.connections.len() < self.max_connections
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn test_connection_pool_creation() {
        let pool = ConnectionPool::new(100);
        assert_eq!(pool.connection_count(), 0);
        assert!(pool.has_capacity());
    }

    #[test]
    fn test_connection_pool_capacity() {
        let pool = ConnectionPool::new(1);

        // Create a dummy connection for testing
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let connection = Connection::new(ConnectionId::new(), addr);

        // Add connection should succeed
        assert!(pool.add_connection(connection).is_ok());
        assert_eq!(pool.connection_count(), 1);
        assert!(!pool.has_capacity());

        // Adding another should fail
        let addr2: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let connection2 = Connection::new(ConnectionId::new(), addr2);
        assert!(pool.add_connection(connection2).is_err());
    }

    #[tokio::test]
    async fn test_network_handle_shutdown() {
        let (tx, rx) = oneshot::channel();
        let handle = NetworkHandle::new(tx);

        // Start a task to receive the shutdown signal
        let shutdown_task = tokio::spawn(async move { rx.await.is_ok() });

        // Send shutdown signal
        assert!(handle.shutdown().is_ok());

        // Verify signal was received
        assert!(shutdown_task.await.unwrap());
    }
}
