//! Pre-built dataset definitions for common testing scenarios.

use super::{TestFixtures, GenerationConfig};
use std::collections::HashMap;

/// Collection of predefined datasets for testing
pub struct Datasets;

impl Datasets {
    /// E-commerce flash sale dataset
    pub fn ecommerce_flash_sale() -> TestFixtures {
        let mut fixtures = TestFixtures::new("ecommerce_flash_sale");
        fixtures.metadata.description = "E-commerce flash sale workload with burst traffic".to_string();
        fixtures.metadata.tags.extend(vec![
            "ecommerce".to_string(),
            "burst".to_string(),
            "high-volume".to_string(),
        ]);
        
        // Generate messages for different event types
        fixtures = Self::add_user_events(fixtures);
        fixtures = Self::add_product_events(fixtures);
        fixtures = Self::add_order_events(fixtures);
        
        fixtures
    }

    /// IoT sensor telemetry dataset
    pub fn iot_telemetry() -> TestFixtures {
        let mut fixtures = TestFixtures::new("iot_telemetry");
        fixtures.metadata.description = "IoT sensor telemetry with periodic bursts".to_string();
        fixtures.metadata.tags.extend(vec![
            "iot".to_string(),
            "telemetry".to_string(),
            "sensors".to_string(),
        ]);
        
        fixtures = Self::add_sensor_data(fixtures);
        fixtures = Self::add_device_status(fixtures);
        
        fixtures
    }

    /// Financial trading dataset
    pub fn financial_trading() -> TestFixtures {
        let mut fixtures = TestFixtures::new("financial_trading");
        fixtures.metadata.description = "Financial trading data with market hours pattern".to_string();
        fixtures.metadata.tags.extend(vec![
            "financial".to_string(),
            "trading".to_string(),
            "market-data".to_string(),
        ]);
        
        fixtures = Self::add_trade_events(fixtures);
        fixtures = Self::add_market_data(fixtures);
        
        fixtures
    }

    /// Social media viral content dataset
    pub fn social_media_viral() -> TestFixtures {
        let mut fixtures = TestFixtures::new("social_media_viral");
        fixtures.metadata.description = "Social media viral content with exponential growth".to_string();
        fixtures.metadata.tags.extend(vec![
            "social".to_string(),
            "viral".to_string(),
            "user-generated".to_string(),
        ]);
        
        fixtures = Self::add_social_events(fixtures);
        fixtures = Self::add_user_interactions(fixtures);
        
        fixtures
    }

    /// Gaming platform dataset
    pub fn gaming_platform() -> TestFixtures {
        let mut fixtures = TestFixtures::new("gaming_platform");
        fixtures.metadata.description = "Gaming platform events with evening peaks".to_string();
        fixtures.metadata.tags.extend(vec![
            "gaming".to_string(),
            "platform".to_string(),
            "real-time".to_string(),
        ]);
        
        fixtures = Self::add_game_events(fixtures);
        fixtures = Self::add_player_actions(fixtures);
        
        fixtures
    }

    // Helper methods for adding specific event types

    fn add_user_events(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("events.user").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..5000 {
            let event_type = match i % 4 {
                0 => "login",
                1 => "logout", 
                2 => "page_view",
                _ => "purchase",
            };
            
            let payload = format!(
                r#"{{"user_id":{},"event_type":"{}","timestamp":"{}","session_id":"{}"}}"#,
                i % 1000,
                event_type,
                chrono::Utc::now().timestamp(),
                uuid::Uuid::new_v4()
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_product_events(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("events.product").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..3000 {
            let event_type = match i % 3 {
                0 => "view",
                1 => "add_to_cart",
                _ => "remove_from_cart",
            };
            
            let payload = format!(
                r#"{{"product_id":{},"event_type":"{}","user_id":{},"price":{}.99}}"#,
                i % 500,
                event_type,
                i % 1000,
                (i % 100) + 10
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_order_events(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("events.order").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..1000 {
            let status = match i % 5 {
                0 => "pending",
                1 => "processing",
                2 => "shipped",
                3 => "delivered",
                _ => "cancelled",
            };
            
            let payload = format!(
                r#"{{"order_id":"ORD-{}","user_id":{},"status":"{}","total":{}.99}}"#,
                i,
                i % 1000,
                status,
                (i % 500) + 20
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_sensor_data(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        use rand::Rng;
        
        let sensors = ["temperature", "humidity", "pressure", "light"];
        
        for sensor_type in &sensors {
            let topic = Topic::new(&format!("sensors.{}", sensor_type)).unwrap();
            fixtures.topics.push(topic.clone());
            
            for i in 0..2000 {
                let mut rng = rand::thread_rng();
                let value = match *sensor_type {
                    "temperature" => rng.gen_range(18.0..35.0),
                    "humidity" => rng.gen_range(30.0..80.0),
                    "pressure" => rng.gen_range(980.0..1030.0),
                    "light" => rng.gen_range(0.0..1000.0),
                    _ => 0.0,
                };
                
                let payload = format!(
                    r#"{{"device_id":"DEV-{}","sensor_type":"{}","value":{:.2},"timestamp":"{}"}}"#,
                    i % 100,
                    sensor_type,
                    value,
                    chrono::Utc::now().timestamp()
                );
                
                let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
                fixtures.messages.push(message);
            }
        }
        
        fixtures
    }

    fn add_device_status(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("devices.status").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..500 {
            let status = match i % 4 {
                0 => "online",
                1 => "offline",
                2 => "maintenance",
                _ => "error",
            };
            
            let payload = format!(
                r#"{{"device_id":"DEV-{}","status":"{}","battery_level":{},"last_seen":"{}"}}"#,
                i % 100,
                status,
                rand::random::<u8>() % 100,
                chrono::Utc::now().timestamp()
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_trade_events(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        use rand::Rng;
        
        let symbols = ["AAPL", "GOOGL", "MSFT", "AMZN", "TSLA"];
        
        for symbol in &symbols {
            let topic = Topic::new(&format!("trades.{}", symbol)).unwrap();
            fixtures.topics.push(topic.clone());
            
            for i in 0..1000 {
                let mut rng = rand::thread_rng();
                let price = rng.gen_range(100.0..500.0);
                let quantity = rng.gen_range(1..1000);
                
                let payload = format!(
                    r#"{{"symbol":"{}","price":{:.2},"quantity":{},"side":"{}","timestamp":"{}"}}"#,
                    symbol,
                    price,
                    quantity,
                    if i % 2 == 0 { "buy" } else { "sell" },
                    chrono::Utc::now().timestamp()
                );
                
                let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
                fixtures.messages.push(message);
            }
        }
        
        fixtures
    }

    fn add_market_data(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        use rand::Rng;
        
        let topic = Topic::new("market.data").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..2000 {
            let mut rng = rand::thread_rng();
            let bid = rng.gen_range(100.0..500.0);
            let ask = bid + rng.gen_range(0.01..0.50);
            
            let payload = format!(
                r#"{{"symbol":"MARKET-{}","bid":{:.2},"ask":{:.2},"volume":{},"timestamp":"{}"}}"#,
                i % 10,
                bid,
                ask,
                rng.gen_range(1000..50000),
                chrono::Utc::now().timestamp()
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_social_events(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("social.events").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..10000 {
            let event_type = match i % 6 {
                0 => "post",
                1 => "like",
                2 => "share",
                3 => "comment",
                4 => "follow",
                _ => "mention",
            };
            
            let payload = format!(
                r#"{{"user_id":{},"event_type":"{}","target_id":{},"content_id":{},"timestamp":"{}"}}"#,
                i % 5000,
                event_type,
                i % 3000,
                i % 1000,
                chrono::Utc::now().timestamp()
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_user_interactions(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("interactions.user").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..5000 {
            let interaction_type = match i % 4 {
                0 => "click",
                1 => "scroll",
                2 => "hover",
                _ => "download",
            };
            
            let payload = format!(
                r#"{{"user_id":{},"interaction_type":"{}","element_id":"elem-{}","duration_ms":{},"timestamp":"{}"}}"#,
                i % 2000,
                interaction_type,
                i % 100,
                rand::random::<u32>() % 5000,
                chrono::Utc::now().timestamp()
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_game_events(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("game.events").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..8000 {
            let event_type = match i % 8 {
                0 => "match_start",
                1 => "match_end",
                2 => "kill",
                3 => "death",
                4 => "achievement",
                5 => "level_up",
                6 => "item_acquired",
                _ => "chat_message",
            };
            
            let payload = format!(
                r#"{{"player_id":{},"event_type":"{}","match_id":"MATCH-{}","value":{},"timestamp":"{}"}}"#,
                i % 1000,
                event_type,
                i % 100,
                rand::random::<u32>() % 1000,
                chrono::Utc::now().timestamp()
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }

    fn add_player_actions(mut fixtures: TestFixtures) -> TestFixtures {
        use kaelix_core::{Message, Topic};
        use bytes::Bytes;
        
        let topic = Topic::new("player.actions").unwrap();
        fixtures.topics.push(topic.clone());
        
        for i in 0..12000 {
            let action_type = match i % 6 {
                0 => "move",
                1 => "attack",
                2 => "defend",
                3 => "use_item",
                4 => "cast_spell",
                _ => "interact",
            };
            
            let payload = format!(
                r#"{{"player_id":{},"action_type":"{}","x":{:.2},"y":{:.2},"z":{:.2},"timestamp":"{}"}}"#,
                i % 1000,
                action_type,
                rand::random::<f32>() * 1000.0,
                rand::random::<f32>() * 1000.0,
                rand::random::<f32>() * 100.0,
                chrono::Utc::now().timestamp()
            );
            
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures
    }
}

/// Dataset generation configurations
pub struct DatasetConfigs;

impl DatasetConfigs {
    /// High-throughput stress test configuration
    pub fn high_throughput_stress() -> GenerationConfig {
        GenerationConfig {
            message_count: 10_000_000,
            payload_size_range: (1024, 4096),
            topic_patterns: vec!["stress.test".to_string()],
            compression: Some("lz4".to_string()),
            seed: Some(12345),
            parameters: {
                let mut params = HashMap::new();
                params.insert("target_rate".to_string(), serde_json::Value::Number(serde_json::Number::from(1_000_000)));
                params.insert("duration_seconds".to_string(), serde_json::Value::Number(serde_json::Number::from(300)));
                params
            },
        }
    }

    /// Low-latency precision test configuration
    pub fn low_latency_precision() -> GenerationConfig {
        GenerationConfig {
            message_count: 100_000,
            payload_size_range: (64, 64),
            topic_patterns: vec!["latency.test".to_string()],
            compression: None,
            seed: Some(54321),
            parameters: {
                let mut params = HashMap::new();
                params.insert("target_latency_micros".to_string(), serde_json::Value::Number(serde_json::Number::from(10)));
                params.insert("precision_required".to_string(), serde_json::Value::Bool(true));
                params
            },
        }
    }

    /// Memory pressure test configuration
    pub fn memory_pressure() -> GenerationConfig {
        GenerationConfig {
            message_count: 1_000_000,
            payload_size_range: (65536, 1048576), // 64KB to 1MB
            topic_patterns: vec!["memory.test".to_string()],
            compression: Some("zstd".to_string()),
            seed: Some(99999),
            parameters: {
                let mut params = HashMap::new();
                params.insert("memory_limit_gb".to_string(), serde_json::Value::Number(serde_json::Number::from(8)));
                params.insert("gc_pressure".to_string(), serde_json::Value::Bool(true));
                params
            },
        }
    }

    /// Multi-topic distribution test configuration
    pub fn multi_topic_distribution() -> GenerationConfig {
        GenerationConfig {
            message_count: 5_000_000,
            payload_size_range: (256, 2048),
            topic_patterns: vec![
                "topic.{shard}.data".to_string(),
                "events.{type}.{region}".to_string(),
                "metrics.{service}.{instance}".to_string(),
            ],
            compression: Some("snappy".to_string()),
            seed: Some(77777),
            parameters: {
                let mut params = HashMap::new();
                params.insert("topic_count".to_string(), serde_json::Value::Number(serde_json::Number::from(1000)));
                params.insert("distribution".to_string(), serde_json::Value::String("zipf".to_string()));
                params
            },
        }
    }
}