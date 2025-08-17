//! Async TCP listener with graceful shutdown capabilities.
//!
//! This module provides a high-performance TCP listener implementation
//! designed for the `MemoryStreamer` broker with support for graceful shutdown
//! and connection management.

use crate::config::NetworkConfig;
use crate::network::connection::{Connection, ConnectionId, ConnectionManager};
use kaelix_core::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};

/// Async TCP listener for client connections.
#[derive(Debug)]
pub struct TcpListener {
    /// Network configuration
    config: NetworkConfig,
    /// Connection manager for tracking active connections
    connection_manager: Arc<ConnectionManager>,
    /// Shutdown signal receiver
    shutdown_rx: Option<broadcast::Receiver<()>>,
}

impl TcpListener {
    /// Create a new TCP listener with the given configuration.
    #[must_use]
    pub fn new(config: NetworkConfig) -> Self {
        let connection_manager = Arc::new(ConnectionManager::new(config.max_connections));

        Self { config, connection_manager, shutdown_rx: None }
    }

    /// Set the shutdown signal receiver.
    pub fn set_shutdown_receiver(&mut self, shutdown_rx: broadcast::Receiver<()>) {
        self.shutdown_rx = Some(shutdown_rx);
    }

    /// Start the TCP listener and accept incoming connections.
    ///
    /// # Errors
    /// Returns an error if the listener cannot bind to the configured address
    /// or if there are issues accepting connections.
    pub async fn start(&mut self) -> Result<()> {
        let listener = TokioTcpListener::bind(self.config.bind_address).await.map_err(|e| {
            kaelix_core::Error::NetworkError(format!(
                "Failed to bind to {}: {}",
                self.config.bind_address, e
            ))
        })?;

        info!(
            "TCP listener started on {} (max connections: {})",
            self.config.bind_address, self.config.max_connections
        );

        // Start the cleanup task for idle connections
        let cleanup_manager: Arc<ConnectionManager> = Arc::clone(&self.connection_manager);
        let cleanup_task = tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(30));
            loop {
                cleanup_interval.tick().await;
                let removed = cleanup_manager.cleanup_idle_connections().await;
                if removed > 0 {
                    debug!("Cleanup task removed {} idle connections", removed);
                }
            }
        });

        // Main accept loop
        let accept_result = self.accept_loop(listener).await;

        // Cancel cleanup task
        cleanup_task.abort();

        accept_result
    }

    /// Main connection acceptance loop.
    #[allow(clippy::cognitive_complexity)] // Inherent complexity of select! loop
    async fn accept_loop(&mut self, listener: TokioTcpListener) -> Result<()> {
        let mut shutdown_rx = self.shutdown_rx.take().ok_or_else(|| {
            kaelix_core::Error::NetworkError("No shutdown receiver configured".into())
        })?;

        loop {
            tokio::select! {
                // Handle incoming connections
                accept_result = listener.accept() => {
                    self.handle_accept_result(accept_result).await;
                }

                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown signal, stopping listener");
                    break;
                }
            }
        }

        // Gracefully close all connections
        self.shutdown_all_connections().await;
        info!("TCP listener stopped");
        Ok(())
    }

    /// Handle the result of accepting a connection.
    #[allow(clippy::cognitive_complexity)] // Appropriate complexity for error handling
    async fn handle_accept_result(&self, accept_result: std::io::Result<(TcpStream, SocketAddr)>) {
        match accept_result {
            Ok((stream, addr)) => {
                if let Err(e) = self.process_new_connection(stream, addr) {
                    error!("Error handling new connection from {}: {}", addr, e);
                }
            },
            Err(e) => {
                error!("Error accepting connection: {}", e);
                // Brief delay to prevent tight error loop
                tokio::time::sleep(Duration::from_millis(100)).await;
            },
        }
    }

    /// Process a new incoming connection.
    ///
    /// # Errors
    /// Returns an error if the connection cannot be handled.
    fn process_new_connection(&self, stream: TcpStream, addr: SocketAddr) -> Result<()> {
        // Check capacity first
        if !self.can_accept_connection() {
            warn!(
                "Rejecting connection from {} - at maximum capacity ({})",
                addr, self.config.max_connections
            );
            drop(stream);
            return Ok(());
        }

        // Configure socket and create connection
        self.setup_connection(stream, addr)
    }

    /// Check if we can accept a new connection.
    fn can_accept_connection(&self) -> bool {
        self.connection_manager.can_accept_connection()
    }

    /// Set up and register a new connection.
    ///
    /// # Errors
    /// Returns an error if connection setup fails.
    fn setup_connection(&self, stream: TcpStream, addr: SocketAddr) -> Result<()> {
        // Configure socket options
        if let Err(e) = Self::configure_socket(&stream) {
            warn!("Failed to configure socket for {}: {}", addr, e);
        }

        // Create and register connection
        let connection_id = ConnectionId::new();
        let connection = Connection::with_stream(connection_id, addr, stream);

        self.register_connection(connection, connection_id, addr)
    }

    /// Register a connection with the manager and spawn handler.
    ///
    /// # Errors
    /// Returns an error if connection registration fails.
    fn register_connection(
        &self,
        connection: Connection,
        connection_id: ConnectionId,
        addr: SocketAddr,
    ) -> Result<()> {
        match self.connection_manager.add_connection(connection) {
            Ok(conn) => {
                self.log_connection_accepted(connection_id, addr);
                self.spawn_connection_handler(conn, connection_id);
                Ok(())
            },
            Err(e) => {
                error!("Failed to add connection from {}: {}", addr, e);
                Err(e)
            },
        }
    }

    /// Log successful connection acceptance.
    fn log_connection_accepted(&self, connection_id: ConnectionId, addr: SocketAddr) {
        info!(
            "Accepted connection {} from {} (total: {})",
            connection_id,
            addr,
            self.connection_manager.connection_count()
        );
    }

    /// Spawn a task to handle the connection.
    fn spawn_connection_handler(&self, conn: Arc<Connection>, connection_id: ConnectionId) {
        let conn_manager = Arc::clone(&self.connection_manager);
        tokio::spawn(async move {
            if let Err(e) = Self::handle_connection(conn, conn_manager).await {
                error!("Connection {} ended with error: {}", connection_id, e);
            }
        });
    }

    /// Configure socket options for optimal performance.
    ///
    /// # Errors
    /// Returns an error if socket configuration fails.
    fn configure_socket(stream: &TcpStream) -> Result<()> {
        use socket2::{Socket, TcpKeepalive};
        use std::os::unix::io::{AsRawFd, FromRawFd};

        // Convert to socket2::Socket for advanced options
        let socket = unsafe { Socket::from_raw_fd(stream.as_raw_fd()) };

        // Enable TCP_NODELAY for low latency
        socket.set_nodelay(true).map_err(|e| {
            kaelix_core::Error::NetworkError(format!("Failed to set TCP_NODELAY: {e}"))
        })?;

        // Configure keep-alive
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(60))
            .with_interval(Duration::from_secs(10));

        socket.set_tcp_keepalive(&keepalive).map_err(|e| {
            kaelix_core::Error::NetworkError(format!("Failed to set TCP keepalive: {e}"))
        })?;

        // Forget the socket to avoid double-close
        std::mem::forget(socket);

        Ok(())
    }

    /// Handle an individual connection.
    ///
    /// # Errors
    /// Returns an error if connection handling fails.
    #[allow(clippy::cognitive_complexity)] // Appropriate complexity for connection lifecycle
    async fn handle_connection(
        connection: Arc<Connection>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        let connection_id = connection.id();
        let remote_addr = connection.remote_addr();

        debug!("Handling connection {} from {}", connection_id, remote_addr);

        let result = Self::execute_connection_with_timeout(&connection).await;
        Self::process_connection_result(&connection, result).await;

        // Cleanup
        let _ = connection_manager.remove_connection(connection_id);
        if let Err(e) = connection.close().await {
            error!("Error closing connection {}: {}", connection_id, e);
        }

        Ok(())
    }

    /// Execute connection logic with timeout.
    async fn execute_connection_with_timeout(
        connection: &Arc<Connection>,
    ) -> std::result::Result<Result<()>, tokio::time::error::Elapsed> {
        let timeout_duration = connection.timeout();
        timeout(timeout_duration, async {
            // Connection handling logic would go here
            // For now, we'll simulate some activity and then close
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok::<(), kaelix_core::Error>(())
        })
        .await
    }

    /// Process the result of connection execution.
    #[allow(clippy::cognitive_complexity)] // Appropriate complexity for result processing
    async fn process_connection_result(
        connection: &Arc<Connection>,
        result: std::result::Result<Result<()>, tokio::time::error::Elapsed>,
    ) {
        let connection_id = connection.id();

        match result {
            Ok(Ok(())) => {
                debug!("Connection {} completed normally", connection_id);
            },
            Ok(Err(e)) => {
                error!("Connection {} error: {}", connection_id, e);
                connection.set_error().await;
            },
            Err(_) => {
                warn!("Connection {} timed out", connection_id);
                connection.set_error().await;
            },
        }
    }

    /// Gracefully shutdown all active connections.
    #[allow(clippy::cognitive_complexity)] // Appropriate complexity for shutdown coordination
    async fn shutdown_all_connections(&self) {
        let connection_ids = self.connection_manager.get_all_connection_ids();
        info!("Shutting down {} active connections", connection_ids.len());

        let shutdown_tasks = self.create_shutdown_tasks(connection_ids);
        self.wait_for_shutdown_completion(shutdown_tasks).await;

        info!("All connections shut down");
    }

    /// Create shutdown tasks for all connections.
    fn create_shutdown_tasks(
        &self,
        connection_ids: Vec<ConnectionId>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut shutdown_tasks = Vec::new();

        for id in connection_ids {
            if let Some(connection) = self.connection_manager.get_connection(id) {
                let task = tokio::spawn(async move {
                    if let Err(e) = connection.close().await {
                        error!("Error closing connection {}: {}", id, e);
                    }
                });
                shutdown_tasks.push(task);
            }
        }

        shutdown_tasks
    }

    /// Wait for all shutdown tasks to complete with timeout.
    async fn wait_for_shutdown_completion(&self, shutdown_tasks: Vec<tokio::task::JoinHandle<()>>) {
        let shutdown_timeout = Duration::from_secs(10);
        if timeout(shutdown_timeout, async {
            for task in shutdown_tasks {
                let _ = task.await;
            }
        })
        .await
        .is_err()
        {
            warn!("Connection shutdown timed out after {:?}", shutdown_timeout);
        }
    }

    /// Get connection statistics.
    #[must_use]
    pub fn connection_stats(&self) -> (usize, u64) {
        (self.connection_manager.connection_count(), self.connection_manager.total_connections())
    }

    /// Get a reference to the connection manager.
    #[must_use]
    pub const fn connection_manager(&self) -> &Arc<ConnectionManager> {
        &self.connection_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NetworkConfig;
    use std::time::Duration;
    use tokio::sync::broadcast;

    fn create_test_config() -> NetworkConfig {
        NetworkConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(), // Use port 0 for automatic assignment
            max_connections: 10,
            connection_timeout: Duration::from_secs(5),
            tls_enabled: false,
        }
    }

    #[test]
    fn test_tcp_listener_creation() {
        let config = create_test_config();
        let listener = TcpListener::new(config.clone());

        assert_eq!(listener.config.max_connections, config.max_connections);
        assert_eq!(listener.connection_manager.connection_count(), 0);
    }

    #[test]
    fn test_shutdown_receiver_configuration() {
        let config = create_test_config();
        let mut listener = TcpListener::new(config);

        let (tx, rx) = broadcast::channel(1);
        listener.set_shutdown_receiver(rx);

        // Verify the receiver was set (indirectly by checking it's no longer None)
        assert!(listener.shutdown_rx.is_some());
        drop(tx); // Clean up
    }

    #[tokio::test]
    async fn test_connection_stats() {
        let config = create_test_config();
        let listener = TcpListener::new(config);

        let (active, total) = listener.connection_stats();
        assert_eq!(active, 0);
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn test_tcp_listener_bind_invalid_address() {
        let mut config = create_test_config();
        // Use an IP address that's guaranteed to be unreachable/unassignable
        config.bind_address = "192.0.2.1:9999".parse().unwrap(); // RFC5737 test-net address

        let mut listener = TcpListener::new(config);
        let (tx, rx) = broadcast::channel(1);
        listener.set_shutdown_receiver(rx);

        let result = listener.start().await;
        assert!(result.is_err());
        drop(tx); // Clean up
    }

    #[tokio::test]
    async fn test_connection_capacity_check() {
        let config = create_test_config();
        let listener = TcpListener::new(config);

        // Initially should be able to accept connections
        assert!(listener.connection_manager.can_accept_connection());

        // Test connection count tracking
        assert_eq!(listener.connection_manager.connection_count(), 0);
    }
}
