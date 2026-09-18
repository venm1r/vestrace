//! Disposable exact indexes: numerical, bounded-memory and durable-authority probes.
mod common;
use uuid::Uuid;
use vestrace_domain::{
    ContentMaterialId, CorpusGenerationId, ModelQualificationRevisionId, ModelRevisionId,
    WorkspaceId,
    embedding::{CanonicalEmbeddingSpace, CanonicalGenerationSnapshot, EmbeddingSpaceKey},
};
use vestrace_infrastructure::embedding_index::{FlatEmbeddingIndex, IndexLimits, IndexVector};
use zeroize::Zeroizing;

fn snapshot(dimensions: u32, count: u64) -> CanonicalGenerationSnapshot {
    let space = EmbeddingSpaceKey::canonical(
        WorkspaceId::new(),
        "index",
        CanonicalEmbeddingSpace {
            model_revision_id: ModelRevisionId::new(),
            model_qualification_revision_id: ModelQualificationRevisionId::new(),
            adapter_profile_revision: "q1".into(),
            request_shape_revision_id: Uuid::now_v7(),
            returned_model: "model".into(),
            encoding_format: "float".into(),
            dimensions,
        },
    )
    .unwrap();
    CanonicalGenerationSnapshot::new(space, CorpusGenerationId::new(), 1, 2, 1, 10, count).unwrap()
}
fn vector(ordinal: u64, values: &[f32]) -> IndexVector {
    IndexVector {
        projection_id: Uuid::now_v7(),
        projection_ordinal: ordinal,
        material_id: ContentMaterialId::new(),
        values: Zeroizing::new(values.to_vec()),
    }
}
fn limits() -> IndexLimits {
    IndexLimits {
        max_members: 100,
        max_bytes: 1024 * 1024,
    }
}

#[test]
fn embedding_index_cosine_is_exact_and_ties_use_projection_ordinal() {
    let index = FlatEmbeddingIndex::build(
        snapshot(2, 3),
        Uuid::now_v7(),
        vec![
            vector(7, &[1.0, 0.0]),
            vector(3, &[1.0, 0.0]),
            vector(10, &[0.0, 1.0]),
        ],
        limits(),
    )
    .unwrap();
    let hits = index.search(&[1.0, 0.0], 2).unwrap();
    assert_eq!(
        hits.iter()
            .map(|hit| hit.projection_ordinal)
            .collect::<Vec<_>>(),
        vec![3, 7]
    );
    assert_eq!(hits[0].distance, 0.0);
    let hits = index.search(&[0.0, 1.0], 3).unwrap();
    assert_eq!(hits[0].projection_ordinal, 10);
    assert_eq!(hits[1].distance, 1.0);
    assert!(index.search(&[1.0, 0.0], 0).unwrap().is_empty());
}
#[test]
fn embedding_index_refuses_nonfinite_dimension_and_zero_norm() {
    for values in [
        vec![f32::NAN, 1.0],
        vec![f32::INFINITY, 1.0],
        vec![f32::NEG_INFINITY, 1.0],
        vec![0.0, 0.0],
        vec![1.0],
        vec![1.0, 2.0, 3.0],
    ] {
        assert!(
            FlatEmbeddingIndex::build(
                snapshot(2, 1),
                Uuid::now_v7(),
                vec![vector(1, &values)],
                limits()
            )
            .is_err()
        );
    }
    let index = FlatEmbeddingIndex::build(
        snapshot(2, 1),
        Uuid::now_v7(),
        vec![vector(1, &[1.0, 0.0])],
        limits(),
    )
    .unwrap();
    for query in [
        vec![f32::NAN, 1.0],
        vec![f32::INFINITY, 1.0],
        vec![0.0, 0.0],
        vec![1.0],
    ] {
        assert!(index.search(&query, 1).is_err());
    }
}
#[test]
fn embedding_index_refuses_limits_and_duplicate_members_without_allocating_snapshot_size() {
    assert!(
        FlatEmbeddingIndex::build(
            snapshot(2, 2),
            Uuid::now_v7(),
            vec![vector(1, &[1.0, 0.0]), vector(2, &[0.0, 1.0])],
            IndexLimits {
                max_members: 1,
                max_bytes: 1024
            }
        )
        .is_err()
    );
    assert!(
        FlatEmbeddingIndex::build(
            snapshot(2, 1),
            Uuid::now_v7(),
            vec![vector(1, &[1.0, 0.0])],
            IndexLimits {
                max_members: 10,
                max_bytes: 1
            }
        )
        .is_err()
    );
    assert!(
        FlatEmbeddingIndex::build(
            snapshot(u32::MAX, u64::MAX),
            Uuid::now_v7(),
            vec![],
            IndexLimits {
                max_members: usize::MAX,
                max_bytes: usize::MAX
            }
        )
        .is_err()
    );
    assert!(
        FlatEmbeddingIndex::build(
            snapshot(2, 2),
            Uuid::now_v7(),
            vec![vector(1, &[1.0, 0.0]), vector(1, &[0.0, 1.0])],
            limits()
        )
        .is_err()
    );
    assert!(
        FlatEmbeddingIndex::build(
            snapshot(2, 2),
            Uuid::now_v7(),
            vec![vector(1, &[1.0, 0.0])],
            limits()
        )
        .is_err()
    );
}
#[test]
fn embedding_index_empty_generation_and_extreme_finite_vectors_are_valid() {
    let empty =
        FlatEmbeddingIndex::build(snapshot(2, 0), Uuid::now_v7(), vec![], limits()).unwrap();
    assert!(empty.search(&[1.0, 0.0], 10).unwrap().is_empty());
    let index = FlatEmbeddingIndex::build(
        snapshot(2, 1),
        Uuid::now_v7(),
        vec![vector(1, &[f32::MAX, f32::MAX])],
        limits(),
    )
    .unwrap();
    let hit = &index.search(&[f32::MAX, f32::MAX], 1).unwrap()[0];
    assert!(hit.distance.is_finite());
    assert!(hit.distance.abs() < 1e-12);
}

mod service_probes {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use vestrace_application::embedding::index::EmbeddingIndexService;
    use vestrace_application::{
        ApplicationError, FenceReceipt, MaterialKeyVault, RequestContext, VaultError,
        embedding::index::*,
    };
    use vestrace_domain::{
        ErasureReceipt, IndexBuildAttemptId, IntentNonce, MaterialKeyId, PrincipalId, VaultReceipt,
        ZeroizingDek,
    };
    use vestrace_infrastructure::embedding_index::{
        EmbeddingIndexRegistry, FlatEmbeddingIndexFactory, IndexMemoryBudget,
    };
    struct Repo {
        plan: EmbeddingIndexBuildPlan,
        published: AtomicBool,
        lose: bool,
        fail_validation: usize,
        validations: AtomicUsize,
        trace: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl EmbeddingIndexRepository for Repo {
        async fn claim_next_build(
            &self,
            _: &RequestContext,
            _: &str,
            limit: u32,
        ) -> Result<Option<EmbeddingIndexBuildPlan>, ApplicationError> {
            assert!(
                limit <= i32::MAX as u32,
                "SQL claim limit must be representable"
            );
            self.trace.lock().unwrap().push("claim");
            Ok(Some(self.plan.clone()))
        }
        async fn claim_local_load(
            &self,
            _: &RequestContext,
            snapshot: &CanonicalGenerationSnapshot,
            _: &str,
        ) -> Result<EmbeddingIndexBuildPlan, ApplicationError> {
            assert_eq!(snapshot, &self.plan.snapshot);
            self.trace.lock().unwrap().push("lazy_claim");
            let mut plan = self.plan.clone();
            plan.purpose = IndexBuildPurpose::LazyLoad;
            Ok(plan)
        }
        async fn load_chunk(
            &self,
            _: &RequestContext,
            _: &EmbeddingIndexBuildPlan,
            _: Option<u64>,
            _: u32,
        ) -> Result<EncryptedProjectionChunk, ApplicationError> {
            if self.fail_validation == usize::MAX - 1 {
                return Err(ApplicationError::Unavailable(
                    "test storage unavailable".into(),
                ));
            }
            if self.fail_validation == usize::MAX - 2 {
                return Err(ApplicationError::Unavailable(
                    "embedding-generation-changed".into(),
                ));
            }
            self.trace.lock().unwrap().push("load");
            Ok(EncryptedProjectionChunk {
                projections: vec![],
                complete: true,
            })
        }
        async fn publish_ready(
            &self,
            _: &RequestContext,
            _: &EmbeddingIndexBuildPlan,
        ) -> Result<GenerationPublicationOutcome, ApplicationError> {
            self.trace.lock().unwrap().push("publish_commit");
            if self.lose {
                Ok(GenerationPublicationOutcome::Discarded)
            } else {
                self.published.store(true, Ordering::Release);
                Ok(GenerationPublicationOutcome::Published)
            }
        }
        async fn validate_current(
            &self,
            _: &RequestContext,
            _: &CanonicalGenerationSnapshot,
        ) -> Result<CurrentGenerationValidation, ApplicationError> {
            if self.fail_validation == usize::MAX {
                return Err(ApplicationError::Unavailable(
                    "test storage unavailable".into(),
                ));
            }
            self.trace.lock().unwrap().push("validate");
            let call = self.validations.fetch_add(1, Ordering::AcqRel) + 1;
            Ok(
                if !self.published.load(Ordering::Acquire) || call == self.fail_validation {
                    CurrentGenerationValidation::Changed
                } else {
                    CurrentGenerationValidation::Current {
                        space_registration_id: self.plan.space_registration_id,
                    }
                },
            )
        }
        async fn finish_attempt(
            &self,
            _: &RequestContext,
            _: IndexBuildAttemptId,
            _owner: &str,
            outcome: IndexBuildOutcome,
        ) -> Result<(), ApplicationError> {
            if self.fail_validation == usize::MAX - 1 {
                assert_eq!(
                    outcome,
                    IndexBuildOutcome::Failed(IndexFailureReason::StorageUnavailable)
                );
            }
            if self.fail_validation == usize::MAX - 2 {
                assert_eq!(outcome, IndexBuildOutcome::Discarded);
            }
            self.trace.lock().unwrap().push(match outcome {
                IndexBuildOutcome::Loaded => "loaded",
                IndexBuildOutcome::Discarded => "discarded",
                _ => "finished",
            });
            Ok(())
        }
        async fn append_attempt_observation(
            &self,
            _: &RequestContext,
            _: IndexBuildAttemptId,
            _owner: &str,
            reason: IndexFailureReason,
        ) -> Result<(), ApplicationError> {
            if self.fail_validation == usize::MAX {
                assert_eq!(reason, IndexFailureReason::StorageUnavailable);
            }
            self.trace.lock().unwrap().push("observation");
            Ok(())
        }
    }
    struct NeverVault;
    impl MaterialKeyVault for NeverVault {
        fn create_if_absent(
            &self,
            _: MaterialKeyId,
            _: IntentNonce,
        ) -> Result<VaultReceipt, VaultError> {
            panic!("no key creation")
        }
        fn unwrap(
            &self,
            _: MaterialKeyId,
            _: &mut dyn FnMut(&ZeroizingDek),
        ) -> Result<(), VaultError> {
            panic!("empty index needs no key")
        }
        fn prepare_erasure(&self, _: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
            panic!("no erasure")
        }
        fn erase(&self, _: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
            panic!("no erasure")
        }
    }
    struct NeverDecoder;
    impl EmbeddingIndexDecoder for NeverDecoder {
        fn decode_into(
            &self,
            _: &RequestContext,
            _: &EncryptedIndexProjection,
            _: &ZeroizingDek,
            _: u32,
            _: &mut dyn FnMut(Zeroizing<Vec<f32>>) -> Result<(), ApplicationError>,
        ) -> Result<(), ApplicationError> {
            panic!("empty index has no vector")
        }
    }
    struct Registry {
        inner: EmbeddingIndexRegistry,
        trace: Arc<Mutex<Vec<&'static str>>>,
    }
    impl EmbeddingIndexRegistryPort<FlatEmbeddingIndex> for Registry {
        fn get(&self, s: &CanonicalGenerationSnapshot, r: Uuid) -> Option<Arc<FlatEmbeddingIndex>> {
            self.inner.get(s, r)
        }
        fn install(&self, index: Arc<FlatEmbeddingIndex>) -> Result<(), ApplicationError> {
            self.trace.lock().unwrap().push("install");
            self.inner.install(index)
        }
        fn remove_space_before_epoch(&self, w: WorkspaceId, r: Uuid, e: u64) {
            self.inner.remove_space_before_epoch(w, r, e);
        }
        fn remove_exact(&self, s: &CanonicalGenerationSnapshot, r: Uuid) {
            self.trace.lock().unwrap().push("remove");
            self.inner.remove_exact(s, r)
        }
    }
    #[tokio::test]
    async fn embedding_index_service_commits_and_double_validates_before_success() {
        let trace = Arc::new(Mutex::new(vec![]));
        let snap = snapshot(2, 0);
        let context = RequestContext::new(snap.workspace_id, PrincipalId::new());
        let repo = Arc::new(Repo {
            plan: EmbeddingIndexBuildPlan {
                attempt_id: IndexBuildAttemptId::new(),
                space_registration_id: Uuid::now_v7(),
                snapshot: snap,
                purpose: IndexBuildPurpose::Startup,
                event_id: None,
                owner: "worker".into(),
            },
            published: AtomicBool::new(false),
            lose: false,
            fail_validation: 0,
            validations: AtomicUsize::new(0),
            trace: trace.clone(),
        });
        let budget = IndexMemoryBudget::new(4096);
        let registry = Arc::new(Registry {
            inner: EmbeddingIndexRegistry::new(),
            trace: trace.clone(),
        });
        let service = EmbeddingIndexService::new(
            repo,
            Arc::new(NeverVault),
            Arc::new(NeverDecoder),
            Arc::new(FlatEmbeddingIndexFactory::new(budget)),
            registry,
            limits(),
            1,
        );
        assert!(
            service
                .reconcile_one(&context, "worker")
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            *trace.lock().unwrap(),
            vec![
                "claim",
                "load",
                "publish_commit",
                "validate",
                "install",
                "validate"
            ]
        );
    }
    #[tokio::test]
    async fn embedding_index_claim_bound_is_postgres_representable() {
        let trace = Arc::new(Mutex::new(vec![]));
        let snap = snapshot(2, 0);
        let context = RequestContext::new(snap.workspace_id, PrincipalId::new());
        let repo = Arc::new(Repo {
            plan: EmbeddingIndexBuildPlan {
                attempt_id: IndexBuildAttemptId::new(),
                space_registration_id: Uuid::now_v7(),
                snapshot: snap,
                purpose: IndexBuildPurpose::Startup,
                event_id: None,
                owner: "worker".into(),
            },
            published: AtomicBool::new(false),
            lose: false,
            fail_validation: 0,
            validations: AtomicUsize::new(0),
            trace: trace.clone(),
        });
        let budget = IndexMemoryBudget::new(4096);
        let registry = Arc::new(Registry {
            inner: EmbeddingIndexRegistry::new(),
            trace: trace.clone(),
        });
        let service = EmbeddingIndexService::new(
            repo,
            Arc::new(NeverVault),
            Arc::new(NeverDecoder),
            Arc::new(FlatEmbeddingIndexFactory::new(budget)),
            registry,
            IndexLimits {
                max_members: usize::MAX,
                max_bytes: 4096,
            },
            1,
        );
        assert!(
            service
                .reconcile_one(&context, "worker")
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            *trace.lock().unwrap(),
            vec![
                "claim",
                "load",
                "publish_commit",
                "validate",
                "install",
                "validate"
            ]
        );
    }
    #[tokio::test]
    async fn embedding_index_service_discards_every_failed_candidate_and_observes_after_publication()
     {
        for (lose, fail_validation) in [(true, 0), (false, 1), (false, 2)] {
            let trace = Arc::new(Mutex::new(vec![]));
            let snap = snapshot(2, 0);
            let context = RequestContext::new(snap.workspace_id, PrincipalId::new());
            let registration = Uuid::now_v7();
            let repo = Arc::new(Repo {
                plan: EmbeddingIndexBuildPlan {
                    attempt_id: IndexBuildAttemptId::new(),
                    space_registration_id: registration,
                    snapshot: snap.clone(),
                    purpose: IndexBuildPurpose::Startup,
                    event_id: None,
                    owner: "worker".into(),
                },
                published: AtomicBool::new(false),
                lose,
                fail_validation,
                validations: AtomicUsize::new(0),
                trace: trace.clone(),
            });
            let budget = IndexMemoryBudget::new(4096);
            let registry = Arc::new(Registry {
                inner: EmbeddingIndexRegistry::new(),
                trace: trace.clone(),
            });
            let service = EmbeddingIndexService::new(
                repo,
                Arc::new(NeverVault),
                Arc::new(NeverDecoder),
                Arc::new(FlatEmbeddingIndexFactory::new(budget.clone())),
                registry.clone(),
                limits(),
                1,
            );
            assert!(service.reconcile_one(&context, "worker").await.is_err());
            assert_eq!(budget.used_bytes(), 0);
            assert!(registry.get(&snap, registration).is_none());
            let trace = trace.lock().unwrap();
            assert_eq!(trace.contains(&"install"), !lose && fail_validation == 2);
            assert_eq!(trace.contains(&"observation"), !lose);
        }
    }
    #[tokio::test]
    async fn embedding_index_lazy_load_does_not_publish_or_advance_the_generation() {
        let trace = Arc::new(Mutex::new(vec![]));
        let snap = snapshot(2, 0);
        let context = RequestContext::new(snap.workspace_id, PrincipalId::new());
        let repo = Arc::new(Repo {
            plan: EmbeddingIndexBuildPlan {
                attempt_id: IndexBuildAttemptId::new(),
                space_registration_id: Uuid::now_v7(),
                snapshot: snap.clone(),
                purpose: IndexBuildPurpose::Startup,
                event_id: None,
                owner: "worker".into(),
            },
            published: AtomicBool::new(true),
            lose: false,
            fail_validation: 0,
            validations: AtomicUsize::new(0),
            trace: trace.clone(),
        });
        let registry = Arc::new(Registry {
            inner: EmbeddingIndexRegistry::new(),
            trace: trace.clone(),
        });
        let service = EmbeddingIndexService::new(
            repo,
            Arc::new(NeverVault),
            Arc::new(NeverDecoder),
            Arc::new(FlatEmbeddingIndexFactory::new(IndexMemoryBudget::new(4096))),
            registry,
            limits(),
            1,
        );
        let index = service
            .ensure_loaded(&context, &snap, "worker")
            .await
            .unwrap();
        assert_eq!(index.snapshot(), &snap);
        assert_eq!(
            *trace.lock().unwrap(),
            vec![
                "validate",
                "lazy_claim",
                "load",
                "validate",
                "install",
                "validate",
                "loaded"
            ]
        );
    }
    #[tokio::test]
    async fn embedding_index_storage_outage_is_not_a_generation_change() {
        for failure in [usize::MAX, usize::MAX - 1, usize::MAX - 2] {
            let trace = Arc::new(Mutex::new(vec![]));
            let snap = snapshot(2, 0);
            let context = RequestContext::new(snap.workspace_id, PrincipalId::new());
            let repo = Arc::new(Repo {
                plan: EmbeddingIndexBuildPlan {
                    attempt_id: IndexBuildAttemptId::new(),
                    space_registration_id: Uuid::now_v7(),
                    snapshot: snap,
                    purpose: IndexBuildPurpose::Startup,
                    event_id: None,
                    owner: "worker".into(),
                },
                published: AtomicBool::new(false),
                lose: false,
                fail_validation: failure,
                validations: AtomicUsize::new(0),
                trace: trace.clone(),
            });
            let budget = IndexMemoryBudget::new(4096);
            let registry = Arc::new(Registry {
                inner: EmbeddingIndexRegistry::new(),
                trace: trace.clone(),
            });
            let service = EmbeddingIndexService::new(
                repo,
                Arc::new(NeverVault),
                Arc::new(NeverDecoder),
                Arc::new(FlatEmbeddingIndexFactory::new(budget.clone())),
                registry,
                limits(),
                1,
            );
            assert!(service.reconcile_one(&context, "worker").await.is_err());
            assert_eq!(budget.used_bytes(), 0);
            assert!(!trace.lock().unwrap().contains(&"install"));
        }
    }
}

async fn assert_index_build_schema(pool: &sqlx::PgPool) {
    for table in [
        "embedding_index_build_attempts",
        "embedding_index_build_observations",
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(exists, "missing index authority {table}");
        for (privilege, expected) in [
            ("SELECT", true),
            ("INSERT", false),
            ("UPDATE", false),
            ("DELETE", false),
            ("TRUNCATE", false),
            ("TRIGGER", false),
            ("REFERENCES", false),
        ] {
            let actual: bool = sqlx::query_scalar("SELECT has_table_privilege('vestrace',$1,$2)")
                .bind(table)
                .bind(privilege)
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(actual, expected, "{table} {privilege}");
        }
        let public_acl:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_class c CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl,acldefault('r',c.relowner))) a WHERE c.oid=$1::regclass AND a.grantee=0)").bind(table).fetch_one(pool).await.unwrap();
        assert!(!public_acl);
        let metadata:(String,bool,bool,bool)=sqlx::query_as("SELECT pg_get_userbyid(relowner),relrowsecurity,relforcerowsecurity,has_table_privilege('vestrace',oid,'INSERT,UPDATE,DELETE') FROM pg_class WHERE oid=$1::regclass").bind(table).fetch_one(pool).await.unwrap();
        assert_eq!(
            metadata,
            ("vestrace_guarded_owner".into(), true, true, false)
        );
    }
    for (signature, public) in [
        ("vestrace_validate_embedding_index_event_cause()", false),
        ("vestrace_embedding_index_snapshot(uuid,uuid)", false),
        ("vestrace_embedding_index_attempt_plan(uuid,uuid)", false),
        ("vestrace_validate_embedding_index_attempt()", false),
        (
            "vestrace_lock_embedding_index_attempt(uuid,uuid,text)",
            false,
        ),
        (
            "vestrace_claim_embedding_index_build(uuid,text,integer)",
            true,
        ),
        (
            "vestrace_validate_local_embedding_generation(uuid,uuid,bigint,bigint,bigint,bigint,bigint)",
            true,
        ),
        (
            "vestrace_claim_embedding_index_load(uuid,uuid,bigint,bigint,bigint,bigint,bigint,text)",
            true,
        ),
        (
            "vestrace_load_embedding_index_chunk(uuid,uuid,text,bigint,integer)",
            true,
        ),
        (
            "vestrace_publish_embedding_index_build(uuid,uuid,text)",
            true,
        ),
        (
            "vestrace_finish_embedding_index_attempt(uuid,uuid,text,text,text)",
            true,
        ),
        (
            "vestrace_observe_embedding_index_attempt(uuid,uuid,text,text)",
            true,
        ),
    ] {
        let authority:(String,bool,bool,bool)=sqlx::query_as("SELECT pg_get_userbyid(proowner),prosecdef,has_function_privilege('vestrace',oid,'EXECUTE'),EXISTS(SELECT 1 FROM aclexplode(COALESCE(proacl,acldefault('f',proowner))) WHERE grantee=0 AND privilege_type='EXECUTE') FROM pg_proc WHERE oid=$1::regprocedure").bind(signature).fetch_one(pool).await.unwrap();
        assert_eq!(
            authority,
            ("vestrace_guarded_owner".into(), true, public, false),
            "{signature}"
        );
    }
    let runtime = common::runtime_pool(pool).await;
    for table in [
        "embedding_index_build_attempts",
        "embedding_index_build_observations",
    ] {
        for sql in [
            format!("INSERT INTO {table} DEFAULT VALUES"),
            format!("UPDATE {table} SET id=id"),
            format!("DELETE FROM {table}"),
            format!("TRUNCATE TABLE {table}"),
        ] {
            let error = sqlx::query(&sql).execute(&runtime).await.unwrap_err();
            assert_eq!(
                error.as_database_error().unwrap().code().as_deref(),
                Some("42501"),
                "{sql}"
            );
        }
    }
    let columns:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='embedding_index_rebuild_events' AND column_name IN ('cause','material_erasure_preparation_id','transition_activation_receipt_id','legacy_cutover_receipt_id','operator_rebuild_request_id')").fetch_one(pool).await.unwrap();
    assert_eq!(columns, 5);
    let xor:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='embedding_index_rebuild_events'::regclass AND conname='embedding_index_change_cause_xor' AND contype='c')").fetch_one(pool).await.unwrap();
    assert!(xor);
    let deferred:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='embedding_index_rebuild_events'::regclass AND tgfoid='vestrace_validate_embedding_index_event_cause()'::regprocedure AND tgdeferrable AND tginitdeferred AND tgenabled='O')").fetch_one(pool).await.unwrap();
    assert!(deferred);
    let circular:i64=sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE contype='f' AND ((conrelid='embedding_index_rebuild_events'::regclass AND confrelid='embedding_job_result_publications'::regclass) OR (conrelid='embedding_job_result_publications'::regclass AND confrelid='embedding_index_rebuild_events'::regclass))").fetch_one(pool).await.unwrap();
    assert_eq!(circular, 2);
    let single_build:bool=sqlx::query_scalar("SELECT indisunique AND indpred IS NOT NULL FROM pg_index WHERE indexrelid='embedding_corpus_generations_one_building_per_space'::regclass").fetch_one(pool).await.unwrap();
    assert!(single_build);
}
#[sqlx::test(migrations = false)]
async fn embedding_index_fresh_authority_has_exact_owner_rls_and_no_runtime_writes(
    pool: sqlx::PgPool,
) {
    common::result_preparation_fixture::provision_result_behavior_database(&pool).await;
    assert_index_build_schema(&pool).await;
}
#[sqlx::test(migrations = false)]
async fn embedding_index_0197_runtime_upgrade_installs_exact_authority(pool: sqlx::PgPool) {
    common::result_preparation_fixture::provision_result_behavior_database_through(&pool, 197)
        .await;
    let runtime = common::runtime_pool(&pool).await;
    sqlx::migrate!("../../migrations")
        .run(&runtime)
        .await
        .unwrap();
    assert_index_build_schema(&pool).await;
}

mod durable_probes {
    use super::common;
    use sqlx::PgPool;
    use uuid::Uuid;
    use vestrace_application::{
        QualificationJobRepository, QualificationProbeCompletion, RequestContext,
    };
    use vestrace_domain::{PrincipalId, QualificationJobId, QualificationProbeResult, WorkspaceId};
    use vestrace_infrastructure::postgres::{PgQualificationJobRepository, PgStore};

    struct QualificationFixture {
        workspace: WorkspaceId,
        principal: PrincipalId,
        job: QualificationJobId,
        target: Uuid,
        model: Uuid,
        qualification: Uuid,
        shape: Uuid,
    }
    async fn set_context(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, workspace: WorkspaceId) {
        sqlx::query("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace.to_string())
            .execute(&mut **tx)
            .await
            .unwrap();
    }

    // Q1 network evidence follows the existing provider_dispatch_is_atomic fixture.
    fn q1_source_fields(
        ordinal: &str,
    ) -> (
        &'static str,
        &'static str,
        bool,
        &'static str,
        bool,
        bool,
        Option<&'static str>,
    ) {
        match ordinal {
            "10" | "20" | "90" => ("plain_text", "none", false, "none", false, false, None),
            "30" => ("plain_text", "none", false, "none", true, false, None),
            "35" => ("plain_text", "none", false, "none", true, true, None),
            "40" => (
                "plain_text",
                "named_probe",
                false,
                "none",
                false,
                false,
                None,
            ),
            "50" => (
                "assistant_tool_call_replay",
                "none",
                false,
                "none",
                false,
                false,
                Some("call_q1"),
            ),
            "60" => ("plain_text", "required", true, "none", false, false, None),
            "70" => (
                "plain_text",
                "none",
                false,
                "strict_nonce_json_schema",
                false,
                false,
                None,
            ),
            "80" => (
                "multipart_image_marker",
                "none",
                false,
                "none",
                false,
                false,
                None,
            ),
            _ => panic!("unknown q1 network ordinal {ordinal}"),
        }
    }

    async fn insert_finalizable_q1_network_probe(
        pool: &PgPool,
        qualification: &QualificationFixture,
        ordinal: &str,
    ) -> (Uuid, Uuid) {
        let effect_id = Uuid::now_v7();
        let evidence_id = Uuid::now_v7();
        let evidence_check_id = Uuid::now_v7();
        let receipt_id = Uuid::now_v7();
        let request_kind = match ordinal {
            "10" => "models_list",
            "90" => "embeddings",
            "20" | "30" | "35" | "40" | "50" | "60" | "70" | "80" => "chat_completions",
            _ => panic!("not a q1 network ordinal: {ordinal}"),
        };
        let (
            message_layout,
            tool_choice,
            parallel_tool_calls,
            response_format,
            stream,
            include_usage,
            replay,
        ) = q1_source_fields(ordinal);

        let mut transaction = pool.begin().await.unwrap();
        set_context(&mut transaction, qualification.workspace).await;
        sqlx::query(
            "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload)
         VALUES($1,$2,'openai-compatible','{}'::jsonb)",
        )
        .bind(effect_id)
        .bind(qualification.workspace.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO model_request_evidence_roots(
             id,workspace_id,external_effect_id,request_kind,
             qualification_target_binding_id,cause_kind,cause_id
         ) VALUES($1,$2,$3,$4,$5,'qualification_probe',$6)",
        )
        .bind(evidence_id)
        .bind(qualification.workspace.as_uuid())
        .bind(effect_id)
        .bind(request_kind)
        .bind(qualification.target)
        .bind(qualification.job.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
        for (node_ordinal, kind, reference, safe_ordinal) in [
            (0_i32, "external_effect", effect_id, None),
            (1_i32, "qualification_target", qualification.target, None),
            (
                2_i32,
                "qualification_probe",
                qualification.job.as_uuid(),
                Some(ordinal),
            ),
        ] {
            sqlx::query(
                "INSERT INTO model_request_evidence_nodes(
                 id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,safe_ordinal
             ) VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(Uuid::now_v7())
            .bind(qualification.workspace.as_uuid())
            .bind(evidence_id)
            .bind(node_ordinal)
            .bind(kind)
            .bind(reference)
            .bind(safe_ordinal)
            .execute(&mut *transaction)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO model_request_evidence_checks(
             id,workspace_id,evidence_root_id,status,missing_reference_count
         ) VALUES($1,$2,$3,'complete',0)",
        )
        .bind(evidence_check_id)
        .bind(qualification.workspace.as_uuid())
        .bind(evidence_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_create_qualification_q1_mre_source(
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(evidence_id)
        .bind(qualification.workspace.as_uuid())
        .bind(ordinal)
        .bind(message_layout)
        .bind(tool_choice)
        .bind(parallel_tool_calls)
        .bind(response_format)
        .bind(stream)
        .bind(include_usage)
        .bind(replay)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
        sqlx::query(
        "INSERT INTO provider_dispatch_causes(
             external_effect_id,workspace_id,model_request_evidence_id,model_request_evidence_check_id,
             cause_kind,qualification_job_id,qualification_target_binding_id,qualification_probe_ordinal
         ) VALUES($1,$2,$3,$4,'qualification_probe',$5,$6,$7)",
    )
    .bind(effect_id)
    .bind(qualification.workspace.as_uuid())
    .bind(evidence_id)
    .bind(evidence_check_id)
    .bind(qualification.job.as_uuid())
    .bind(qualification.target)
    .bind(ordinal)
    .execute(&mut *transaction)
    .await
    .unwrap();
        sqlx::query("RESET ROLE")
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO external_effect_receipts(
             id,effect_id,workspace_id,outcome_status,payload
         ) VALUES($1,$2,$3,'acknowledged','{}'::jsonb)",
        )
        .bind(receipt_id)
        .bind(effect_id)
        .bind(qualification.workspace.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO external_effect_lifecycle_transitions(
             id,effect_id,workspace_id,status,cause,cause_ref,recorded_at
         ) VALUES($1,$2,$3,'acknowledged','receipt_recorded',$4,NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(effect_id)
        .bind(qualification.workspace.as_uuid())
        .bind(receipt_id.to_string())
        .execute(&mut *transaction)
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        (effect_id, evidence_id)
    }

    async fn record_complete_q1_matrix(
        owner: &PgPool,
        runtime: &PgPool,
        qualification: &QualificationFixture,
    ) {
        let context = RequestContext::new(qualification.workspace, qualification.principal);
        let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime.clone()));
        let mut network = std::collections::BTreeMap::new();
        for ordinal in ["10", "20", "30", "35", "40", "50", "60", "70", "80", "90"] {
            network.insert(
                ordinal,
                insert_finalizable_q1_network_probe(owner, qualification, ordinal).await,
            );
        }
        for ordinal in [
            "00", "10", "15", "20", "30", "35", "40", "50", "60", "70", "80", "90",
        ] {
            let (external_effect_id, model_request_evidence_id) = network
                .get(ordinal)
                .copied()
                .map(|(effect, evidence)| (Some(effect), Some(evidence)))
                .unwrap_or((None, None));
            repository
                .record_probe_result(
                    &context,
                    QualificationProbeCompletion {
                        probe_result_id: Uuid::now_v7(),
                        job_id: qualification.job,
                        ordinal: ordinal.to_owned(),
                        result: QualificationProbeResult::Pass,
                        external_effect_id,
                        model_request_evidence_id,
                    },
                )
                .await
                .expect("the complete q1 fixture must record every ordinal");
        }
    }

    async fn canonical_qualification(owner: &PgPool, runtime: &PgPool) -> QualificationFixture {
        let f = QualificationFixture {
            workspace: WorkspaceId::new(),
            principal: PrincipalId::new(),
            job: QualificationJobId::new(),
            target: Uuid::now_v7(),
            model: Uuid::now_v7(),
            qualification: Uuid::now_v7(),
            shape: Uuid::now_v7(),
        };
        let connector = Uuid::now_v7();
        let connection = Uuid::now_v7();
        let revision = Uuid::now_v7();
        let guard = Uuid::now_v7();
        let no_auth = Uuid::now_v7();
        let provider = Uuid::now_v7();
        let chat_model = Uuid::now_v7();
        let embedding_model = Uuid::now_v7();
        let chat_revision = Uuid::now_v7();
        sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
            .bind(f.workspace.as_uuid())
            .bind(format!("canonical-{}", f.workspace))
            .execute(owner)
            .await
            .unwrap();
        sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
            .bind(f.principal.as_uuid())
            .bind(f.workspace.as_uuid())
            .bind(f.principal.to_string())
            .execute(owner)
            .await
            .unwrap();
        sqlx::query("INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,'canonical','local')").bind(connector).bind(f.workspace.as_uuid()).execute(owner).await.unwrap();
        sqlx::query("INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) VALUES($1,$2,$3,$4,'canonical','active')").bind(connection).bind(connector).bind(f.workspace.as_uuid()).bind(f.principal.as_uuid()).execute(owner).await.unwrap();
        sqlx::query(
        "INSERT INTO providers(id,workspace_id,name,locality) VALUES($1,$2,'canonical','local')",
    )
    .bind(provider)
    .bind(f.workspace.as_uuid())
    .execute(owner)
    .await
    .unwrap();
        for (id, name) in [(chat_model, "chat"), (embedding_model, "embedding")] {
            sqlx::query("INSERT INTO models(id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken) VALUES($1,$2,$3,$4,4096,0,0)").bind(id).bind(provider).bind(f.workspace.as_uuid()).bind(name).execute(owner).await.unwrap();
        }
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
            .bind(guard)
            .bind(f.workspace.as_uuid())
            .bind(connection)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1','http://127.0.0.1:1234/v1','q1','loopback_only','none',NULL,0)").bind(revision).bind(f.workspace.as_uuid()).bind(connection).bind(guard).execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
            .bind(no_auth)
            .bind(f.workspace.as_uuid())
            .bind(connection)
            .bind(revision)
            .execute(&mut *tx)
            .await
            .unwrap();
        for (id, model, wire, kind) in [
            (chat_revision, chat_model, "chat-model", "chat"),
            (
                f.model,
                embedding_model,
                "text-embedding-nomic-embed-text-v1.5",
                "embedding",
            ),
        ] {
            sqlx::query("SELECT vestrace_create_model_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,NULL,NULL,NULL,NULL,NULL,NULL,0)").bind(id).bind(f.workspace.as_uuid()).bind(model).bind(connection).bind(guard).bind(revision).bind(wire).bind(kind).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("SELECT vestrace_create_model_request_shape_revision($1,$2,1,'embeddings',false,ARRAY[]::TEXT[])").bind(f.shape).bind(f.workspace.as_uuid()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        // Structural Running target setup matches the existing Q1 repository tests.
        // Results and qualification publication themselves use their runtime commands.
        let mut tx = owner.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state) VALUES($1,$2,$3,'q1','running')").bind(f.job.as_uuid()).bind(f.workspace.as_uuid()).bind(revision).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id,chat_model_revision_id,embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6,$7,$8)").bind(f.target).bind(f.workspace.as_uuid()).bind(f.job.as_uuid()).bind(connection).bind(revision).bind(no_auth).bind(chat_revision).bind(f.model).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        record_complete_q1_matrix(owner, runtime, &f).await;
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SELECT vestrace_finalize_qualification_job($1,$2,$3,$4,$5)")
            .bind(f.workspace.as_uuid())
            .bind(f.job.as_uuid())
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .bind(f.qualification)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        f
    }

    #[sqlx::test(migrations = false)]
    async fn embedding_index_empty_startup_publish_and_local_claim_reuse(pool: PgPool) {
        use vestrace_application::embedding::index::*;
        use vestrace_infrastructure::postgres::PgEmbeddingIndexRepository;
        common::result_preparation_fixture::provision_result_behavior_database(&pool).await;
        let runtime = common::runtime_pool(&pool).await;
        let f = canonical_qualification(&pool, &runtime).await;
        let registration = Uuid::now_v7();
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
            .bind(f.workspace.as_uuid())
            .bind(f.model)
            .bind(registration)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let context = RequestContext::new(f.workspace, f.principal);
        let repo = PgEmbeddingIndexRepository::new(PgStore::from_pool(runtime.clone()));
        let forbidden_event = Uuid::now_v7();
        let mut future = pool.begin().await.unwrap();
        set_context(&mut future, f.workspace).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *future)
            .await
            .unwrap();
        sqlx::query("INSERT INTO embedding_index_rebuild_events(id,workspace_id,publication_id,space_registration_id,before_corpus_revision,after_corpus_revision,before_generation_epoch,after_generation_epoch,before_live_member_count,after_live_member_count,built_through_projection_ordinal,cause,operator_rebuild_request_id) VALUES($1,$2,NULL,$3,0,1,0,1,0,1,1,'operator_rebuild',$4)").bind(forbidden_event).bind(f.workspace.as_uuid()).bind(registration).bind(Uuid::now_v7()).execute(&mut *future).await.unwrap();
        let future_error = future.commit().await.unwrap_err();
        assert_eq!(
            future_error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
        assert_eq!(
            future_error.as_database_error().unwrap().message(),
            "embedding index change cause authority is not installed"
        );
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM embedding_index_rebuild_events WHERE id=$1)",
        )
        .bind(forbidden_event)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!exists);
        let a = repo
            .claim_next_build(&context, "worker-a", 10)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(a.purpose, IndexBuildPurpose::Startup);
        let mut raw = runtime.begin().await.unwrap();
        set_context(&mut raw, f.workspace).await;
        let error = sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,1)")
            .bind(Uuid::now_v7())
            .bind(f.workspace.as_uuid())
            .bind(registration)
            .execute(&mut *raw)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23505")
        );
        raw.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM embedding_corpus_generations WHERE workspace_id=$1",
        )
        .bind(f.workspace.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);

        assert_eq!(a.snapshot.member_count, 0);
        let replay = repo
            .claim_next_build(&context, "worker-a", 10)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(a.attempt_id, replay.attempt_id);
        assert!(
            repo.load_chunk(&context, &a, None, 1)
                .await
                .unwrap()
                .complete
        );
        assert_eq!(
            repo.publish_ready(&context, &a).await.unwrap(),
            GenerationPublicationOutcome::Published
        );
        assert_eq!(
            repo.publish_ready(&context, &a).await.unwrap(),
            GenerationPublicationOutcome::Published
        );
        assert!(matches!(
            repo.validate_current(&context, &a.snapshot).await.unwrap(),
            CurrentGenerationValidation::Current { .. }
        ));
        let load = repo
            .claim_local_load(&context, &a.snapshot, "loader")
            .await
            .unwrap();
        let load2 = repo
            .claim_local_load(&context, &a.snapshot, "loader")
            .await
            .unwrap();
        assert_eq!(load.attempt_id, load2.attempt_id);
        let (same_a, same_b) = tokio::join!(
            repo.claim_local_load(&context, &a.snapshot, "parallel-loader"),
            repo.claim_local_load(&context, &a.snapshot, "parallel-loader")
        );
        assert_eq!(same_a.unwrap().attempt_id, same_b.unwrap().attempt_id);

        assert!(
            repo.finish_attempt(
                &context,
                load.attempt_id,
                "other",
                IndexBuildOutcome::Loaded
            )
            .await
            .is_err()
        );
        repo.finish_attempt(
            &context,
            load.attempt_id,
            "loader",
            IndexBuildOutcome::Loaded,
        )
        .await
        .unwrap();
        let facts:(i64,i64,i64)=sqlx::query_as("SELECT generation_epoch,guard_version,(SELECT count(*) FROM embedding_corpus_generations WHERE workspace_id=$1) FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$2").bind(f.workspace.as_uuid()).bind(registration).fetch_one(&pool).await.unwrap();
        assert_eq!(facts, (1, 2, 1));
    }
    #[sqlx::test(migrations = false)]
    async fn embedding_index_expired_claim_recovers_without_new_generation(pool: PgPool) {
        use vestrace_application::embedding::index::*;
        use vestrace_infrastructure::postgres::PgEmbeddingIndexRepository;
        common::result_preparation_fixture::provision_result_behavior_database(&pool).await;
        let runtime = common::runtime_pool(&pool).await;
        let f = canonical_qualification(&pool, &runtime).await;
        let registration = Uuid::now_v7();
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
            .bind(f.workspace.as_uuid())
            .bind(f.model)
            .bind(registration)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let context = RequestContext::new(f.workspace, f.principal);
        let repo = PgEmbeddingIndexRepository::new(PgStore::from_pool(runtime.clone()));
        let original = repo
            .claim_next_build(&context, "expiring", 10)
            .await
            .unwrap()
            .unwrap();
        let deadline:bool=sqlx::query_scalar("SELECT claim_deadline=created_at+INTERVAL '60 seconds' FROM embedding_index_build_attempts WHERE id=$1").bind(original.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
        assert!(deadline);
        tokio::time::sleep(std::time::Duration::from_secs(61)).await;
        let recovered = repo
            .claim_next_build(&context, "expiring", 10)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(recovered.attempt_id, original.attempt_id);
        assert_eq!(recovered.snapshot, original.snapshot);
        let terminal: (String, String) = sqlx::query_as(
            "SELECT state,safe_reason FROM embedding_index_build_attempts WHERE id=$1",
        )
        .bind(original.attempt_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(terminal, ("discarded".into(), "claim_expired".into()));
    }
    #[sqlx::test(migrations = false)]
    async fn embedding_index_distinct_owners_overlap_one_publication_cas(pool: PgPool) {
        use vestrace_application::embedding::index::*;
        use vestrace_infrastructure::postgres::PgEmbeddingIndexRepository;
        common::result_preparation_fixture::provision_result_behavior_database(&pool).await;
        let runtime = common::runtime_pool(&pool).await;
        let f = canonical_qualification(&pool, &runtime).await;
        let registration = Uuid::now_v7();
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
            .bind(f.workspace.as_uuid())
            .bind(f.model)
            .bind(registration)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let context = RequestContext::new(f.workspace, f.principal);
        let repo = PgEmbeddingIndexRepository::new(PgStore::from_pool(runtime.clone()));
        use std::sync::Arc;
        let repo = Arc::new(repo);
        let a = repo
            .claim_next_build(&context, "owner-a", 10)
            .await
            .unwrap()
            .unwrap();
        let b = repo
            .claim_next_build(&context, "owner-b", 10)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(a.attempt_id, b.attempt_id);
        assert_eq!(a.snapshot, b.snapshot);
        let mut gate = pool.begin().await.unwrap();
        let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *gate)
            .await
            .unwrap();
        sqlx::query("SELECT 1 FROM embedding_space_corpus_states WHERE workspace_id=$1 AND space_registration_id=$2 FOR UPDATE").bind(f.workspace.as_uuid()).bind(registration).execute(&mut *gate).await.unwrap();
        let r = repo.clone();
        let c = context.clone();
        let pa = a.clone();
        let first = tokio::spawn(async move { r.publish_ready(&c, &pa).await });
        let r = repo.clone();
        let c = context.clone();
        let pb = b.clone();
        let second = tokio::spawn(async move { r.publish_ready(&c, &pb).await });
        let mut observed = false;
        for _ in 0..200 {
            let waiting:i64=sqlx::query_scalar("WITH RECURSIVE blocked(pid) AS (SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) UNION SELECT a.pid FROM pg_stat_activity a JOIN blocked b ON b.pid=ANY(pg_blocking_pids(a.pid))) SELECT count(DISTINCT pid) FROM blocked").bind(holder).fetch_one(&pool).await.unwrap();
            if waiting >= 2 {
                observed = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        gate.commit().await.unwrap();
        let first = first.await.unwrap().unwrap();
        let second = second.await.unwrap().unwrap();
        assert!(
            observed,
            "both publishers must actually overlap at corpus guard"
        );
        assert_ne!(first, second);
        let facts:(i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE state='published'),count(*) FILTER(WHERE state='discarded'),count(DISTINCT generation_id) FROM embedding_index_build_attempts WHERE workspace_id=$1").bind(f.workspace.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(facts, (1, 1, 1));
        let current:Option<Uuid>=sqlx::query_scalar("SELECT current_generation_id FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$2").bind(f.workspace.as_uuid()).bind(registration).fetch_one(&pool).await.unwrap();
        assert_eq!(current, Some(a.snapshot.generation_id.as_uuid()));
    }
    #[sqlx::test(migrations = false)]
    async fn embedding_index_real_encrypted_members_build_and_reload(pool: PgPool) {
        use common::result_preparation_fixture::*;
        use std::sync::Arc;
        use vestrace_application::{
            EmbeddingResultFinalizationAuthority, EmbeddingResultFinalizationService,
            EmbeddingResultPreparationId, ExternalEffectRepository,
        };
        use vestrace_domain::{EmbeddingJobId, ExternalEffectId};
        use vestrace_infrastructure::postgres::{
            EmbeddingOutputHmacCommitter, PgEmbeddingResultFinalizationRepository,
            PgExternalEffectRepository,
        };
        provision_result_behavior_database_through(&pool, 197).await;
        let runtime = common::runtime_pool(&pool).await;
        let f = canonical_qualification(&pool, &runtime).await;
        let registration = Uuid::now_v7();
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'encrypted',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',768)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let (connection_id,connection_revision_id,connection_qualification_id,no_auth_binding_id):(Uuid,Uuid,Uuid,Uuid)=sqlx::query_as("SELECT r.connection_id,q.connection_revision_id,q.connection_qualification_revision_id,n.id FROM model_qualification_revisions q JOIN connection_revisions r ON r.id=q.connection_revision_id JOIN no_auth_binding_revisions n ON n.connection_revision_id=r.id WHERE q.id=$1").bind(f.qualification).fetch_one(&pool).await.unwrap();
        let context = RequestContext::new(f.workspace, f.principal);
        let snapshot_id = Uuid::now_v7();
        let intent = common::workspace_scoped_intent(&context, snapshot_id);
        PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
            .insert_intent(&context, &intent)
            .await
            .unwrap();
        let accepted = common::AcceptedJob {
            context,
            job_id: EmbeddingJobId::new(),
            connection_id,
            connection_revision_id,
            connection_qualification_id,
            model_revision_id: f.model,
            space_registration_id: registration,
            model_qualification_id: f.qualification,
            no_auth_binding_id,
            snapshot_id,
            external_effect_id: intent.id().as_uuid(),
            evidence_id: Uuid::now_v7(),
            intent,
            credential: None,
        };
        let mut tx = pool.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)").bind(snapshot_id).bind(f.workspace.as_uuid()).bind(connection_id).bind(connection_revision_id).bind(connection_qualification_id).bind(f.model).bind(f.qualification).bind(no_auth_binding_id).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)").bind(f.workspace.as_uuid()).bind(snapshot_id).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let result = result_fixture_for_accepted(
            &pool,
            runtime.clone(),
            accepted,
            DeliveryPolicyCase::ExactAllowed,
            true,
        )
        .await;
        let preparation = Uuid::now_v7();
        commit_real_vectors(
            &result,
            preparation,
            Uuid::now_v7(),
            &exact_attempt(&result),
        )
        .await
        .unwrap();
        let authority = EmbeddingResultFinalizationAuthority {
            preparation_id: EmbeddingResultPreparationId::from_uuid(preparation),
            job_id: result.accepted.job_id,
            effect_id: ExternalEffectId::from_uuid(result.accepted.external_effect_id),
        };
        EmbeddingResultFinalizationService::new(
            Arc::new(PgEmbeddingResultFinalizationRepository::new(
                PgStore::from_pool(runtime.clone()),
            )),
            Arc::new(result.vault.vault(f.workspace)),
            Arc::new(EmbeddingOutputHmacCommitter::new()),
        )
        .finalize(&result.accepted.context, &authority)
        .await
        .unwrap();

        let preserved:Vec<(Uuid,Uuid,i64,i64)>=sqlx::query_as("SELECT id,publication_id,after_corpus_revision,built_through_projection_ordinal FROM embedding_index_rebuild_events WHERE workspace_id=$1 ORDER BY id").bind(f.workspace.as_uuid()).fetch_all(&pool).await.unwrap();
        assert_eq!(preserved.len(), 1);
        sqlx::migrate!("../../migrations")
            .run(&runtime)
            .await
            .unwrap();
        let upgraded:Vec<(Uuid,Uuid,i64,i64)>=sqlx::query_as("SELECT id,publication_id,after_corpus_revision,built_through_projection_ordinal FROM embedding_index_rebuild_events WHERE workspace_id=$1 ORDER BY id").bind(f.workspace.as_uuid()).fetch_all(&pool).await.unwrap();
        assert_eq!(preserved, upgraded);
        use vestrace_application::embedding::index::*;
        use vestrace_infrastructure::embedding_index::{
            ContentMaterialIndexDecoder, EmbeddingIndexRegistry, FlatEmbeddingIndexFactory,
            IndexMemoryBudget,
        };
        use vestrace_infrastructure::postgres::PgEmbeddingIndexRepository;
        let repo = Arc::new(PgEmbeddingIndexRepository::new(PgStore::from_pool(
            runtime.clone(),
        )));
        assert!(
            repo.claim_next_build(&result.accepted.context, "bounded", 1)
                .await
                .is_err()
        );
        let captured: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM embedding_corpus_generations WHERE workspace_id=$1",
        )
        .bind(f.workspace.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(captured, 0, "member bound must refuse before capture");
        let budget = IndexMemoryBudget::new(4 * 1024 * 1024);
        let registry = Arc::new(EmbeddingIndexRegistry::new());
        let service = EmbeddingIndexService::new(
            repo.clone(),
            Arc::new(result.vault.vault(f.workspace)),
            Arc::new(ContentMaterialIndexDecoder::new(budget.clone())),
            Arc::new(FlatEmbeddingIndexFactory::new(budget.clone())),
            registry.clone(),
            IndexLimits {
                max_members: 2,
                max_bytes: 1024 * 1024,
            },
            1,
        );
        let index = service
            .reconcile_one(&result.accepted.context, "builder")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(index.snapshot().member_count, 2);
        let query = vec![1.0f32; 768];
        let hits = index.search(&query, 2).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.distance.is_finite()));
        let snapshot = index.snapshot().clone();
        drop(index);
        registry.remove_exact(&snapshot, registration);
        assert_eq!(budget.used_bytes(), 0);
        let reloaded = service
            .ensure_loaded(&result.accepted.context, &snapshot, "restart")
            .await
            .unwrap();
        assert_eq!(reloaded.search(&query, 2).unwrap(), hits);
        assert_eq!(reloaded.snapshot(), &snapshot);
        let forged_id = Uuid::now_v7();
        let mut forged = pool.begin().await.unwrap();
        set_context(&mut forged, f.workspace).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *forged)
            .await
            .unwrap();
        sqlx::query("INSERT INTO embedding_index_build_attempts(id,workspace_id,space_registration_id,generation_id,cause,claim_owner,claim_deadline,state) VALUES($1,$2,$3,$4,'startup','forged-startup',NOW()+INTERVAL '60 seconds','building')").bind(forged_id).bind(f.workspace.as_uuid()).bind(registration).bind(snapshot.generation_id.as_uuid()).execute(&mut *forged).await.unwrap();
        let error = forged.commit().await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().message(),
            "embedding index startup requires initial empty corpus"
        );
        let persisted: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM embedding_index_build_attempts WHERE id=$1)",
        )
        .bind(forged_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!persisted);

        // Explicit isolated corruption probe: production hydration must not silently skip a member.
        let saved:(Uuid,Uuid,Uuid,Uuid,Vec<u8>,chrono::DateTime<chrono::Utc>)=sqlx::query_as("SELECT id,workspace_id,intent_id,material_id,ciphertext,created_at FROM content_material_bytes WHERE material_id=$1").bind(result.outputs[0].material_id.as_uuid()).fetch_one(&pool).await.unwrap();
        {
            let probe = repo
                .claim_local_load(&result.accepted.context, &snapshot, "oversized")
                .await
                .unwrap();
            let mut corrupt = pool.begin().await.unwrap();
            set_context(&mut corrupt, f.workspace).await;
            sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
                .execute(&mut *corrupt)
                .await
                .unwrap();
            sqlx::query("ALTER TABLE content_material_bytes DISABLE TRIGGER content_material_bytes_deferred_invariant").execute(&mut *corrupt).await.unwrap();
            sqlx::query("UPDATE content_material_bytes SET ciphertext=$2 WHERE id=$1")
                .bind(saved.0)
                .bind(vec![0u8; 1048577])
                .execute(&mut *corrupt)
                .await
                .unwrap();
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                .execute(&mut *corrupt)
                .await
                .unwrap();
            sqlx::query("ALTER TABLE content_material_bytes ENABLE TRIGGER content_material_bytes_deferred_invariant").execute(&mut *corrupt).await.unwrap();
            corrupt.commit().await.unwrap();
            let enabled:bool=sqlx::query_scalar("SELECT tgenabled='O' FROM pg_trigger WHERE tgrelid='content_material_bytes'::regclass AND tgname='content_material_bytes_deferred_invariant'").fetch_one(&pool).await.unwrap();
            assert!(enabled);
            let error = repo
                .load_chunk(&result.accepted.context, &probe, None, 1)
                .await
                .err()
                .expect("corrupt selected member must refuse");
            assert!(
                matches!(error,vestrace_application::ApplicationError::Unavailable(ref m) if m=="embedding-index-material-unavailable")
            );
            let state: String =
                sqlx::query_scalar("SELECT state FROM embedding_index_build_attempts WHERE id=$1")
                    .bind(probe.attempt_id.as_uuid())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(state, "building");
            let mut restore = pool.begin().await.unwrap();
            set_context(&mut restore, f.workspace).await;
            sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
                .execute(&mut *restore)
                .await
                .unwrap();
            sqlx::query("UPDATE content_material_bytes SET ciphertext=$2 WHERE id=$1")
                .bind(saved.0)
                .bind(&saved.4)
                .execute(&mut *restore)
                .await
                .unwrap();
            restore.commit().await.unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, Vec<u8>>(
                    "SELECT ciphertext FROM content_material_bytes WHERE id=$1"
                )
                .bind(saved.0)
                .fetch_one(&pool)
                .await
                .unwrap(),
                saved.4
            );
            assert_eq!(
                repo.load_chunk(&result.accepted.context, &probe, None, 1)
                    .await
                    .unwrap()
                    .projections
                    .len(),
                1
            );
        }
        // The independent 0195 publication invariant refuses a missing row before runtime hydration.
        let mut deletion = pool.begin().await.unwrap();
        set_context(&mut deletion, f.workspace).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *deletion)
            .await
            .unwrap();
        sqlx::query("ALTER TABLE content_material_bytes DISABLE TRIGGER content_material_bytes_deferred_invariant").execute(&mut *deletion).await.unwrap();
        sqlx::query("DELETE FROM content_material_bytes WHERE id=$1")
            .bind(saved.0)
            .execute(&mut *deletion)
            .await
            .unwrap();
        let refused = deletion.commit().await.unwrap_err();
        assert_eq!(
            refused.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
        assert_eq!(
            refused.as_database_error().unwrap().message(),
            "result output identity or bytes incomplete"
        );
        assert_eq!(
            sqlx::query_scalar::<_, Vec<u8>>(
                "SELECT ciphertext FROM content_material_bytes WHERE id=$1"
            )
            .bind(saved.0)
            .fetch_one(&pool)
            .await
            .unwrap(),
            saved.4
        );
        let enabled:bool=sqlx::query_scalar("SELECT tgenabled='O' FROM pg_trigger WHERE tgrelid='content_material_bytes'::regclass AND tgname='content_material_bytes_deferred_invariant'").fetch_one(&pool).await.unwrap();
        assert!(enabled);
        let pending = repo
            .claim_local_load(&result.accepted.context, &snapshot, "stale-loader")
            .await
            .unwrap();
        let accepted2 =
            common::prepare_additional_delivery_embedding_job(&runtime, &result.accepted).await;
        let result2 = result_fixture_for_accepted(
            &pool,
            runtime.clone(),
            accepted2,
            DeliveryPolicyCase::ExactAllowed,
            false,
        )
        .await;
        let preparation2 = Uuid::now_v7();
        commit_real_vectors(
            &result2,
            preparation2,
            Uuid::now_v7(),
            &exact_attempt(&result2),
        )
        .await
        .unwrap();
        let authority2 = EmbeddingResultFinalizationAuthority {
            preparation_id: EmbeddingResultPreparationId::from_uuid(preparation2),
            job_id: result2.accepted.job_id,
            effect_id: ExternalEffectId::from_uuid(result2.accepted.external_effect_id),
        };
        EmbeddingResultFinalizationService::new(
            Arc::new(PgEmbeddingResultFinalizationRepository::new(
                PgStore::from_pool(runtime.clone()),
            )),
            Arc::new(result2.vault.vault(f.workspace)),
            Arc::new(EmbeddingOutputHmacCommitter::new()),
        )
        .finalize(&result2.accepted.context, &authority2)
        .await
        .unwrap();
        assert_eq!(
            repo.validate_current(&result.accepted.context, &snapshot)
                .await
                .unwrap(),
            CurrentGenerationValidation::Changed
        );
        assert!(
            repo.load_chunk(&result.accepted.context, &pending, None, 1)
                .await
                .is_err()
        );
        assert!(
            service
                .ensure_loaded(&result.accepted.context, &snapshot, "stale-retry")
                .await
                .is_err()
        );
        let current:Option<Uuid>=sqlx::query_scalar("SELECT current_generation_id FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$2").bind(f.workspace.as_uuid()).bind(registration).fetch_one(&pool).await.unwrap();
        assert_eq!(current, None);
        // Physical removal is safe after the committed invalidation; retained readers retain their reservation.
        registry.remove_exact(&snapshot, registration);
        assert!(budget.used_bytes() > 0);
        drop(reloaded);
        assert_eq!(budget.used_bytes(), 0);
    }
    use common::result_preparation_fixture::{CommitAttempt, ResultFixture};
    async fn commit_real_vectors(
        fixture: &ResultFixture,
        preparation: Uuid,
        receipt: Uuid,
        attempt: &CommitAttempt,
    ) -> Result<Uuid, sqlx::Error> {
        let mut transaction = fixture.runtime.begin().await.unwrap();
        set_context(&mut transaction, fixture.accepted.context.workspace_id).await;
        use vestrace_application::{
            EmbeddingOutputKeyBinding, EmbeddingResultPreparationId, MaterialKeyVault,
        };
        let vault = fixture.vault.vault(fixture.accepted.context.workspace_id);
        let mut frames = Vec::new();
        for output in &fixture.outputs {
            let binding = EmbeddingOutputKeyBinding {
                workspace_id: fixture.accepted.context.workspace_id,
                job_id: fixture.accepted.job_id,
                output_ordinal: output.output_ordinal,
                intent_id: output.intent_id,
                material_id: output.material_id,
                key_id: output.key_id,
                nonce: output.nonce,
            };
            let preparation = EmbeddingResultPreparationId::from_uuid(preparation);
            let witness = vault.bind_embedding_output(&binding, preparation).unwrap();
            let mut framed = None;
            let plaintext = zeroize::Zeroizing::new(
                (0..768)
                    .flat_map(|n| {
                        if n == output.output_ordinal as usize {
                            1.0f32.to_bits().to_be_bytes()
                        } else {
                            0.0f32.to_bits().to_be_bytes()
                        }
                    })
                    .collect::<Vec<_>>(),
            );
            vault
                .with_bound_embedding_output_key(&binding, preparation, witness, &mut |dek| {
                    framed = Some(
                        vestrace_infrastructure::crypto::ContentMaterialCodec::new()
                            .seal(
                                binding.workspace_id,
                                binding.material_id,
                                binding.key_id,
                                dek,
                                &plaintext,
                            )
                            .unwrap(),
                    );
                })
                .unwrap();
            frames.push(framed.unwrap());
        }
        let result = async {
        let (lease, dispatch): (Uuid, Uuid) = sqlx::query_as(
            "SELECT lease.id,lifecycle.id FROM provider_concurrency_leases lease \
             JOIN external_effect_lifecycle_transitions lifecycle ON lifecycle.workspace_id=lease.workspace_id AND lifecycle.effect_id=lease.external_effect_id \
             WHERE lease.workspace_id=$1 AND lease.external_effect_id=$2 AND lifecycle.status='dispatching' \
             ORDER BY lifecycle.ordinal DESC LIMIT 1",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.external_effect_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_lock_embedding_result_completion_authority($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(attempt.job_id)
        .bind(attempt.effect_id)
        .bind(fixture.accepted.connection_id)
        .bind(fixture.accepted.connection_revision_id)
        .bind(dispatch)
        .bind(lease)
        .fetch_one(&mut *transaction)
        .await?;
        let version: i64 = sqlx::query_scalar(
            "SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.job_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query_scalar(
            "SELECT vestrace_commit_embedding_result_preparation($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(attempt.job_id)
        .bind(attempt.effect_id)
        .bind(preparation)
        .bind(receipt)
        .bind(version + attempt.expected_version_delta)
        .bind(&attempt.response_model)
        .bind((0..attempt.output_count).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
        .bind(frames)
        .bind(&attempt.dimensions)
        .fetch_one(&mut *transaction)
        .await
    }
    .await;
        match result {
            Ok(prepared) => {
                transaction.commit().await?;
                Ok(prepared)
            }
            Err(error) => {
                transaction.rollback().await.unwrap();
                Err(error)
            }
        }
    }
}

#[test]
fn embedding_index_provisioner_uses_the_same_closed_ddl() {
    let migration = include_str!("../../../migrations/0198_embedding_index_builds.sql");
    let provisioner = include_str!("../../../docker/postgres/init-runtime-role.sh");
    let ddl = migration
        .split("-- BEGIN INDEX DDL\n")
        .nth(1)
        .unwrap()
        .split("-- END INDEX DDL")
        .next()
        .unwrap()
        .trim();
    assert!(
        provisioner.contains(ddl),
        "runtime bridge and SQLx fallback must install byte-identical authority DDL"
    );
}
