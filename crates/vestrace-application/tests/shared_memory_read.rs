use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use vestrace_application::{
    ApplicationError, RequestContext, SharedMemoryReadClock, SharedMemoryReadPermit,
    SharedMemoryReadService, SharedMemoryRevisionReader, SharedMemoryRevisionRecord,
};
use vestrace_domain::{
    Timestamp,
    enterprise::{
        MemoryMount, MemoryMountAcceptance, MemoryShareGrant, MemoryShareGrantRevision,
        MemoryShareGrantRevisionSpec, ShareOperation, ShareTarget, TargetSharePolicy,
    },
    id::{MemoryId, MemoryRevisionId, PrincipalId, WorkspaceId},
};

#[derive(Clone)]
struct FixedClock {
    at: Timestamp,
    calls: Arc<Mutex<usize>>,
}

impl SharedMemoryReadClock for FixedClock {
    fn now(&self) -> Timestamp {
        *self.calls.lock().unwrap() += 1;
        self.at
    }
}

#[derive(Clone, Default)]
struct RecordingReader {
    calls: Arc<Mutex<Vec<PermitCall>>>,
    answer: Arc<Mutex<Option<ReaderAnswer>>>,
}

type PermitCall = (String, String, String, String, String, String, String);
type ReaderAnswer = Result<Option<SharedMemoryRevisionRecord>, String>;

#[async_trait]
impl SharedMemoryRevisionReader for RecordingReader {
    async fn read_exact(
        &self,
        permit: SharedMemoryReadPermit,
    ) -> Result<Option<SharedMemoryRevisionRecord>, ApplicationError> {
        self.calls.lock().unwrap().push((
            permit.source_workspace_id().to_string(),
            permit.source_memory_id().to_string(),
            permit.memory_revision_id().to_string(),
            permit.grant_revision_id().to_string(),
            permit.source_generation().to_owned(),
            permit.target_workspace_id().to_string(),
            permit.target_principal_id().to_string(),
        ));
        match self
            .answer
            .lock()
            .unwrap()
            .take()
            .expect("configured answer")
        {
            Ok(value) => Ok(value),
            Err(message) => Err(ApplicationError::Storage(message)),
        }
    }
}

struct Fixture {
    at: Timestamp,
    context: RequestContext,
    source_workspace_id: WorkspaceId,
    source_memory_id: MemoryId,
    source_revision_id: MemoryRevisionId,
    grant: MemoryShareGrant,
    mount: MemoryMount,
    policy: TargetSharePolicy,
    record: SharedMemoryRevisionRecord,
}

fn operations(read_content: bool) -> BTreeSet<ShareOperation> {
    let mut operations = BTreeSet::from([ShareOperation::IncludeContext]);
    if read_content {
        operations.insert(ShareOperation::ReadContent);
    }
    operations
}

fn timestamp(day: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 8, day, 12, 0, 0)
        .single()
        .unwrap()
}

fn fixture(
    source_operations: BTreeSet<ShareOperation>,
    mount_operations: BTreeSet<ShareOperation>,
    policy_operations: BTreeSet<ShareOperation>,
    grant_valid_until: Option<Timestamp>,
    mount_valid_until: Option<Timestamp>,
    policy_valid_until: Option<Timestamp>,
    at: Timestamp,
) -> Fixture {
    let accepted_at = timestamp(1);
    let source_workspace_id = WorkspaceId::new();
    let target_workspace_id = WorkspaceId::new();
    let target_principal_id = PrincipalId::new();
    let source_memory_id = MemoryId::new();
    let source_revision_id = MemoryRevisionId::new();
    let revision = MemoryShareGrantRevision::issue(
        MemoryShareGrantRevisionSpec {
            source_workspace_id,
            target: ShareTarget::ExactWorkspace(target_workspace_id),
            memory_id: source_memory_id,
            memory_revision_id: source_revision_id,
            source_generation: "source-generation-7".to_owned(),
            operations: source_operations,
            valid_from: accepted_at,
            valid_until: grant_valid_until,
        },
        accepted_at,
    )
    .unwrap();
    let grant = MemoryShareGrant::issue(revision, accepted_at).unwrap();
    let policy = TargetSharePolicy::new(
        target_workspace_id,
        target_principal_id,
        policy_operations,
        policy_valid_until,
    )
    .unwrap();
    let mount = MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id,
            target_principal_id,
            operations: mount_operations,
            valid_until: mount_valid_until,
        },
        &grant,
        &policy,
        accepted_at,
    )
    .unwrap();
    Fixture {
        at,
        context: RequestContext::new(target_workspace_id, target_principal_id),
        source_workspace_id,
        source_memory_id,
        source_revision_id,
        grant,
        mount,
        policy,
        record: SharedMemoryRevisionRecord {
            source_workspace_id,
            source_memory_id,
            memory_revision_id: source_revision_id,
            content: "source content".to_owned(),
        },
    }
}

fn valid_fixture() -> Fixture {
    fixture(
        operations(true),
        operations(true),
        operations(true),
        None,
        None,
        None,
        timestamp(2),
    )
}

fn service(fixture: &Fixture, reader: RecordingReader) -> SharedMemoryReadService {
    SharedMemoryReadService::new(
        Arc::new(reader),
        Arc::new(FixedClock {
            at: fixture.at,
            calls: Arc::new(Mutex::new(0)),
        }),
    )
}

fn clocked_service(
    fixture: &Fixture,
    reader: RecordingReader,
    calls: Arc<Mutex<usize>>,
) -> SharedMemoryReadService {
    SharedMemoryReadService::new(
        Arc::new(reader),
        Arc::new(FixedClock {
            at: fixture.at,
            calls,
        }),
    )
}

fn configure(reader: &RecordingReader, answer: Result<Option<SharedMemoryRevisionRecord>, String>) {
    *reader.answer.lock().unwrap() = Some(answer);
}

fn assert_pre_reader_denial(
    result: Result<Option<vestrace_application::SharedMemoryReadResult>, ApplicationError>,
    fixture: &Fixture,
    reader: &RecordingReader,
    expected_message: &str,
) {
    assert!(
        matches!(result, Err(ApplicationError::Policy(message)) if message == expected_message)
    );
    assert!(reader.calls.lock().unwrap().is_empty());
    assert!(fixture.mount.disclosures().is_empty());
}

#[tokio::test]
async fn successful_read_consumes_exact_permit_and_records_one_disclosure() {
    let mut fixture = valid_fixture();
    let reader = RecordingReader::default();
    configure(&reader, Ok(Some(fixture.record.clone())));
    let calls = Arc::new(Mutex::new(0));
    let service = clocked_service(&fixture, reader.clone(), calls.clone());

    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        reader.calls.lock().unwrap().as_slice(),
        &[(
            fixture.source_workspace_id.to_string(),
            fixture.source_memory_id.to_string(),
            fixture.source_revision_id.to_string(),
            fixture.grant.revision().id().to_string(),
            "source-generation-7".to_owned(),
            fixture.context.workspace_id.to_string(),
            fixture.context.principal_id.to_string(),
        )]
    );
    assert_eq!(
        result.shared_ref().source_workspace_id(),
        fixture.source_workspace_id
    );
    assert_eq!(
        result.shared_ref().source_memory_id(),
        fixture.source_memory_id
    );
    assert_eq!(
        result.shared_ref().memory_revision_id(),
        fixture.source_revision_id
    );
    assert_eq!(
        result.shared_ref().grant_revision_id(),
        fixture.grant.revision().id()
    );
    assert_eq!(
        result.shared_ref().source_generation(),
        "source-generation-7"
    );
    assert_eq!(result.content(), "source content");
    assert_eq!(result.disclosure().operation(), ShareOperation::ReadContent);
    assert_eq!(fixture.mount.disclosures().len(), 1);
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn wrong_target_workspace_denies_before_clock_and_reader() {
    let mut fixture = valid_fixture();
    let reader = RecordingReader::default();
    let calls = Arc::new(Mutex::new(0));
    let service = clocked_service(&fixture, reader.clone(), calls.clone());
    let context = RequestContext::new(WorkspaceId::new(), fixture.context.principal_id);
    let result = service
        .read_shared(
            &context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read target identity does not match the accepted mount",
    );
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn wrong_target_principal_denies_before_clock_and_reader() {
    let mut fixture = valid_fixture();
    let reader = RecordingReader::default();
    let calls = Arc::new(Mutex::new(0));
    let service = clocked_service(&fixture, reader.clone(), calls.clone());
    let context = RequestContext::new(fixture.context.workspace_id, PrincipalId::new());
    let result = service
        .read_shared(
            &context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read target identity does not match the accepted mount",
    );
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn source_without_read_content_denies_before_reader() {
    let mut fixture = fixture(
        operations(false),
        operations(false),
        operations(false),
        None,
        None,
        None,
        timestamp(2),
    );
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: SourceOperationDenied",
    );
}

#[tokio::test]
async fn mount_without_read_content_denies_before_reader() {
    let mut fixture = fixture(
        operations(true),
        operations(false),
        operations(true),
        None,
        None,
        None,
        timestamp(2),
    );
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: MountOperationDenied",
    );
}

#[tokio::test]
async fn current_target_policy_without_read_content_denies_before_reader() {
    let mut fixture = valid_fixture();
    let current_policy = TargetSharePolicy::new(
        fixture.context.workspace_id,
        fixture.context.principal_id,
        operations(false),
        None,
    )
    .unwrap();
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &current_policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: TargetPolicyDenied",
    );
}

#[tokio::test]
async fn wrong_grant_revision_denies_before_reader() {
    let mut fixture = valid_fixture();
    let other = valid_fixture();
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &other.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: GrantRevisionMismatch",
    );
}

#[tokio::test]
async fn revoked_source_grant_denies_before_reader() {
    let mut fixture = valid_fixture();
    fixture.grant.revoke(fixture.at).unwrap();
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: SourceGrantInactive",
    );
}

#[tokio::test]
async fn expired_source_grant_denies_before_reader() {
    let boundary = timestamp(3);
    let mut fixture = fixture(
        operations(true),
        operations(true),
        operations(true),
        Some(boundary),
        None,
        None,
        boundary,
    );
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: SourceGrantInactive",
    );
}

#[tokio::test]
async fn stale_mount_denies_before_reader() {
    let mut fixture = valid_fixture();
    let other = valid_fixture();
    fixture.mount.sync_with_source(&other.grant, fixture.at);
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: MountInactive",
    );
}

#[tokio::test]
async fn expired_mount_denies_before_reader() {
    let boundary = timestamp(3);
    let mut fixture = fixture(
        operations(true),
        operations(true),
        operations(true),
        None,
        Some(boundary),
        None,
        boundary,
    );
    let reader = RecordingReader::default();
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert_pre_reader_denial(
        result,
        &fixture,
        &reader,
        "shared read denied: MountInactive",
    );
}

#[tokio::test]
async fn trusted_clock_is_sampled_once_and_reused_for_disclosure() {
    let mut fixture = valid_fixture();
    let reader = RecordingReader::default();
    configure(&reader, Ok(Some(fixture.record.clone())));
    let calls = Arc::new(Mutex::new(0));
    let service = clocked_service(&fixture, reader, calls.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(result.disclosure().disclosed_at(), fixture.at);
}

#[tokio::test]
async fn missing_revision_returns_none_without_disclosure() {
    let mut fixture = valid_fixture();
    let reader = RecordingReader::default();
    configure(&reader, Ok(None));
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await
        .unwrap();
    assert!(result.is_none());
    assert_eq!(reader.calls.lock().unwrap().len(), 1);
    assert!(fixture.mount.disclosures().is_empty());
}

#[tokio::test]
async fn storage_failure_returns_no_content_or_disclosure() {
    let mut fixture = valid_fixture();
    let reader = RecordingReader::default();
    configure(&reader, Err("storage unavailable".to_owned()));
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert!(
        matches!(result, Err(ApplicationError::Storage(message)) if message == "storage unavailable")
    );
    assert_eq!(reader.calls.lock().unwrap().len(), 1);
    assert!(fixture.mount.disclosures().is_empty());
}

async fn assert_mismatched_record_denied(mutate: impl FnOnce(&mut SharedMemoryRevisionRecord)) {
    let mut fixture = valid_fixture();
    let mut record = fixture.record.clone();
    mutate(&mut record);
    let reader = RecordingReader::default();
    configure(&reader, Ok(Some(record)));
    let service = service(&fixture, reader.clone());
    let result = service
        .read_shared(
            &fixture.context,
            &fixture.grant,
            &mut fixture.mount,
            &fixture.policy,
        )
        .await;
    assert!(
        matches!(result, Err(ApplicationError::Storage(message)) if message == "shared memory reader returned a revision outside the authorized namespace")
    );
    assert_eq!(reader.calls.lock().unwrap().len(), 1);
    assert!(fixture.mount.disclosures().is_empty());
}

#[tokio::test]
async fn mismatched_record_workspace_is_storage_failure() {
    assert_mismatched_record_denied(|record| record.source_workspace_id = WorkspaceId::new()).await;
}

#[tokio::test]
async fn mismatched_record_memory_is_storage_failure() {
    assert_mismatched_record_denied(|record| record.source_memory_id = MemoryId::new()).await;
}

#[tokio::test]
async fn mismatched_record_revision_is_storage_failure() {
    assert_mismatched_record_denied(|record| record.memory_revision_id = MemoryRevisionId::new())
        .await;
}
