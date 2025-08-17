//! Security Testing Framework for MemoryStreamer
//! Comprehensive security validation infrastructure

mod context;
mod scenario;
mod threat_model;
mod metrics;

pub use context::SecurityTestContext;
pub use scenario::SecurityScenario;
pub use threat_model::ThreatModel;
pub use metrics::SecurityMetrics;

/// Core security testing prelude
pub mod prelude {
    pub use super::{SecurityTestContext, SecurityScenario, ThreatModel, SecurityMetrics};
}
