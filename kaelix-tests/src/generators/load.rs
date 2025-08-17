//! Load pattern generation for comprehensive testing scenarios.
//!
//! This module provides sophisticated load generation patterns that
//! simulate real-world traffic patterns and stress testing scenarios.

use crate::generators::data::{MessageGenerator, GenerationStats};
use kaelix_core::{Message, Result, Error};
use futures::{Stream, StreamExt};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use tokio::time::{interval, sleep, Interval};
use rand::Rng;

/// Load pattern types for different testing scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadPattern {
    /// Constant load at specified rate
    Constant { 
        rate: u64 
    },
    /// Burst pattern with peaks and valleys
    Burst { 
        base: u64, 
        peak: u64, 
        duration: Duration, 
        interval: Duration 
    },
    /// Sine wave pattern for gradual load changes
    Sine { 
        min: u64, 
        max: u64, 
        period: Duration 
    },
    /// Sawtooth pattern for linear ramp-up/down
    Sawtooth { 
        min: u64, 
        max: u64, 
        period: Duration 
    },
    /// Step function for discrete load levels
    Step { 
        steps: Vec<(u64, Duration)> 
    },
    /// Realistic load profile based on production patterns
    Realistic { 
        profile: LoadProfile 
    },
    /// Interactive workload with think time
    Interactive { 
        think_time: Duration 
    },
}

/// Predefined realistic load profiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadProfile {
    /// Business hours pattern (9-5 with peaks)
    BusinessHours,
    /// E-commerce flash sale pattern
    FlashSale,
    /// IoT sensor burst pattern
    IoTBurst,
    /// Gaming server load pattern
    Gaming,
    /// Social media viral content pattern
    SocialViral,
    /// Financial trading hours pattern
    Trading,
    /// Custom profile with hourly rates
    Custom(Vec<f64>), // 24 hourly multipliers
}

/// Load generator with configurable patterns and jitter
#[derive(Debug)]
pub struct LoadGenerator {
    /// Load pattern to follow
    pattern: LoadPattern,
    /// Optional jitter percentage (0.0 to 1.0)
    jitter: Option<f64>,
    /// Warmup duration before starting load
    warmup_duration: Duration,
    /// Message generator for creating payloads
    message_generator: MessageGenerator,
    /// Load statistics
    stats: Arc<parking_lot::RwLock<LoadStats>>,
    /// Generation counter
    counter: Arc<AtomicU64>,
}

/// Statistics for load generation
#[derive(Debug, Clone, Default)]
pub struct LoadStats {
    /// Total messages generated
    pub messages_generated: u64,
    /// Total execution time
    pub execution_time: Duration,
    /// Average rate achieved
    pub average_rate: f64,
    /// Peak rate achieved
    pub peak_rate: f64,
    /// Minimum rate achieved
    pub min_rate: f64,
    /// Rate variance from target
    pub rate_variance: f64,
    /// Load pattern efficiency
    pub pattern_efficiency: f64,
    /// Jitter statistics
    pub jitter_stats: JitterStats,
}

/// Jitter statistics for timing analysis
#[derive(Debug, Clone, Default)]
pub struct JitterStats {
    /// Average jitter amount
    pub avg_jitter: f64,
    /// Maximum jitter observed
    pub max_jitter: f64,
    /// Jitter standard deviation
    pub jitter_std_dev: f64,
}

impl LoadGenerator {
    /// Create a new load generator with specified pattern
    pub fn new(pattern: LoadPattern) -> Self {
        Self {
            pattern,
            jitter: None,
            warmup_duration: Duration::from_secs(5),
            message_generator: MessageGenerator::new(),
            stats: Arc::new(parking_lot::RwLock::new(LoadStats::default())),
            counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Add jitter to the load pattern
    pub fn with_jitter(mut self, jitter_percent: f64) -> Self {
        self.jitter = Some(jitter_percent.clamp(0.0, 1.0));
        self
    }

    /// Set warmup duration
    pub fn with_warmup(mut self, warmup: Duration) -> Self {
        self.warmup_duration = warmup;
        self
    }

    /// Set custom message generator
    pub fn with_message_generator(mut self, generator: MessageGenerator) -> Self {
        self.message_generator = generator;
        self
    }

    /// Generate load according to pattern for specified duration
    pub async fn generate(&self, pattern: LoadPattern) -> impl Stream<Item = Message> + '_ {
        async_stream::stream! {
            // Warmup phase
            if !self.warmup_duration.is_zero() {
                self.warmup().await;
            }

            // Main generation phase
            match pattern {
                LoadPattern::Constant { rate } => {
                    yield_all!(self.generate_constant(rate));
                }
                LoadPattern::Burst { base, peak, duration, interval } => {
                    yield_all!(self.generate_burst(base, peak, duration, interval));
                }
                LoadPattern::Sine { min, max, period } => {
                    yield_all!(self.generate_sine(min, max, period));
                }
                LoadPattern::Sawtooth { min, max, period } => {
                    yield_all!(self.generate_sawtooth(min, max, period));
                }
                LoadPattern::Step { steps } => {
                    yield_all!(self.generate_step(steps));
                }
                LoadPattern::Realistic { profile } => {
                    yield_all!(self.generate_realistic(profile));
                }
                LoadPattern::Interactive { think_time } => {
                    yield_all!(self.generate_interactive(think_time));
                }
            }
        }
    }

    /// Run a complete load scenario for specified duration
    pub async fn run_scenario(&self, duration: Duration) -> Result<LoadStats> {
        let start = Instant::now();
        let mut message_count = 0u64;
        let mut rates = Vec::new();
        let mut last_check = start;

        let stream = self.generate(self.pattern.clone());
        tokio::pin!(stream);

        while start.elapsed() < duration {
            if let Some(message) = stream.next().await {
                message_count += 1;
                
                // Track rate every second
                let now = Instant::now();
                if now.duration_since(last_check) >= Duration::from_secs(1) {
                    let current_rate = message_count as f64 / start.elapsed().as_secs_f64();
                    rates.push(current_rate);
                    last_check = now;
                }
            }
        }

        let execution_time = start.elapsed();
        let average_rate = message_count as f64 / execution_time.as_secs_f64();
        
        let stats = LoadStats {
            messages_generated: message_count,
            execution_time,
            average_rate,
            peak_rate: rates.iter().fold(0.0, |a, &b| a.max(b)),
            min_rate: rates.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
            rate_variance: self.calculate_variance(&rates, average_rate),
            pattern_efficiency: self.calculate_efficiency(&rates),
            jitter_stats: self.calculate_jitter_stats(),
        };

        // Update internal stats
        *self.stats.write() = stats.clone();

        Ok(stats)
    }

    /// Generate constant rate load
    fn generate_constant(&self, rate: u64) -> impl Stream<Item = Message> + '_ {
        let interval_duration = Duration::from_nanos(1_000_000_000 / rate.max(1));
        
        async_stream::stream! {
            let mut interval = interval(interval_duration);
            loop {
                interval.tick().await;
                
                // Apply jitter if configured
                if let Some(jitter) = self.jitter {
                    let jitter_amount = self.calculate_jitter(interval_duration, jitter);
                    sleep(jitter_amount).await;
                }
                
                if let Ok(message) = self.message_generator.generate_single().await {
                    self.counter.fetch_add(1, Ordering::Relaxed);
                    yield message;
                }
            }
        }
    }

    /// Generate burst pattern load
    fn generate_burst(
        &self, 
        base: u64, 
        peak: u64, 
        duration: Duration, 
        interval: Duration
    ) -> impl Stream<Item = Message> + '_ {
        async_stream::stream! {
            let mut burst_timer = interval(interval);
            let mut is_bursting = false;
            let mut burst_start = Instant::now();

            loop {
                // Check burst state
                if !is_bursting {
                    burst_timer.tick().await;
                    is_bursting = true;
                    burst_start = Instant::now();
                } else if burst_start.elapsed() >= duration {
                    is_bursting = false;
                    continue;
                }

                // Calculate current rate
                let current_rate = if is_bursting { peak } else { base };
                let sleep_duration = Duration::from_nanos(1_000_000_000 / current_rate.max(1));
                
                sleep(sleep_duration).await;
                
                if let Ok(message) = self.message_generator.generate_single().await {
                    self.counter.fetch_add(1, Ordering::Relaxed);
                    yield message;
                }
            }
        }
    }

    /// Generate sine wave pattern load
    fn generate_sine(&self, min: u64, max: u64, period: Duration) -> impl Stream<Item = Message> + '_ {
        async_stream::stream! {
            let start = Instant::now();
            let amplitude = (max - min) as f64 / 2.0;
            let offset = min as f64 + amplitude;
            
            loop {
                let elapsed = start.elapsed().as_secs_f64();
                let phase = 2.0 * std::f64::consts::PI * elapsed / period.as_secs_f64();
                let rate = offset + amplitude * phase.sin();
                let rate = rate.max(1.0) as u64;
                
                let sleep_duration = Duration::from_nanos(1_000_000_000 / rate);
                sleep(sleep_duration).await;
                
                if let Ok(message) = self.message_generator.generate_single().await {
                    self.counter.fetch_add(1, Ordering::Relaxed);
                    yield message;
                }
            }
        }
    }

    /// Generate sawtooth pattern load
    fn generate_sawtooth(&self, min: u64, max: u64, period: Duration) -> impl Stream<Item = Message> + '_ {
        async_stream::stream! {
            let start = Instant::now();
            let range = max - min;
            
            loop {
                let elapsed = start.elapsed().as_secs_f64();
                let cycle_progress = (elapsed % period.as_secs_f64()) / period.as_secs_f64();
                let rate = min + (range as f64 * cycle_progress) as u64;
                
                let sleep_duration = Duration::from_nanos(1_000_000_000 / rate.max(1));
                sleep(sleep_duration).await;
                
                if let Ok(message) = self.message_generator.generate_single().await {
                    self.counter.fetch_add(1, Ordering::Relaxed);
                    yield message;
                }
            }
        }
    }

    /// Generate step pattern load
    fn generate_step(&self, steps: Vec<(u64, Duration)>) -> impl Stream<Item = Message> + '_ {
        async_stream::stream! {
            for (rate, duration) in steps.iter().cycle() {
                let step_start = Instant::now();
                let sleep_duration = Duration::from_nanos(1_000_000_000 / rate.max(1));
                
                while step_start.elapsed() < *duration {
                    sleep(sleep_duration).await;
                    
                    if let Ok(message) = self.message_generator.generate_single().await {
                        self.counter.fetch_add(1, Ordering::Relaxed);
                        yield message;
                    }
                }
            }
        }
    }

    /// Generate realistic load patterns
    fn generate_realistic(&self, profile: LoadProfile) -> impl Stream<Item = Message> + '_ {
        async_stream::stream! {
            let multipliers = self.get_profile_multipliers(&profile);
            let base_rate = 1000u64; // Base rate for scaling
            
            loop {
                let now = chrono::Utc::now();
                let hour = now.hour() as usize;
                let multiplier = multipliers.get(hour).unwrap_or(&1.0);
                let current_rate = (base_rate as f64 * multiplier) as u64;
                
                let sleep_duration = Duration::from_nanos(1_000_000_000 / current_rate.max(1));
                sleep(sleep_duration).await;
                
                if let Ok(message) = self.message_generator.generate_single().await {
                    self.counter.fetch_add(1, Ordering::Relaxed);
                    yield message;
                }
            }
        }
    }

    /// Generate interactive workload with think time
    fn generate_interactive(&self, think_time: Duration) -> impl Stream<Item = Message> + '_ {
        async_stream::stream! {
            loop {
                // Generate message
                if let Ok(message) = self.message_generator.generate_single().await {
                    self.counter.fetch_add(1, Ordering::Relaxed);
                    yield message;
                }
                
                // Think time with jitter
                let actual_think_time = if let Some(jitter) = self.jitter {
                    let jitter_amount = self.calculate_jitter(think_time, jitter);
                    think_time + jitter_amount
                } else {
                    think_time
                };
                
                sleep(actual_think_time).await;
            }
        }
    }

    /// Warmup phase before main generation
    async fn warmup(&self) {
        tracing::info!("Starting warmup phase for {:?}", self.warmup_duration);
        
        let warmup_rate = 100; // Low rate during warmup
        let warmup_interval = Duration::from_millis(1000 / warmup_rate);
        let start = Instant::now();
        
        while start.elapsed() < self.warmup_duration {
            if let Ok(_) = self.message_generator.generate_single().await {
                // Discard warmup messages
            }
            sleep(warmup_interval).await;
        }
        
        tracing::info!("Warmup phase completed");
    }

    /// Calculate jitter amount
    fn calculate_jitter(&self, base_duration: Duration, jitter_percent: f64) -> Duration {
        let jitter_nanos = base_duration.as_nanos() as f64 * jitter_percent;
        let random_jitter = rand::thread_rng().gen_range(-jitter_nanos..=jitter_nanos);
        Duration::from_nanos(random_jitter.abs() as u64)
    }

    /// Get profile multipliers for realistic patterns
    fn get_profile_multipliers(&self, profile: &LoadProfile) -> Vec<f64> {
        match profile {
            LoadProfile::BusinessHours => vec![
                0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.4, 0.7, // 0-7 AM
                1.0, 1.2, 1.5, 1.8, 1.5, 1.8, 2.0, 1.8, // 8-15 (business hours)
                1.5, 1.2, 0.8, 0.6, 0.4, 0.3, 0.2, 0.1, // 16-23 PM
            ],
            LoadProfile::FlashSale => vec![
                0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.3, // 0-7 AM
                0.5, 0.8, 1.0, 5.0, 8.0, 3.0, 2.0, 1.5, // 8-15 (sale peak at noon)
                1.0, 0.8, 0.6, 0.4, 0.3, 0.2, 0.1, 0.1, // 16-23 PM
            ],
            LoadProfile::IoTBurst => vec![
                0.8, 0.6, 0.4, 0.3, 0.3, 0.5, 0.8, 1.0, // 0-7 AM
                1.2, 1.0, 0.8, 1.5, 2.0, 1.8, 1.5, 1.2, // 8-15 
                1.0, 1.2, 1.8, 2.5, 2.0, 1.5, 1.2, 1.0, // 16-23 (evening peak)
            ],
            LoadProfile::Gaming => vec![
                0.3, 0.2, 0.1, 0.1, 0.1, 0.1, 0.2, 0.3, // 0-7 AM
                0.5, 0.4, 0.3, 0.4, 0.6, 0.8, 1.0, 1.2, // 8-15
                1.5, 2.0, 2.5, 3.0, 2.8, 2.0, 1.5, 0.8, // 16-23 (evening gaming)
            ],
            LoadProfile::SocialViral => vec![
                0.3, 0.2, 0.1, 0.1, 0.1, 0.2, 0.4, 0.8, // 0-7 AM
                1.2, 1.5, 2.0, 3.0, 2.5, 2.0, 1.8, 2.2, // 8-15
                2.8, 3.5, 4.0, 3.0, 2.0, 1.5, 1.0, 0.6, // 16-23 (viral peak evening)
            ],
            LoadProfile::Trading => vec![
                0.1, 0.1, 0.1, 0.1, 0.1, 0.3, 0.8, 1.5, // 0-7 AM
                2.5, 3.0, 2.8, 2.5, 2.0, 2.2, 2.8, 3.2, // 8-15 (trading hours)
                2.0, 1.5, 1.0, 0.5, 0.3, 0.2, 0.1, 0.1, // 16-23 PM
            ],
            LoadProfile::Custom(multipliers) => {
                if multipliers.len() >= 24 {
                    multipliers.clone()
                } else {
                    // Extend or repeat to fill 24 hours
                    let mut extended = multipliers.clone();
                    while extended.len() < 24 {
                        extended.push(*multipliers.last().unwrap_or(&1.0));
                    }
                    extended
                }
            }
        }
    }

    /// Calculate rate variance
    fn calculate_variance(&self, rates: &[f64], mean: f64) -> f64 {
        if rates.is_empty() {
            return 0.0;
        }
        
        let variance = rates.iter()
            .map(|&rate| (rate - mean).powi(2))
            .sum::<f64>() / rates.len() as f64;
        
        variance.sqrt() / mean // Coefficient of variation
    }

    /// Calculate pattern efficiency
    fn calculate_efficiency(&self, rates: &[f64]) -> f64 {
        if rates.is_empty() {
            return 0.0;
        }
        
        let target_rate = match &self.pattern {
            LoadPattern::Constant { rate } => *rate as f64,
            LoadPattern::Burst { peak, .. } => *peak as f64,
            _ => rates.iter().sum::<f64>() / rates.len() as f64,
        };
        
        let actual_avg = rates.iter().sum::<f64>() / rates.len() as f64;
        (actual_avg / target_rate).min(1.0)
    }

    /// Calculate jitter statistics
    fn calculate_jitter_stats(&self) -> JitterStats {
        // This would be implemented with actual timing measurements
        // For now, return default stats
        JitterStats::default()
    }

    /// Get current statistics
    pub fn stats(&self) -> LoadStats {
        self.stats.read().clone()
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        *self.stats.write() = LoadStats::default();
        self.counter.store(0, Ordering::Relaxed);
    }
}

/// Convenience constructors for common patterns
impl LoadGenerator {
    /// Create constant load generator
    pub fn constant(rate: u64) -> Self {
        Self::new(LoadPattern::Constant { rate })
    }

    /// Create burst load generator
    pub fn burst(base: u64, peak: u64, duration: Duration, interval: Duration) -> Self {
        Self::new(LoadPattern::Burst { base, peak, duration, interval })
    }

    /// Create sine wave load generator
    pub fn sine(min: u64, max: u64, period: Duration) -> Self {
        Self::new(LoadPattern::Sine { min, max, period })
    }

    /// Create business hours load generator
    pub fn business_hours() -> Self {
        Self::new(LoadPattern::Realistic { profile: LoadProfile::BusinessHours })
    }

    /// Create flash sale load generator
    pub fn flash_sale() -> Self {
        Self::new(LoadPattern::Realistic { profile: LoadProfile::FlashSale })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_constant_load_generator() {
        let generator = LoadGenerator::constant(100);
        let start = Instant::now();
        
        let messages: Vec<_> = generator.generate(LoadPattern::Constant { rate: 100 })
            .take(50)
            .collect()
            .await;
        
        let elapsed = start.elapsed();
        assert_eq!(messages.len(), 50);
        
        // Should take approximately 0.5 seconds at 100 msg/sec
        assert!(elapsed >= Duration::from_millis(450));
        assert!(elapsed <= Duration::from_millis(550));
    }

    #[tokio::test]
    async fn test_burst_load_generator() {
        let generator = LoadGenerator::burst(
            10, 
            100, 
            Duration::from_millis(100), 
            Duration::from_millis(500)
        );
        
        let messages: Vec<_> = generator.generate(LoadPattern::Burst {
            base: 10,
            peak: 100,
            duration: Duration::from_millis(100),
            interval: Duration::from_millis(500),
        })
        .take(20)
        .collect()
        .await;
        
        assert_eq!(messages.len(), 20);
    }

    #[tokio::test]
    async fn test_sine_load_generator() {
        let generator = LoadGenerator::sine(10, 100, Duration::from_secs(2));
        
        let messages: Vec<_> = generator.generate(LoadPattern::Sine {
            min: 10,
            max: 100,
            period: Duration::from_secs(2),
        })
        .take(50)
        .collect()
        .await;
        
        assert_eq!(messages.len(), 50);
    }

    #[tokio::test]
    async fn test_realistic_load_profiles() {
        let profiles = vec![
            LoadProfile::BusinessHours,
            LoadProfile::FlashSale,
            LoadProfile::Gaming,
        ];

        for profile in profiles {
            let generator = LoadGenerator::new(LoadPattern::Realistic { profile });
            let multipliers = generator.get_profile_multipliers(&LoadProfile::BusinessHours);
            assert_eq!(multipliers.len(), 24);
        }
    }

    #[tokio::test]
    async fn test_load_generator_with_jitter() {
        let generator = LoadGenerator::constant(100)
            .with_jitter(0.1); // 10% jitter
        
        let messages: Vec<_> = generator.generate(LoadPattern::Constant { rate: 100 })
            .take(10)
            .collect()
            .await;
        
        assert_eq!(messages.len(), 10);
    }

    #[tokio::test]
    async fn test_run_scenario() {
        let generator = LoadGenerator::constant(50);
        let stats = generator.run_scenario(Duration::from_millis(500)).await.unwrap();
        
        assert!(stats.messages_generated > 0);
        assert!(stats.average_rate > 0.0);
        assert!(stats.execution_time >= Duration::from_millis(500));
    }
}