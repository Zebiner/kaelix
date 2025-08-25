//! # High Availability Failover Client
//!
//! Provides intelligent failover capabilities for cluster clients with automatic
//! endpoint discovery, health monitoring, and request routing.

use crate::{
    ha::{HaError, HaResult, TopologyChange},
};

use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Configuration for client failover behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverClientConfig {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    /// Initial retry delay
    pub initial_retry_delay: Duration,
    /// Maximum retry delay (for exponential backoff)
    pub max_retry_delay: Duration,
    /// Health check interval for endpoints
    pub health_check_interval: Duration,
    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: usize,
    /// Circuit breaker recovery timeout
    pub circuit_breaker_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
}

impl Default for FailoverClientConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay: Duration::from_millis(100),
            max_retry_delay: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(5),
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout: Duration::from_secs(60),
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// Retry configuration for failed requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: usize,
    /// Base delay between retries
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0 to 1.0)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

/// Health status of an endpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointHealth {
    /// Endpoint is healthy and available
    Healthy,
    /// Endpoint is degraded but still usable
    Degraded,
    /// Endpoint has failed and should not be used
    Failed,
    /// Endpoint is recovering from failure
    Recovering,
}

/// Information about a single endpoint
#[derive(Debug)]
struct EndpointInfo {
    /// Network address
    address: SocketAddr,
    /// Current health status
    health: EndpointHealth,
    /// Number of consecutive failures
    consecutive_failures: AtomicUsize,
    /// Last successful request timestamp
    last_success: Option<Instant>,
    /// Last failure timestamp
    last_failure: Option<Instant>,
    /// Response time history (for latency tracking)
    response_times: VecDeque<Duration>,
    /// Total requests sent to this endpoint
    total_requests: AtomicU64,
    /// Total successful requests
    successful_requests: AtomicU64,
}

impl Clone for EndpointInfo {
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            health: self.health,
            consecutive_failures: AtomicUsize::new(self.consecutive_failures.load(Ordering::Relaxed)),
            last_success: self.last_success,
            last_failure: self.last_failure,
            response_times: self.response_times.clone(),
            total_requests: AtomicU64::new(self.total_requests.load(Ordering::Relaxed)),
            successful_requests: AtomicU64::new(self.successful_requests.load(Ordering::Relaxed)),
        }
    }
}

impl EndpointInfo {
    fn new(address: SocketAddr) -> Self {
        Self {
            address,
            health: EndpointHealth::Healthy,
            consecutive_failures: AtomicUsize::new(0),
            last_success: None,
            last_failure: None,
            response_times: VecDeque::with_capacity(100), // Keep last 100 response times
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
        }
    }

    fn record_success(&mut self, response_time: Duration) {
        self.health = EndpointHealth::Healthy;
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.last_success = Some(Instant::now());
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
        
        // Update response time history
        if self.response_times.len() >= 100 {
            self.response_times.pop_front();
        }
        self.response_times.push_back(response_time);
    }

    fn record_failure(&mut self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        self.last_failure = Some(Instant::now());
        
        // Update health based on failure count
        self.health = match failures {
            1..=2 => EndpointHealth::Degraded,
            _ => EndpointHealth::Failed,
        };
    }

    fn is_healthy(&self) -> bool {
        matches!(self.health, EndpointHealth::Healthy | EndpointHealth::Recovering)
    }

    fn average_response_time(&self) -> Option<Duration> {
        if self.response_times.is_empty() {
            None
        } else {
            let total: Duration = self.response_times.iter().sum();
            Some(total / self.response_times.len() as u32)
        }
    }

    fn success_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            1.0
        } else {
            let successful = self.successful_requests.load(Ordering::Relaxed);
            successful as f64 / total as f64
        }
    }
}

/// Performance metrics for failover client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverClientMetrics {
    /// Total requests made
    pub total_requests: u64,
    /// Total successful requests
    pub successful_requests: u64,
    /// Total failed requests
    pub failed_requests: u64,
    /// Total retries attempted
    pub total_retries: u64,
    /// Average response time
    pub average_response_time: Duration,
    /// Current active endpoints
    pub active_endpoints: usize,
    /// Current failed endpoints
    pub failed_endpoints: usize,
}

impl Default for FailoverClientMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_retries: 0,
            average_response_time: Duration::ZERO,
            active_endpoints: 0,
            failed_endpoints: 0,
        }
    }
}

/// High-level request result with retry information
#[derive(Debug)]
pub struct RequestResult<T> {
    /// The actual response
    pub response: T,
    /// Which endpoint was used
    pub endpoint: SocketAddr,
    /// Total time taken including retries
    pub total_duration: Duration,
    /// Number of retries required
    pub retries: usize,
}

/// High availability client with automatic failover
pub struct FailoverClient {
    /// Client configuration
    config: FailoverClientConfig,
    /// Available endpoints with health information
    endpoints: Arc<RwLock<Vec<EndpointInfo>>>,
    /// Topology change receiver
    topology_rx: Option<broadcast::Receiver<TopologyChange>>,
    /// Client metrics
    metrics: Arc<RwLock<FailoverClientMetrics>>,
}

impl FailoverClient {
    /// Create a new failover client
    pub fn new(
        config: FailoverClientConfig,
        initial_endpoints: Vec<SocketAddr>,
        topology_rx: Option<broadcast::Receiver<TopologyChange>>,
    ) -> Self {
        let endpoints = initial_endpoints
            .into_iter()
            .map(EndpointInfo::new)
            .collect();

        Self {
            config,
            endpoints: Arc::new(RwLock::new(endpoints)),
            topology_rx,
            metrics: Arc::new(RwLock::new(FailoverClientMetrics::default())),
        }
    }

    /// Start the failover client
    pub async fn start(&mut self) -> HaResult<()> {
        info!("Starting HA failover client");
        
        // Start topology monitoring if available
        if let Some(topology_rx) = self.topology_rx.take() {
            self.start_topology_monitoring(topology_rx).await?;
        }

        // Start health monitoring
        self.start_health_monitoring().await?;

        Ok(())
    }

    /// Start monitoring topology changes
    async fn start_topology_monitoring(&self, mut topology_rx: broadcast::Receiver<TopologyChange>) -> HaResult<()> {
        let endpoints = Arc::clone(&self.endpoints);
        
        tokio::spawn(async move {
            while let Ok(change) = topology_rx.recv().await {
                debug!("Received topology change: {:?}", change);
                
                let mut endpoints_guard = endpoints.write().unwrap();
                
                match change {
                    TopologyChange::NodeAdded { address, .. } => {
                        // Add new endpoint if not already present
                        if !endpoints_guard.iter().any(|e| e.address == address) {
                            endpoints_guard.push(EndpointInfo::new(address));
                            info!("Added new endpoint: {}", address);
                        }
                    },
                    TopologyChange::NodeRemoved { address, .. } => {
                        // Remove endpoint
                        endpoints_guard.retain(|e| e.address != address);
                        info!("Removed endpoint: {}", address);
                    },
                    TopologyChange::NodeUpdated { address, .. } => {
                        // Update existing endpoint (currently just mark as recovering)
                        if let Some(endpoint) = endpoints_guard.iter_mut().find(|e| e.address == address) {
                            endpoint.health = EndpointHealth::Recovering;
                            info!("Updated endpoint: {}", address);
                        }
                    },
                }
            }
        });

        Ok(())
    }

    /// Start health monitoring for endpoints
    async fn start_health_monitoring(&self) -> HaResult<()> {
        let endpoints = Arc::clone(&self.endpoints);
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.health_check_interval);
            
            loop {
                interval.tick().await;
                
                // Perform health checks
                let mut endpoints_guard = endpoints.write().unwrap();
                for endpoint in endpoints_guard.iter_mut() {
                    // Simple health check - in reality, you'd ping the endpoint
                    if endpoint.health == EndpointHealth::Failed {
                        // Check if enough time has passed for recovery
                        if let Some(last_failure) = endpoint.last_failure {
                            if last_failure.elapsed() > config.circuit_breaker_timeout {
                                endpoint.health = EndpointHealth::Recovering;
                                debug!("Endpoint {} marked for recovery", endpoint.address);
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Select the best available endpoint
    fn select_endpoint(&self) -> HaResult<SocketAddr> {
        let endpoints = self.endpoints.read().unwrap();
        
        // First, try to find healthy endpoints
        let healthy_endpoints: Vec<_> = endpoints
            .iter()
            .filter(|e| e.is_healthy())
            .collect();

        if !healthy_endpoints.is_empty() {
            // Select endpoint with best success rate and lowest latency
            let best_endpoint = healthy_endpoints
                .iter()
                .min_by(|a, b| {
                    let a_score = Self::endpoint_score(a);
                    let b_score = Self::endpoint_score(b);
                    a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap();
            
            return Ok(best_endpoint.address);
        }

        // If no healthy endpoints, try recovering ones
        let recovering_endpoints: Vec<_> = endpoints
            .iter()
            .filter(|e| e.health == EndpointHealth::Recovering)
            .collect();

        if !recovering_endpoints.is_empty() {
            return Ok(recovering_endpoints[0].address);
        }

        // As a last resort, try degraded endpoints
        let degraded_endpoints: Vec<_> = endpoints
            .iter()
            .filter(|e| e.health == EndpointHealth::Degraded)
            .collect();

        if !degraded_endpoints.is_empty() {
            return Ok(degraded_endpoints[0].address);
        }

        Err(HaError::NoHealthyEndpoints)
    }

    /// Calculate a score for endpoint selection (lower is better)
    fn endpoint_score(endpoint: &EndpointInfo) -> f64 {
        let success_rate = endpoint.success_rate();
        let avg_response_time = endpoint.average_response_time()
            .unwrap_or(Duration::from_millis(100))
            .as_millis() as f64;
        
        // Combine success rate (inverted) and response time
        (1.0 - success_rate) * 1000.0 + avg_response_time
    }

    /// Execute a request with automatic failover and retries
    pub async fn execute_request<F, T, E>(
        &self,
        request_fn: F,
    ) -> HaResult<RequestResult<T>>
    where
        F: Fn(SocketAddr) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>,
        E: std::fmt::Debug,
    {
        let start_time = Instant::now();
        let mut retries = 0;

        loop {
            // Select an endpoint
            let endpoint = self.select_endpoint()?;
            
            debug!("Attempting request to endpoint: {}", endpoint);
            
            // Record request attempt
            {
                let mut endpoints_guard = self.endpoints.write().unwrap();
                if let Some(endpoint_info) = endpoints_guard.iter_mut().find(|e| e.address == endpoint) {
                    endpoint_info.total_requests.fetch_add(1, Ordering::Relaxed);
                }
            }
            
            // Execute the request
            let request_start = Instant::now();
            match tokio::time::timeout(self.config.request_timeout, request_fn(endpoint)).await {
                Ok(Ok(response)) => {
                    // Success - record metrics and return
                    let request_duration = request_start.elapsed();
                    
                    {
                        let mut endpoints_guard = self.endpoints.write().unwrap();
                        if let Some(endpoint_info) = endpoints_guard.iter_mut().find(|e| e.address == endpoint) {
                            endpoint_info.record_success(request_duration);
                        }
                    }
                    
                    // Update metrics
                    {
                        let mut metrics = self.metrics.write().unwrap();
                        metrics.total_requests += 1;
                        metrics.successful_requests += 1;
                        metrics.total_retries += retries;
                    }
                    
                    return Ok(RequestResult {
                        response,
                        endpoint,
                        total_duration: start_time.elapsed(),
                        retries,
                    });
                },
                Ok(Err(e)) => {
                    // Request failed - record failure
                    warn!("Request to {} failed: {:?}", endpoint, e);
                    self.record_endpoint_failure(endpoint).await;
                },
                Err(_) => {
                    // Timeout
                    warn!("Request to {} timed out", endpoint);
                    self.record_endpoint_failure(endpoint).await;
                },
            }
            
            // Check if we should retry
            if retries >= self.config.max_retries {
                let mut metrics = self.metrics.write().unwrap();
                metrics.total_requests += 1;
                metrics.failed_requests += 1;
                metrics.total_retries += retries;
                
                return Err(HaError::AllEndpointsFailed);
            }
            
            // Calculate retry delay with exponential backoff and jitter
            retries += 1;
            let delay = self.calculate_retry_delay(retries);
            debug!("Retrying in {:?} (attempt {})", delay, retries + 1);
            tokio::time::sleep(delay).await;
        }
    }

    /// Record an endpoint failure
    async fn record_endpoint_failure(&self, endpoint: SocketAddr) {
        let mut endpoints_guard = self.endpoints.write().unwrap();
        if let Some(endpoint_info) = endpoints_guard.iter_mut().find(|e| e.address == endpoint) {
            endpoint_info.record_failure();
        }
    }

    /// Calculate retry delay with exponential backoff and jitter
    fn calculate_retry_delay(&self, retry_count: usize) -> Duration {
        let base_delay = self.config.initial_retry_delay.as_millis() as f64;
        let max_delay = self.config.max_retry_delay.as_millis() as f64;
        
        // Exponential backoff
        let delay = base_delay * 2.0_f64.powi(retry_count as i32 - 1);
        let delay = delay.min(max_delay);
        
        // Add jitter
        let jitter = delay * 0.1 * (rand::random::<f64>() * 2.0 - 1.0);
        let final_delay = (delay + jitter).max(base_delay);
        
        Duration::from_millis(final_delay as u64)
    }

    /// Get current metrics
    pub fn metrics(&self) -> FailoverClientMetrics {
        let metrics = self.metrics.read().unwrap();
        let endpoints = self.endpoints.read().unwrap();
        
        let active_endpoints = endpoints.iter().filter(|e| e.is_healthy()).count();
        let failed_endpoints = endpoints.iter().filter(|e| e.health == EndpointHealth::Failed).count();
        
        FailoverClientMetrics {
            active_endpoints,
            failed_endpoints,
            ..metrics.clone()
        }
    }

    /// Get current endpoint statuses
    pub fn endpoint_status(&self) -> Vec<(SocketAddr, EndpointHealth, f64)> {
        let endpoints = self.endpoints.read().unwrap();
        endpoints
            .iter()
            .map(|e| (e.address, e.health, e.success_rate()))
            .collect()
    }

    /// Add a new endpoint
    pub async fn add_endpoint(&self, address: SocketAddr) -> HaResult<()> {
        let mut endpoints = self.endpoints.write().unwrap();
        
        if endpoints.iter().any(|e| e.address == address) {
            return Err(HaError::EndpointAlreadyExists);
        }
        
        endpoints.push(EndpointInfo::new(address));
        info!("Added endpoint: {}", address);
        Ok(())
    }

    /// Remove an endpoint
    pub async fn remove_endpoint(&self, address: SocketAddr) -> HaResult<()> {
        let mut endpoints = self.endpoints.write().unwrap();
        let initial_len = endpoints.len();
        
        endpoints.retain(|e| e.address != address);
        
        if endpoints.len() == initial_len {
            return Err(HaError::EndpointNotFound);
        }
        
        info!("Removed endpoint: {}", address);
        Ok(())
    }

    /// Stop the failover client
    pub async fn stop(&self) -> HaResult<()> {
        info!("Stopping HA failover client");
        // In a real implementation, we would stop background tasks here
        Ok(())
    }
}

impl std::fmt::Debug for FailoverClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FailoverClient")
            .field("config", &self.config)
            .field("endpoint_count", &self.endpoints.read().unwrap().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_failover_client_creation() {
        let config = FailoverClientConfig::default();
        let endpoints = vec![
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081),
        ];
        
        let client = FailoverClient::new(config, endpoints, None);
        assert_eq!(client.endpoint_status().len(), 2);
    }

    #[test]
    fn test_endpoint_score_calculation() {
        let endpoint = EndpointInfo::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
        );
        
        let score = FailoverClient::endpoint_score(&endpoint);
        assert!(score >= 0.0);
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = FailoverClientConfig::default();
        let client = FailoverClient::new(config, vec![], None);
        
        let delay1 = client.calculate_retry_delay(1);
        let delay2 = client.calculate_retry_delay(2);
        let delay3 = client.calculate_retry_delay(3);
        
        // Should increase with retry count (approximately)
        assert!(delay2 >= delay1);
        assert!(delay3 >= delay2);
    }

    #[tokio::test]
    async fn test_endpoint_management() {
        let client = FailoverClient::new(FailoverClientConfig::default(), vec![], None);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        
        // Add endpoint
        assert!(client.add_endpoint(addr).await.is_ok());
        assert_eq!(client.endpoint_status().len(), 1);
        
        // Try to add same endpoint again
        assert!(client.add_endpoint(addr).await.is_err());
        
        // Remove endpoint
        assert!(client.remove_endpoint(addr).await.is_ok());
        assert_eq!(client.endpoint_status().len(), 0);
        
        // Try to remove non-existent endpoint
        assert!(client.remove_endpoint(addr).await.is_err());
    }
}