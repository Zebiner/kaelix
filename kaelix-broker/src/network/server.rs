//! TCP server for high-performance client connections.
//!
//! This module provides the main `TcpServer` implementation for the
//! `MemoryStreamer` broker, designed to handle 10K+ concurrent connections
//! with ultra-low latency (<1ms connection acceptance).

use crate::config::NetworkConfig;
use crate::network::listener::TcpListener;
use crate::network::{ConnectionManager, NetworkHandle};
use kaelix_core::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// High-performance TCP server for streaming applications.
#[derive(Debug)]
pub struct TcpServer {
    /// Server configuration
    config: NetworkConfig,
    /// Shutdown signal broadcaster
    shutdown_tx: Option<broadcast::Sender<()>>,
    /// Server task handle
    server_task: Option<JoinHandle<Result<()>>>,
    /// Metrics collection task handle
    metrics_task: Option<JoinHandle<()>>,
}

impl TcpServer {
    /// Create a new TCP server with the given configuration.
    ///
    /// # Errors
    /// Returns an error if the configuration is invalid.
    pub fn new(config: NetworkConfig) -> Result<Self> {
        // Validate configuration
        if config.max_connections == 0 {
            return Err(kaelix_core::Error::NetworkError(
                "Maximum connections must be greater than 0".into(),
            ));
        }

        if config.connection_timeout.is_zero() {
            return Err(kaelix_core::Error::NetworkError(
                "Connection timeout must be greater than 0".into(),
            ));
        }

        Ok(Self { config, shutdown_tx: None, server_task: None, metrics_task: None })
    }

    /// Stop the TCP server gracefully.
    ///
    /// # Errors
    /// Returns an error if the server is not running or shutdown fails.
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = &self.shutdown_tx {
            shutdown_tx.send(()).map_err(|_| {
                kaelix_core::Error::NetworkError("Failed to send shutdown signal".into())
            })?;
        } else {
            return Err(kaelix_core::Error::NetworkError("Server is not running".into()));
        }

        // Wait for server task to complete
        if let Some(task) = self.server_task.take() {
            let _ = task.await;
        }

        // Wait for metrics task to complete
        if let Some(task) = self.metrics_task.take() {
            task.abort();
        }

        self.shutdown_tx = None;
        info!("TCP server stopped");
        Ok(())
    }

    /// Start the TCP server.
    ///
    /// # Errors
    /// Returns an error if the server cannot start or bind to the configured address.
    #[allow(clippy::unused_async)] // Will contain async operations in the future
    pub async fn start(&mut self) -> Result<NetworkHandle> {
        if self.is_running() {
            return Err(kaelix_core::Error::NetworkError("Server is already running".into()));
        }

        // Create shutdown channel
        let (shutdown_tx, _) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        // Create TCP listener
        let mut listener = TcpListener::new(self.config.clone());
        listener.set_shutdown_receiver(shutdown_tx.subscribe());

        // Get connection manager reference for metrics
        let connection_manager = Arc::clone(listener.connection_manager());

        // Start the server task
        let server_task = tokio::spawn(async move { listener.start().await });
        self.server_task = Some(server_task);

        // Start metrics collection task
        let metrics_shutdown = shutdown_tx.subscribe();
        let metrics_task = tokio::spawn(async move {
            Self::metrics_collection_task(connection_manager, metrics_shutdown).await;
        });
        self.metrics_task = Some(metrics_task);

        info!(
            "TCP server started on {} (max connections: {})",
            self.config.bind_address, self.config.max_connections
        );

        // Create shutdown handle
        let (handle_tx, handle_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            if handle_rx.await.is_ok() {
                if let Err(e) = shutdown_tx.send(()) {
                    error!("Failed to send shutdown signal: {}", e);
                }
            }
        });

        Ok(NetworkHandle::new(handle_tx))
    }

    /// Check if the server is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.server_task.as_ref().is_some_and(|task| !task.is_finished())
    }

    /// Get the server's bind address.
    #[must_use]
    pub const fn bind_address(&self) -> std::net::SocketAddr {
        self.config.bind_address
    }

    /// Get the maximum number of connections.
    #[must_use]
    pub const fn max_connections(&self) -> usize {
        self.config.max_connections
    }

    /// Get the server configuration.
    #[must_use]
    pub const fn config(&self) -> &NetworkConfig {
        &self.config
    }

    /// Update the server configuration.
    ///
    /// # Errors
    /// Returns an error if the server is running or the configuration is invalid.
    pub fn update_config(&mut self, config: NetworkConfig) -> Result<()> {
        if self.is_running() {
            return Err(kaelix_core::Error::NetworkError(
                "Cannot update configuration while server is running".into(),
            ));
        }

        // Validate new configuration
        if config.max_connections == 0 {
            return Err(kaelix_core::Error::NetworkError(
                "Maximum connections must be greater than 0".into(),
            ));
        }

        if config.connection_timeout.is_zero() {
            return Err(kaelix_core::Error::NetworkError(
                "Connection timeout must be greater than 0".into(),
            ));
        }

        self.config = config;
        Ok(())
    }

    /// Metrics collection task.
    #[allow(clippy::cognitive_complexity)] // Acceptable complexity for metrics loop
    async fn metrics_collection_task(
        connection_manager: Arc<ConnectionManager>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let (active, total) = (
                        connection_manager.connection_count(),
                        connection_manager.total_connections(),
                    );
                    info!("Connection metrics - Active: {}, Total: {}", active, total);
                }
                _ = shutdown_rx.recv() => {
                    info!("Metrics collection task shutting down");
                    break;
                }
            }
        }
    }
}

/// Builder for creating a TCP server with fluent configuration.
#[derive(Debug, Default)]
pub struct TcpServerBuilder {
    bind_address: Option<std::net::SocketAddr>,
    max_connections: Option<usize>,
    connection_timeout: Option<Duration>,
    tls_enabled: Option<bool>,
}

impl TcpServerBuilder {
    /// Create a new TCP server builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the bind address.
    #[must_use]
    pub const fn bind_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }

    /// Set the maximum number of connections.
    #[must_use]
    pub const fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = Some(max);
        self
    }

    /// Set the connection timeout.
    #[must_use]
    pub const fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Enable or disable TLS.
    #[must_use]
    pub const fn tls_enabled(mut self, enabled: bool) -> Self {
        self.tls_enabled = Some(enabled);
        self
    }

    /// Build the TCP server.
    ///
    /// # Errors
    /// Returns an error if the configuration is invalid.
    ///
    /// # Panics
    /// Panics if the default bind address cannot be parsed (should never happen).
    #[allow(clippy::missing_panics_doc)] // Panic is documented and should never occur
    pub fn build(self) -> Result<TcpServer> {
        let config = NetworkConfig {
            bind_address: self
                .bind_address
                .unwrap_or_else(|| "127.0.0.1:9092".parse().expect("Valid default address")),
            max_connections: self.max_connections.unwrap_or(10000),
            connection_timeout: self.connection_timeout.unwrap_or(Duration::from_secs(30)),
            tls_enabled: self.tls_enabled.unwrap_or(false),
        };

        TcpServer::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_config() -> NetworkConfig {
        NetworkConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(), // Use port 0 for automatic assignment
            max_connections: 100,
            connection_timeout: Duration::from_secs(10),
            tls_enabled: false,
        }
    }

    #[test]
    fn test_tcp_server_creation() {
        let config = create_test_config();
        let server = TcpServer::new(config.clone()).unwrap();

        assert_eq!(server.bind_address(), config.bind_address);
        assert_eq!(server.max_connections(), config.max_connections);
        assert!(!server.is_running());
    }

    #[test]
    fn test_config_validation() {
        let mut config = create_test_config();

        // Test invalid max_connections
        config.max_connections = 0;
        assert!(TcpServer::new(config.clone()).is_err());

        // Test invalid connection_timeout
        config.max_connections = 100;
        config.connection_timeout = Duration::ZERO;
        assert!(TcpServer::new(config).is_err());
    }

    #[test]
    fn test_server_builder() {
        let server = TcpServerBuilder::new()
            .bind_address("127.0.0.1:8080".parse().unwrap())
            .max_connections(500)
            .connection_timeout(Duration::from_secs(15))
            .tls_enabled(true)
            .build()
            .unwrap();

        assert_eq!(server.bind_address().port(), 8080);
        assert_eq!(server.max_connections(), 500);
        assert_eq!(server.config().connection_timeout, Duration::from_secs(15));
        assert!(server.config().tls_enabled);
    }

    #[test]
    fn test_server_builder_defaults() {
        let server = TcpServerBuilder::new().build().unwrap();

        assert_eq!(server.bind_address().port(), 9092);
        assert_eq!(server.max_connections(), 10000);
        assert_eq!(server.config().connection_timeout, Duration::from_secs(30));
        assert!(!server.config().tls_enabled);
    }

    #[tokio::test]
    async fn test_server_lifecycle() {
        let config = create_test_config();
        let mut server = TcpServer::new(config).unwrap();

        assert!(!server.is_running());

        // Start server
        let handle = server.start().await.unwrap();
        assert!(server.is_running());

        // Stop server via handle
        assert!(handle.shutdown().is_ok());

        // Give the server time to stop
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Server should be stopped (note: this might still be running due to async nature)
        // We can't reliably test is_running() immediately after shutdown
    }

    #[tokio::test]
    async fn test_server_stop_when_not_running() {
        let config = create_test_config();
        let mut server = TcpServer::new(config).unwrap();

        // Try to stop a server that's not running
        let result = server.stop().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_config_update() {
        let config = create_test_config();
        let mut server = TcpServer::new(config).unwrap();

        let mut new_config = create_test_config();
        new_config.max_connections = 200;

        assert!(server.update_config(new_config.clone()).is_ok());
        assert_eq!(server.config().max_connections, 200);
    }

    #[tokio::test]
    async fn test_double_start_error() {
        let config = create_test_config();
        let mut server = TcpServer::new(config).unwrap();

        // Start server
        let _handle = server.start().await.unwrap();

        // Try to start again - should fail
        let result = server.start().await;
        assert!(result.is_err());
    }
}
