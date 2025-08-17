use kaelix_tests::prelude::*;

#[test]
fn test_core_framework_initialization() {
    // Validate core testing framework can be initialized
    let framework = TestFramework::new();
    assert!(framework.is_ready(), "Test framework should be ready");
}

#[test]
fn test_framework_configuration() {
    // Validate framework can be configured correctly
    let mut framework = TestFramework::new();
    framework.configure(|config| {
        config.set_verbosity(LogLevel::Debug);
        config.enable_performance_tracking();
    });
    
    assert!(framework.is_configured(), "Framework should be configurable");
}
