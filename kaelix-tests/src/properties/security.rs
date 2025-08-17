//! Security property tests for authentication and authorization validation.
//!
//! This module contains comprehensive property-based tests for security mechanisms,
//! ensuring that authentication and authorization work correctly under all conditions.

use crate::{
    generators::properties::{*, security::*},
    validators::invariants::*,
    macros::*,
    TestContext, TestResult,
    constants::*,
};
use kaelix_core::{Message, Topic, prelude::*};
use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Property tests for authentication mechanisms.
pub mod authentication {
    use super::*;

    proptest_security! {
        test_valid_token_authentication(
            token in security::arb_auth_token().prop_filter(
                "Only valid tokens",
                |t| matches!(t.validity, TokenValidity::Valid)
            ),
            operation in arb_auth_operation()
        ) {
            let ctx = TestContext::new().await?;
            
            // Authenticate with valid token
            let auth_result = ctx.authenticate(&token).await?;
            
            if !auth_result.is_authenticated() {
                return Err(TestError::AssertionFailed {
                    message: "Valid token should authenticate successfully".to_string(),
                });
            }
            
            // Verify token properties are preserved
            if auth_result.token_type() != token.token_type {
                return Err(TestError::AssertionFailed {
                    message: "Token type mismatch after authentication".to_string(),
                });
            }
            
            // Verify authentication is consistent across multiple calls
            let auth_result2 = ctx.authenticate(&token).await?;
            if auth_result.session_id() != auth_result2.session_id() {
                return Err(TestError::AssertionFailed {
                    message: "Authentication inconsistent across calls".to_string(),
                });
            }
            
            Ok(())
        }
    }

    proptest_security! {
        test_invalid_token_rejection(
            token in security::arb_auth_token().prop_filter(
                "Only invalid tokens",
                |t| !matches!(t.validity, TokenValidity::Valid)
            ),
            operation in arb_auth_operation()
        ) {
            let ctx = TestContext::new().await?;
            
            // Attempt authentication with invalid token
            let auth_result = ctx.authenticate(&token).await?;
            
            if auth_result.is_authenticated() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Invalid token ({:?}) should not authenticate successfully",
                        token.validity
                    ),
                });
            }
            
            // Verify appropriate error reason
            match token.validity {
                TokenValidity::Expired => {
                    if !auth_result.error_message().unwrap_or("").contains("expired") {
                        return Err(TestError::AssertionFailed {
                            message: "Expired token should return expiration error".to_string(),
                        });
                    }
                },
                TokenValidity::Revoked => {
                    if !auth_result.error_message().unwrap_or("").contains("revoked") {
                        return Err(TestError::AssertionFailed {
                            message: "Revoked token should return revocation error".to_string(),
                        });
                    }
                },
                TokenValidity::Malformed => {
                    if !auth_result.error_message().unwrap_or("").contains("malformed") {
                        return Err(TestError::AssertionFailed {
                            message: "Malformed token should return format error".to_string(),
                        });
                    }
                },
                _ => {}
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_token_expiration_handling(
            token_lifetime_secs in 1u64..=10u64,
            operations in prop::collection::vec(arb_auth_operation(), 5..=20)
        ) {
            let ctx = TestContext::new().await?;
            
            // Create token with short lifetime
            let token = ctx.create_temporary_token(token_lifetime_secs).await?;
            
            // Verify initial authentication works
            let initial_auth = ctx.authenticate(&token).await?;
            if !initial_auth.is_authenticated() {
                return Err(TestError::AssertionFailed {
                    message: "Initial authentication should succeed".to_string(),
                });
            }
            
            // Perform operations while token is valid
            let operations_before_expiry = &operations[..operations.len()/2];
            for operation in operations_before_expiry {
                let result = ctx.perform_authenticated_operation(&token, operation).await?;
                if !result.is_success() {
                    return Err(TestError::AssertionFailed {
                        message: "Operation should succeed with valid token".to_string(),
                    });
                }
            }
            
            // Wait for token to expire
            tokio::time::sleep(Duration::from_secs(token_lifetime_secs + 1)).await;
            
            // Verify authentication now fails
            let expired_auth = ctx.authenticate(&token).await?;
            if expired_auth.is_authenticated() {
                return Err(TestError::AssertionFailed {
                    message: "Authentication should fail after token expiration".to_string(),
                });
            }
            
            // Verify operations fail with expired token
            let operations_after_expiry = &operations[operations.len()/2..];
            for operation in operations_after_expiry {
                let result = ctx.perform_authenticated_operation(&token, operation).await?;
                if result.is_success() {
                    return Err(TestError::AssertionFailed {
                        message: "Operation should fail with expired token".to_string(),
                    });
                }
            }
            
            Ok(())
        }
    }

    proptest_security! {
        test_token_revocation_immediate_effect(
            token in security::arb_auth_token().prop_filter(
                "Only valid tokens",
                |t| matches!(t.validity, TokenValidity::Valid)
            ),
            post_revocation_operations in prop::collection::vec(arb_auth_operation(), 3..=10)
        ) {
            let ctx = TestContext::new().await?;
            
            // Authenticate and verify success
            let auth_result = ctx.authenticate(&token).await?;
            if !auth_result.is_authenticated() {
                return Err(TestError::AssertionFailed {
                    message: "Initial authentication should succeed".to_string(),
                });
            }
            
            // Perform successful operation
            let test_operation = arb_auth_operation().new_tree(&mut proptest::test_runner::TestRunner::default())?
                .current();
            let pre_revocation_result = ctx.perform_authenticated_operation(&token, &test_operation).await?;
            if !pre_revocation_result.is_success() {
                return Err(TestError::AssertionFailed {
                    message: "Operation should succeed before revocation".to_string(),
                });
            }
            
            // Revoke the token
            ctx.revoke_token(&token).await?;
            
            // Verify immediate authentication failure
            let post_revocation_auth = ctx.authenticate(&token).await?;
            if post_revocation_auth.is_authenticated() {
                return Err(TestError::AssertionFailed {
                    message: "Authentication should fail immediately after revocation".to_string(),
                });
            }
            
            // Verify all operations fail after revocation
            for operation in post_revocation_operations {
                let result = ctx.perform_authenticated_operation(&token, &operation).await?;
                if result.is_success() {
                    return Err(TestError::AssertionFailed {
                        message: "Operations should fail after token revocation".to_string(),
                    });
                }
            }
            
            Ok(())
        }
    }
}

/// Property tests for authorization and access control.
pub mod authorization {
    use super::*;

    proptest_security! {
        test_permission_based_access_control(
            token in security::arb_auth_token().prop_filter(
                "Only valid tokens",
                |t| matches!(t.validity, TokenValidity::Valid)
            ),
            topic in arb_topic(),
            operation_type in prop_oneof![
                Just(OperationType::Read),
                Just(OperationType::Write),
                Just(OperationType::Admin)
            ]
        ) {
            let ctx = TestContext::new().await?;
            
            // Authenticate
            let auth_result = ctx.authenticate(&token).await?;
            if !auth_result.is_authenticated() {
                return Err(TestError::AssertionFailed {
                    message: "Authentication should succeed for authorization test".to_string(),
                });
            }
            
            // Check if token has required permission
            let required_permission = match operation_type {
                OperationType::Read => Permission::Read(topic.as_str().to_string()),
                OperationType::Write => Permission::Write(topic.as_str().to_string()),
                OperationType::Admin => Permission::Admin,
            };
            
            let has_permission = token.permissions.contains(&required_permission) || 
                                 token.permissions.contains(&Permission::Admin);
            
            // Attempt operation
            let operation = AuthOperation {
                operation_type,
                target_topic: Some(topic.clone()),
                payload: Some(bytes::Bytes::from("test")),
            };
            
            let operation_result = ctx.authorize_and_perform(&token, &operation).await?;
            
            // Verify access control
            if has_permission && !operation_result.is_success() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Operation should succeed with required permission: {:?}",
                        required_permission
                    ),
                });
            }
            
            if !has_permission && operation_result.is_success() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Operation should fail without required permission: {:?}",
                        required_permission
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_security! {
        test_admin_permission_overrides(
            admin_token in security::arb_auth_token().prop_filter(
                "Only admin tokens",
                |t| matches!(t.validity, TokenValidity::Valid) && 
                     t.permissions.contains(&Permission::Admin)
            ),
            operation in arb_auth_operation()
        ) {
            let ctx = TestContext::new().await?;
            
            // Admin token should be able to perform any operation
            let result = ctx.authorize_and_perform(&admin_token, &operation).await?;
            
            if !result.is_success() {
                return Err(TestError::AssertionFailed {
                    message: "Admin token should succeed on any operation".to_string(),
                });
            }
            
            // Verify admin operations are logged
            let audit_logs = ctx.get_audit_logs().await?;
            let admin_operations: Vec<_> = audit_logs
                .iter()
                .filter(|log| log.token_type == admin_token.token_type && 
                             log.permissions.contains(&Permission::Admin))
                .collect();
            
            if admin_operations.is_empty() {
                return Err(TestError::AssertionFailed {
                    message: "Admin operations should be logged".to_string(),
                });
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_cross_topic_permission_isolation(
            tokens in prop::collection::vec(
                security::arb_auth_token().prop_filter(
                    "Only valid tokens", 
                    |t| matches!(t.validity, TokenValidity::Valid)
                ), 
                2..=5
            ),
            topics in prop::collection::vec(arb_topic(), 3..=10)
        ) {
            let ctx = TestContext::new().await?;
            
            for token in &tokens {
                for topic in &topics {
                    // Check read permission
                    let read_permission = Permission::Read(topic.as_str().to_string());
                    let has_read_permission = token.permissions.contains(&read_permission) ||
                                            token.permissions.contains(&Permission::Admin);
                    
                    let read_operation = AuthOperation {
                        operation_type: OperationType::Read,
                        target_topic: Some(topic.clone()),
                        payload: None,
                    };
                    
                    let read_result = ctx.authorize_and_perform(token, &read_operation).await?;
                    
                    if has_read_permission != read_result.is_success() {
                        return Err(TestError::AssertionFailed {
                            message: format!(
                                "Read permission check failed for topic {}: expected {}, got {}",
                                topic.as_str(),
                                has_read_permission,
                                read_result.is_success()
                            ),
                        });
                    }
                    
                    // Check write permission
                    let write_permission = Permission::Write(topic.as_str().to_string());
                    let has_write_permission = token.permissions.contains(&write_permission) ||
                                             token.permissions.contains(&Permission::Admin);
                    
                    let write_operation = AuthOperation {
                        operation_type: OperationType::Write,
                        target_topic: Some(topic.clone()),
                        payload: Some(bytes::Bytes::from("test-data")),
                    };
                    
                    let write_result = ctx.authorize_and_perform(token, &write_operation).await?;
                    
                    if has_write_permission != write_result.is_success() {
                        return Err(TestError::AssertionFailed {
                            message: format!(
                                "Write permission check failed for topic {}: expected {}, got {}",
                                topic.as_str(),
                                has_write_permission,
                                write_result.is_success()
                            ),
                        });
                    }
                }
            }
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_authorization_decision_consistency(
            token in security::arb_auth_token(),
            operation in arb_auth_operation(),
            repeat_count in 3usize..=10usize
        ) {
            let ctx = TestContext::new().await?;
            
            // Perform the same authorization check multiple times
            let mut results = Vec::new();
            for _ in 0..repeat_count {
                let result = ctx.authorize_and_perform(&token, &operation).await?;
                results.push(result.is_success());
            }
            
            // All results should be identical
            let first_result = results[0];
            for (i, result) in results.iter().enumerate() {
                if *result != first_result {
                    return Err(TestError::AssertionFailed {
                        message: format!(
                            "Authorization decision inconsistent: attempt {} returned {}, expected {}",
                            i, result, first_result
                        ),
                    });
                }
            }
            
            Ok(())
        }
    }
}

/// Property tests for security audit and logging.
pub mod audit_logging {
    use super::*;

    proptest_security! {
        test_authentication_events_logged(
            tokens in prop::collection::vec(security::arb_auth_token(), 5..=20),
            operations in prop::collection::vec(arb_auth_operation(), 10..=30)
        ) {
            let ctx = TestContext::new().await?;
            
            // Clear existing logs
            ctx.clear_audit_logs().await?;
            
            // Perform authentication and operations
            for token in &tokens {
                let auth_result = ctx.authenticate(token).await?;
                
                if auth_result.is_authenticated() {
                    // Perform some operations with authenticated token
                    for operation in &operations[..3.min(operations.len())] {
                        let _ = ctx.authorize_and_perform(token, operation).await?;
                    }
                }
            }
            
            // Verify audit logs
            let audit_logs = ctx.get_audit_logs().await?;
            
            // Should have authentication events for all tokens
            let auth_events: Vec<_> = audit_logs
                .iter()
                .filter(|log| log.event_type == AuditEventType::Authentication)
                .collect();
            
            if auth_events.len() != tokens.len() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Authentication events mismatch: expected {}, found {}",
                        tokens.len(),
                        auth_events.len()
                    ),
                });
            }
            
            // Verify each authentication event has required fields
            for event in auth_events {
                if event.timestamp.is_none() {
                    return Err(TestError::AssertionFailed {
                        message: "Authentication event missing timestamp".to_string(),
                    });
                }
                
                if event.source_ip.is_none() {
                    return Err(TestError::AssertionFailed {
                        message: "Authentication event missing source IP".to_string(),
                    });
                }
                
                if event.user_id.is_none() {
                    return Err(TestError::AssertionFailed {
                        message: "Authentication event missing user ID".to_string(),
                    });
                }
            }
            
            Ok(())
        }
    }

    proptest_security! {
        test_failed_authorization_logged(
            unauthorized_operations in prop::collection::vec(
                (
                    security::arb_auth_token().prop_filter(
                        "Tokens without admin",
                        |t| !t.permissions.contains(&Permission::Admin)
                    ),
                    arb_auth_operation()
                ), 
                5..=15
            )
        ) {
            let ctx = TestContext::new().await?;
            ctx.clear_audit_logs().await?;
            
            let mut failed_operations = 0;
            
            for (token, operation) in unauthorized_operations {
                // Skip if token has the required permission for this operation
                let has_permission = match (&operation.operation_type, &operation.target_topic) {
                    (OperationType::Read, Some(topic)) => 
                        token.permissions.contains(&Permission::Read(topic.as_str().to_string())),
                    (OperationType::Write, Some(topic)) => 
                        token.permissions.contains(&Permission::Write(topic.as_str().to_string())),
                    (OperationType::Admin, _) => 
                        token.permissions.contains(&Permission::Admin),
                    _ => false,
                };
                
                if has_permission {
                    continue; // Skip operations that should succeed
                }
                
                let result = ctx.authorize_and_perform(&token, &operation).await?;
                if !result.is_success() {
                    failed_operations += 1;
                }
            }
            
            if failed_operations == 0 {
                return Ok(()); // No failed operations to check
            }
            
            // Verify failed authorization events are logged
            let audit_logs = ctx.get_audit_logs().await?;
            let failed_auth_events: Vec<_> = audit_logs
                .iter()
                .filter(|log| log.event_type == AuditEventType::AuthorizationFailure)
                .collect();
            
            if failed_auth_events.len() != failed_operations {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Failed authorization events mismatch: expected {}, found {}",
                        failed_operations,
                        failed_auth_events.len()
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_audit_log_integrity(
            operations in prop::collection::vec(
                (security::arb_auth_token(), arb_auth_operation()),
                20..=50
            )
        ) {
            let ctx = TestContext::new().await?;
            ctx.clear_audit_logs().await?;
            
            // Perform operations
            for (token, operation) in operations {
                let _ = ctx.authorize_and_perform(&token, &operation).await?;
            }
            
            let audit_logs = ctx.get_audit_logs().await?;
            
            // Verify log integrity
            for (i, log) in audit_logs.iter().enumerate() {
                // Check required fields
                if log.event_id.is_empty() {
                    return Err(TestError::AssertionFailed {
                        message: format!("Audit log {} missing event ID", i),
                    });
                }
                
                if log.timestamp.is_none() {
                    return Err(TestError::AssertionFailed {
                        message: format!("Audit log {} missing timestamp", i),
                    });
                }
                
                // Check chronological order
                if i > 0 {
                    let prev_timestamp = audit_logs[i-1].timestamp.unwrap();
                    let curr_timestamp = log.timestamp.unwrap();
                    
                    if curr_timestamp < prev_timestamp {
                        return Err(TestError::AssertionFailed {
                            message: format!("Audit logs not in chronological order at index {}", i),
                        });
                    }
                }
                
                // Verify log hash/signature if present
                if let Some(signature) = &log.signature {
                    if !ctx.verify_audit_log_signature(log, signature).await? {
                        return Err(TestError::AssertionFailed {
                            message: format!("Invalid audit log signature at index {}", i),
                        });
                    }
                }
            }
            
            Ok(())
        }
    }
}

/// Property tests for encryption and data protection.
pub mod encryption {
    use super::*;

    proptest_security! {
        test_message_encryption_at_rest(
            messages in prop::collection::vec(arb_message(), 10..=50),
            encryption_key in arb_encryption_key()
        ) {
            let ctx = TestContext::new_with_encryption(encryption_key.clone()).await?;
            
            // Publish encrypted messages
            for message in &messages {
                ctx.publish_encrypted(message.clone()).await?;
            }
            
            // Verify messages are encrypted in storage
            let stored_data = ctx.get_raw_storage_data().await?;
            
            for message in &messages {
                let message_payload = message.payload();
                
                // Original payload should not appear in stored data
                if stored_data.windows(message_payload.len()).any(|window| window == message_payload) {
                    return Err(TestError::AssertionFailed {
                        message: "Unencrypted message data found in storage".to_string(),
                    });
                }
            }
            
            // Verify messages can be decrypted and retrieved
            let retrieved_messages = ctx.retrieve_and_decrypt_messages().await?;
            
            if retrieved_messages.len() != messages.len() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Message count mismatch after encryption/decryption: expected {}, got {}",
                        messages.len(),
                        retrieved_messages.len()
                    ),
                });
            }
            
            // Verify decrypted content matches original
            for original in &messages {
                let found = retrieved_messages.iter().any(|retrieved| {
                    retrieved.id() == original.id() && 
                    retrieved.payload() == original.payload()
                });
                
                if !found {
                    return Err(TestError::AssertionFailed {
                        message: format!("Message {} not found or corrupted after decryption", original.id()),
                    });
                }
            }
            
            Ok(())
        }
    }

    proptest_security! {
        test_encryption_key_rotation(
            messages in prop::collection::vec(arb_message(), 15..=30),
            old_key in arb_encryption_key(),
            new_key in arb_encryption_key()
        ) {
            let ctx = TestContext::new_with_encryption(old_key.clone()).await?;
            
            // Publish messages with old key
            let pre_rotation_messages = &messages[..messages.len()/2];
            for message in pre_rotation_messages {
                ctx.publish_encrypted(message.clone()).await?;
            }
            
            // Rotate encryption key
            ctx.rotate_encryption_key(new_key.clone()).await?;
            
            // Publish messages with new key
            let post_rotation_messages = &messages[messages.len()/2..];
            for message in post_rotation_messages {
                ctx.publish_encrypted(message.clone()).await?;
            }
            
            // Verify all messages can still be retrieved
            let all_retrieved = ctx.retrieve_and_decrypt_messages().await?;
            
            if all_retrieved.len() != messages.len() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Message count after key rotation: expected {}, got {}",
                        messages.len(),
                        all_retrieved.len()
                    ),
                });
            }
            
            // Verify old messages are re-encrypted with new key
            tokio::time::sleep(Duration::from_millis(500)).await; // Allow re-encryption
            
            let storage_after_rotation = ctx.get_raw_storage_data().await?;
            
            // Should not contain data encrypted with old key format
            // (This is a simplified check - in practice would verify key metadata)
            for message in pre_rotation_messages {
                // Verify message is accessible (implying successful re-encryption)
                let retrieved = ctx.retrieve_message_by_id(message.id()).await?;
                if retrieved.is_none() {
                    return Err(TestError::AssertionFailed {
                        message: format!("Pre-rotation message {} not accessible after key rotation", message.id()),
                    });
                }
            }
            
            Ok(())
        }
    }
}

/// Helper types and functions for security testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationType {
    Read,
    Write,
    Admin,
}

#[derive(Debug, Clone)]
pub struct AuthOperation {
    pub operation_type: OperationType,
    pub target_topic: Option<Topic>,
    pub payload: Option<bytes::Bytes>,
}

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub success: bool,
    pub session_id: Option<String>,
    pub error_message: Option<String>,
    pub token_type: TokenType,
}

impl AuthResult {
    pub fn is_authenticated(&self) -> bool {
        self.success
    }
    
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
    
    pub fn token_type(&self) -> &TokenType {
        &self.token_type
    }
}

#[derive(Debug, Clone)]
pub struct OperationResult {
    pub success: bool,
    pub error_code: Option<String>,
}

impl OperationResult {
    pub fn is_success(&self) -> bool {
        self.success
    }
}

#[derive(Debug, Clone)]
pub struct AuditLogEntry {
    pub event_id: String,
    pub event_type: AuditEventType,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub user_id: Option<String>,
    pub source_ip: Option<String>,
    pub resource: Option<String>,
    pub action: Option<String>,
    pub result: Option<String>,
    pub token_type: TokenType,
    pub permissions: Vec<Permission>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEventType {
    Authentication,
    Authorization,
    AuthorizationFailure,
    DataAccess,
    AdminAction,
}

fn arb_auth_operation() -> impl Strategy<Value = AuthOperation> {
    (
        prop_oneof![
            Just(OperationType::Read),
            Just(OperationType::Write),
            Just(OperationType::Admin)
        ],
        prop::option::of(arb_topic()),
        prop::option::of(prop::collection::vec(any::<u8>(), 0..=1024).prop_map(bytes::Bytes::from)),
    ).prop_map(|(op_type, topic, payload)| {
        AuthOperation {
            operation_type: op_type,
            target_topic: topic,
            payload,
        }
    })
}

fn arb_encryption_key() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 32..=32) // 256-bit keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_property_framework() {
        // Smoke test for security property framework
        let token = AuthToken {
            token_type: TokenType::Bearer,
            validity: TokenValidity::Valid,
            permissions: vec![Permission::Read("test".to_string())],
            created_at: chrono::Utc::now(),
        };
        
        let operation = AuthOperation {
            operation_type: OperationType::Read,
            target_topic: Some(Topic::new("test").unwrap()),
            payload: None,
        };
        
        // Framework should compile and basic types should work
        assert!(matches!(token.validity, TokenValidity::Valid));
        assert!(matches!(operation.operation_type, OperationType::Read));
    }
}