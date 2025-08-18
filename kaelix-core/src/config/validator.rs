//! Configuration validation and constraint checking
//!
//! This module provides comprehensive validation for MemoryStreamer configuration
//! with both structural validation (via validator crate) and semantic validation
//! for performance, security, and operational constraints.
//!
//! Validation ensures that:
//! - All configuration values are within acceptable ranges
//! - Performance targets are realistic and achievable
//! - Security settings are properly configured
//! - Network settings are valid and safe
//! - Resource constraints are respected

use crate::config::schema::*;
use crate::error::{Result};
use std::net::IpAddr;
use std::path::Path;
use tracing::{debug, warn};
use validator::Validate;

/// Comprehensive configuration validator
///
/// Provides both structural validation (data types, ranges) and semantic validation
/// (performance constraints, security requirements, operational safety).
pub struct ConfigValidator {
    /// Validation context for environment-specific checks
    context: ValidationContext,
}

/// Validation context for environment-specific configuration checking
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// Available CPU cores for thread validation
    pub cpu_cores: usize,
    /// Available memory for memory configuration validation
    pub available_memory: u64,
    /// Network interfaces available for binding
    pub network_interfaces: Vec<IpAddr>,
    /// Whether running in production environment
    pub is_production: bool,
}

impl ConfigValidator {
    /// Create a new configuration validator with system context
    pub fn new() -> Self {
        Self {
            context: ValidationContext::detect_system(),
        }
    }

    /// Create a validator with custom validation context
    pub fn with_context(context: ValidationContext) -> Self {
        Self { context }
    }

    /// Validate complete MemoryStreamer configuration
    ///
    /// Performs comprehensive validation including:
    /// - Structural validation using validator crate
    /// - Performance constraint validation
    /// - Security configuration validation
    /// - Network and protocol compatibility checks
    /// - Resource usage validation
    ///
    /// # Arguments
    /// * `config` - The configuration to validate
    ///
    /// # Returns
    /// * `Ok(())` - Configuration is valid
    /// * `Err(Error)` - Configuration has validation errors
    pub fn validate(&self, config: &MemoryStreamerConfig) -> Result<()> {
        debug!("Starting comprehensive configuration validation");

        // 1. Structural validation using validator crate
        config.validate().map_err(|e| crate::error::Error::Configuration {
            message: format!("Structural validation failed: {}", e)
        })?;

        // 2. Performance constraint validation
        self.validate_performance_constraints(&config.performance)?;

        // 3. Network configuration validation
        self.validate_network_configuration(&config.network)?;

        // 4. Protocol configuration validation
        self.validate_protocol_configuration(&config.protocol)?;

        // 5. Security configuration validation
        self.validate_security_configuration(&config.security)?;

        // 6. Cross-component validation
        self.validate_cross_component_constraints(config)?;

        // 7. Environment-specific validation
        self.validate_environment_constraints(config)?;

        debug!("Configuration validation completed successfully");
        Ok(())
    }

    /// Validate performance-related configuration constraints
    fn validate_performance_constraints(&self, config: &PerformanceConfig) -> Result<()> {
        debug!("Validating performance constraints");

        // Validate worker thread configuration
        let worker_threads = config.worker_threads.unwrap_or(self.context.cpu_cores);
        
        if worker_threads > self.context.cpu_cores * 2 {
            warn!(
                "Worker threads ({}) exceed 2x CPU cores ({}), may cause oversubscription",
                worker_threads, self.context.cpu_cores
            );
        }

        // Validate latency target is achievable
        if config.target_latency_p99_us < 5 && self.context.is_production {
            warn!("Sub-5μs latency targets may be difficult to achieve in production");
        }

        // Validate throughput and latency relationship
        let latency_throughput_product = config.target_latency_p99_us * config.max_throughput;
        if latency_throughput_product > 1_000_000_000 {
            warn!("High latency-throughput product may indicate unrealistic performance targets");
        }

        Ok(())
    }

    /// Validate network configuration
    fn validate_network_configuration(&self, config: &NetworkConfig) -> Result<()> {
        debug!("Validating network configuration");

        // Parse and validate bind address
        let bind_addr: IpAddr = config.bind_address.parse()
            .map_err(|_| crate::error::Error::Configuration {
                message: format!("Invalid bind address: {}", config.bind_address)
            })?;

        // Check if address is available on system
        if !self.context.network_interfaces.is_empty() 
            && !self.context.network_interfaces.contains(&bind_addr) 
            && !bind_addr.is_unspecified() {
            warn!("Bind address {} may not be available on this system", bind_addr);
        }

        // Validate connection limits are reasonable
        if config.max_connections > 1_000_000 && self.context.is_production {
            warn!("Very high connection limit may exhaust system resources");
        }

        // Validate buffer sizes
        self.validate_network_buffers(&config.buffers)?;

        Ok(())
    }

    /// Validate network buffer configuration
    fn validate_network_buffers(&self, config: &NetworkBufferConfig) -> Result<()> {
        // Calculate total buffer memory usage
        let per_connection_memory = config.send_buffer_size + config.recv_buffer_size;
        let total_buffer_memory = per_connection_memory as u64 * 1000; // Estimate for 1000 connections

        if total_buffer_memory > self.context.available_memory / 4 {
            warn!("Network buffers may consume excessive memory");
        }

        Ok(())
    }

    /// Validate protocol configuration
    fn validate_protocol_configuration(&self, config: &ProtocolConfig) -> Result<()> {
        debug!("Validating protocol configuration");

        // Validate payload size is reasonable
        if config.max_payload_size > 50 * 1024 * 1024 { // 50MB
            warn!("Large payload sizes may impact latency");
        }

        // Validate compression configuration
        if config.enable_compression {
            self.validate_compression_config(&config.compression)?;
        }

        // Validate batching configuration
        self.validate_batching_config(&config.batching)?;

        Ok(())
    }

    /// Validate compression configuration
    fn validate_compression_config(&self, config: &CompressionConfig) -> Result<()> {
        match config.algorithm {
            CompressionAlgorithm::None => {
                warn!("Compression enabled but algorithm set to None");
            }
            CompressionAlgorithm::Lz4 => {
                // LZ4 is fast, good choice for low latency
            }
            CompressionAlgorithm::Zstd => {
                if config.level > 3 {
                    warn!("High Zstd compression levels may increase latency");
                }
            }
            CompressionAlgorithm::Snappy => {
                // Snappy is balanced choice
            }
        }

        Ok(())
    }

    /// Validate batching configuration
    fn validate_batching_config(&self, config: &BatchingConfig) -> Result<()> {
        if config.enabled {
            // Check batch delay isn't too high
            if config.max_batch_delay_us > 1000 {
                warn!("High batch delay may impact latency");
            }

            // Check batch size is reasonable
            if config.max_batch_size > 100 {
                warn!("Large batch sizes may increase memory usage");
            }
        }

        Ok(())
    }

    /// Validate security configuration
    fn validate_security_configuration(&self, config: &SecurityConfig) -> Result<()> {
        debug!("Validating security configuration");

        // Validate TLS configuration if enabled
        if config.tls.enabled {
            self.validate_tls_config(&config.tls)?;
        }

        // Validate authentication configuration
        self.validate_auth_config(&config.authentication)?;

        // Production security checks
        if self.context.is_production {
            if !config.tls.enabled {
                warn!("TLS is disabled in production environment");
            }
            if !config.authentication.enabled {
                warn!("Authentication is disabled in production environment");
            }
        }

        Ok(())
    }

    /// Validate TLS configuration
    fn validate_tls_config(&self, config: &TlsConfig) -> Result<()> {
        if config.enabled {
            // Check certificate file exists if provided
            if let Some(ref cert_path) = config.cert_path {
                if !Path::new(cert_path).exists() {
                    return Err(crate::error::Error::Configuration {
                        message: format!("TLS certificate file not found: {}", cert_path)
                    });
                }
            }

            // Check private key file exists if provided
            if let Some(ref key_path) = config.key_path {
                if !Path::new(key_path).exists() {
                    return Err(crate::error::Error::Configuration {
                        message: format!("TLS private key file not found: {}", key_path)
                    });
                }
            }

            // Check CA file if provided
            if let Some(ref ca_path) = config.ca_path {
                if !Path::new(ca_path).exists() {
                    return Err(crate::error::Error::Configuration {
                        message: format!("TLS CA file not found: {}", ca_path)
                    });
                }
            }
        }

        Ok(())
    }

    /// Validate authentication configuration
    fn validate_auth_config(&self, config: &AuthenticationConfig) -> Result<()> {
        if config.enabled {
            // Basic authentication validation
            // In practice would validate specific auth method configuration
            debug!("Authentication is enabled - further validation would check specific auth methods");
        }

        Ok(())
    }

    /// Validate cross-component constraints and interactions
    fn validate_cross_component_constraints(&self, config: &MemoryStreamerConfig) -> Result<()> {
        debug!("Validating cross-component constraints");

        // Check metrics port doesn't conflict with main port
        if config.observability.metrics.enable_metrics {
            if config.observability.metrics.metrics_port == config.network.port {
                return Err(crate::error::Error::Configuration {
                    message: "Metrics port cannot be the same as the main service port".to_string()
                });
            }
        }

        // Check if TLS and high performance settings are compatible
        let worker_threads = config.performance.worker_threads.unwrap_or(self.context.cpu_cores);
        if config.security.tls.enabled && worker_threads > 16 {
            warn!("TLS with many worker threads may impact performance");
        }

        // Validate compression and batching interaction
        if config.protocol.enable_compression && config.protocol.batching.enabled {
            if config.protocol.compression.threshold > config.protocol.batching.max_batch_size * 100 {
                warn!("Compression threshold may be too high for batch sizes");
            }
        }

        Ok(())
    }

    /// Validate environment-specific constraints
    fn validate_environment_constraints(&self, config: &MemoryStreamerConfig) -> Result<()> {
        debug!("Validating environment-specific constraints");

        if self.context.is_production {
            // Production-specific validations
            self.validate_production_constraints(config)?;
        } else {
            // Development-specific validations
            self.validate_development_constraints(config)?;
        }

        Ok(())
    }

    /// Validate production environment constraints
    fn validate_production_constraints(&self, config: &MemoryStreamerConfig) -> Result<()> {
        // Ensure security is properly configured
        if !config.security.tls.enabled && !config.security.authentication.enabled {
            warn!("Production deployment without TLS or authentication");
        }

        // Validate performance targets are conservative
        if config.performance.target_latency_p99_us < 10 {
            warn!("Ultra-low latency targets in production require careful tuning");
        }

        // Check observability is enabled
        if !config.observability.metrics.enable_metrics {
            warn!("Metrics disabled in production environment");
        }

        if !config.observability.tracing.enabled {
            warn!("Tracing disabled in production environment");
        }

        Ok(())
    }

    /// Validate development environment constraints
    fn validate_development_constraints(&self, _config: &MemoryStreamerConfig) -> Result<()> {
        // Development environment is more permissive
        debug!("Development environment validation passed");
        Ok(())
    }
}

impl Default for ConfigValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationContext {
    /// Detect system characteristics for validation context
    pub fn detect_system() -> Self {
        let cpu_cores = num_cpus::get();
        let available_memory = Self::detect_available_memory();
        let network_interfaces = Self::detect_network_interfaces();
        let is_production = std::env::var("NODE_ENV")
            .map(|env| env == "production")
            .unwrap_or(false);

        Self {
            cpu_cores,
            available_memory,
            network_interfaces,
            is_production,
        }
    }

    /// Detect available system memory
    fn detect_available_memory() -> u64 {
        // Simplified memory detection - in practice would use system APIs
        8 * 1024 * 1024 * 1024 // 8GB default
    }

    /// Detect available network interfaces
    fn detect_network_interfaces() -> Vec<IpAddr> {
        // Simplified interface detection - in practice would enumerate interfaces
        vec![
            "127.0.0.1".parse().unwrap(),
            "0.0.0.0".parse().unwrap(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::*;

    fn create_test_context() -> ValidationContext {
        ValidationContext {
            cpu_cores: 8,
            available_memory: 16 * 1024 * 1024 * 1024, // 16GB
            network_interfaces: vec![
                "127.0.0.1".parse().unwrap(),
                "0.0.0.0".parse().unwrap(),
            ],
            is_production: false,
        }
    }

    fn create_valid_config() -> MemoryStreamerConfig {
        MemoryStreamerConfig {
            network: NetworkConfig {
                bind_address: "127.0.0.1".to_string(),
                port: 8080,
                max_connections: 1000,
                connection_timeout: std::time::Duration::from_millis(5000),
                keep_alive: KeepAliveConfig {
                    enabled: true,
                    idle_time: 600,
                    interval: 60,
                    probes: 9,
                },
                buffers: NetworkBufferConfig {
                    send_buffer_size: 65536,
                    recv_buffer_size: 65536,
                },
            },
            protocol: ProtocolConfig {
                max_payload_size: 1024 * 1024, // 1MB
                frame_timeout: std::time::Duration::from_millis(1000),
                enable_compression: false,
                compression: CompressionConfig {
                    algorithm: CompressionAlgorithm::None,
                    level: 1,
                    threshold: 1024,
                },
                batching: BatchingConfig {
                    enabled: false,
                    max_batch_size: 10,
                    max_batch_delay_us: 100,
                },
            },
            performance: PerformanceConfig {
                target_latency_p99_us: 100,
                max_throughput: 1_000_000,
                worker_threads: Some(4),
                numa_awareness: false,
                cpu_affinity: CpuAffinityConfig {
                    enabled: false,
                    cpu_set: vec![],
                },
                memory: MemoryConfig {
                    message_pool_size: 1000,
                    connection_pool_size: 100,
                    enable_hugepages: false,
                },
                io: IoConfig {
                    io_uring: false,
                    epoll_timeout_ms: 1,
                    max_events: 1024,
                },
            },
            observability: ObservabilityConfig {
                metrics: MetricsConfig {
                    enable_metrics: true,
                    metrics_port: 9090,
                    export_interval_secs: 10,
                    retention_days: 7,
                },
                tracing: TracingConfig {
                    enabled: true,
                    level: TracingLevel::Info,
                    output: TracingOutput::Stdout,
                    sample_rate: 1.0,
                },
                health: HealthConfig {
                    enabled: true,
                    port: 8081,
                    path: "/health".to_string(),
                },
            },
            security: SecurityConfig {
                tls: TlsConfig {
                    enabled: false,
                    cert_path: Some("cert.pem".to_string()),
                    key_path: Some("key.pem".to_string()),
                    ca_path: None,
                    verify_client: false,
                },
                authentication: AuthenticationConfig {
                    enabled: false,
                    method: AuthenticationMethod::None,
                    token_secret: Some("a".repeat(32)),
                },
                access_control: AccessControlConfig {
                    enabled: false,
                    allow_list: vec![],
                    deny_list: vec![],
                },
            },
        }
    }

    #[test]
    fn test_valid_configuration() {
        let validator = ConfigValidator::with_context(create_test_context());
        let config = create_valid_config();
        assert!(validator.validate(&config).is_ok());
    }

    #[test]
    fn test_invalid_bind_address() {
        let validator = ConfigValidator::with_context(create_test_context());
        let mut config = create_valid_config();
        config.network.bind_address = "invalid-address".to_string();
        assert!(validator.validate(&config).is_err());
    }

    #[test]
    fn test_port_conflict() {
        let validator = ConfigValidator::with_context(create_test_context());
        let mut config = create_valid_config();
        config.observability.metrics.metrics_port = config.network.port;
        assert!(validator.validate(&config).is_err());
    }

    #[test]
    fn test_performance_warnings() {
        let validator = ConfigValidator::with_context(create_test_context());
        let mut config = create_valid_config();
        config.performance.worker_threads = Some(32); // More than 2x CPU cores
        // Should pass validation but generate warnings
        assert!(validator.validate(&config).is_ok());
    }
}