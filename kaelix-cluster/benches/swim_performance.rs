use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use kaelix_cluster::{
    membership::{NodeInfo, SwimMembership, SwimConfig, PhiFailureDetector},
    types::NodeId,
};
use std::{
    net::SocketAddr, 
    time::Duration,
};

fn create_test_node(id: Option<NodeId>, port: u16) -> NodeInfo {
    let node_id = id.unwrap_or_else(NodeId::generate);
    let address: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    NodeInfo::new(node_id, address)
}

fn create_swim_with_nodes(node_count: usize) -> SwimMembership {
    let local_node = create_test_node(None, 8000);
    let config = SwimConfig::default();
    let mut swim = SwimMembership::new(local_node, config);
    
    // Add nodes
    for i in 1..node_count {
        let node = create_test_node(None, 8000 + i as u16);
        swim.add_member(node).unwrap();
    }
    
    swim
}

/// Benchmark ping target selection across different cluster sizes
/// Target: <100μs for 10,000 nodes
fn bench_ping_target_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("ping_target_selection");
    
    for &node_count in [10, 100, 1000, 10000].iter() {
        let swim = create_swim_with_nodes(node_count);
        
        group.bench_with_input(
            BenchmarkId::new("select_ping_target", node_count),
            &node_count,
            |b, _| {
                b.iter(|| {
                    black_box(swim.select_ping_target())
                });
            },
        );
    }
    group.finish();
}

/// Benchmark membership update processing
/// Target: <1ms for 1000 updates
fn bench_membership_update_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("membership_update_processing");
    
    for &update_count in [1, 10, 100, 1000].iter() {
        let mut swim = create_swim_with_nodes(100);
        
        // Create updates
        let updates: Vec<NodeInfo> = (0..update_count)
            .map(|i| create_test_node(None, 9000 + i as u16))
            .collect();
        
        group.bench_with_input(
            BenchmarkId::new("process_membership_update", update_count),
            &update_count,
            |b, _| {
                b.iter(|| {
                    swim.process_membership_update(black_box(updates.clone())).unwrap()
                });
            },
        );
    }
    group.finish();
}

/// Benchmark phi-accrual failure detector performance
/// Target: <10μs per operation
fn bench_failure_detector(c: &mut Criterion) {
    let mut group = c.benchmark_group("phi_failure_detector");
    
    let mut detector = PhiFailureDetector::new(100, 8.0);
    
    // Populate with history
    for i in 0..100 {
        detector.heartbeat(Duration::from_millis(1000 + i % 200));
    }
    
    group.bench_function("heartbeat", |b| {
        b.iter(|| {
            detector.heartbeat(black_box(Duration::from_millis(1000)))
        });
    });
    
    group.bench_function("phi_calculation", |b| {
        b.iter(|| {
            black_box(detector.phi(Duration::from_millis(1200)))
        });
    });
    
    group.bench_function("is_failed_check", |b| {
        b.iter(|| {
            black_box(detector.is_failed(Duration::from_millis(1200)))
        });
    });
    
    group.finish();
}

/// Benchmark gossip target selection
/// Target: <50μs for 10,000 nodes
fn bench_gossip_target_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("gossip_target_selection");
    
    for &node_count in [10, 100, 1000, 10000].iter() {
        let swim = create_swim_with_nodes(node_count);
        let local_id = swim.local_node().id;
        
        group.bench_with_input(
            BenchmarkId::new("select_gossip_targets", node_count),
            &node_count,
            |b, _| {
                b.iter(|| {
                    black_box(swim.select_gossip_targets(&local_id))
                });
            },
        );
    }
    group.finish();
}

/// Benchmark proxy node selection for indirect pings
/// Target: <100μs for 10,000 nodes
fn bench_proxy_node_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("proxy_node_selection");
    
    for &node_count in [10, 100, 1000, 10000].iter() {
        let swim = create_swim_with_nodes(node_count);
        let target_id = NodeId::generate();
        
        group.bench_with_input(
            BenchmarkId::new("select_proxy_nodes", node_count),
            &node_count,
            |b, _| {
                b.iter(|| {
                    black_box(swim.select_proxy_nodes(&target_id, 3))
                });
            },
        );
    }
    group.finish();
}

/// Benchmark membership statistics calculation
/// Target: <1ms for 10,000 nodes
fn bench_membership_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("membership_stats");
    
    for &node_count in [10, 100, 1000, 10000].iter() {
        let mut swim = create_swim_with_nodes(node_count);
        
        // Mark some nodes as suspect/dead to make stats more realistic
        let members: Vec<NodeId> = swim.members().keys().take(node_count / 4).copied().collect();
        for (i, node_id) in members.iter().enumerate() {
            if *node_id != swim.local_node().id {
                if i % 2 == 0 {
                    swim.mark_suspect(node_id).unwrap();
                } else {
                    swim.mark_dead(node_id).unwrap();
                }
            }
        }
        
        group.bench_with_input(
            BenchmarkId::new("stats", node_count),
            &node_count,
            |b, _| {
                b.iter(|| {
                    black_box(swim.stats())
                });
            },
        );
    }
    group.finish();
}

/// Benchmark suspicion timeout processing
/// Target: <5ms for 1000 suspected nodes
fn bench_suspicion_timeout_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("suspicion_timeout_processing");
    
    for &suspect_count in [10, 100, 1000].iter() {
        let mut swim = create_swim_with_nodes(suspect_count * 2);
        
        // Mark half the nodes as suspect
        let members: Vec<NodeId> = swim.members().keys().take(suspect_count).copied().collect();
        for node_id in members {
            if node_id != swim.local_node().id {
                swim.mark_suspect(&node_id).unwrap();
            }
        }
        
        group.bench_with_input(
            BenchmarkId::new("process_suspicion_timeouts", suspect_count),
            &suspect_count,
            |b, _| {
                b.iter(|| {
                    swim.process_suspicion_timeouts()
                });
            },
        );
    }
    group.finish();
}

/// Benchmark SWIM message serialization/deserialization
/// Target: <100μs per message
fn bench_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialization");
    
    let node_id = NodeId::generate();
    let updates: Vec<NodeInfo> = (0..10).map(|i| create_test_node(None, 8000 + i)).collect();
    
    let ping_msg = kaelix_cluster::membership::SwimMessage::Ping {
        source: node_id,
        incarnation: 123,
        updates: updates.clone(),
    };
    
    let ack_msg = kaelix_cluster::membership::SwimMessage::Ack {
        source: node_id,
        incarnation: 456,
        updates: updates.clone(),
    };
    
    let gossip_msg = kaelix_cluster::membership::SwimMessage::Gossip {
        source: node_id,
        updates: updates,
    };
    
    group.bench_function("serialize_ping", |b| {
        b.iter(|| {
            black_box(bincode::serialize(&ping_msg).unwrap())
        });
    });
    
    group.bench_function("serialize_ack", |b| {
        b.iter(|| {
            black_box(bincode::serialize(&ack_msg).unwrap())
        });
    });
    
    group.bench_function("serialize_gossip", |b| {
        b.iter(|| {
            black_box(bincode::serialize(&gossip_msg).unwrap())
        });
    });
    
    let serialized_ping = bincode::serialize(&ping_msg).unwrap();
    
    group.bench_function("deserialize_ping", |b| {
        b.iter(|| {
            black_box(bincode::deserialize::<kaelix_cluster::membership::SwimMessage>(&serialized_ping).unwrap())
        });
    });
    
    group.finish();
}

/// Benchmark large cluster operations
/// This validates convergence time for large clusters
fn bench_large_cluster_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_cluster_operations");
    group.sample_size(10); // Smaller sample size for large operations
    
    // Test membership convergence speed for different cluster sizes
    for &cluster_size in [100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("full_membership_scan", cluster_size),
            &cluster_size,
            |b, &cluster_size| {
                let swim = create_swim_with_nodes(cluster_size);
                b.iter(|| {
                    // Simulate a full cluster state evaluation
                    let alive_count = swim.alive_member_count();
                    let stats = swim.stats();
                    let target = swim.select_ping_target();
                    let gossip_targets = swim.select_gossip_targets(&swim.local_node().id);
                    black_box((alive_count, stats, target, gossip_targets))
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    swim_benches,
    bench_ping_target_selection,
    bench_membership_update_processing,
    bench_failure_detector,
    bench_gossip_target_selection,
    bench_proxy_node_selection,
    bench_membership_stats,
    bench_suspicion_timeout_processing,
    bench_message_serialization,
    bench_large_cluster_operations
);
criterion_main!(swim_benches);