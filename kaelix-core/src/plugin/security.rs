//! Security policies and threat assessment for the plugin system.
//!
//! This module provides comprehensive security controls including capability-based
//! access control, threat level assessment, and security policy enforcement.

use crate::plugin::error::{PluginError, PluginResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

/// Capability-based access control set for plugins.
///
/// Provides fine-grained control over what operations a plugin can perform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilitySet {
    /// Available capabilities mapped to their permission levels
    capabilities: HashMap<String, PermissionLevel>,
}

/// Permission levels for capabilities
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionLevel {
    /// No permission
    None,
    /// Read-only permission
    Read,
    /// Write permission (includes read)
    Write,
    /// Full administrative permission
    Admin,
}

impl CapabilitySet {
    /// Create a new empty capability set
    pub fn new() -> Self {
        Self { capabilities: HashMap::new() }
    }

    /// Create a capability set with default safe permissions
    pub fn default_safe() -> Self {
        let mut capabilities = HashMap::new();
        capabilities.insert("log".to_string(), PermissionLevel::Write);
        capabilities.insert("memory".to_string(), PermissionLevel::Read);
        Self { capabilities }
    }

    /// Grant a capability with specific permission level
    pub fn grant(&mut self, capability: String, level: PermissionLevel) {
        self.capabilities.insert(capability, level);
    }

    /// Revoke a capability
    pub fn revoke(&mut self, capability: &str) {
        self.capabilities.remove(capability);
    }

    /// Check if a capability is granted
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities
            .get(capability)
            .map_or(false, |&level| level != PermissionLevel::None)
    }

    /// Get permission level for a capability
    pub fn get_permission_level(&self, capability: &str) -> PermissionLevel {
        self.capabilities.get(capability).copied().unwrap_or(PermissionLevel::None)
    }

    /// Get total number of granted capabilities
    pub fn capabilities_count(&self) -> usize {
        self.capabilities.len()
    }

    /// Get all granted capabilities
    pub fn granted_capabilities(&self) -> Vec<&String> {
        self.capabilities.keys().collect()
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self::new()
    }
}

/// Security policy configuration for plugins
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityPolicy {
    /// Default capability set for new plugins
    pub default_capabilities: CapabilitySet,
    /// Maximum threat level allowed
    pub max_threat_level: ThreatLevel,
    /// Whether to enable sandbox mode
    pub sandbox_enabled: bool,
    /// Custom security rules
    pub custom_rules: Vec<SecurityRule>,
    /// Security audit settings
    pub audit_config: AuditConfig,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            default_capabilities: CapabilitySet::default_safe(),
            max_threat_level: ThreatLevel::Medium,
            sandbox_enabled: true,
            custom_rules: Vec::new(),
            audit_config: AuditConfig::default(),
        }
    }
}

/// Threat level assessment for plugins
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreatLevel {
    /// Minimal threat - basic operations only
    Minimal,
    /// Low threat - limited capabilities
    Low,
    /// Medium threat - moderate capabilities
    Medium,
    /// High threat - extensive capabilities
    High,
    /// Critical threat - unrestricted access
    Critical,
}

impl fmt::Display for ThreatLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThreatLevel::Minimal => write!(f, "MINIMAL"),
            ThreatLevel::Low => write!(f, "LOW"),
            ThreatLevel::Medium => write!(f, "MEDIUM"),
            ThreatLevel::High => write!(f, "HIGH"),
            ThreatLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

impl ThreatLevel {
    /// Convert threat level to numeric score
    pub fn score(&self) -> u8 {
        match self {
            ThreatLevel::Minimal => 1,
            ThreatLevel::Low => 2,
            ThreatLevel::Medium => 3,
            ThreatLevel::High => 4,
            ThreatLevel::Critical => 5,
        }
    }

    /// Create threat level from score
    pub fn from_score(score: u8) -> Self {
        match score {
            1 => ThreatLevel::Minimal,
            2 => ThreatLevel::Low,
            3 => ThreatLevel::Medium,
            4 => ThreatLevel::High,
            _ => ThreatLevel::Critical,
        }
    }
}

/// Security context for plugin operations
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Unique context identifier
    pub id: Uuid,
    /// Current security policy
    pub policy: SecurityPolicy,
    /// Plugin capabilities
    pub capabilities: CapabilitySet,
    /// Current threat level assessment
    pub threat_level: ThreatLevel,
    /// Audit trail
    pub audit_trail: Vec<SecurityEvent>,
}

impl SecurityContext {
    /// Create a new security context
    pub fn new(id: Uuid, capabilities: CapabilitySet) -> Self {
        let policy = SecurityPolicy::default();
        let threat_level = Self::assess_threat_level(&policy, &capabilities);

        Self { id, policy, capabilities, threat_level, audit_trail: Vec::new() }
    }

    /// Create a security context with specific policy
    pub fn with_policy(id: Uuid, policy: SecurityPolicy, capabilities: CapabilitySet) -> Self {
        let threat_level = Self::assess_threat_level(&policy, &capabilities);

        Self { id, policy, capabilities, threat_level, audit_trail: Vec::new() }
    }

    /// Assess threat level based on policy and capabilities
    fn assess_threat_level(_policy: &SecurityPolicy, capabilities: &CapabilitySet) -> ThreatLevel {
        // Simple heuristic based on capability count
        let capability_count = capabilities.capabilities_count();

        match capability_count {
            0..=2 => ThreatLevel::Minimal,
            3..=5 => ThreatLevel::Low,
            6..=10 => ThreatLevel::Medium,
            11..=20 => ThreatLevel::High,
            _ => ThreatLevel::Critical,
        }
    }

    /// Check if an operation is allowed
    pub fn check_operation(&mut self, operation: &SecurityOperation) -> PluginResult<()> {
        // Check capability permissions
        if !self.allows_operation(operation) {
            return Err(PluginError::SecurityViolation(format!(
                "Operation not allowed by capabilities: {:?}",
                operation
            )));
        }

        // Apply custom security rules
        for rule in &self.policy.custom_rules {
            if !rule.allows_operation(operation) {
                return Err(PluginError::SecurityViolation(format!(
                    "Operation blocked by security rule: {}",
                    rule.name
                )));
            }
        }

        // Log the operation for audit
        let event = SecurityEvent::new(operation.clone(), true);
        self.audit_trail.push(event);

        Ok(())
    }

    /// Check if capabilities allow an operation
    fn allows_operation(&self, operation: &SecurityOperation) -> bool {
        match operation {
            SecurityOperation::NetworkAccess { .. } => self.capabilities.has_capability("network"),
            SecurityOperation::FileSystemRead { .. } => {
                self.capabilities.has_capability("filesystem_read")
            },
            SecurityOperation::FileSystemWrite { .. } => {
                self.capabilities.has_capability("filesystem_write")
            },
            SecurityOperation::MemoryAllocation { .. } => {
                self.capabilities.has_capability("memory")
            },
            SecurityOperation::SystemCall { .. } => self.capabilities.has_capability("system"),
            SecurityOperation::IpcMessage { .. } => self.capabilities.has_capability("ipc"),
        }
    }

    /// Check if a specific capability is granted
    pub fn check_capability(&self, capability: &str) -> bool {
        self.capabilities.has_capability(capability)
    }

    /// Update threat level based on behavior
    pub fn update_threat_level(&mut self, delta: i8) {
        let current_score = self.threat_level.score();
        let new_score = (current_score as i8 + delta).max(1).min(5) as u8;
        self.threat_level = ThreatLevel::from_score(new_score);
    }
}

/// Security operation types for validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityOperation {
    NetworkAccess { host: String, port: u16 },
    FileSystemRead { path: String },
    FileSystemWrite { path: String },
    MemoryAllocation { size: usize },
    SystemCall { call: String },
    IpcMessage { target: String, size: usize },
}

/// Custom security rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityRule {
    pub name: String,
    pub description: String,
    pub rule_type: SecurityRuleType,
    pub conditions: Vec<SecurityCondition>,
}

impl SecurityRule {
    /// Check if this rule allows the operation
    pub fn allows_operation(&self, operation: &SecurityOperation) -> bool {
        match self.rule_type {
            SecurityRuleType::Allow => {
                // Allow rule - operation is allowed if conditions match
                self.conditions.iter().any(|c| c.matches_operation(operation))
            },
            SecurityRuleType::Deny => {
                // Deny rule - operation is denied if conditions match
                !self.conditions.iter().any(|c| c.matches_operation(operation))
            },
        }
    }
}

/// Security rule type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityRuleType {
    Allow,
    Deny,
}

/// Security condition for rules
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityCondition {
    pub field: String,
    pub operator: ConditionOperator,
    pub value: String,
}

impl SecurityCondition {
    /// Check if this condition matches the operation
    pub fn matches_operation(&self, operation: &SecurityOperation) -> bool {
        let field_value = self.extract_field_value(operation);
        self.operator.matches(&field_value, &self.value)
    }

    /// Extract field value from operation
    fn extract_field_value(&self, operation: &SecurityOperation) -> String {
        match (self.field.as_str(), operation) {
            ("host", SecurityOperation::NetworkAccess { host, .. }) => host.clone(),
            ("port", SecurityOperation::NetworkAccess { port, .. }) => port.to_string(),
            ("path", SecurityOperation::FileSystemRead { path }) => path.clone(),
            ("path", SecurityOperation::FileSystemWrite { path }) => path.clone(),
            ("size", SecurityOperation::MemoryAllocation { size }) => size.to_string(),
            ("call", SecurityOperation::SystemCall { call }) => call.clone(),
            ("target", SecurityOperation::IpcMessage { target, .. }) => target.clone(),
            _ => String::new(),
        }
    }
}

/// Condition operator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConditionOperator {
    Equals,
    NotEquals,
    Contains,
    StartsWith,
    EndsWith,
    GreaterThan,
    LessThan,
}

impl ConditionOperator {
    /// Check if the operator matches
    pub fn matches(&self, field_value: &str, condition_value: &str) -> bool {
        match self {
            ConditionOperator::Equals => field_value == condition_value,
            ConditionOperator::NotEquals => field_value != condition_value,
            ConditionOperator::Contains => field_value.contains(condition_value),
            ConditionOperator::StartsWith => field_value.starts_with(condition_value),
            ConditionOperator::EndsWith => field_value.ends_with(condition_value),
            ConditionOperator::GreaterThan => {
                field_value.parse::<f64>().unwrap_or(0.0)
                    > condition_value.parse::<f64>().unwrap_or(0.0)
            },
            ConditionOperator::LessThan => {
                field_value.parse::<f64>().unwrap_or(0.0)
                    < condition_value.parse::<f64>().unwrap_or(0.0)
            },
        }
    }
}

/// Security event for audit trail
#[derive(Debug, Clone)]
pub struct SecurityEvent {
    pub timestamp: std::time::SystemTime,
    pub operation: SecurityOperation,
    pub allowed: bool,
    pub threat_level: Option<ThreatLevel>,
}

impl SecurityEvent {
    /// Create a new security event
    pub fn new(operation: SecurityOperation, allowed: bool) -> Self {
        Self { timestamp: std::time::SystemTime::now(), operation, allowed, threat_level: None }
    }

    /// Set threat level for the event
    pub fn with_threat_level(mut self, threat_level: ThreatLevel) -> Self {
        self.threat_level = Some(threat_level);
        self
    }
}

/// Audit configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_all_operations: bool,
    pub log_denied_operations: bool,
    pub max_audit_entries: usize,
    pub audit_file_path: Option<String>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_all_operations: false,
            log_denied_operations: true,
            max_audit_entries: 10000,
            audit_file_path: Some("plugin_security_audit.log".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_set_creation() {
        let mut caps = CapabilitySet::new();
        assert_eq!(caps.capabilities_count(), 0);
        assert!(!caps.has_capability("network"));

        caps.grant("network".to_string(), PermissionLevel::Read);
        assert!(caps.has_capability("network"));
        assert_eq!(caps.get_permission_level("network"), PermissionLevel::Read);
    }

    #[test]
    fn test_threat_level_scoring() {
        assert_eq!(ThreatLevel::Minimal.score(), 1);
        assert_eq!(ThreatLevel::Critical.score(), 5);

        assert_eq!(ThreatLevel::from_score(3), ThreatLevel::Medium);
    }

    #[test]
    fn test_security_context_creation() {
        let capabilities = CapabilitySet::default_safe();
        let context = SecurityContext::new(Uuid::new_v4(), capabilities);

        // Should have minimal threat level with default safe capabilities
        assert_eq!(context.threat_level, ThreatLevel::Minimal);
    }

    #[test]
    fn test_security_rule_evaluation() {
        let rule = SecurityRule {
            name: "block_sensitive_paths".to_string(),
            description: "Block access to sensitive file paths".to_string(),
            rule_type: SecurityRuleType::Deny,
            conditions: vec![SecurityCondition {
                field: "path".to_string(),
                operator: ConditionOperator::StartsWith,
                value: "/etc/".to_string(),
            }],
        };

        let operation = SecurityOperation::FileSystemRead { path: "/etc/passwd".to_string() };

        // Should be denied by the rule
        assert!(!rule.allows_operation(&operation));
    }

    #[test]
    fn test_capability_permissions() {
        let mut caps = CapabilitySet::new();
        caps.grant("filesystem_read".to_string(), PermissionLevel::Read);

        let mut context = SecurityContext::new(Uuid::new_v4(), caps);

        let read_op = SecurityOperation::FileSystemRead { path: "/tmp/test".to_string() };
        let write_op = SecurityOperation::FileSystemWrite { path: "/tmp/test".to_string() };

        // Read should be allowed
        assert!(context.check_operation(&read_op).is_ok());

        // Write should be denied (no filesystem_write capability)
        assert!(context.check_operation(&write_op).is_err());
    }
}
