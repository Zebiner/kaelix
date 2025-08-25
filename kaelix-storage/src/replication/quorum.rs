use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::join_all;
use kaelix_cluster::types::NodeId;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::RwLock,
    time::timeout,
};
use tracing::{debug, info, warn};

use super::{ReplicationError, Result};

/// Quorum configuration for read and write operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumConfig {
    /// Minimum number of replicas required for write operations
    pub write_quorum: usize,
    /// Minimum number of replicas required for read operations
    pub read_quorum: usize,
    /// Total number of replicas in the system
    pub total_replicas: usize,
    /// Timeout for quorum operations
    pub operation_timeout: Duration,
    /// Whether to use strict quorum (fail if not enough replicas available)
    pub strict_mode: bool,
}

impl Default for QuorumConfig {
    fn default() -> Self {
        Self {
            write_quorum: 2,
            read_quorum: 1,
            total_replicas: 3,
            operation_timeout: Duration::from_millis(1000),
            strict_mode: true,
        }
    }
}

impl QuorumConfig {
    /// Validate quorum configuration
    pub fn validate(&self) -> Result<()> {
        if self.write_quorum == 0 {
            return Err(ReplicationError::ConfigurationError(
                "write_quorum must be greater than 0".to_string(),
            ));
        }
        
        if self.read_quorum == 0 {
            return Err(ReplicationError::ConfigurationError(
                "read_quorum must be greater than 0".to_string(),
            ));
        }
        
        if self.write_quorum > self.total_replicas {
            return Err(ReplicationError::ConfigurationError(
                "write_quorum cannot exceed total_replicas".to_string(),
            ));
        }
        
        if self.read_quorum > self.total_replicas {
            return Err(ReplicationError::ConfigurationError(
                "read_quorum cannot exceed total_replicas".to_string(),
            ));
        }
        
        // Ensure we can achieve strong consistency if needed
        if self.write_quorum + self.read_quorum <= self.total_replicas {
            warn!("Quorum configuration may not guarantee strong consistency");
        }
        
        Ok(())
    }
    
    /// Check if configuration supports strong consistency
    pub fn supports_strong_consistency(&self) -> bool {
        self.write_quorum + self.read_quorum > self.total_replicas
    }
}

/// Result of a quorum operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumResult {
    /// Number of successful responses
    pub successful_responses: usize,
    /// Number of failed responses
    pub failed_responses: usize,
    /// Time taken for the operation
    pub operation_time: Duration,
    /// Whether quorum was achieved
    pub quorum_achieved: bool,
    /// List of responding replicas
    pub responding_replicas: Vec<NodeId>,
    /// List of failed replicas
    pub failed_replicas: Vec<NodeId>,
}

/// Data returned from a read operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResponse {
    /// The actual data
    pub data: Option<Vec<u8>>,
    /// Version/timestamp of the data
    pub version: u64,
    /// Node that provided this response
    pub source_node: NodeId,
    /// Timestamp when data was written
    pub timestamp: u64,
}

/// Response from a write operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResponse {
    /// Whether the write was successful
    pub success: bool,
    /// Version assigned to the written data
    pub version: u64,
    /// Node that handled the write
    pub source_node: NodeId,
    /// Any error message if write failed
    pub error: Option<String>,
}

/// Manages quorum-based operations for strong consistency
pub struct QuorumManager {
    /// Quorum configuration
    config: QuorumConfig,
    /// Available replicas
    available_replicas: Arc<RwLock<HashSet<NodeId>>>,
    /// Performance metrics
    metrics: Arc<RwLock<QuorumMetrics>>,
}

/// Performance metrics for quorum operations
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct QuorumMetrics {
    /// Total read operations attempted
    pub total_reads: u64,
    /// Total write operations attempted
    pub total_writes: u64,
    /// Successful reads that achieved quorum
    pub successful_reads: u64,
    /// Successful writes that achieved quorum
    pub successful_writes: u64,
    /// Average read latency in microseconds
    pub avg_read_latency_us: u64,
    /// Average write latency in microseconds
    pub avg_write_latency_us: u64,
    /// Number of quorum failures
    pub quorum_failures: u64,
    /// Number of timeout failures
    pub timeout_failures: u64,
}

impl QuorumManager {
    /// Create a new quorum manager
    pub fn new(config: QuorumConfig) -> Result<Self> {
        config.validate()?;
        
        Ok(Self {
            config,
            available_replicas: Arc::new(RwLock::new(HashSet::new())),
            metrics: Arc::new(RwLock::new(QuorumMetrics::default())),
        })
    }
    
    /// Update the set of available replicas
    pub async fn update_available_replicas(&self, replicas: HashSet<NodeId>) {
        let mut available = self.available_replicas.write().await;
        *available = replicas;
        
        debug!("Updated available replicas: {} nodes", available.len());
    }
    
    /// Add a replica to the available set
    pub async fn add_replica(&self, replica: NodeId) {
        let mut available = self.available_replicas.write().await;
        available.insert(replica);
        
        debug!("Added replica: {:?}", replica);
    }
    
    /// Remove a replica from the available set
    pub async fn remove_replica(&self, replica: &NodeId) {
        let mut available = self.available_replicas.write().await;
        available.remove(replica);
        
        debug!("Removed replica: {:?}", replica);
    }
    
    /// Perform a quorum read operation
    pub async fn quorum_read<F, Fut>(
        &self,
        key: &str,
        read_fn: F,
    ) -> Result<(Option<Vec<u8>>, QuorumResult)>
    where
        F: Fn(NodeId, String) -> Fut + Send + Sync + Clone,
        Fut: std::future::Future<Output = Result<ReadResponse>> + Send,
    {
        let start_time = Instant::now();
        let available_replicas: Vec<NodeId> = {
            let replicas = self.available_replicas.read().await;
            replicas.iter().cloned().collect()
        };
        
        if available_replicas.len() < self.config.read_quorum {
            let mut metrics = self.metrics.write().await;
            metrics.total_reads += 1;
            metrics.quorum_failures += 1;
            
            return Err(ReplicationError::QuorumNotAvailable(format!(
                "Not enough replicas available: {} < {}",
                available_replicas.len(),
                self.config.read_quorum
            )));
        }
        
        // Select replicas for read (could be all or a subset for optimization)
        let selected_replicas = if available_replicas.len() <= self.config.total_replicas {
            available_replicas
        } else {
            available_replicas.into_iter().take(self.config.total_replicas).collect()
        };
        
        // Execute reads in parallel
        let read_futures: Vec<_> = selected_replicas
            .iter()
            .map(|&replica| {
                let read_fn = read_fn.clone();
                let key = key.to_string();
                async move {
                    let result = timeout(
                        self.config.operation_timeout,
                        read_fn(replica, key),
                    ).await;
                    
                    match result {
                        Ok(Ok(response)) => Ok((replica, response)),
                        Ok(Err(e)) => Err((replica, e)),
                        Err(_) => Err((replica, ReplicationError::OperationTimeout)),
                    }
                }
            })
            .collect();
        
        let results = join_all(read_futures).await;
        
        // Process results
        let mut successful_responses = Vec::new();
        let mut failed_replicas = Vec::new();
        
        for result in results {
            match result {
                Ok((replica, response)) => {
                    successful_responses.push((replica, response));
                }
                Err((replica, _error)) => {
                    failed_replicas.push(replica);
                }
            }
        }
        
        let operation_time = start_time.elapsed();
        let quorum_achieved = successful_responses.len() >= self.config.read_quorum;
        
        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_reads += 1;
            if quorum_achieved {
                metrics.successful_reads += 1;
            } else {
                metrics.quorum_failures += 1;
            }
            
            let latency_us = operation_time.as_micros() as u64;
            let total_successful = metrics.successful_reads;
            if total_successful > 0 {
                metrics.avg_read_latency_us = (metrics.avg_read_latency_us * (total_successful - 1) + latency_us) / total_successful;
            }
        }
        
        if !quorum_achieved {
            return Err(ReplicationError::QuorumNotAchieved(format!(
                "Read quorum not achieved: {} < {}",
                successful_responses.len(),
                self.config.read_quorum
            )));
        }
        
        // Find the latest version among successful responses
        let latest_response = successful_responses
            .iter()
            .max_by_key(|(_, response)| response.version)
            .map(|(_, response)| response.clone());
        
        let result = QuorumResult {
            successful_responses: successful_responses.len(),
            failed_responses: failed_replicas.len(),
            operation_time,
            quorum_achieved,
            responding_replicas: successful_responses.into_iter().map(|(id, _)| id).collect(),
            failed_replicas,
        };
        
        let data = latest_response.and_then(|r| r.data);
        Ok((data, result))
    }
    
    /// Perform a quorum write operation
    pub async fn quorum_write<F, Fut>(
        &self,
        key: &str,
        data: Vec<u8>,
        write_fn: F,
    ) -> Result<QuorumResult>
    where
        F: Fn(NodeId, String, Vec<u8>) -> Fut + Send + Sync + Clone,
        Fut: std::future::Future<Output = Result<WriteResponse>> + Send,
    {
        let start_time = Instant::now();
        let available_replicas: Vec<NodeId> = {
            let replicas = self.available_replicas.read().await;
            replicas.iter().cloned().collect()
        };
        
        if available_replicas.len() < self.config.write_quorum {
            let mut metrics = self.metrics.write().await;
            metrics.total_writes += 1;
            metrics.quorum_failures += 1;
            
            return Err(ReplicationError::QuorumNotAvailable(format!(
                "Not enough replicas available for write: {} < {}",
                available_replicas.len(),
                self.config.write_quorum
            )));
        }
        
        // Execute writes in parallel to all available replicas
        let write_futures: Vec<_> = available_replicas
            .iter()
            .map(|&replica| {
                let write_fn = write_fn.clone();
                let key = key.to_string();
                let data = data.clone();
                async move {
                    let result = timeout(
                        self.config.operation_timeout,
                        write_fn(replica, key, data),
                    ).await;
                    
                    match result {
                        Ok(Ok(response)) => Ok((replica, response)),
                        Ok(Err(e)) => Err((replica, e)),
                        Err(_) => Err((replica, ReplicationError::OperationTimeout)),
                    }
                }
            })
            .collect();
        
        let results = join_all(write_futures).await;
        
        // Process results
        let mut successful_responses = Vec::new();
        let mut failed_replicas = Vec::new();
        
        for result in results {
            match result {
                Ok((replica, response)) => {
                    if response.success {
                        successful_responses.push((replica, response));
                    } else {
                        failed_replicas.push(replica);
                    }
                }
                Err((replica, _error)) => {
                    failed_replicas.push(replica);
                }
            }
        }
        
        let operation_time = start_time.elapsed();
        let quorum_achieved = successful_responses.len() >= self.config.write_quorum;
        
        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_writes += 1;
            if quorum_achieved {
                metrics.successful_writes += 1;
            } else {
                metrics.quorum_failures += 1;
            }
            
            let latency_us = operation_time.as_micros() as u64;
            let total_successful = metrics.successful_writes;
            if total_successful > 0 {
                metrics.avg_write_latency_us = (metrics.avg_write_latency_us * (total_successful - 1) + latency_us) / total_successful;
            }
        }
        
        if !quorum_achieved {
            return Err(ReplicationError::QuorumNotAchieved(format!(
                "Write quorum not achieved: {} < {}",
                successful_responses.len(),
                self.config.write_quorum
            )));
        }
        
        info!("Write quorum achieved: {}/{} replicas", 
              successful_responses.len(), available_replicas.len());
        
        Ok(QuorumResult {
            successful_responses: successful_responses.len(),
            failed_responses: failed_replicas.len(),
            operation_time,
            quorum_achieved,
            responding_replicas: successful_responses.into_iter().map(|(id, _)| id).collect(),
            failed_replicas,
        })
    }
    
    /// Get current metrics
    pub async fn metrics(&self) -> QuorumMetrics {
        self.metrics.read().await.clone()
    }
    
    /// Get current configuration
    pub fn config(&self) -> &QuorumConfig {
        &self.config
    }
    
    /// Check if enough replicas are available for reads
    pub async fn can_read(&self) -> bool {
        let available = self.available_replicas.read().await;
        available.len() >= self.config.read_quorum
    }
    
    /// Check if enough replicas are available for writes
    pub async fn can_write(&self) -> bool {
        let available = self.available_replicas.read().await;
        available.len() >= self.config.write_quorum
    }
    
    /// Get the current number of available replicas
    pub async fn available_replica_count(&self) -> usize {
        let available = self.available_replicas.read().await;
        available.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_quorum_config_validation() {
        let mut config = QuorumConfig::default();
        assert!(config.validate().is_ok());
        
        config.write_quorum = 0;
        assert!(config.validate().is_err());
        
        config.write_quorum = 5;
        config.total_replicas = 3;
        assert!(config.validate().is_err());
    }
    
    #[tokio::test]
    async fn test_quorum_manager_creation() {
        let config = QuorumConfig::default();
        let manager = QuorumManager::new(config).unwrap();
        
        assert!(!manager.can_read().await);
        assert!(!manager.can_write().await);
    }
    
    #[tokio::test]
    async fn test_replica_management() {
        let config = QuorumConfig::default();
        let manager = QuorumManager::new(config).unwrap();
        
        let node1 = NodeId::generate();
        let node2 = NodeId::generate();
        
        manager.add_replica(node1).await;
        assert_eq!(manager.available_replica_count().await, 1);
        
        manager.add_replica(node2).await;
        assert_eq!(manager.available_replica_count().await, 2);
        
        manager.remove_replica(&node1).await;
        assert_eq!(manager.available_replica_count().await, 1);
    }
}