//! Test scenarios for comprehensive system validation.

pub mod load;

pub use load::{
    LoadTestScenario, ProducerConfig, ConsumerConfig, PayloadConfig,
    SuccessCriteria, ResourceLimits, ScenarioResult, LatencyPercentiles,
    ResourceUtilization, ContentType,
};