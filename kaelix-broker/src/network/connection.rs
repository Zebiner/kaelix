//! Connection state management for client connections.
//!
//! This module provides efficient connection state tracking and management
//! for the `MemoryStreamer` TCP server.

use kaelix_core::Result;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Unique identifier for a client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(Uuid);

impl ConnectionId {
    /// Create a new unique connection ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Current state of a client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection is being established
    Connecting,
    /// Connection is active and ready for data
    Active,
    /// Connection is being gracefully closed
    Closing,
    /// Connection has been terminated
    Closed,
    /// Connection encountered an error
    Error,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connecting => write!(f, "connecting"),
            Self::Active => write!(f, "active"),
            Self::Closing => write!(f, "closing"),
            Self::Closed => write!(f, "closed"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Statistics for a client connection.
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    /// Number of bytes received
    pub bytes_received: u64,
    /// Number of bytes sent
    pub bytes_sent: u64,
    /// Number of messages received
    pub messages_received: u64,
    /// Number of messages sent
    pub messages_sent: u64,
    /// Connection established timestamp
    pub connected_at: Instant,
    /// Last activity timestamp
    pub last_activity: Instant,
}

impl ConnectionStats {
    /// Create new connection statistics.
    #[must_use]
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            bytes_received: 0,
            bytes_sent: 0,
            messages_received: 0,
            messages_sent: 0,
            connected_at: now,
            last_activity: now,
        }
    }

    /// Update bytes received counter.
    pub fn add_bytes_received(&mut self, bytes: u64) {
        self.bytes_received = self.bytes_received.saturating_add(bytes);
        self.last_activity = Instant::now();
    }

    /// Update bytes sent counter.
    pub fn add_bytes_sent(&mut self, bytes: u64) {
        self.bytes_sent = self.bytes_sent.saturating_add(bytes);
        self.last_activity = Instant::now();
    }

    /// Update messages received counter.
    pub fn add_message_received(&mut self) {
        self.messages_received = self.messages_received.saturating_add(1);
        self.last_activity = Instant::now();
    }

    /// Update messages sent counter.
    pub fn add_message_sent(&mut self) {
        self.messages_sent = self.messages_sent.saturating_add(1);
        self.last_activity = Instant::now();
    }

    /// Get connection duration.
    #[must_use]
    pub fn connection_duration(&self) -> Duration {
        self.connected_at.elapsed()
    }

    /// Get time since last activity.
    #[must_use]
    pub fn idle_duration(&self) -> Duration {
        self.last_activity.elapsed()
    }
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a client connection to the broker.
#[derive(Debug)]
pub struct Connection {
    /// Unique connection identifier
    id: ConnectionId,
    /// Remote socket address
    remote_addr: SocketAddr,
    /// Current connection state
    state: Arc<Mutex<ConnectionState>>,
    /// Connection statistics
    stats: Arc<Mutex<ConnectionStats>>,
    /// TCP stream for communication
    stream: Option<Arc<Mutex<TcpStream>>>,
    /// Connection timeout duration
    timeout: Duration,
}

impl Connection {
    /// Create a new connection instance.
    #[must_use]
    pub fn new(id: ConnectionId, remote_addr: SocketAddr) -> Self {
        Self {
            id,
            remote_addr,
            state: Arc::new(Mutex::new(ConnectionState::Connecting)),
            stats: Arc::new(Mutex::new(ConnectionStats::new())),
            stream: None,
            timeout: Duration::from_secs(30), // Default timeout
        }
    }

    /// Create a connection with an established TCP stream.
    #[must_use]
    pub fn with_stream(id: ConnectionId, remote_addr: SocketAddr, stream: TcpStream) -> Self {
        Self {
            id,
            remote_addr,
            state: Arc::new(Mutex::new(ConnectionState::Active)),
            stats: Arc::new(Mutex::new(ConnectionStats::new())),
            stream: Some(Arc::new(Mutex::new(stream))),
            timeout: Duration::from_secs(30),
        }
    }

    /// Get the connection ID.
    #[must_use]
    pub const fn id(&self) -> ConnectionId {
        self.id
    }

    /// Get the remote socket address.
    #[must_use]
    pub const fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Get the current connection state.
    pub async fn state(&self) -> ConnectionState {
        *self.state.lock().await
    }

    /// Set the connection state.
    pub async fn set_state(&self, new_state: ConnectionState) {
        let mut state = self.state.lock().await;
        *state = new_state;
    }

    /// Get connection statistics.
    pub async fn stats(&self) -> ConnectionStats {
        self.stats.lock().await.clone()
    }

    /// Update connection statistics for received data.
    pub async fn record_bytes_received(&self, bytes: u64) {
        let mut stats = self.stats.lock().await;
        stats.add_bytes_received(bytes);
    }

    /// Update connection statistics for sent data.
    pub async fn record_bytes_sent(&self, bytes: u64) {
        let mut stats = self.stats.lock().await;
        stats.add_bytes_sent(bytes);
    }

    /// Update connection statistics for received message.
    pub async fn record_message_received(&self) {
        let mut stats = self.stats.lock().await;
        stats.add_message_received();
    }

    /// Update connection statistics for sent message.
    pub async fn record_message_sent(&self) {
        let mut stats = self.stats.lock().await;
        stats.add_message_sent();
    }

    /// Set the connection timeout.
    #[allow(clippy::missing_const_for_fn)] // Cannot be const due to &mut self
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Get the connection timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Check if the connection is idle beyond the timeout period.
    pub async fn is_idle(&self) -> bool {
        let stats = self.stats.lock().await;
        stats.idle_duration() > self.timeout
    }

    /// Check if the connection is in an active state.
    pub async fn is_active(&self) -> bool {
        matches!(self.state().await, ConnectionState::Active)
    }

    /// Gracefully close the connection.
    ///
    /// # Errors
    /// Returns an error if the connection cannot be closed properly.
    pub async fn close(&self) -> Result<()> {
        self.set_state(ConnectionState::Closing).await;

        // If we have a stream, attempt graceful shutdown
        if let Some(stream) = &self.stream {
            let mut stream_guard = stream.lock().await;
            if let Err(e) = stream_guard.shutdown().await {
                tracing::warn!("Error during connection shutdown: {}", e);
            }
        }

        self.set_state(ConnectionState::Closed).await;
        Ok(())
    }

    /// Mark the connection as having encountered an error.
    pub async fn set_error(&self) {
        self.set_state(ConnectionState::Error).await;
    }
}

impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Connection {}

/// Connection manager for handling multiple client connections efficiently.
#[derive(Debug)]
pub struct ConnectionManager {
    /// Active connections mapped by ID
    connections: Arc<dashmap::DashMap<ConnectionId, Arc<Connection>>>,
    /// Maximum number of concurrent connections
    max_connections: usize,
    /// Global connection counter
    connection_counter: AtomicU64,
}

impl ConnectionManager {
    /// Create a new connection manager.
    #[must_use]
    pub fn new(max_connections: usize) -> Self {
        Self {
            connections: Arc::new(dashmap::DashMap::new()),
            max_connections,
            connection_counter: AtomicU64::new(0),
        }
    }

    /// Add a new connection to the manager.
    ///
    /// # Errors
    /// Returns an error if the connection limit is exceeded.
    pub fn add_connection(&self, connection: Connection) -> Result<Arc<Connection>> {
        if self.connections.len() >= self.max_connections {
            return Err(kaelix_core::Error::NetworkError("Maximum connections exceeded".into()));
        }

        let connection = Arc::new(connection);
        let id = connection.id();
        self.connections.insert(id, Arc::clone(&connection));
        self.connection_counter.fetch_add(1, Ordering::Relaxed);

        tracing::info!("Added connection {} from {}", id, connection.remote_addr());
        Ok(connection)
    }

    /// Remove a connection from the manager.
    #[must_use]
    pub fn remove_connection(&self, id: ConnectionId) -> Option<Arc<Connection>> {
        if let Some((_, connection)) = self.connections.remove(&id) {
            tracing::info!("Removed connection {} from {}", id, connection.remote_addr());
            Some(connection)
        } else {
            None
        }
    }

    /// Get a connection by ID.
    #[must_use]
    pub fn get_connection(&self, id: ConnectionId) -> Option<Arc<Connection>> {
        self.connections.get(&id).map(|entry| Arc::clone(entry.value()))
    }

    /// Get the current number of active connections.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Get the total number of connections ever created.
    #[must_use]
    pub fn total_connections(&self) -> u64 {
        self.connection_counter.load(Ordering::Relaxed)
    }

    /// Check if the manager can accept new connections.
    #[must_use]
    pub fn can_accept_connection(&self) -> bool {
        self.connections.len() < self.max_connections
    }

    /// Get all active connection IDs.
    #[must_use]
    pub fn get_all_connection_ids(&self) -> Vec<ConnectionId> {
        self.connections.iter().map(|entry| *entry.key()).collect()
    }

    /// Clean up idle connections.
    ///
    /// # Errors
    /// Returns an error if connection cleanup fails.
    pub async fn cleanup_idle_connections(&self) -> usize {
        let mut to_remove = Vec::new();

        // Collect IDs of idle connections
        for entry in self.connections.iter() {
            let connection = entry.value();
            if connection.is_idle().await {
                to_remove.push(*entry.key());
            }
        }

        // Remove idle connections
        let mut removed_count = 0;
        for id in to_remove {
            if let Some(connection) = self.remove_connection(id) {
                let _ = connection.close().await;
                removed_count += 1;
            }
        }

        if removed_count > 0 {
            tracing::info!("Cleaned up {} idle connections", removed_count);
        }

        removed_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_connection_id_creation() {
        let id1 = ConnectionId::new();
        let id2 = ConnectionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_connection_state_display() {
        assert_eq!(format!("{}", ConnectionState::Active), "active");
        assert_eq!(format!("{}", ConnectionState::Closed), "closed");
    }

    #[test]
    fn test_connection_stats() {
        let mut stats = ConnectionStats::new();
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.messages_received, 0);

        stats.add_bytes_received(100);
        stats.add_message_received();

        assert_eq!(stats.bytes_received, 100);
        assert_eq!(stats.messages_received, 1);
    }

    #[tokio::test]
    async fn test_connection_creation() {
        let id = ConnectionId::new();
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connection = Connection::new(id, addr);

        assert_eq!(connection.id(), id);
        assert_eq!(connection.remote_addr(), addr);
        assert_eq!(connection.state().await, ConnectionState::Connecting);
    }

    #[tokio::test]
    async fn test_connection_state_transitions() {
        let id = ConnectionId::new();
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connection = Connection::new(id, addr);

        assert_eq!(connection.state().await, ConnectionState::Connecting);

        connection.set_state(ConnectionState::Active).await;
        assert_eq!(connection.state().await, ConnectionState::Active);
        assert!(connection.is_active().await);
    }

    #[tokio::test]
    async fn test_connection_statistics_tracking() {
        let id = ConnectionId::new();
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connection = Connection::new(id, addr);

        connection.record_bytes_received(100).await;
        connection.record_message_received().await;

        let stats = connection.stats().await;
        assert_eq!(stats.bytes_received, 100);
        assert_eq!(stats.messages_received, 1);
    }

    #[test]
    fn test_connection_manager_creation() {
        let manager = ConnectionManager::new(100);
        assert_eq!(manager.connection_count(), 0);
        assert!(manager.can_accept_connection());
    }

    #[test]
    fn test_connection_manager_add_remove() {
        let manager = ConnectionManager::new(10);
        let id = ConnectionId::new();
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connection = Connection::new(id, addr);

        // Add connection
        let arc_conn = manager.add_connection(connection).unwrap();
        assert_eq!(manager.connection_count(), 1);
        assert_eq!(arc_conn.id(), id);

        // Remove connection
        let removed = manager.remove_connection(id);
        assert!(removed.is_some());
        assert_eq!(manager.connection_count(), 0);
    }

    #[test]
    fn test_connection_manager_capacity_limit() {
        let manager = ConnectionManager::new(1);
        let id1 = ConnectionId::new();
        let id2 = ConnectionId::new();
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        // Add first connection - should succeed
        let connection1 = Connection::new(id1, addr);
        assert!(manager.add_connection(connection1).is_ok());
        assert!(!manager.can_accept_connection());

        // Add second connection - should fail
        let connection2 = Connection::new(id2, addr);
        assert!(manager.add_connection(connection2).is_err());
    }

    #[tokio::test]
    async fn test_connection_idle_detection() {
        let id = ConnectionId::new();
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let mut connection = Connection::new(id, addr);

        // Set a very short timeout for testing
        connection.set_timeout(Duration::from_millis(10));

        // Connection should not be idle initially
        assert!(!connection.is_idle().await);

        // Wait longer than timeout
        sleep(Duration::from_millis(20)).await;

        // Connection should now be idle
        assert!(connection.is_idle().await);
    }
}
