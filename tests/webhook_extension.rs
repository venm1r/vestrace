use vestrace_domain::{
    id::*, webhook::*, now,
};

#[test]
fn test_webhook_subscription_creation() {
    let sub_id = WebhookSubscriptionId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let sub = WebhookSubscription {
        id: sub_id,
        workspace_id: ws_id,
        target_url: "https://example.com/webhooks/vestrace".into(),
        secret_token: "sec_hmac_key_123".into(),
        events: vec!["run.completed".into(), "approval.requested".into()],
        enabled: true,
        created_at: at,
    };

    assert_eq!(sub.events.len(), 2);
    assert!(sub.enabled);
}
