use vestrace_domain::security::{Capability, Sensitivity};

#[test]
fn test_capability_parsing() {
    let cap: Capability = "memory.read".parse().unwrap();
    assert_eq!(cap.to_string(), "memory.read");

    assert!("unknown.cap".parse::<Capability>().is_err());
}

#[test]
fn test_sensitivity_ordering() {
    assert!(Sensitivity::Public < Sensitivity::Internal);
    assert!(Sensitivity::Internal < Sensitivity::Confidential);
    assert!(Sensitivity::Confidential < Sensitivity::Restricted);
}
