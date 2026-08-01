use vestrace_domain::{
    budget::BudgetAccount, id::*, now,
};

#[test]
fn test_budget_account_limits() {
    let at = now();
    let account_id = BudAccountId::new();
    let ws_id = WorkspaceId::new();

    let mut account = BudgetAccount::new(account_id, ws_id, "API Calls", 100.0, at).unwrap();
    assert!(account.reserve(50.0).is_ok());
    assert_eq!(account.balance, 50.0);

    // Reserve pushing balance past hard_limit fails
    assert!(account.reserve(60.0).is_err());
    assert_eq!(account.balance, 50.0);
}
