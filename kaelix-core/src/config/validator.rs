//! # Configuration Validation
//!
//! Validates configuration settings for consistency, safety, and correctness.

use crate::{
    Error, Result,
    config::schema::{MemoryStreamerConfig, ValidationContext},
};
use std::net::SocketAddr;
use tracing::{debug, warn};
use validator::{Validate, ValidationError, ValidationErrors};

/// Configuration validator with comprehensive validation rules
pub struct ConfigValidator {
    /// Validation context with system information
    context: ValidationContext,
}

impl ConfigValidator {
    /// Create a new configuration validator
    pub fn new() -> Self {
        Self { context: ValidationContext::detect() }
    }

    /// Create validator with custom context
    pub fn with_context(context: ValidationContext) -> Self {
        Self { context }
    }

    /// Validate a configuration
    pub fn validate(config: &MemoryStreamerConfig) -> Result<()> {
        let validator = Self::new();
        validator.validate_config(config)
    }

    /// Perform comprehensive validation of the configuration
    pub fn validate_config(&self, config: &MemoryStreamerConfig) -> Result<()> {
        debug!("Starting configuration validation");

        // Basic field validation using validator crate
        config.validate().map_err(Error::from)?;

        // Custom business logic validation
        self.validate_network_config(config)?;
        self.validate_performance_config(config)?;
        self.validate_security_config(config)?;
        self.validate_telemetry_config(config)?;
        self.validate_storage_config(config)?;
        self.validate_integration_constraints(config)?;

        debug!("Configuration validation completed successfully");
        Ok(())
    }

    /// Validate network configuration
    fn validate_network_config(&self, config: &MemoryStreamerConfig) -> Result<()> {
        // Validate host/port combinations
        let bind_addr = format!("{}:{}", config.network.host, config.network.port);
        bind_addr
            .parse::<SocketAddr>()
            .map_err(|e| Error::Configuration(format!("Invalid bind address {bind_addr}: {e}")))?;

        // Check port ranges
        if config.network.port < 1024 && config.network.host != "localhost" {
            warn!("Using privileged port {} may require special permissions", config.network.port);
        }

        // Note: No need to check port > 65535 since u16 max is 65535

        // Validate connection limits
        if config.network.max_connections == 0 {
            return Err(Error::Configuration(
                "Maximum connections must be greater than 0".to_string(),
            ));
        }

        if config.network.max_connections > 1_000_000 {
            warn!("Very high connection limit may cause resource exhaustion");
        }

        // Validate timeout values
        if config.network.read_timeout_ms == 0 || config.network.write_timeout_ms == 0 {
            return Err(Error::Configuration(
                "Network timeouts must be greater than 0".to_string(),
            ));
        }

        if config.network.read_timeout_ms > 300_000 || config.network.write_timeout_ms > 300_000 {
            warn!("Network timeouts over 5 minutes may cause poor user experience");
        }

        Ok(())
    }

    /// Validate performance configuration
    fn validate_performance_config(&self, config: &MemoryStreamerConfig) -> Result<()> {
        // Validate worker thread count (performance config is under telemetry)
        let worker_threads =
            config.telemetry.performance.worker_threads.unwrap_or(self.context.cpu_cores);

        if worker_threads == 0 {
            return Err(Error::Configuration("Worker threads must be greater than 0".to_string()));
        }

        if worker_threads > self.context.cpu_cores * 4 {
            warn!(
                "Worker threads ({worker_threads}) significantly exceed CPU cores ({}), may cause contention",
                self.context.cpu_cores
            );
        }

        // Validate buffer sizes
        if config.telemetry.performance.buffer_size == 0 {
            return Err(Error::Configuration("Buffer size must be greater than 0".to_string()));
        }

        // Check for power-of-2 buffer sizes for optimal performance
        if !config.telemetry.performance.buffer_size.is_power_of_two() {
            debug!("Buffer size is not power of 2, may impact performance");
        }

        // Validate batch sizes
        if config.telemetry.performance.batch_size > config.telemetry.performance.buffer_size as u32
        {
            return Err(Error::Configuration("Batch size cannot exceed buffer size".to_string()));
        }

        // Memory validation
        let estimated_memory_mb = self.estimate_memory_usage(config);
        if estimated_memory_mb > self.context.available_memory_mb * 80 / 100 {
            warn!(
                "Configuration may use {estimated_memory_mb}MB memory, which is {}% of available {}MB",
                (estimated_memory_mb * 100) / self.context.available_memory_mb,
                self.context.available_memory_mb
            );
        }

        Ok(())
    }

    /// Validate security configuration
    fn validate_security_config(&self, config: &MemoryStreamerConfig) -> Result<()> {
        // TLS configuration validation
        if config.security.tls.enabled {
            if config.security.tls.cert_path.is_empty() {
                return Err(Error::Configuration(
                    "TLS certificate path cannot be empty when TLS is enabled".to_string(),
                ));
            }

            if config.security.tls.key_path.is_empty() {
                return Err(Error::Configuration(
                    "TLS key path cannot be empty when TLS is enabled".to_string(),
                ));
            }
        }

        // Authentication validation
        if config.security.authentication.enabled {
            match config.security.authentication.method.as_str() {
                "token" | "jwt" | "oauth" => {
                    // Valid authentication methods
                },
                _ => {
                    return Err(Error::Configuration(format!(
                        "Unsupported authentication method: {}",
                        config.security.authentication.method
                    )));
                },
            }
        }

        // Rate limiting validation
        if config.security.rate_limiting.enabled {
            if config.security.rate_limiting.requests_per_second == 0 {
                return Err(Error::Configuration(
                    "Rate limit requests per second must be greater than 0".to_string(),
                ));
            }

            if config.security.rate_limiting.burst_size == 0 {
                return Err(Error::Configuration(
                    "Rate limit burst size must be greater than 0".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate telemetry configuration
    fn validate_telemetry_config(&self, config: &MemoryStreamerConfig) -> Result<()> {
        // Metrics configuration
        if config.telemetry.metrics.enabled && config.telemetry.metrics.prometheus.enabled {
            let metrics_port = config.telemetry.metrics.prometheus.port;

            if metrics_port == config.network.port {
                return Err(Error::Configuration(
                    "Metrics port cannot be the same as the main service port".to_string(),
                ));
            }

            if metrics_port < 1024 {
                warn!("Metrics port {metrics_port} may require special permissions");
            }
        }

        // Tracing configuration
        if config.telemetry.tracing.enabled && config.telemetry.tracing.jaeger.endpoint.is_empty() {
            return Err(Error::Configuration(
                "Jaeger endpoint cannot be empty when tracing is enabled".to_string(),
            ));
        }

        // Logging configuration
        if config.telemetry.logging.max_file_size_mb == 0 {
            return Err(Error::Configuration(
                "Log file max size must be greater than 0".to_string(),
            ));
        }

        if config.telemetry.logging.max_files == 0 {
            return Err(Error::Configuration("Max log files must be greater than 0".to_string()));
        }

        Ok(())
    }

    /// Validate storage configuration
    fn validate_storage_config(&self, config: &MemoryStreamerConfig) -> Result<()> {
        // Storage config is nested under runtime
        let storage_config = &config.runtime.storage;

        // Validate data directory
        if storage_config.data_dir.is_empty() {
            return Err(Error::Configuration("Data directory cannot be empty".to_string()));
        }

        // Validate retention policy
        if storage_config.retention_hours == 0 {
            return Err(Error::Configuration("Retention hours must be greater than 0".to_string()));
        }

        // Validate compression settings
        if storage_config.compression.enabled {
            match storage_config.compression.algorithm {
                crate::config::schema::CompressionAlgorithm::Lz4
                | crate::config::schema::CompressionAlgorithm::Snappy
                | crate::config::schema::CompressionAlgorithm::Zstd
                | crate::config::schema::CompressionAlgorithm::Gzip => {
                    // Valid compression algorithms
                },
                crate::config::schema::CompressionAlgorithm::None => {
                    warn!("Compression enabled but algorithm is set to None");
                },
            }

            if storage_config.compression.level > 9 {
                return Err(Error::Configuration("Compression level cannot exceed 9".to_string()));
            }
        }

        Ok(())
    }

    /// Validate integration constraints and cross-cutting concerns
    fn validate_integration_constraints(&self, config: &MemoryStreamerConfig) -> Result<()> {
        // Check if TLS and high performance settings are compatible
        let worker_threads =
            config.telemetry.performance.worker_threads.unwrap_or(self.context.cpu_cores);
        if config.security.tls.enabled && worker_threads > 16 {
            warn!("TLS with many worker threads may impact performance");
        }

        // Validate protocol configuration compatibility (protocol config is under network)
        if config.network.protocol.compression.enabled && config.security.tls.enabled {
            // TLS already provides compression, warn about double compression
            warn!("Both TLS and protocol compression enabled, may reduce efficiency");
        }

        // Check batch processing compatibility with network settings
        if config.network.protocol.batching.enabled {
            let max_batch_size = config.network.protocol.batching.max_batch_size;
            if max_batch_size * 1024 > config.network.max_frame_size as u32 {
                return Err(Error::Configuration(
                    "Maximum batch size may exceed network frame size limits".to_string(),
                ));
            }
        }

        // Validate resource allocation doesn't exceed system limits
        let total_connections = config.network.max_connections;
        let estimated_memory_per_conn_kb = 64; // Conservative estimate
        let estimated_connection_memory_mb =
            (total_connections * estimated_memory_per_conn_kb) / 1024;

        if estimated_connection_memory_mb as u64 > self.context.available_memory_mb / 2 {
            warn!(
                "Connection memory usage may exceed 50% of available memory ({estimated_connection_memory_mb} MB)"
            );
        }

        Ok(())
    }

    /// Estimate memory usage based on configuration
    fn estimate_memory_usage(&self, config: &MemoryStreamerConfig) -> u64 {
        let worker_threads =
            config.telemetry.performance.worker_threads.unwrap_or(self.context.cpu_cores) as u64;
        let buffer_size_kb = config.telemetry.performance.buffer_size as u64 / 1024;
        let max_connections = config.network.max_connections as u64;

        // Rough estimates in MB
        let worker_memory = worker_threads * buffer_size_kb / 1024; // Each worker has buffers
        let connection_memory = max_connections * 64 / 1024; // ~64KB per connection
        let base_memory = 100; // Base application memory

        worker_memory + connection_memory + base_memory
    }

    /// Validate configuration against environment constraints
    pub fn validate_environment_compatibility(
        &self,
        config: &MemoryStreamerConfig,
    ) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Check if the system can handle the configuration
        let estimated_memory = self.estimate_memory_usage(config);
        if estimated_memory > self.context.available_memory_mb {
            warnings.push(format!(
                "Configuration requires {estimated_memory}MB but only {}MB available",
                self.context.available_memory_mb
            ));
        }

        // Check CPU requirements
        let worker_threads =
            config.telemetry.performance.worker_threads.unwrap_or(self.context.cpu_cores);
        if worker_threads > self.context.cpu_cores {
            warnings.push(format!(
                "Configuration requires {worker_threads} worker threads but only {} CPU cores available",
                self.context.cpu_cores
            ));
        }

        // Check disk space for storage if enabled
        if config.runtime.storage.enabled {
            // We can't easily check available disk space without additional dependencies
            // This would be a good place to add such checks in a real implementation
            warnings.push("Disk space validation not implemented".to_string());
        }

        Ok(warnings)
    }

    /// Get the validation context
    pub fn context(&self) -> &ValidationContext {
        &self.context
    }
}

impl Default for ConfigValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Custom validation functions for use with the validator crate
/// Validate that a port is in the valid range
fn validate_port(port: u16) -> std::result::Result<(), ValidationError> {
    if port == 0 {
        return Err(ValidationError::new("port_zero"));
    }
    Ok(())
}

/// Validate that a path is not empty
fn validate_non_empty_path(path: &str) -> std::result::Result<(), ValidationError> {
    if path.trim().is_empty() {
        return Err(ValidationError::new("empty_path"));
    }
    Ok(())
}

/// Validate memory size is reasonable
fn validate_memory_size(size: u64) -> std::result::Result<(), ValidationError> {
    if size == 0 {
        return Err(ValidationError::new("zero_memory"));
    }
    if size > 1024 * 1024 * 1024 * 100 {
        // 100GB limit
        return Err(ValidationError::new("excessive_memory"));
    }
    Ok(())
}

/// Format validation errors for user display
pub fn format_validation_errors(errors: &ValidationErrors) -> Vec<String> {
    let mut formatted = Vec::new();

    for (_field, field_errors) in errors.field_errors() {
        for error in field_errors {
            let message = match error.code.as_ref() {
                "port_zero" => "Port cannot be zero".to_string(),
                "empty_path" => "Path cannot be empty".to_string(),
                "zero_memory" => "Memory size must be greater than zero".to_string(),
                "excessive_memory" => "Memory size is too large".to_string(),
                "length" => {
                    if let Some(min) = error.params.get("min") {
                        format!("Value must be at least {min} characters")
                    } else if let Some(max) = error.params.get("max") {
                        format!("Value must be at most {max} characters")
                    } else {
                        "Invalid length".to_string()
                    }
                },
                "range" => {
                    if let Some(min) = error.params.get("min") {
                        if let Some(max) = error.params.get("max") {
                            format!("Value must be between {min} and {max}")
                        } else {
                            format!("Value must be at least {min}")
                        }
                    } else if let Some(max) = error.params.get("max") {
                        format!("Value must be at most {max}")
                    } else {
                        "Value out of range".to_string()
                    }
                },
                _ => error.message.clone().unwrap_or_else(|| "Invalid value".into()).to_string(),
            };
            formatted.push(message);
        }
    }

    formatted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::MemoryStreamerConfig;

    #[test]
    fn test_validator_creation() {
        let validator = ConfigValidator::new();
        assert!(validator.context.cpu_cores > 0);
        assert!(validator.context.available_memory_mb > 0);
    }

    #[test]
    fn test_default_config_validation() {
        let config = MemoryStreamerConfig::default();
        let result = ConfigValidator::validate(&config);
        // Default configuration should be valid
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_port_validation() {
        let mut config = MemoryStreamerConfig::default();
        config.network.port = 0;

        let validator = ConfigValidator::new();
        let result = validator.validate_network_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_estimation() {
        let validator = ConfigValidator::new();
        let config = MemoryStreamerConfig::default();

        let memory_usage = validator.estimate_memory_usage(&config);
        assert!(memory_usage > 0);
    }

    #[test]
    fn test_environment_compatibility() {
        let validator = ConfigValidator::new();
        let config = MemoryStreamerConfig::default();

        let warnings = validator.validate_environment_compatibility(&config);
        assert!(warnings.is_ok());
    }

    #[test]
    fn test_custom_validation_functions() {
        assert!(validate_port(8080).is_ok());
        assert!(validate_port(0).is_err());

        assert!(validate_non_empty_path("/valid/path").is_ok());
        assert!(validate_non_empty_path("").is_err());
        assert!(validate_non_empty_path("   ").is_err());

        assert!(validate_memory_size(1024).is_ok());
        assert!(validate_memory_size(0).is_err());
    }

    #[test]
    fn test_format_validation_errors() {
        let mut errors = ValidationErrors::new();
        errors.add("port", ValidationError::new("port_zero"));
        errors.add("path", ValidationError::new("empty_path"));

        let formatted = format_validation_errors(&errors);
        assert_eq!(formatted.len(), 2);
        assert!(formatted.contains(&"Port cannot be zero".to_string()));
        assert!(formatted.contains(&"Path cannot be empty".to_string()));
    }
}
