//! Plugin capability system for security and feature management.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Plugin capabilities and permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCapabilities {
    /// Message processing capabilities
    pub message_processing: MessageProcessingCapabilities,
    
    /// I/O capabilities 
    pub io: IoCapabilities,
    
    /// Network capabilities
    pub network: NetworkCapabilities,
    
    /// System capabilities
    pub system: SystemCapabilities,
    
    /// Custom capabilities (plugin-specific)
    pub custom: HashSet<String>,
}

impl PluginCapabilities {
    /// Create a new empty capability set.
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create capabilities for read-only message processing.
    pub fn message_processor() -> Self {
        Self {
            message_processing: MessageProcessingCapabilities {
                can_read: true,
                can_modify: false,
                can_route: false,
                can_split: false,
                can_merge: false,
            },
            ..Default::default()
        }
    }
    
    /// Create capabilities for message transformation.
    pub fn message_transformer() -> Self {
        Self {
            message_processing: MessageProcessingCapabilities {
                can_read: true,
                can_modify: true,
                can_route: false,
                can_split: false,
                can_merge: false,
            },
            ..Default::default()
        }
    }
    
    /// Create capabilities for message routing.
    pub fn message_router() -> Self {
        Self {
            message_processing: MessageProcessingCapabilities {
                can_read: true,
                can_modify: false,
                can_route: true,
                can_split: true,
                can_merge: true,
            },
            ..Default::default()
        }
    }
    
    /// Add a custom capability.
    pub fn with_custom_capability(mut self, capability: String) -> Self {
        self.custom.insert(capability);
        self
    }
    
    /// Check if plugin has a specific custom capability.
    pub fn has_custom_capability(&self, capability: &str) -> bool {
        self.custom.contains(capability)
    }
}

/// Message processing capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageProcessingCapabilities {
    /// Can read message content
    pub can_read: bool,
    
    /// Can modify message content
    pub can_modify: bool,
    
    /// Can route messages to different topics/partitions
    pub can_route: bool,
    
    /// Can split a message into multiple messages
    pub can_split: bool,
    
    /// Can merge multiple messages into one
    pub can_merge: bool,
}

/// I/O capabilities for file system access.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IoCapabilities {
    /// Can read from file system
    pub can_read_files: bool,
    
    /// Can write to file system
    pub can_write_files: bool,
    
    /// Can create directories
    pub can_create_directories: bool,
    
    /// Can delete files/directories
    pub can_delete: bool,
    
    /// Allowed file path patterns
    pub allowed_paths: Vec<String>,
}

/// Network capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkCapabilities {
    /// Can make outbound HTTP requests
    pub can_http_client: bool,
    
    /// Can create HTTP servers
    pub can_http_server: bool,
    
    /// Can make TCP connections
    pub can_tcp_client: bool,
    
    /// Can create TCP servers
    pub can_tcp_server: bool,
    
    /// Can make UDP connections
    pub can_udp: bool,
    
    /// Allowed network hosts/domains
    pub allowed_hosts: Vec<String>,
    
    /// Allowed network ports
    pub allowed_ports: Vec<u16>,
}

/// System capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemCapabilities {
    /// Can execute system commands
    pub can_execute_commands: bool,
    
    /// Can access environment variables
    pub can_access_env: bool,
    
    /// Can spawn child processes
    pub can_spawn_processes: bool,
    
    /// Can access system time
    pub can_access_time: bool,
    
    /// Can access random number generation
    pub can_access_random: bool,
}

/// Capability level for fine-grained access control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityLevel {
    /// No access
    None,
    
    /// Read-only access
    Read,
    
    /// Read and write access
    Write,
    
    /// Full administrative access
    Admin,
}

impl Default for CapabilityLevel {
    fn default() -> Self {
        Self::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_capabilities_creation() {
        let caps = PluginCapabilities::new();
        assert!(!caps.message_processing.can_read);
        assert!(!caps.message_processing.can_modify);
        
        let processor_caps = PluginCapabilities::message_processor();
        assert!(processor_caps.message_processing.can_read);
        assert!(!processor_caps.message_processing.can_modify);
        
        let transformer_caps = PluginCapabilities::message_transformer();
        assert!(transformer_caps.message_processing.can_read);
        assert!(transformer_caps.message_processing.can_modify);
        
        let router_caps = PluginCapabilities::message_router();
        assert!(router_caps.message_processing.can_read);
        assert!(router_caps.message_processing.can_route);
    }
    
    #[test]
    fn test_custom_capabilities() {
        let caps = PluginCapabilities::new()
            .with_custom_capability("custom.feature.1".to_string())
            .with_custom_capability("custom.feature.2".to_string());
            
        assert!(caps.has_custom_capability("custom.feature.1"));
        assert!(caps.has_custom_capability("custom.feature.2"));
        assert!(!caps.has_custom_capability("custom.feature.3"));
    }
    
    #[test]
    fn test_capability_serialization() {
        let caps = PluginCapabilities::message_transformer()
            .with_custom_capability("test.capability".to_string());
            
        let serialized = serde_json::to_string(&caps).unwrap();
        let deserialized: PluginCapabilities = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(caps, deserialized);
    }
}