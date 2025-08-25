use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use kaelix_cluster::{
    membership::{NodeInfo, NodeStatus, SwimMembership, SwimConfig, PhiFailureDetector},
    types::NodeId,
    communication::{MessageRouter, TcpTransport, NetworkMessage},
};
use std::{
    net::SocketAddr, 
    time::{Duration, Instant},
    sync::Arc,
};
use tokio::runtime::Runtime;

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

fn bench_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialization");
    
    let node_id = NodeId::generate();
    let updates = (0..10).map(|i| create_test_node(None, 8000 + i)).collect();
    
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
        updates: updates.clone(),
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

async fn bench_async_protocol_round_impl(node_count: usize) -> Duration {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let mut swim = create_swim_with_nodes(node_count);
        
        // Create a dummy router (in real benchmarks, we'd need a proper mock)
        let transport = TcpTransport::new("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let router = MessageRouter::new(NodeId::generate(), transport);
        
        let start = Instant::now();
        let _ = swim.execute_protocol_round(&router).await;
        start.elapsed()
    })
}

fn bench_protocol_round_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("protocol_round_execution");
    group.sample_size(10); // Smaller sample size for async benchmarks
    
    for &node_count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("execute_protocol_round", node_count),
            &node_count,
            |b, &node_count| {
                let rt = Runtime::new().unwrap();
                b.to_async(&rt).iter(|| async {
                    let mut swim = create_swim_with_nodes(node_count);
                    
                    // Create a dummy router 
                    let transport = TcpTransport::new("127.0.0.1:0".parse().unwrap()).await.unwrap();
                    let router = MessageRouter::new(NodeId::generate(), transport);
                    
                    black_box(swim.execute_protocol_round(&router).await)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_ping_target_selection,
    bench_membership_update_processing,
    bench_failure_detector,
    bench_gossip_target_selection,
    bench_proxy_node_selection,
    bench_membership_stats,
    bench_suspicion_timeout_processing,
    bench_message_serialization,
    bench_protocol_round_execution
);
criterion_main!(benches);