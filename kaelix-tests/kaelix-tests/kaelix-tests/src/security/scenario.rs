use std::error::Error;

/// Standardized security test scenario trait
pub trait SecurityScenario {
    /// Execute the security test scenario
    fn execute(&self) -> Result<(), Box<dyn Error>>;

    /// Name of the scenario for logging and identification
    fn name(&self) -> &'static str;

    /// Describe the security goal of this scenario
    fn description(&self) -> &'static str;
}

/// Marker trait for validation scenarios
pub trait SecurityValidation: SecurityScenario {
    /// Validate a specific security property
    fn validate(&self) -> bool;
}
