use vestrace_domain::{
    artifact::*, id::*, now,
};

#[test]
fn test_artifact_and_revision_creation() {
    let artifact_id = ArtifactId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let artifact = Artifact {
        id: artifact_id,
        workspace_id: ws_id,
        name: "output.txt".into(),
        status: ArtifactStatus::Quarantined,
        created_at: at,
    };

    let revision = ArtifactRevision {
        id: ArtifactRevisionId::new(),
        artifact_id: artifact.id,
        workspace_id: ws_id,
        revision_number: 1,
        media_type: "text/plain".into(),
        content_hash: "sha256:fakehash".into(),
        byte_size: 1024,
        created_at: at,
    };

    assert_eq!(artifact.status, ArtifactStatus::Quarantined);
    assert_eq!(revision.byte_size, 1024);
}
