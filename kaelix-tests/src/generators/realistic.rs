//! Realistic data patterns based on production workload analysis.
//!
//! This module provides realistic message size distributions and load patterns
//! that mirror real-world streaming data characteristics.

use crate::generators::data::PayloadDistribution;
use crate::generators::load::{LoadPattern, LoadProfile};
use std::time::Duration;
use rand_distr::{Distribution, Normal, Exponential, Pareto};
use rand::Rng;

/// Collection of realistic patterns based on production data analysis
pub struct RealisticPatterns;

impl RealisticPatterns {
    // Message size distributions based on real-world analysis

    /// Web server access logs (typically 100-2000 bytes)
    pub fn web_logs() -> PayloadDistribution {
        PayloadDistribution::Normal {
            mean: 400.0,      // Average log line
            std_dev: 150.0,   // Some variance in log content
        }
    }

    /// IoT sensor telemetry (small, consistent messages)
    pub fn iot_telemetry() -> PayloadDistribution {
        PayloadDistribution::Normal {
            mean: 128.0,      // Small sensor readings
            std_dev: 32.0,    // Low variance
        }
    }

    /// Financial trading messages (fixed format, consistent size)
    pub fn financial_trades() -> PayloadDistribution {
        PayloadDistribution::Uniform {
            min: 256,         // FIX protocol messages
            max: 512,         // With market data
        }
    }

    /// Chat and messaging applications (high variance)
    pub fn chat_messages() -> PayloadDistribution {
        PayloadDistribution::Pareto {
            scale: 50.0,      // Minimum message size
            shape: 1.16,      // Heavy tail for long messages
        }
    }

    /// Video/media metadata (larger, variable size)
    pub fn video_metadata() -> PayloadDistribution {
        PayloadDistribution::Normal {
            mean: 8192.0,     // 8KB average metadata
            std_dev: 3072.0,  // Significant variance
        }
    }

    /// Database change events (CDC)
    pub fn database_cdc() -> PayloadDistribution {
        PayloadDistribution::Realistic // Mixed small updates, large inserts
    }

    /// Application metrics and monitoring
    pub fn application_metrics() -> PayloadDistribution {
        PayloadDistribution::Exponential {
            lambda: 0.002,    // Most metrics are small, few large aggregations
        }
    }

    /// User behavior analytics events
    pub fn analytics_events() -> PayloadDistribution {
        PayloadDistribution::Normal {
            mean: 1024.0,     // JSON events with user data
            std_dev: 512.0,   // Moderate variance
        }
    }

    // Load patterns based on real-world traffic analysis

    /// Standard business hours (9-5) with gradual ramp up/down
    pub fn business_hours() -> LoadPattern {
        LoadPattern::Realistic {
            profile: LoadProfile::BusinessHours,
        }
    }

    /// E-commerce flash sales (sudden spikes)
    pub fn flash_sales() -> LoadPattern {
        LoadPattern::Burst {
            base: 1_000,      // Normal traffic
            peak: 50_000,     // Flash sale spike
            duration: Duration::from_minutes(15), // Short duration
            interval: Duration::from_hours(6),    // Several times per day
        }
    }

    /// IoT sensor bursts (periodic data collection)
    pub fn iot_burst() -> LoadPattern {
        LoadPattern::Realistic {
            profile: LoadProfile::IoTBurst,
        }
    }

    /// Gaming server load (evening peaks, weekend spikes)
    pub fn gaming_peak() -> LoadPattern {
        LoadPattern::Realistic {
            profile: LoadProfile::Gaming,
        }
    }

    /// Social media viral content (exponential growth then decay)
    pub fn social_media_viral() -> LoadPattern {
        LoadPattern::Realistic {
            profile: LoadProfile::SocialViral,
        }
    }

    /// Financial market hours (opening/closing volatility)
    pub fn trading_hours() -> LoadPattern {
        LoadPattern::Realistic {
            profile: LoadProfile::Trading,
        }
    }

    /// News and media spikes (event-driven traffic)
    pub fn news_spikes() -> LoadPattern {
        LoadPattern::Burst {
            base: 5_000,      // Regular news consumption
            peak: 100_000,    // Breaking news spike
            duration: Duration::from_minutes(30), // News cycle duration
            interval: Duration::from_hours(8),    // Multiple news cycles
        }
    }

    /// Batch processing windows (periodic high load)
    pub fn batch_processing() -> LoadPattern {
        LoadPattern::Step {
            steps: vec![
                (1_000, Duration::from_hours(8)),    // Low activity
                (25_000, Duration::from_hours(4)),   // Batch processing
                (5_000, Duration::from_hours(8)),    // Post-processing
                (1_000, Duration::from_hours(4)),    // Idle time
            ],
        }
    }

    /// Monitoring and alerting (steady with occasional spikes)
    pub fn monitoring_alerts() -> LoadPattern {
        LoadPattern::Sine {
            min: 2_000,       // Baseline monitoring
            max: 8_000,       // Alert spikes
            period: Duration::from_hours(12), // Twice daily pattern
        }
    }

    /// Content delivery network (CDN) traffic
    pub fn cdn_traffic() -> LoadPattern {
        LoadPattern::Realistic {
            profile: LoadProfile::Custom(vec![
                // 24-hour pattern for global CDN
                0.3, 0.2, 0.1, 0.1, 0.1, 0.2, 0.4, 0.7, // 0-7 AM
                1.0, 1.2, 1.3, 1.4, 1.3, 1.4, 1.5, 1.6, // 8-15 PM
                1.8, 2.0, 2.2, 2.0, 1.5, 1.0, 0.7, 0.5, // 16-23 PM
            ]),
        }
    }

    // Advanced pattern generators

    /// Generate pattern based on time zones (follow the sun)
    pub fn follow_the_sun() -> LoadPattern {
        // Pattern that follows peak hours across time zones
        LoadPattern::Realistic {
            profile: LoadProfile::Custom(vec![
                // Start with Asia-Pacific peak
                2.0, 1.8, 1.5, 1.2, 1.0, 0.8, 0.6, 0.5, // 0-7 AM UTC (Asia afternoon)
                0.4, 0.5, 0.7, 1.0, 1.3, 1.6, 1.8, 2.0, // 8-15 PM UTC (Europe)
                2.2, 2.0, 1.8, 1.5, 1.2, 1.0, 0.8, 0.6, // 16-23 PM UTC (Americas)
            ]),
        }
    }

    /// Generate pattern for disaster recovery testing
    pub fn disaster_recovery() -> LoadPattern {
        LoadPattern::Step {
            steps: vec![
                (10_000, Duration::from_minutes(30)),  // Normal operation
                (0, Duration::from_minutes(5)),        // Complete outage
                (2_000, Duration::from_minutes(10)),   // Partial recovery
                (8_000, Duration::from_minutes(15)),   // Recovery phase
                (10_000, Duration::from_minutes(30)),  // Full recovery
            ],
        }
    }

    /// Generate pattern for load testing ramp-up
    pub fn load_test_ramp() -> LoadPattern {
        LoadPattern::Sawtooth {
            min: 1_000,       // Start load
            max: 100_000,     // Peak load
            period: Duration::from_minutes(60), // 1 hour ramp-up
        }
    }

    // Utility methods for pattern customization

    /// Combine multiple patterns with weights
    pub fn weighted_combination(patterns: Vec<(LoadPattern, f64)>) -> LoadPattern {
        // For simplicity, return the first pattern
        // In a real implementation, this would blend the patterns
        patterns.into_iter().next().map(|(p, _)| p).unwrap_or(LoadPattern::Constant { rate: 1000 })
    }

    /// Scale pattern by factor
    pub fn scale_pattern(pattern: LoadPattern, factor: f64) -> LoadPattern {
        match pattern {
            LoadPattern::Constant { rate } => LoadPattern::Constant {
                rate: (rate as f64 * factor) as u64,
            },
            LoadPattern::Burst { base, peak, duration, interval } => LoadPattern::Burst {
                base: (base as f64 * factor) as u64,
                peak: (peak as f64 * factor) as u64,
                duration,
                interval,
            },
            LoadPattern::Sine { min, max, period } => LoadPattern::Sine {
                min: (min as f64 * factor) as u64,
                max: (max as f64 * factor) as u64,
                period,
            },
            LoadPattern::Sawtooth { min, max, period } => LoadPattern::Sawtooth {
                min: (min as f64 * factor) as u64,
                max: (max as f64 * factor) as u64,
                period,
            },
            other => other, // Some patterns don't scale linearly
        }
    }

    /// Add jitter to make patterns more realistic
    pub fn add_jitter(pattern: LoadPattern, jitter_percent: f64) -> LoadPattern {
        // This would be implemented to add randomness to the pattern
        // For now, return the original pattern
        pattern
    }

    // Real-world scenario builders

    /// Black Friday e-commerce scenario
    pub fn black_friday_ecommerce() -> (PayloadDistribution, LoadPattern) {
        let payload = PayloadDistribution::Normal {
            mean: 2048.0,     // Larger product data with images
            std_dev: 1024.0,  // High variance in product complexity
        };

        let load = LoadPattern::Burst {
            base: 10_000,     // Regular shopping traffic
            peak: 500_000,    // Black Friday peak
            duration: Duration::from_hours(6), // Peak shopping hours
            interval: Duration::from_hours(24), // Once per day
        };

        (payload, load)
    }

    /// Cryptocurrency trading frenzy
    pub fn crypto_trading_frenzy() -> (PayloadDistribution, LoadPattern) {
        let payload = Self::financial_trades();

        let load = LoadPattern::Burst {
            base: 50_000,     // High baseline for crypto
            peak: 2_000_000,  // Extreme spikes during volatility
            duration: Duration::from_minutes(20), // Quick price movements
            interval: Duration::from_hours(2),    // Frequent volatility
        };

        (payload, load)
    }

    /// Live streaming event (sports, concerts)
    pub fn live_streaming_event() -> (PayloadDistribution, LoadPattern) {
        let payload = Self::video_metadata();

        let load = LoadPattern::Step {
            steps: vec![
                (5_000, Duration::from_minutes(30)),   // Pre-event buildup
                (100_000, Duration::from_hours(2)),    // Event peak
                (50_000, Duration::from_minutes(30)),  // Post-event discussion
                (10_000, Duration::from_hours(1)),     // Cool down
            ],
        };

        (payload, load)
    }

    /// IoT smart city deployment
    pub fn smart_city_iot() -> (PayloadDistribution, LoadPattern) {
        let payload = Self::iot_telemetry();

        let load = LoadPattern::Realistic {
            profile: LoadProfile::Custom(vec![
                // Traffic patterns follow city activity
                0.2, 0.1, 0.1, 0.1, 0.2, 0.4, 0.8, 1.2, // 0-7 AM (morning rush builds)
                1.0, 0.8, 0.7, 0.8, 0.9, 1.0, 1.1, 1.3, // 8-15 PM (business hours)
                1.5, 1.4, 1.2, 1.0, 0.8, 0.6, 0.4, 0.3, // 16-23 PM (evening rush, then quiet)
            ]),
        };

        (payload, load)
    }

    /// Social media breaking news
    pub fn breaking_news_social() -> (PayloadDistribution, LoadPattern) {
        let payload = Self::chat_messages(); // Varied user responses

        let load = LoadPattern::Step {
            steps: vec![
                (5_000, Duration::from_minutes(60)),    // Normal activity
                (200_000, Duration::from_minutes(15)),  // Initial spike
                (100_000, Duration::from_minutes(30)),  // Sustained high activity
                (50_000, Duration::from_hours(2)),      // Gradual decline
                (20_000, Duration::from_hours(4)),      // Extended discussion
            ],
        };

        (payload, load)
    }
}

/// Advanced pattern generation utilities
pub struct PatternGenerator;

impl PatternGenerator {
    /// Generate custom distribution from histogram data
    pub fn from_histogram(buckets: Vec<(usize, f64)>) -> PayloadDistribution {
        // Convert histogram to distribution parameters
        let total_weight: f64 = buckets.iter().map(|(_, w)| w).sum();
        let weighted_mean = buckets.iter()
            .map(|(size, weight)| *size as f64 * weight / total_weight)
            .sum::<f64>();
        
        let variance = buckets.iter()
            .map(|(size, weight)| {
                let diff = *size as f64 - weighted_mean;
                diff * diff * weight / total_weight
            })
            .sum::<f64>();
        
        PayloadDistribution::Normal {
            mean: weighted_mean,
            std_dev: variance.sqrt(),
        }
    }

    /// Generate load pattern from time series data
    pub fn from_time_series(data_points: Vec<(chrono::DateTime<chrono::Utc>, u64)>) -> LoadPattern {
        // Analyze time series to detect patterns
        if data_points.len() < 24 {
            return LoadPattern::Constant { rate: 1000 };
        }

        // Extract hourly averages for 24-hour pattern
        let mut hourly_rates = vec![0.0; 24];
        let mut hourly_counts = vec![0; 24];

        for (timestamp, rate) in data_points {
            let hour = timestamp.hour() as usize;
            hourly_rates[hour] += rate as f64;
            hourly_counts[hour] += 1;
        }

        for hour in 0..24 {
            if hourly_counts[hour] > 0 {
                hourly_rates[hour] /= hourly_counts[hour] as f64;
            }
        }

        // Normalize to multipliers
        let max_rate = hourly_rates.iter().fold(0.0, |a, &b| a.max(b));
        if max_rate > 0.0 {
            for rate in &mut hourly_rates {
                *rate /= max_rate;
            }
        }

        LoadPattern::Realistic {
            profile: LoadProfile::Custom(hourly_rates),
        }
    }

    /// Generate pattern that simulates system degradation
    pub fn degradation_pattern(initial_rate: u64, degradation_factor: f64, duration: Duration) -> LoadPattern {
        let steps_count = 10;
        let step_duration = duration / steps_count as u32;
        let mut steps = Vec::new();

        for i in 0..steps_count {
            let factor = degradation_factor.powf(i as f64);
            let rate = (initial_rate as f64 * factor) as u64;
            steps.push((rate, step_duration));
        }

        LoadPattern::Step { steps }
    }

    /// Generate pattern for circuit breaker testing
    pub fn circuit_breaker_pattern() -> LoadPattern {
        LoadPattern::Step {
            steps: vec![
                (10_000, Duration::from_minutes(5)),   // Normal load
                (50_000, Duration::from_minutes(2)),   // Overload triggers circuit breaker
                (0, Duration::from_minutes(1)),        // Circuit breaker open
                (5_000, Duration::from_minutes(2)),    // Circuit breaker half-open
                (10_000, Duration::from_minutes(5)),   // Circuit breaker closed
            ],
        }
    }

    /// Generate realistic error injection pattern
    pub fn error_injection_pattern(base_rate: u64, error_burst_probability: f64) -> LoadPattern {
        // This would implement sophisticated error injection
        // For now, return a burst pattern
        LoadPattern::Burst {
            base: base_rate,
            peak: base_rate / 10, // Errors reduce effective throughput
            duration: Duration::from_seconds(30),
            interval: Duration::from_minutes(10),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_realistic_payload_distributions() {
        let web_logs = RealisticPatterns::web_logs();
        match web_logs {
            PayloadDistribution::Normal { mean, std_dev } => {
                assert_eq!(mean, 400.0);
                assert_eq!(std_dev, 150.0);
            }
            _ => panic!("Expected Normal distribution for web logs"),
        }

        let iot = RealisticPatterns::iot_telemetry();
        match iot {
            PayloadDistribution::Normal { mean, std_dev } => {
                assert_eq!(mean, 128.0);
                assert_eq!(std_dev, 32.0);
            }
            _ => panic!("Expected Normal distribution for IoT"),
        }
    }

    #[test]
    fn test_realistic_load_patterns() {
        let business_hours = RealisticPatterns::business_hours();
        match business_hours {
            LoadPattern::Realistic { profile: LoadProfile::BusinessHours } => {
                // Correct pattern type
            }
            _ => panic!("Expected BusinessHours pattern"),
        }

        let flash_sale = RealisticPatterns::flash_sales();
        match flash_sale {
            LoadPattern::Burst { base, peak, .. } => {
                assert_eq!(base, 1_000);
                assert_eq!(peak, 50_000);
            }
            _ => panic!("Expected Burst pattern for flash sales"),
        }
    }

    #[test]
    fn test_scenario_combinations() {
        let (payload, load) = RealisticPatterns::black_friday_ecommerce();
        
        match payload {
            PayloadDistribution::Normal { mean, .. } => {
                assert_eq!(mean, 2048.0);
            }
            _ => panic!("Expected Normal distribution"),
        }

        match load {
            LoadPattern::Burst { peak, .. } => {
                assert_eq!(peak, 500_000);
            }
            _ => panic!("Expected Burst pattern"),
        }
    }

    #[test]
    fn test_pattern_scaling() {
        let original = LoadPattern::Constant { rate: 1000 };
        let scaled = RealisticPatterns::scale_pattern(original, 2.0);
        
        match scaled {
            LoadPattern::Constant { rate } => {
                assert_eq!(rate, 2000);
            }
            _ => panic!("Expected scaled constant pattern"),
        }
    }

    #[test]
    fn test_histogram_to_distribution() {
        let histogram = vec![
            (100, 0.1),  // 10% of messages are 100 bytes
            (500, 0.6),  // 60% of messages are 500 bytes
            (1000, 0.3), // 30% of messages are 1000 bytes
        ];
        
        let distribution = PatternGenerator::from_histogram(histogram);
        match distribution {
            PayloadDistribution::Normal { mean, .. } => {
                // Weighted mean should be around 600
                assert!((mean - 600.0).abs() < 50.0);
            }
            _ => panic!("Expected Normal distribution"),
        }
    }

    #[test]
    fn test_degradation_pattern() {
        let pattern = PatternGenerator::degradation_pattern(10_000, 0.9, Duration::from_minutes(10));
        
        match pattern {
            LoadPattern::Step { steps } => {
                assert_eq!(steps.len(), 10);
                // First step should be highest rate
                assert_eq!(steps[0].0, 10_000);
                // Last step should be significantly lower
                assert!(steps[9].0 < 5_000);
            }
            _ => panic!("Expected Step pattern for degradation"),
        }
    }
}