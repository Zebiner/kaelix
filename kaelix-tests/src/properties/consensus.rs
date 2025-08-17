//! Raft consensus property tests for distributed streaming system validation.
//!
//! This module contains comprehensive property-based tests for the Raft consensus
//! protocol implementation, ensuring correctness under various failure scenarios.

use crate::{
    generators::properties::*,
    validators::invariants::*,
    macros::*,
    TestContext, TestResult,
    constants::*,
};
use kaelix_core::{Message, MessageId, PartitionId, Offset, prelude::*};
use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// Property tests for leader election in Raft consensus.
pub mod leader_election {
    use super::*;

    proptest_invariant! {
        test_single_leader_per_term(
            node_count in 3usize..=7usize,  // Typical cluster sizes
            term_duration_secs in 5u64..=30u64
        ) {
            let ctx = TestContext::new().await?;
            
            // Create cluster with specified number of nodes
            let cluster = ctx.create_cluster(node_count).await?;
            
            // Allow time for leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Verify exactly one leader exists
            let leaders = cluster.get_leaders().await?;
            if leaders.len() != 1 {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Expected exactly 1 leader, found {} leaders: {:?}",
                        leaders.len(), leaders
                    ),
                });
            }
            
            // Wait for term duration and verify leader stability
            tokio::time::sleep(Duration::from_secs(term_duration_secs)).await;
            
            let leaders_after = cluster.get_leaders().await?;
            if leaders_after.len() != 1 {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Leader stability violated: {} leaders after term",
                        leaders_after.len()
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_chaos! {
        test_leader_election_after_failure(
            scenario in arb_chaos_scenario().prop_filter(
                "Only node crash scenarios",
                |s| matches!(s.failure_type, FailureType::NodeCrash)
            ),
            node_count in 3usize..=5usize
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(node_count).await?;
            
            // Wait for initial leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            let initial_leader = cluster.get_leaders().await?[0].clone();
            
            // Inject node crash (targeting the leader)
            cluster.crash_node(&initial_leader).await?;
            
            // Wait for re-election
            tokio::time::sleep(Duration::from_secs(scenario.timing.duration_seconds.min(10))).await;
            
            // Verify new leader elected
            let new_leaders = cluster.get_leaders().await?;
            if new_leaders.is_empty() {
                return Err(TestError::AssertionFailed {
                    message: "No leader elected after crash".to_string(),
                });
            }
            
            if new_leaders.len() > 1 {
                return Err(TestError::AssertionFailed {
                    message: format!("Multiple leaders after crash: {:?}", new_leaders),
                });
            }
            
            // New leader should be different from crashed leader
            if new_leaders[0] == initial_leader {
                return Err(TestError::AssertionFailed {
                    message: "Same leader after crash (should be impossible)".to_string(),
                });
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_leader_election_timing(
            node_count in 3usize..=7usize,
            network_delay_ms in 10u64..=100u64
        ) {
            let ctx = TestContext::new().await?;
            
            // Create cluster with network delay
            let cluster = ctx.create_cluster_with_network_delay(node_count, network_delay_ms).await?;
            
            let start = Instant::now();
            
            // Wait for leader election
            let mut leader_elected = false;
            for _ in 0..50 { // Check every 100ms for up to 5 seconds
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                let leaders = cluster.get_leaders().await?;
                if leaders.len() == 1 {
                    leader_elected = true;
                    break;
                }
            }
            
            let election_time = start.elapsed();
            
            if !leader_elected {
                return Err(TestError::AssertionFailed {
                    message: "Leader election failed within timeout".to_string(),
                });
            }
            
            // Election time should be reasonable given network delay
            let max_expected_time = Duration::from_millis(network_delay_ms * 20 + 1000);
            if election_time > max_expected_time {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Leader election took too long: {:.2}s (max expected: {:.2}s)",
                        election_time.as_secs_f64(),
                        max_expected_time.as_secs_f64()
                    ),
                });
            }
            
            println!("Leader elected in {:.2}s with {}ms network delay", 
                election_time.as_secs_f64(), network_delay_ms);
            
            Ok(())
        }
    }
}

/// Property tests for log replication in Raft consensus.
pub mod log_replication {
    use super::*;

    proptest_invariant! {
        test_log_consistency_across_replicas(
            messages in prop::collection::vec(arb_message(), 20..=100),
            node_count in 3usize..=5usize
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(node_count).await?;
            
            // Wait for leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Publish messages through the cluster
            for message in messages {
                cluster.publish_message(message).await?;
            }
            
            // Allow time for replication
            tokio::time::sleep(Duration::from_secs(1)).await;
            
            // Get logs from all nodes
            let node_logs = cluster.get_all_node_logs().await?;
            
            // Verify log consistency
            let mut reference_log = None;
            for (node_id, log) in &node_logs {
                if reference_log.is_none() {
                    reference_log = Some(log);
                } else {
                    let reference = reference_log.unwrap();
                    
                    // Check that committed entries are identical
                    let min_committed = reference.committed_index.min(log.committed_index);
                    
                    for i in 0..=min_committed {
                        if reference.entries.get(i as usize) != log.entries.get(i as usize) {
                            return Err(TestError::AssertionFailed {
                                message: format!(
                                    "Log inconsistency at index {} between nodes",
                                    i
                                ),
                            });
                        }
                    }
                }
            }
            
            Ok(())
        }
    }

    proptest_chaos! {
        test_log_replication_during_network_partition(
            scenario in arb_chaos_scenario().prop_filter(
                "Only network partition scenarios",
                |s| matches!(s.failure_type, FailureType::NetworkPartition)
            ),
            messages in prop::collection::vec(arb_message(), 10..=50)
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(5).await?; // 5-node cluster for partition testing
            
            // Wait for initial leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Send some messages before partition
            let pre_partition_messages = &messages[..messages.len()/2];
            for message in pre_partition_messages {
                cluster.publish_message(message.clone()).await?;
            }
            
            // Create network partition (split into 3-2)
            cluster.create_network_partition(&["node-1", "node-2", "node-3"], &["node-4", "node-5"]).await?;
            
            // Allow partition to settle
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Try to send messages to both partitions
            let post_partition_messages = &messages[messages.len()/2..];
            let majority_partition_result = cluster.publish_to_partition(
                &["node-1", "node-2", "node-3"],
                post_partition_messages.to_vec()
            ).await;
            
            let minority_partition_result = cluster.publish_to_partition(
                &["node-4", "node-5"],
                post_partition_messages.to_vec()
            ).await;
            
            // Majority partition should succeed, minority should fail
            if majority_partition_result.is_err() {
                return Err(TestError::AssertionFailed {
                    message: "Majority partition should remain operational".to_string(),
                });
            }
            
            if minority_partition_result.is_ok() {
                return Err(TestError::AssertionFailed {
                    message: "Minority partition should not accept writes".to_string(),
                });
            }
            
            // Heal partition
            cluster.heal_network_partition().await?;
            
            // Allow time for reconciliation
            tokio::time::sleep(Duration::from_secs(3)).await;
            
            // Verify all nodes eventually converge
            let final_logs = cluster.get_all_node_logs().await?;
            let mut reference_committed_log = None;
            
            for (_, log) in &final_logs {
                let committed_entries = &log.entries[..=log.committed_index as usize];
                
                if reference_committed_log.is_none() {
                    reference_committed_log = Some(committed_entries);
                } else {
                    if committed_entries != reference_committed_log.unwrap() {
                        return Err(TestError::AssertionFailed {
                            message: "Nodes did not converge after partition heal".to_string(),
                        });
                    }
                }
            }
            
            Ok(())
        }
    }

    proptest_benchmark! {
        bench_replication_throughput(
            batch_size in 100usize..=1000usize,
            node_count in 3usize..=7usize
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(node_count).await?;
            
            // Wait for leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Generate test messages
            let mut messages = Vec::with_capacity(batch_size);
            for i in 0..batch_size {
                let payload = bytes::Bytes::from(format!("replication-test-{}", i));
                messages.push(Message::new("bench-topic", payload)?);
            }
            
            let start = Instant::now();
            
            // Send all messages
            for message in messages {
                cluster.publish_message(message).await?;
            }
            
            // Wait for full replication to all nodes
            cluster.wait_for_replication_sync().await?;
            
            let duration = start.elapsed();
            let throughput = batch_size as f64 / duration.as_secs_f64();
            
            println!("Replication throughput: {:.2} msg/sec across {} nodes", 
                throughput, node_count);
            
            // Minimum performance requirement for replication
            let min_throughput = 100.0; // Adjust based on requirements
            if throughput < min_throughput {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Replication throughput too low: {:.2} msg/sec (min: {})",
                        throughput, min_throughput
                    ),
                });
            }
            
            Ok(())
        }
    }
}

/// Property tests for safety properties in Raft consensus.
pub mod safety_properties {
    use super::*;

    proptest_invariant! {
        test_election_safety(
            node_count in 3usize..=7usize,
            election_cycles in 2usize..=5usize
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(node_count).await?;
            
            for cycle in 0..election_cycles {
                // Wait for leader election
                tokio::time::sleep(Duration::from_secs(2)).await;
                
                // Get current term and leader
                let current_state = cluster.get_cluster_state().await?;
                let current_term = current_state.current_term;
                let leaders_in_term: Vec<_> = current_state.nodes
                    .iter()
                    .filter(|(_, node)| node.role == NodeRole::Leader && node.term == current_term)
                    .collect();
                
                // Safety property: At most one leader per term
                if leaders_in_term.len() > 1 {
                    return Err(TestError::AssertionFailed {
                        message: format!(
                            "Election safety violated: {} leaders in term {} (cycle {})",
                            leaders_in_term.len(), current_term, cycle
                        ),
                    });
                }
                
                // Trigger new election by stopping current leader
                if let Some((leader_id, _)) = leaders_in_term.first() {
                    cluster.stop_node(leader_id).await?;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    cluster.start_node(leader_id).await?;
                }
            }
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_log_matching_property(
            messages in prop::collection::vec(arb_message(), 30..=100),
            node_count in 3usize..=5usize
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(node_count).await?;
            
            // Wait for leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Send messages
            for message in messages {
                cluster.publish_message(message).await?;
                
                // Small delay to allow for replication
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            // Allow final replication
            tokio::time::sleep(Duration::from_secs(1)).await;
            
            // Get logs from all nodes
            let node_logs = cluster.get_all_node_logs().await?;
            
            // Verify log matching property: if two logs contain an entry with the same
            // index and term, then all preceding entries are identical
            for (node1_id, log1) in &node_logs {
                for (node2_id, log2) in &node_logs {
                    if node1_id == node2_id {
                        continue;
                    }
                    
                    let min_len = log1.entries.len().min(log2.entries.len());
                    
                    for i in 0..min_len {
                        let entry1 = &log1.entries[i];
                        let entry2 = &log2.entries[i];
                        
                        // If entries at same index have same term, all preceding entries must match
                        if entry1.term == entry2.term {
                            for j in 0..=i {
                                if log1.entries[j] != log2.entries[j] {
                                    return Err(TestError::AssertionFailed {
                                        message: format!(
                                            "Log matching violated: entries at index {} differ between {} and {} but have same term at index {}",
                                            j, node1_id, node2_id, i
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_leader_completeness(
            messages in prop::collection::vec(arb_message(), 20..=50),
            node_count in 3usize..=5usize
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(node_count).await?;
            
            // Wait for initial leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Send and commit messages
            let mut committed_message_ids = Vec::new();
            for message in messages {
                cluster.publish_message(message.clone()).await?;
                
                // Wait for commit confirmation
                cluster.wait_for_message_commit(message.id()).await?;
                committed_message_ids.push(message.id());
            }
            
            // Force leader change
            let current_leader = cluster.get_leaders().await?[0].clone();
            cluster.crash_node(&current_leader).await?;
            
            // Wait for new leader election
            tokio::time::sleep(Duration::from_secs(3)).await;
            
            let new_leaders = cluster.get_leaders().await?;
            if new_leaders.is_empty() {
                return Err(TestError::AssertionFailed {
                    message: "No new leader elected".to_string(),
                });
            }
            
            let new_leader = &new_leaders[0];
            
            // Verify leader completeness: new leader must have all committed entries
            let new_leader_log = cluster.get_node_log(new_leader).await?;
            
            for committed_id in committed_message_ids {
                let found = new_leader_log.entries
                    .iter()
                    .any(|entry| entry.message_id == committed_id);
                
                if !found {
                    return Err(TestError::AssertionFailed {
                        message: format!(
                            "Leader completeness violated: new leader {} missing committed message {}",
                            new_leader, committed_id
                        ),
                    });
                }
            }
            
            Ok(())
        }
    }
}

/// Property tests for state machine safety in Raft consensus.
pub mod state_machine_safety {
    use super::*;

    proptest_invariant! {
        test_state_machine_consistency(
            operations in prop::collection::vec(arb_state_machine_operation(), 20..=100),
            node_count in 3usize..=5usize
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(node_count).await?;
            
            // Wait for leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Apply operations through consensus
            for operation in operations {
                cluster.apply_state_machine_operation(operation).await?;
            }
            
            // Allow all operations to be applied
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Get state machine state from all nodes
            let node_states = cluster.get_all_state_machine_states().await?;
            
            // Verify all state machines are in the same state
            let mut reference_state = None;
            for (node_id, state) in &node_states {
                if reference_state.is_none() {
                    reference_state = Some(state);
                } else {
                    if state != reference_state.unwrap() {
                        return Err(TestError::AssertionFailed {
                            message: format!(
                                "State machine inconsistency detected on node {}",
                                node_id
                            ),
                        });
                    }
                }
            }
            
            Ok(())
        }
    }

    proptest_chaos! {
        test_state_machine_safety_under_failures(
            operations in prop::collection::vec(arb_state_machine_operation(), 30..=100),
            scenario in arb_chaos_scenario()
        ) {
            let ctx = TestContext::new().await?;
            let cluster = ctx.create_cluster(5).await?;
            
            // Wait for initial leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Apply some operations before failure
            let pre_failure_ops = &operations[..operations.len()/2];
            for operation in pre_failure_ops {
                cluster.apply_state_machine_operation(operation.clone()).await?;
            }
            
            // Inject failure
            match scenario.failure_type {
                FailureType::NodeCrash => {
                    let nodes = cluster.get_node_ids().await?;
                    cluster.crash_node(&nodes[0]).await?;
                },
                FailureType::NetworkPartition => {
                    let nodes = cluster.get_node_ids().await?;
                    let (partition1, partition2) = nodes.split_at(3);
                    cluster.create_network_partition(partition1, partition2).await?;
                },
                _ => {
                    // Other failure types not implemented for this test
                    return Ok(());
                }
            }
            
            // Continue applying operations during failure
            let during_failure_ops = &operations[operations.len()/2..];
            for operation in during_failure_ops {
                // Some operations may fail during failure scenario
                let _ = cluster.apply_state_machine_operation(operation.clone()).await;
            }
            
            // Recover from failure
            match scenario.failure_type {
                FailureType::NodeCrash => {
                    let nodes = cluster.get_node_ids().await?;
                    cluster.restart_node(&nodes[0]).await?;
                },
                FailureType::NetworkPartition => {
                    cluster.heal_network_partition().await?;
                },
                _ => {}
            }
            
            // Allow recovery and synchronization
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            // Verify state machine safety: all nodes converge to consistent state
            let final_states = cluster.get_all_state_machine_states().await?;
            let mut reference_state = None;
            
            for (node_id, state) in &final_states {
                if reference_state.is_none() {
                    reference_state = Some(state);
                } else {
                    if state != reference_state.unwrap() {
                        return Err(TestError::AssertionFailed {
                            message: format!(
                                "State machine divergence after failure recovery on node {}",
                                node_id
                            ),
                        });
                    }
                }
            }
            
            Ok(())
        }
    }
}

/// Helper types for consensus testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateMachineOperation {
    Put { key: String, value: String },
    Delete { key: String },
    Increment { key: String },
}

fn arb_state_machine_operation() -> impl Strategy<Value = StateMachineOperation> {
    prop_oneof![
        (any::<String>(), any::<String>()).prop_map(|(k, v)| StateMachineOperation::Put { key: k, value: v }),
        any::<String>().prop_map(|k| StateMachineOperation::Delete { key: k }),
        any::<String>().prop_map(|k| StateMachineOperation::Increment { key: k }),
    ]
}

#[derive(Debug, Clone)]
pub struct ClusterState {
    pub current_term: u64,
    pub nodes: HashMap<String, NodeConsensusState>,
}

#[derive(Debug, Clone)]
pub struct NodeConsensusState {
    pub role: NodeRole,
    pub term: u64,
    pub voted_for: Option<String>,
    pub log_length: usize,
    pub commit_index: u64,
}

#[derive(Debug, Clone)]
pub struct NodeLog {
    pub entries: Vec<LogEntry>,
    pub committed_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub term: u64,
    pub message_id: MessageId,
    pub operation: StateMachineOperation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consensus_property_framework() {
        // Smoke test to ensure consensus property tests compile
        let result: Result<(), TestError> = async {
            let _operation = StateMachineOperation::Put {
                key: "test".to_string(),
                value: "value".to_string(),
            };
            Ok(())
        }.await;
        
        assert!(result.is_ok(), "Consensus property test framework should initialize");
    }
}