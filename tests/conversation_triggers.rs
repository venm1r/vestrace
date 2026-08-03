use vestrace_domain::{conversation::*, id::*, now};

#[test]
fn test_conversation_thread_and_trigger() {
    let thread_id = ConversationThreadId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let thread = ConversationThread {
        id: thread_id,
        workspace_id: ws_id,
        title: "Customer Inquiry".into(),
        channel_type: ChannelType::Web,
        created_at: at,
    };

    let trigger = ExternalTrigger {
        id: TriggerId::new(),
        workspace_id: ws_id,
        trigger_type: "webhook.github".into(),
        name: "PR Events".into(),
        enabled: true,
        created_at: at,
    };

    assert_eq!(thread.channel_type, ChannelType::Web);
    assert!(trigger.enabled);
}
