use vestrace_domain::{
    id::*, state_engine::*, now,
};

#[test]
fn test_signed_run_export_and_profile() {
    let export_id = RunExportId::new();
    let ws_id = WorkspaceId::new();
    let run_id = AgentRunId::new();
    let at = now();

    let export = SignedRunExport {
        id: export_id,
        workspace_id: ws_id,
        run_id,
        signature: "ed25519:fakesig".into(),
        profile: CaptureProfile::Operational,
        created_at: at,
    };

    assert_eq!(export.profile, CaptureProfile::Operational);
    assert_eq!(export.signature, "ed25519:fakesig");
}
