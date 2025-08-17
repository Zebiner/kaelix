use kaelix_tests::prelude::*;
use proptest::prelude::*;

#[test]
fn test_property_based_generation() {
    // Validate property-based testing can generate complex scenarios
    prop_compose! {
        fn complex_message_strategy()(
            payload in any::<Vec<u8>>(),
            topic in any::<String>(),
            priority in 0..100u8
        ) -> Message {
            Message::new(payload, topic, priority)
        }
    }

    proptest! {
        #[test]
        fn validate_message_generation(msg in complex_message_strategy()) {
            assert!(msg.payload.len() > 0, "Generated message should have payload");
            assert!(!msg.topic.is_empty(), "Generated message should have topic");
        }
    }
}
