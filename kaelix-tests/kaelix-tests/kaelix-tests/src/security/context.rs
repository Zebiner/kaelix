use std::sync::Arc;
use std::collections::HashMap;
use tracing::{info, warn};

/// Isolated security testing environment
pub struct SecurityTestContext {
    scenarios: HashMap<String, Box<dyn SecurityScenario>>,
    sensitive_data: Vec<Arc<[u8]>>,
}

impl SecurityTestContext {
    pub fn new() -> Self {
        Self {
            scenarios: HashMap::new(),
            sensitive_data: Vec::new(),
        }
    }

    /// Register a security scenario for testing
    pub fn register_scenario<S: SecurityScenario + 'static>(&mut self, name: &str, scenario: S) {
        self.scenarios.insert(name.to_string(), Box::new(scenario));
        info!(scenario = name, "Registered security scenario");
    }

    /// Execute all registered scenarios
    pub fn run_scenarios(&self) -> Result<(), Vec<String>> {
        let mut failures = Vec::new();

        for (name, scenario) in &self.scenarios {
            match scenario.execute() {
                Ok(_) => info!(scenario = name, "Security scenario passed"),
                Err(e) => {
                    warn!(scenario = name, error = %e, "Security scenario failed");
                    failures.push(name.clone());
                }
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures)
        }
    }

    /// Securely zeroize and clear sensitive test data
    pub fn clear_sensitive_data(&mut self) {
        for data in &mut self.sensitive_data {
            Arc::get_mut(data).map(|slice| slice.fill(0));
        }
        self.sensitive_data.clear();
    }
}

impl Drop for SecurityTestContext {
    fn drop(&mut self) {
        self.clear_sensitive_data();
    }
}
