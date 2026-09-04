use chrono::{Duration, TimeZone, Utc};
use serde::Deserialize;
use vestrace_application::{ContextPackBuilder, NormalizedRetrievalRequest, RetrievalRequest};
use vestrace_domain::{
    Confidence, Derivation, DerivationMethod, EvidenceRef, EvidenceRole, Importance, Memory,
    MemoryKind, MemoryRevision, MemoryStatus, RetrievalCandidate, StructuredMemory,
    TimePerspective,
    id::{
        EventId, MemoryId, MemoryRevisionId, MemorySourceId, PrincipalId, RetrievalRunId,
        WorkspaceId,
    },
};

const C8_FIXTURE: &str = include_str!("fixtures/qualification/c8-core-memory.json");

#[derive(Debug, Deserialize)]
struct C8FixtureManifest {
    schema_version: String,
    profile: String,
    release_gate: String,
    status: String,
    cases: Vec<C8FixtureCase>,
    known_limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct C8FixtureCase {
    id: String,
    status: String,
    requirements: Vec<String>,
    test: String,
    note: String,
}

fn fixed_at() -> vestrace_domain::Timestamp {
    Utc.with_ymd_and_hms(2026, 8, 11, 0, 0, 0).single().unwrap()
}

fn fixture_revision(
    memory_id: MemoryId,
    workspace_id: WorkspaceId,
    revision_id: MemoryRevisionId,
    revision_number: u32,
    content: &str,
    valid_from: Option<vestrace_domain::Timestamp>,
    valid_until: Option<vestrace_domain::Timestamp>,
) -> MemoryRevision {
    MemoryRevision {
        id: revision_id,
        memory_id,
        workspace_id,
        revision_number,
        content: content.to_owned(),
        structured: Some(StructuredMemory::Assertion(
            vestrace_domain::memory::structured::AssertionData {
                subject: "service".to_owned(),
                predicate: "uses_architecture".to_owned(),
                object: content.to_owned(),
            },
        )),
        confidence: Confidence::new(0.9).unwrap(),
        importance: Importance::new(0.8).unwrap(),
        created_at: fixed_at(),
        valid_from,
        valid_until,
        change_reason: Some("c8 fixture correction".to_owned()),
        canonical_hash: Some(format!("hash-{revision_number}")),
        classification: Some("internal".to_owned()),
    }
}

#[test]
fn c8_fixture_manifest_is_traceable_but_does_not_claim_qualification() {
    let manifest: C8FixtureManifest = serde_json::from_str(C8_FIXTURE).unwrap();

    assert_eq!(
        manifest.schema_version,
        "vestrace.c8.core-memory-fixture.v1"
    );
    assert_eq!(manifest.profile, "CORE+MEMORY");
    assert_eq!(manifest.release_gate, "v0.2 Correct");
    assert_eq!(manifest.status, "evidence_fixture_only");
    assert!(!manifest.known_limitations.is_empty());
    assert!(manifest.cases.iter().any(|case| case.status == "covered"));
    assert!(manifest.cases.iter().any(|case| case.status == "blocked"));

    let required_ids = [
        "ARC-001", "ARC-002", "ARC-003", "ARC-004", "ARC-005", "ARC-006", "ARC-007", "ARC-008",
        "ARC-009", "ARC-010", "MEM-001", "MEM-002", "MEM-003", "MEM-004", "MEM-005", "MEM-006",
        "MEM-007", "MEM-008", "MEM-009", "MEM-010", "MEM-011", "MEM-012", "MEM-013", "MEM-014",
        "MEM-015", "MEM-016", "MEM-017", "MEM-018", "MEM-019", "MEM-020", "TMP-001", "TMP-002",
        "TMP-003", "TMP-004", "TMP-005", "TMP-006", "TMP-007", "TMP-008", "TMP-009", "TMP-010",
        "MUT-001", "MUT-002", "MUT-003", "MUT-004", "MUT-005", "MUT-006", "MUT-007", "MUT-008",
        "RET-001", "RET-002", "RET-003", "RET-004", "RET-005", "RET-006", "RET-007", "RET-008",
        "RET-009", "RET-010", "RET-011", "RET-012", "RET-013", "RET-014", "RET-015", "CAP-001",
        "CAP-002", "CAP-003", "CAP-004", "CAP-010", "CAP-011", "CAP-012",
    ];

    let mapped_ids: Vec<&str> = manifest
        .cases
        .iter()
        .flat_map(|case| case.requirements.iter().map(String::as_str))
        .collect();
    for requirement_id in required_ids {
        assert!(
            mapped_ids.contains(&requirement_id),
            "C8 requirement {requirement_id} has no fixture mapping"
        );
    }
    for case in &manifest.cases {
        assert!(!case.id.is_empty());
        assert!(!case.requirements.is_empty());
        assert!(case.test.starts_with("c8_"));
        assert!(matches!(case.status.as_str(), "covered" | "blocked"));
        assert!(!case.note.is_empty());
    }
}

#[test]
fn c8_memory_evolution_preserves_revisions_evidence_and_status_history() {
    let workspace_id = WorkspaceId::new();
    let memory_id = MemoryId::new();
    let event_id = EventId::new();
    let t0 = fixed_at();
    let t1 = t0 + Duration::hours(1);
    let first_revision_id = MemoryRevisionId::new();
    let corrected_revision_id = MemoryRevisionId::new();
    let first_revision = fixture_revision(
        memory_id,
        workspace_id,
        first_revision_id,
        1,
        "modular monolith",
        Some(t0),
        Some(t1),
    );
    let corrected_revision = fixture_revision(
        memory_id,
        workspace_id,
        corrected_revision_id,
        2,
        "modular monolith with bounded contexts",
        Some(t1),
        None,
    );

    assert!(first_revision.validate_temporal_range().is_ok());
    assert!(corrected_revision.validate_temporal_range().is_ok());
    assert_ne!(first_revision.id, corrected_revision.id);
    assert_eq!(first_revision.revision_number, 1);
    assert_eq!(corrected_revision.revision_number, 2);
    assert_ne!(first_revision.content, corrected_revision.content);

    let active = Memory::new(memory_id, workspace_id, MemoryKind::Fact, t0)
        .activate(&first_revision, t0)
        .unwrap();
    let superseded = active.supersede(t1).unwrap();
    assert_eq!(superseded.status, MemoryStatus::Superseded);
    assert!(superseded.activate(&corrected_revision, t1).is_err());

    let source = vestrace_domain::MemorySource::new_direct(
        MemorySourceId::new(),
        memory_id,
        workspace_id,
        event_id,
        t0,
    );
    assert_eq!(source.role, EvidenceRole::DirectSource);
    assert_eq!(source.evidence_ref, Some(EvidenceRef::event(event_id)));

    let derivation = Derivation::new(
        vestrace_domain::id::DerivationId::new(),
        workspace_id,
        DerivationMethod::Extraction,
        t1,
    )
    .with_input(EvidenceRef::event(event_id))
    .with_output(EvidenceRef::MemoryRevisionRef {
        memory_id,
        revision_id: corrected_revision_id,
    });
    assert!(!derivation.input_refs.is_empty());
    assert!(matches!(
        derivation.output_ref,
        Some(EvidenceRef::MemoryRevisionRef { revision_id, .. }) if revision_id == corrected_revision_id
    ));
}

#[test]
fn c8_temporal_retrieval_and_context_fixture_preserve_perspective_provenance_and_budget() {
    let workspace_id = WorkspaceId::new();
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();
    let as_of = fixed_at() + Duration::hours(2);
    let request = RetrievalRequest::new(workspace_id, "architecture")
        .with_time_perspective(TimePerspective::AsOf(as_of));
    let normalized = NormalizedRetrievalRequest::normalize(request).unwrap();
    assert_eq!(normalized.time_perspective, TimePerspective::AsOf(as_of));
    assert!(
        normalized
            .allowed_statuses
            .contains(&MemoryStatus::Superseded)
    );

    let candidate = RetrievalCandidate {
        memory_id,
        revision_id,
        kind: MemoryKind::Fact,
        memory_status: MemoryStatus::Superseded,
        revision_number: 1,
        content: "historical architecture".to_owned(),
        classification: None,
        valid_from: Some(as_of - Duration::hours(1)),
        valid_until: Some(as_of + Duration::hours(1)),
        revision_created_at: as_of,
        source_generation: 4,
        score: 1.0,
        channel_rank: 1,
        channel: "text".to_owned(),
        explanation: "historical fixture match".to_owned(),
        conflict_ids: Vec::new(),
    };
    let pack = ContextPackBuilder::new(64)
        .build_with_temporal_perspective(
            workspace_id,
            PrincipalId::new(),
            RetrievalRunId::new(),
            TimePerspective::AsOf(as_of),
            &[candidate],
        )
        .unwrap();

    assert_eq!(pack.temporal_perspective, TimePerspective::AsOf(as_of));
    assert!(pack.used_tokens <= pack.token_budget);
    let item = &pack.sections[0].items[0];
    assert_eq!(item.memory_id, memory_id);
    assert_eq!(item.revision_id, revision_id);
    assert_eq!(item.memory_status, MemoryStatus::Superseded);
    assert_eq!(item.source_generation, 4);
    assert!(!item.inclusion_explanation.is_empty());
}

#[test]
fn c8_structured_memory_fixture_carries_schema_identity_and_temporal_range_rejects_inversion() {
    let workspace_id = WorkspaceId::new();
    let memory_id = MemoryId::new();
    let revision = fixture_revision(
        memory_id,
        workspace_id,
        MemoryRevisionId::new(),
        1,
        "fact",
        Some(fixed_at()),
        Some(fixed_at() - Duration::minutes(1)),
    );

    let encoded = serde_json::to_value(revision.structured.clone().unwrap()).unwrap();
    assert_eq!(encoded["schema_version"], "v1.assertion");
    assert!(revision.validate_temporal_range().is_err());
    assert!(Confidence::new(f32::NAN).is_err());
    assert!(Importance::new(1.1).is_err());
}
