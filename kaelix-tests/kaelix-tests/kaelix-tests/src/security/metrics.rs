use std::time::Duration;
use measure_time::measure_time;

/// Security performance metrics tracking
pub struct SecurityMetrics {
    acl_decision_time: Duration,
    encryption_overhead: f64,
    authentication_latency: Duration,
}

impl SecurityMetrics {
    pub fn new() -> Self {
        Self {
            acl_decision_time: Duration::ZERO,
            encryption_overhead: 0.0,
            authentication_latency: Duration::ZERO,
        }
    }

    /// Measure ACL decision performance
    pub fn measure_acl_decision(&mut self, test_fn: impl FnOnce()) {
        let (result, duration) = measure_time(test_fn);
        self.acl_decision_time = duration;
    }

    /// Track encryption performance overhead
    pub fn track_encryption_overhead(&mut self, throughput_reduction: f64) {
        self.encryption_overhead = throughput_reduction;
    }

    /// Measure authentication latency
    pub fn measure_authentication_latency(&mut self, test_fn: impl FnOnce()) {
        let (result, duration) = measure_time(test_fn);
        self.authentication_latency = duration;
    }

    /// Performance validation reports
    pub fn validate_performance(&self) -> bool {
        self.acl_decision_time < Duration::from_nanos(100) &&
        self.encryption_overhead < 0.1 &&
        self.authentication_latency < Duration::from_millis(10)
    }
}
