use vestrace_domain::{id::*, models::*, now};

#[test]
fn test_cost_profile_validation() {
    let valid = ModelCostProfile::new(0.15, 0.60);
    assert!(valid.is_ok());

    let invalid = ModelCostProfile::new(-0.01, 0.50);
    assert!(invalid.is_err());
}
