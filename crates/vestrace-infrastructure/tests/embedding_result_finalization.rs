//! Persisted delivery publication through the real host vault and guarded SQL.
mod common;
use common::result_preparation_fixture::*;
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use uuid::Uuid;
use vestrace_application::{
    EmbeddingOutputKeyBinding, EmbeddingResultBoundOutput, EmbeddingResultCommitter,
    EmbeddingResultFinalizationAuthority, EmbeddingResultFinalizationProgress,
    EmbeddingResultFinalizationRepository, EmbeddingResultFinalizationService,
    EmbeddingResultPreparationId, MaterialKeyVault, VaultError,
};
use vestrace_domain::{
    ErasureReceipt, ExternalEffectId, IntentNonce, MaterialKeyId, VaultReceipt, ZeroizingDek,
};
use vestrace_infrastructure::postgres::{
    EmbeddingOutputHmacCommitter, PgEmbeddingResultFinalizationRepository, PgStore,
};

#[derive(Debug, PartialEq, Eq, FromRow)]
struct Facts {
    publications: i64,
    events: i64,
    bindings: i64,
    history: i64,
    generic_attachments: i64,
    live_materials: i64,
    live_intents: i64,
    live_projections: i64,
    commitments: i64,
    ordinary_references: i64,
    nonterminal_source_blockers: i64,
    job_state: String,
    job_version: i64,
    corpus_revision: i64,
    live_member_count: i64,
    generation_epoch: i64,
}
async fn facts(pool: &PgPool, f: &ResultFixture, p: Uuid) -> Facts {
    sqlx::query_as("SELECT \
        (SELECT count(*) FROM embedding_job_result_publications WHERE workspace_id=$1 AND preparation_id=$2) AS publications, \
        (SELECT count(*) FROM embedding_index_rebuild_events WHERE workspace_id=$1 AND space_registration_id=$4) AS events, \
        (SELECT count(*) FROM embedding_result_key_binding_receipts WHERE workspace_id=$1 AND preparation_id=$2) AS bindings, \
        (SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=$1 AND preparation_id=$2) AS history, \
        (SELECT count(*) FROM prepared_material_attachments a JOIN embedding_job_result_prepared_attachments h ON h.prepared_attachment_id=a.id WHERE h.workspace_id=$1 AND h.preparation_id=$2) AS generic_attachments, \
        (SELECT count(*) FROM content_materials m JOIN embedding_projection_entries p ON p.material_id=m.id AND p.workspace_id=m.workspace_id WHERE p.workspace_id=$1 AND p.preparation_id=$2 AND m.state='live') AS live_materials, \
        (SELECT count(*) FROM material_key_creation_intents i JOIN embedding_projection_entries p ON p.intent_id=i.id AND p.workspace_id=i.workspace_id WHERE p.workspace_id=$1 AND p.preparation_id=$2 AND i.state='live') AS live_intents, \
        (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2 AND state='live' AND retention_eligibility_state='blocked_pending_erasure_propagation') AS live_projections, \
        (SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=$1 AND preparation_id=$2 AND octet_length(output_commitment)=32) AS commitments, \
        (SELECT count(*) FROM content_material_ordinary_references r JOIN embedding_projection_entries p ON p.material_id=r.material_id AND p.workspace_id=r.workspace_id WHERE p.workspace_id=$1 AND p.preparation_id=$2) AS ordinary_references, \
        (SELECT count(*) FROM embedding_delivery_source_memberships s JOIN material_erasure_blockers b ON b.id=s.blocker_id AND b.workspace_id=s.workspace_id WHERE s.workspace_id=$1 AND s.job_id=$3 AND b.state='nonterminal') AS nonterminal_source_blockers, \
        (SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$3) AS job_state, \
        (SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$3) AS job_version, \
        (SELECT corpus_revision FROM embedding_space_corpus_states WHERE workspace_id=$1 AND space_registration_id=$4) AS corpus_revision, \
        (SELECT live_member_count FROM embedding_space_corpus_states WHERE workspace_id=$1 AND space_registration_id=$4) AS live_member_count, \
        (SELECT generation_epoch FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$4) AS generation_epoch")
        .bind(f.accepted.context.workspace_id.as_uuid()).bind(p).bind(f.accepted.job_id.as_uuid()).bind(f.accepted.space_registration_id)
        .fetch_one(pool).await.unwrap()
}
async fn history(pool: &PgPool, p: Uuid) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(h) ORDER BY output_ordinal) FROM embedding_job_result_prepared_attachments h WHERE preparation_id=$1")
        .bind(p).fetch_one(pool).await.unwrap()
}

#[derive(FromRow)]
struct PublishedOutput {
    output_ordinal: i64,
    projection_id: Uuid,
    intent_id: Uuid,
    material_id: Uuid,
    material_key_id: Uuid,
    intent_nonce: Uuid,
    binding_receipt: Uuid,
    ciphertext: Vec<u8>,
    output_commitment: Vec<u8>,
}
async fn assert_exact_commitments(
    pool: &PgPool,
    fixture: &ResultFixture,
    authority: &EmbeddingResultFinalizationAuthority,
) {
    let rows: Vec<PublishedOutput> = sqlx::query_as("SELECT p.output_ordinal,p.id AS projection_id,p.intent_id,p.material_id,p.material_key_id,b.intent_nonce,b.binding_receipt,bytes.ciphertext,p.output_commitment FROM embedding_projection_entries p JOIN embedding_result_key_binding_receipts b ON b.workspace_id=p.workspace_id AND b.preparation_id=p.preparation_id AND b.projection_id=p.id JOIN content_material_bytes bytes ON bytes.workspace_id=p.workspace_id AND bytes.intent_id=p.intent_id WHERE p.preparation_id=$1 ORDER BY p.output_ordinal")
        .bind(authority.preparation_id.as_uuid()).fetch_all(pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    let vault = fixture.vault.vault(fixture.accepted.context.workspace_id);
    for row in rows {
        let output = EmbeddingResultBoundOutput {
            binding: EmbeddingOutputKeyBinding {
                workspace_id: fixture.accepted.context.workspace_id,
                job_id: authority.job_id,
                output_ordinal: u64::try_from(row.output_ordinal).unwrap(),
                intent_id: vestrace_domain::MaterialKeyCreationIntentId::from_uuid(row.intent_id),
                material_id: vestrace_domain::ContentMaterialId::from_uuid(row.material_id),
                key_id: MaterialKeyId::from_uuid(row.material_key_id),
                nonce: IntentNonce::from_uuid(row.intent_nonce),
            },
            projection_id: row.projection_id,
            receipt: vestrace_domain::MaterialKeyBindingReceipt::from_uuid(row.binding_receipt),
            ciphertext: row.ciphertext,
        };
        let mut computed = None;
        vault
            .with_bound_embedding_output_key(
                &output.binding,
                authority.preparation_id,
                output.receipt,
                &mut |dek| {
                    computed = Some(
                        EmbeddingOutputHmacCommitter::new()
                            .commitment(&fixture.accepted.context, authority, &output, dek)
                            .unwrap(),
                    );
                },
            )
            .unwrap();
        assert_eq!(
            computed.expect("exact bound callback must run").as_slice(),
            row.output_commitment.as_slice()
        );
    }
}
fn authority(f: &ResultFixture, p: Uuid) -> EmbeddingResultFinalizationAuthority {
    EmbeddingResultFinalizationAuthority {
        preparation_id: EmbeddingResultPreparationId::from_uuid(p),
        job_id: f.accepted.job_id,
        effect_id: ExternalEffectId::from_uuid(f.accepted.external_effect_id),
    }
}
struct NeverVault;
impl MaterialKeyVault for NeverVault {
    fn create_if_absent(
        &self,
        _: MaterialKeyId,
        _: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        panic!("published replay touched vault")
    }
    fn unwrap(&self, _: MaterialKeyId, _: &mut dyn FnMut(&ZeroizingDek)) -> Result<(), VaultError> {
        panic!("published replay touched vault")
    }
    fn prepare_erasure(
        &self,
        _: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        panic!("published replay touched vault")
    }
    fn erase(&self, _: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        panic!("published replay touched vault")
    }
    fn bind_embedding_output(
        &self,
        _: &vestrace_application::EmbeddingOutputKeyBinding,
        _: EmbeddingResultPreparationId,
    ) -> Result<vestrace_domain::MaterialKeyBindingReceipt, VaultError> {
        panic!("published replay touched vault")
    }
    fn with_bound_embedding_output_key(
        &self,
        _: &vestrace_application::EmbeddingOutputKeyBinding,
        _: EmbeddingResultPreparationId,
        _: vestrace_domain::MaterialKeyBindingReceipt,
        _: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        panic!("published replay touched vault")
    }
}
#[sqlx::test(migrations = false)]
async fn all_outputs_publish_with_one_corpus_event(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let a = authority(&f, p);
    let before = facts(&pool, &f, p).await;
    let immutable_history = history(&pool, p).await;
    assert_eq!(before.history, 2);
    assert_eq!(before.generic_attachments, 2);
    assert_eq!(before.live_materials, 0);
    assert_eq!(before.publications, 0);
    // This result fixture intentionally uses the legacy registration boundary.
    // A ready canonical generation requires the qualified canonical registration
    // exercised by embedding_canonical_generations, so this publication proves
    // the independent corpus/guard epoch update without inserting an invalid
    // ready legacy generation.
    let repo = Arc::new(PgEmbeddingResultFinalizationRepository::new(
        PgStore::from_pool(f.runtime.clone()),
    ));
    let publication = EmbeddingResultFinalizationService::new(
        repo.clone(),
        Arc::new(f.vault.vault(f.accepted.context.workspace_id)),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    )
    .finalize(&f.accepted.context, &a)
    .await
    .unwrap();
    let after = facts(&pool, &f, p).await;
    assert_eq!(
        after,
        Facts {
            publications: 1,
            events: 1,
            bindings: 2,
            history: 2,
            generic_attachments: 0,
            live_materials: 2,
            live_intents: 2,
            live_projections: 2,
            commitments: 2,
            ordinary_references: 0,
            nonterminal_source_blockers: 0,
            job_state: "succeeded".into(),
            job_version: before.job_version + 1,
            corpus_revision: before.corpus_revision + 1,
            live_member_count: before.live_member_count + 2,
            generation_epoch: before.generation_epoch + 1
        }
    );
    assert_exact_commitments(&pool, &f, &a).await;
    assert_eq!(
        history(&pool, p).await,
        immutable_history,
        "immutable attachment history survives generic attachment deletion"
    );
    assert_eq!(publication.output_count, 2);
    assert_eq!(publication.terminal_job_version, after.job_version as u64);
    assert_eq!(
        publication.resulting_corpus_revision,
        after.corpus_revision as u64
    );
    let invalidated_generation_count: i32 = sqlx::query_scalar(
        "SELECT cardinality(invalidated_generation_ids) \
         FROM embedding_index_rebuild_events WHERE publication_id=$1",
    )
    .bind(publication.publication_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(invalidated_generation_count, 0);
    let exact_event:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM embedding_index_rebuild_events e JOIN embedding_job_result_publications p ON p.id=e.publication_id AND p.rebuild_event_id=e.id AND p.workspace_id=e.workspace_id WHERE p.id=$1 AND e.after_corpus_revision=e.before_corpus_revision+1 AND e.after_live_member_count=e.before_live_member_count+2 AND e.after_generation_epoch=e.before_generation_epoch+1)").bind(publication.publication_id).fetch_one(&pool).await.unwrap();
    assert!(exact_event);
    let replay = EmbeddingResultFinalizationService::new(
        repo,
        Arc::new(NeverVault),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    )
    .finalize(&f.accepted.context, &a)
    .await
    .unwrap();
    assert_eq!(replay, publication);
    assert_eq!(facts(&pool, &f, p).await, after);
}

// Test-only faults alter one function in this disposable database, never migration
// files or privileges. The same persisted-state and primary-error oracles run
// with and without the selected fault.
struct PrimaryGateMutation {
    signature: &'static str,
    original: String,
    authority: (String, Option<String>, bool),
}

fn definition_sha256(value: &str) -> String {
    use std::fmt::Write;
    ring::digest::digest(&ring::digest::SHA256, value.as_bytes())
        .as_ref()
        .iter()
        .fold(String::new(), |mut output, byte| {
            write!(&mut output, "{byte:02X}").unwrap();
            output
        })
}

async fn function_authority(pool: &PgPool, signature: &str) -> (String, Option<String>, bool) {
    sqlx::query_as("SELECT pg_get_userbyid(proowner)::text, proacl::text, has_function_privilege('vestrace',oid,'EXECUTE') FROM pg_proc WHERE oid=$1::regprocedure")
        .bind(signature).fetch_one(pool).await.unwrap()
}

async fn install_primary_gate_mutant(pool: &PgPool, kind: &str) -> Option<PrimaryGateMutation> {
    if std::env::var("VESTRACE_TEST_FINALIZATION_PRIMARY_MUTANT")
        .ok()
        .as_deref()
        != Some(kind)
    {
        return None;
    }
    let (signature, gate) = match kind {
        "generic" => (
            "vestrace_finalize_bound_content_material(uuid)",
            "IF intent_row.owner_kind='embedding_job_output' THEN RAISE EXCEPTION 'embedding output requires specialized publication' USING ERRCODE='23514'; END IF;",
        ),
        "all_bound" => (
            "vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[])",
            "IF phase<>'ready_to_publish' THEN RAISE EXCEPTION 'publication requires all exact bindings' USING ERRCODE='23514'; END IF;",
        ),
        _ => panic!("unknown primary gate fault"),
    };
    let original: String = sqlx::query_scalar("SELECT pg_get_functiondef($1::regprocedure)")
        .bind(signature)
        .fetch_one(pool)
        .await
        .unwrap();
    let authority = function_authority(pool, signature).await;
    assert_eq!(original.matches(gate).count(), 1);
    let mutant = original.replacen(gate, "", 1);
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(&mutant).execute(&mut *tx).await.unwrap();
    let installed: String = sqlx::query_scalar("SELECT pg_get_functiondef($1::regprocedure)")
        .bind(signature)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(function_authority(pool, signature).await, authority);
    assert_eq!(installed.as_bytes(), mutant.as_bytes());
    assert_ne!(definition_sha256(&original), definition_sha256(&installed));
    eprintln!(
        "PRIMARY_GATE_MUTATION kind={kind} original_sha256={} mutant_sha256={}",
        definition_sha256(&original),
        definition_sha256(&installed)
    );
    Some(PrimaryGateMutation {
        signature,
        original,
        authority,
    })
}

async fn restore_primary_gate(pool: &PgPool, mutation: Option<PrimaryGateMutation>) {
    let Some(mutation) = mutation else { return };
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(&mutation.original)
        .execute(&mut *tx)
        .await
        .unwrap();
    let restored: String = sqlx::query_scalar("SELECT pg_get_functiondef($1::regprocedure)")
        .bind(mutation.signature)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let restored_authority = function_authority(pool, mutation.signature).await;
    assert_eq!(restored_authority, mutation.authority);
    eprintln!(
        "PRIMARY_GATE_AUTHORITY unchanged=true owner={} acl={:?} runtime_execute={}",
        restored_authority.0, restored_authority.1, restored_authority.2
    );
    assert_eq!(
        restored.as_bytes(),
        mutation.original.as_bytes(),
        "exact function definition restoration"
    );
    eprintln!(
        "PRIMARY_GATE_RESTORED signature={} restored_sha256={} byte_identical=true",
        mutation.signature,
        definition_sha256(&restored)
    );
}

fn assert_primary_refusal(error: sqlx::Error, kind: &str, message: &str) {
    let database = error.as_database_error().expect("guarded SQL refusal");
    eprintln!(
        "PRIMARY_GATE_ERROR kind={kind} sqlstate={} message={}",
        database.code().as_deref().unwrap_or("none"),
        database.message()
    );
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert_eq!(database.message(), message, "exact primary refusal");
}

#[sqlx::test(migrations = false)]
async fn partial_binding_preserves_history_and_stays_non_live(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let a = authority(&f, p);
    let initial = history(&pool, p).await;
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    let EmbeddingResultFinalizationProgress::NeedsBinding(bindings) =
        repo.load_progress(&f.accepted.context, &a).await.unwrap()
    else {
        panic!("fresh preparation must need bindings")
    };
    assert_eq!(bindings.len(), 2);
    let vault = f.vault.vault(f.accepted.context.workspace_id);
    let receipt = vault
        .bind_embedding_output(&bindings[0], a.preparation_id)
        .unwrap();
    assert_eq!(
        repo.record_binding(&f.accepted.context, &a, &bindings[0], &receipt)
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(
        repo.record_binding(&f.accepted.context, &a, &bindings[0], &receipt)
            .await
            .unwrap(),
        receipt,
        "partial exact record replay converges"
    );
    let state = facts(&pool, &f, p).await;
    assert_eq!(state.bindings, 1);
    assert_eq!(state.live_materials, 0);
    assert_eq!(state.live_intents, 0);
    assert_eq!(state.live_projections, 0);
    assert_eq!(state.publications, 0);
    assert_eq!(state.events, 0);
    assert_eq!(state.generic_attachments, 2);
    assert_eq!(state.nonterminal_source_blockers, 6);
    assert_eq!(history(&pool, p).await, initial);
    // Exercise SQL authority directly: the application never asks to publish a subset.
    let projections: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM embedding_projection_entries WHERE preparation_id=$1 ORDER BY output_ordinal")
        .bind(p).fetch_all(&pool).await.unwrap();
    let mutation = install_primary_gate_mutant(&pool, "all_bound").await;
    let mut premature = f.runtime.begin().await.unwrap();
    scoped(&mut premature, f.accepted.context.workspace_id.as_uuid()).await;
    let attempted =
        sqlx::query("SELECT vestrace_publish_embedding_job_result($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(f.accepted.context.workspace_id.as_uuid())
            .bind(p)
            .bind(a.job_id.as_uuid())
            .bind(a.effect_id.as_uuid())
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .bind(projections)
            .bind(vec![0_i64, 1])
            .bind(vec![vec![17_u8; 32], vec![18_u8; 32]])
            .execute(&mut *premature)
            .await;
    let attempted = match attempted {
        Ok(_) => premature.commit().await,
        Err(error) => {
            premature.rollback().await.unwrap();
            Err(error)
        }
    };
    let observed = facts(&pool, &f, p).await;
    let observed_history = history(&pool, p).await;
    restore_primary_gate(&pool, mutation).await;
    eprintln!(
        "PRIMARY_GATE_OBSERVER kind=all_bound facts_unchanged={} history_unchanged={} facts={observed:?}",
        observed == state,
        observed_history == initial
    );
    assert_eq!(
        observed, state,
        "partial publication must leave the complete persisted baseline unchanged"
    );
    assert_eq!(
        observed_history, initial,
        "partial publication must preserve complete history"
    );
    assert_primary_refusal(
        attempted.unwrap_err(),
        "all_bound",
        "publication requires all exact bindings",
    );
    let EmbeddingResultFinalizationProgress::NeedsBinding(missing) =
        repo.load_progress(&f.accepted.context, &a).await.unwrap()
    else {
        panic!("partial binding must remain resumable")
    };
    assert_eq!(missing, vec![bindings[1].clone()]);
    EmbeddingResultFinalizationService::new(
        Arc::new(repo),
        Arc::new(f.vault.vault(f.accepted.context.workspace_id)),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    )
    .finalize(&f.accepted.context, &a)
    .await
    .unwrap();
    assert_eq!(facts(&pool, &f, p).await.live_materials, 2);
    assert_eq!(history(&pool, p).await, initial);
}

async fn finish_fixture(
    f: &ResultFixture,
    p: Uuid,
) -> vestrace_application::EmbeddingResultPublication {
    let repo = Arc::new(PgEmbeddingResultFinalizationRepository::new(
        PgStore::from_pool(f.runtime.clone()),
    ));
    EmbeddingResultFinalizationService::new(
        repo,
        Arc::new(f.vault.vault(f.accepted.context.workspace_id)),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    )
    .finalize(&f.accepted.context, &authority(f, p))
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn publication_releases_only_job_owned_credential_blocker(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let first = result_fixture_with_pinned_credential(&pool).await;
    let p1 = Uuid::now_v7();
    commit_result(&first, p1, Uuid::now_v7()).await;
    let next =
        common::prepare_additional_delivery_embedding_job(&first.runtime, &first.accepted).await;
    let second = result_fixture_for_accepted(
        &pool,
        first.runtime.clone(),
        next,
        DeliveryPolicyCase::ExactAllowed,
        false,
    )
    .await;
    let p2 = Uuid::now_v7();
    commit_result(&second, p2, Uuid::now_v7()).await;
    let unrelated = first
        .accepted
        .credential
        .as_ref()
        .unwrap()
        .completion_blocker_id;
    let owned:Vec<(Uuid,Uuid,Uuid)>=sqlx::query_as("SELECT job_id,credential_intent_id,blocker_id FROM embedding_job_credential_completion_blockers WHERE workspace_id=$1 ORDER BY job_id")
        .bind(first.accepted.context.workspace_id.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(owned.len(), 2);
    assert_eq!(owned[0].1, owned[1].1);
    assert_ne!(owned[0].2, owned[1].2);
    assert!(owned.iter().all(|o| o.2 != unrelated));
    let first_owned = owned
        .iter()
        .find(|o| o.0 == first.accepted.job_id.as_uuid())
        .unwrap()
        .2;
    let second_owned = owned
        .iter()
        .find(|o| o.0 == second.accepted.job_id.as_uuid())
        .unwrap()
        .2;
    let first_publication = finish_fixture(&first, p1).await;
    let states: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id,state FROM material_erasure_blockers WHERE id=ANY($1) ORDER BY id",
    )
    .bind(vec![first_owned, second_owned, unrelated])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(states.len(), 3);
    for (id, state) in states {
        assert_eq!(
            state,
            if id == first_owned {
                "terminal"
            } else {
                "nonterminal"
            }
        );
    }
    assert_eq!(facts(&pool, &second, p2).await.live_materials, 0);
    let second_publication = finish_fixture(&second, p2).await;
    assert_eq!(
        second_publication.resulting_corpus_revision,
        first_publication.resulting_corpus_revision + 1
    );
    assert_eq!(
        second_publication.live_member_count,
        first_publication.live_member_count + 2
    );
    assert_eq!(
        finish_fixture(&first, p1).await,
        first_publication,
        "historical replay survives later same-space publication"
    );
    let unrelated_state: String =
        sqlx::query_scalar("SELECT state FROM material_erasure_blockers WHERE id=$1")
            .bind(unrelated)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unrelated_state, "nonterminal");
}

#[sqlx::test(migrations = false)]
async fn legacy_marker_adoption_never_transfers_old_blocker(pool: PgPool) {
    provision_result_behavior_database_through(&pool, 194).await;
    let f = result_fixture_with_pinned_credential(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let old: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM embedding_job_result_preparations p WHERE id=$1",
    )
    .bind(p)
    .fetch_one(&pool)
    .await
    .unwrap();
    let historical =
        Uuid::parse_str(old["credential_erasure_blocker_id"].as_str().unwrap()).unwrap();
    vestrace_infrastructure::HISTORICAL_MIGRATOR
        .run(&f.runtime)
        .await
        .unwrap();
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    let a = authority(&f, p);
    assert!(matches!(
        repo.load_progress(&f.accepted.context, &a).await.unwrap(),
        EmbeddingResultFinalizationProgress::NeedsBinding(_)
    ));
    let adopted:(Uuid,Uuid)=sqlx::query_as("SELECT historical_blocker_id,owned_blocker_id FROM embedding_result_credential_blocker_adoptions WHERE workspace_id=$1 AND preparation_id=$2").bind(f.accepted.context.workspace_id.as_uuid()).bind(p).fetch_one(&pool).await.unwrap();
    assert_eq!(adopted.0, historical);
    assert_ne!(adopted.1, historical);
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    let replay: Uuid = sqlx::query_scalar(
        "SELECT vestrace_adopt_embedding_result_credential_blocker($1,$2,$3,$4)",
    )
    .bind(f.accepted.context.workspace_id.as_uuid())
    .bind(p)
    .bind(f.accepted.job_id.as_uuid())
    .bind(f.accepted.external_effect_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(replay, adopted.1);
    let unchanged: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM embedding_job_result_preparations p WHERE id=$1",
    )
    .bind(p)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(old, unchanged);
    finish_fixture(&f, p).await;
    let old_state: String =
        sqlx::query_scalar("SELECT state FROM material_erasure_blockers WHERE id=$1")
            .bind(historical)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old_state, "nonterminal");
    let own_state: String =
        sqlx::query_scalar("SELECT state FROM material_erasure_blockers WHERE id=$1")
            .bind(adopted.1)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(own_state, "terminal");
}

#[sqlx::test(migrations = false)]
async fn legacy_adoption_revocation_first_refuses_without_partial_owner(pool: PgPool) {
    provision_result_behavior_database_through(&pool, 194).await;
    let f = result_fixture_with_pinned_credential(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let credential = f.accepted.credential.as_ref().unwrap();
    let (intent,guard,version):(Uuid,Uuid,i64)=sqlx::query_as("SELECT i.id,g.id,s.current_revision_version FROM credential_key_creation_intents i JOIN connection_execution_guards g ON g.workspace_id=i.workspace_id AND g.connection_id=i.connection_id JOIN credential_slots s ON s.workspace_id=i.workspace_id AND s.id=i.credential_slot_id WHERE i.workspace_id=$1 AND i.credential_revision_id=$2").bind(f.accepted.context.workspace_id.as_uuid()).bind(credential.revision_id).fetch_one(&pool).await.unwrap();
    let audit = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES($1,$2,$3,'credential.fixture.revoked','credential',$4,'{}',NOW())").bind(audit).bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.context.principal_id.as_uuid()).bind(credential.revision_id).execute(&pool).await.unwrap();
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("SELECT vestrace_revoke_credential($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(f.accepted.context.workspace_id.as_uuid())
        .bind(f.accepted.connection_id)
        .bind(guard)
        .bind(credential.activation_guard_id)
        .bind(credential.slot_id)
        .bind(credential.revision_id)
        .bind(intent)
        .bind(audit)
        .bind(version)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    vestrace_infrastructure::HISTORICAL_MIGRATOR
        .run(&f.runtime)
        .await
        .unwrap();
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM embedding_job_result_preparations p WHERE id=$1",
    )
    .bind(p)
    .fetch_one(&pool)
    .await
    .unwrap();
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    assert!(
        repo.load_progress(&f.accepted.context, &authority(&f, p))
            .await
            .is_err()
    );
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM embedding_job_credential_completion_blockers WHERE workspace_id=$1),(SELECT count(*) FROM embedding_result_credential_blocker_adoptions WHERE workspace_id=$1)").bind(f.accepted.context.workspace_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM embedding_job_result_preparations p WHERE id=$1",
    )
    .bind(p)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
}

async fn named_actor(runtime: &PgPool, name: &str) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            runtime
                .connect_options()
                .as_ref()
                .clone()
                .application_name(name),
        )
        .await
        .unwrap()
}
async fn observe_blocked_actor(pool: &PgPool, name: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock' AND cardinality(pg_blocking_pids(pid))>0)").bind(name).fetch_one(pool).await.unwrap();
        if blocked {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actor must be observed blocked, timeout is not proof: {name}"
        );
        tokio::task::yield_now().await;
    }
}
#[sqlx::test(migrations = false)]
async fn concurrent_finalizers_converge_on_one_publication(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let before = facts(&pool, &f, p).await;
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM connection_execution_guards WHERE workspace_id=$1 AND connection_id=$2 FOR UPDATE").bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.connection_id).fetch_one(&mut *gate).await.unwrap();
    let first = EmbeddingResultFinalizationService::new(
        Arc::new(PgEmbeddingResultFinalizationRepository::new(
            PgStore::from_pool(named_actor(&f.runtime, "14e-first-finalizer").await),
        )),
        Arc::new(f.vault.vault(f.accepted.context.workspace_id)),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    );
    let c1 = f.accepted.context.clone();
    let a1 = authority(&f, p);
    let one = tokio::spawn(async move { first.finalize(&c1, &a1).await });
    observe_blocked_actor(&pool, "14e-first-finalizer").await;
    let second = EmbeddingResultFinalizationService::new(
        Arc::new(PgEmbeddingResultFinalizationRepository::new(
            PgStore::from_pool(named_actor(&f.runtime, "14e-second-finalizer").await),
        )),
        Arc::new(f.vault.vault(f.accepted.context.workspace_id)),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    );
    let c2 = f.accepted.context.clone();
    let a2 = authority(&f, p);
    let two = tokio::spawn(async move { second.finalize(&c2, &a2).await });
    observe_blocked_actor(&pool, "14e-second-finalizer").await;
    assert_eq!(facts(&pool, &f, p).await, before);
    gate.rollback().await.unwrap();
    let first = one.await.unwrap().unwrap();
    let second = two.await.unwrap().unwrap();
    assert_eq!(first, second);
    let after = facts(&pool, &f, p).await;
    assert_eq!(after.publications, 1);
    assert_eq!(after.events, 1);
    assert_eq!(after.live_materials, 2);
    assert_eq!(after.job_version, before.job_version + 1);
}

#[sqlx::test(migrations = false)]
async fn finalizers_serialize_with_source_erasure_in_both_orders(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    for erasure_first in [true, false] {
        let f = result_fixture(&pool).await;
        let p = Uuid::now_v7();
        commit_result(&f, p, Uuid::now_v7()).await;
        let ready_commitments = bind_all_and_compute(&f, p).await;
        let source = f.sources.iter().map(|s| s.as_uuid()).min().unwrap();
        let workspace = f.accepted.context.workspace_id.as_uuid();
        // A read-only row lock queues the real runtime commands deterministically.
        let mut gate = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM content_materials WHERE id=$1 FOR UPDATE")
            .bind(source)
            .fetch_one(&mut *gate)
            .await
            .unwrap();
        let erase_pool = named_actor(&f.runtime, "14e-source-eraser").await;
        let erasure = async move {
            let mut tx = erase_pool.begin().await.unwrap();
            scoped(&mut tx, workspace).await;
            let result = sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
                .bind(source)
                .fetch_all(&mut *tx)
                .await;
            match result {
                Ok(_) => {
                    tx.commit().await.unwrap();
                    Ok(())
                }
                Err(e) => {
                    tx.rollback().await.unwrap();
                    Err(e)
                }
            }
        };
        let repository = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(
            named_actor(&f.runtime, "14e-source-finalizer").await,
        ));
        let context = f.accepted.context.clone();
        let a = authority(&f, p);
        let finalizer = async move {
            repository
                .publish(
                    &context,
                    &a,
                    Uuid::now_v7(),
                    Uuid::now_v7(),
                    ready_commitments,
                )
                .await
        };
        let (erase_task, finalize_task) = if erasure_first {
            let erase_task = tokio::spawn(erasure);
            observe_blocked_actor(&pool, "14e-source-eraser").await;
            let finalize_task = tokio::spawn(finalizer);
            observe_blocked_actor(&pool, "14e-source-finalizer").await;
            (erase_task, finalize_task)
        } else {
            let finalize_task = tokio::spawn(finalizer);
            observe_blocked_actor(&pool, "14e-source-finalizer").await;
            let erase_task = tokio::spawn(erasure);
            observe_blocked_actor(&pool, "14e-source-eraser").await;
            (erase_task, finalize_task)
        };
        assert_eq!(facts(&pool, &f, p).await.live_materials, 0);
        gate.rollback().await.unwrap();
        let erased = erase_task.await.unwrap();
        let publication = finalize_task.await.unwrap().unwrap();
        assert_eq!(publication.output_count, 2);
        if erasure_first {
            let error = erased.unwrap_err();
            assert_eq!(
                error.as_database_error().and_then(|e| e.code()).as_deref(),
                Some("23514")
            );
        } else {
            erased.unwrap();
        }
        let persisted = facts(&pool, &f, p).await;
        assert_eq!(persisted.publications, 1);
        assert_eq!(persisted.live_materials, 2);
        assert_eq!(persisted.generic_attachments, 0);
        assert_eq!(
            finish_fixture(&f, p).await,
            publication,
            "published historical replay survives source state changes"
        );
    }
}

async fn bind_all_and_compute(
    f: &ResultFixture,
    p: Uuid,
) -> Vec<vestrace_application::EmbeddingOutputCommitment> {
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    let a = authority(f, p);
    let vault = f.vault.vault(f.accepted.context.workspace_id);
    let EmbeddingResultFinalizationProgress::NeedsBinding(bindings) =
        repo.load_progress(&f.accepted.context, &a).await.unwrap()
    else {
        panic!("fresh preparation must need binding")
    };
    for binding in bindings {
        let receipt = vault
            .bind_embedding_output(&binding, a.preparation_id)
            .unwrap();
        repo.record_binding(&f.accepted.context, &a, &binding, &receipt)
            .await
            .unwrap();
    }
    let EmbeddingResultFinalizationProgress::ReadyToPublish(outputs) =
        repo.load_progress(&f.accepted.context, &a).await.unwrap()
    else {
        panic!("complete receipts must permit publication")
    };
    outputs
        .iter()
        .map(|output| {
            let mut computed = None;
            vault
                .with_bound_embedding_output_key(
                    &output.binding,
                    a.preparation_id,
                    output.receipt,
                    &mut |dek| {
                        computed = Some(
                            EmbeddingOutputHmacCommitter::new()
                                .commitment(&f.accepted.context, &a, output, dek)
                                .unwrap(),
                        );
                    },
                )
                .unwrap();
            vestrace_application::EmbeddingOutputCommitment {
                projection_id: output.projection_id,
                output_ordinal: output.binding.output_ordinal,
                commitment: computed.unwrap(),
            }
        })
        .collect()
}

#[sqlx::test(migrations = false)]
async fn publication_preserves_exact_attachment_history(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let before = history(&pool, p).await;
    // Replacing only the generic attachment id preserves all P02 intent/byte
    // counts. The195 phase invariant independently protects immutable history.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    let stale_id: Uuid=sqlx::query_scalar("SELECT prepared_attachment_id FROM embedding_job_result_prepared_attachments WHERE preparation_id=$1 AND output_ordinal=0").bind(p).fetch_one(&mut *tx).await.unwrap();
    let replacement = Uuid::now_v7();
    sqlx::query("UPDATE prepared_material_attachments SET id=$1 WHERE id=$2")
        .bind(replacement)
        .bind(stale_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let result = tx.commit().await;
    let dangling: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM prepared_material_attachments WHERE id=$1)",
    )
    .bind(stale_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !dangling,
        "unsafe persisted state: immutable attachment history points at missing generic attachment"
    );
    assert_eq!(
        result
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23514")
    );
    assert_eq!(history(&pool, p).await, before);
}

#[sqlx::test(migrations = false)]
async fn generic_finalizer_cannot_publish_bound_embedding_output(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let a = authority(&f, p);
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    let EmbeddingResultFinalizationProgress::NeedsBinding(outputs) =
        repo.load_progress(&f.accepted.context, &a).await.unwrap()
    else {
        panic!("prepared phase")
    };
    let binding = &outputs[0];
    let receipt = f
        .vault
        .vault(f.accepted.context.workspace_id)
        .bind_embedding_output(binding, a.preparation_id)
        .unwrap();
    repo.record_binding(&f.accepted.context, &a, binding, &receipt)
        .await
        .unwrap();
    let baseline = facts(&pool, &f, p).await;
    let initial = history(&pool, p).await;
    assert_eq!(
        (
            baseline.live_materials,
            baseline.live_intents,
            baseline.live_projections,
            baseline.ordinary_references,
            baseline.publications,
            baseline.events
        ),
        (0, 0, 0, 0, 0, 0)
    );
    let mutation = install_primary_gate_mutant(&pool, "generic").await;
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    let result = sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(binding.intent_id.as_uuid())
        .execute(&mut *tx)
        .await;
    let result = match result {
        Ok(_) => tx.commit().await,
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    };
    let observed = facts(&pool, &f, p).await;
    let observed_history = history(&pool, p).await;
    restore_primary_gate(&pool, mutation).await;
    eprintln!(
        "PRIMARY_GATE_OBSERVER kind=generic facts_unchanged={} history_unchanged={} facts={observed:?}",
        observed == baseline,
        observed_history == initial
    );
    assert_eq!(
        observed, baseline,
        "generic finalization must leave the complete persisted baseline unchanged"
    );
    assert_eq!(
        observed_history, initial,
        "generic finalization must preserve complete history"
    );
    assert_primary_refusal(
        result.unwrap_err(),
        "generic",
        "embedding output requires specialized publication",
    );
}

#[sqlx::test(migrations = false)]
async fn published_direct_replay_requires_the_exact_commitment_tuple(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let commitments = bind_all_and_compute(&f, p).await;
    let a = authority(&f, p);
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    let winner = repo
        .publish(
            &f.accepted.context,
            &a,
            Uuid::now_v7(),
            Uuid::now_v7(),
            commitments.clone(),
        )
        .await
        .unwrap();
    let before = facts(&pool, &f, p).await;
    assert_eq!(
        repo.publish(
            &f.accepted.context,
            &a,
            Uuid::now_v7(),
            Uuid::now_v7(),
            commitments.clone()
        )
        .await
        .unwrap(),
        winner
    );
    for field in 0..3 {
        let mut changed = commitments.clone();
        match field {
            0 => changed[0].commitment[0] ^= 1,
            1 => changed[0].projection_id = Uuid::now_v7(),
            _ => changed[0].output_ordinal = 1,
        }
        assert!(
            repo.publish(
                &f.accepted.context,
                &a,
                Uuid::now_v7(),
                Uuid::now_v7(),
                changed
            )
            .await
            .is_err(),
            "changed immutable publication commitment tuple accepted"
        );
        assert_eq!(facts(&pool, &f, p).await, before);
    }
    assert_eq!(finish_fixture(&f, p).await, winner);
}

async fn revoke_pinned_credential(pool: &PgPool, f: &ResultFixture) -> Uuid {
    let credential = f.accepted.credential.as_ref().unwrap();
    let(intent,guard,version):(Uuid,Uuid,i64)=sqlx::query_as("SELECT i.id,g.id,s.current_revision_version FROM credential_key_creation_intents i JOIN connection_execution_guards g ON g.workspace_id=i.workspace_id AND g.connection_id=i.connection_id JOIN credential_slots s ON s.workspace_id=i.workspace_id AND s.id=i.credential_slot_id WHERE i.workspace_id=$1 AND i.credential_revision_id=$2").bind(f.accepted.context.workspace_id.as_uuid()).bind(credential.revision_id).fetch_one(pool).await.unwrap();
    let audit = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES($1,$2,$3,'credential.fixture.revoked','credential',$4,'{}',NOW())").bind(audit).bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.context.principal_id.as_uuid()).bind(credential.revision_id).execute(pool).await.unwrap();
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("SELECT vestrace_revoke_credential($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(f.accepted.context.workspace_id.as_uuid())
        .bind(f.accepted.connection_id)
        .bind(guard)
        .bind(credential.activation_guard_id)
        .bind(credential.slot_id)
        .bind(credential.revision_id)
        .bind(intent)
        .bind(audit)
        .bind(version)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    intent
}
#[sqlx::test(migrations = false)]
async fn legacy_adoption_erasure_first_after_observed_lease_expiry_refuses(pool: PgPool) {
    provision_result_behavior_database_through(&pool, 194).await;
    let runtime = common::runtime_pool(&pool).await;
    let accepted =
        common::prepare_legacy_delivery_with_expiring_credential_lease(&pool, &runtime).await;
    let f = result_fixture_for_accepted(
        &pool,
        runtime,
        accepted,
        DeliveryPolicyCase::ExactAllowed,
        true,
    )
    .await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let blocker = f
        .accepted
        .credential
        .as_ref()
        .unwrap()
        .completion_blocker_id;
    let valid:bool=sqlx::query_scalar("SELECT state='nonterminal' AND blocker_kind='lease' AND usable_until>clock_timestamp() FROM material_erasure_blockers WHERE id=$1").bind(blocker).fetch_one(&pool).await.unwrap();
    assert!(
        valid,
        "the original194marker must commit under an actually unexpired credential lease"
    );
    let marker: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM embedding_job_result_preparations p WHERE id=$1",
    )
    .bind(p)
    .fetch_one(&pool)
    .await
    .unwrap();
    // The immutable marker was created under0194; repair the schema before
    // erasure wins. No claim that unmodified0194 could commit this erasure.
    vestrace_infrastructure::HISTORICAL_MIGRATOR
        .run(&f.runtime)
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let expired: bool = sqlx::query_scalar(
            "SELECT usable_until<=clock_timestamp() FROM material_erasure_blockers WHERE id=$1",
        )
        .bind(blocker)
        .fetch_one(&pool)
        .await
        .unwrap();
        if expired {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "database lease expiry must be observed"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let intent = revoke_pinned_credential(&pool, &f).await;
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("SELECT * FROM vestrace_prepare_retired_or_revoked_credential_erasure($1)")
        .bind(intent)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let erased: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM material_erasure_preparations WHERE credential_intent_id=$1)",
    )
    .bind(intent)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        erased,
        "real guarded phase-one erasure must have committed before adoption"
    );
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    assert!(
        repo.load_progress(&f.accepted.context, &authority(&f, p))
            .await
            .is_err()
    );
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM embedding_job_credential_completion_blockers WHERE workspace_id=$1),(SELECT count(*) FROM embedding_result_credential_blocker_adoptions WHERE workspace_id=$1)").bind(f.accepted.context.workspace_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
    let unchanged: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM embedding_job_result_preparations p WHERE id=$1",
    )
    .bind(p)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(marker, unchanged);
    // This lifecycle refusal is also protected by expired historical evidence;
    // it is not claimed as a mutation-sensitive isolated active-state probe.
}

struct NoReplaySealer(std::sync::atomic::AtomicUsize);
impl vestrace_application::EmbeddingResultSealer for NoReplaySealer {
    fn seal_embedding_vector(
        &self,
        _: &vestrace_application::EmbeddingOutputKeyBinding,
        _: &ZeroizingDek,
        _: &[u8],
    ) -> Result<Vec<u8>, vestrace_application::ApplicationError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(vestrace_application::ApplicationError::Unavailable(
            "replay sealer unavailable".into(),
        ))
    }
}
async fn preparation_dispatch_authority(
    pool: &PgPool,
    f: &ResultFixture,
) -> vestrace_application::ProviderDispatchAuthority {
    let row:(Uuid,Uuid,Uuid,chrono::DateTime<chrono::Utc>)=sqlx::query_as("SELECT a.id,l.id,t.id,t.dispatch_expires_at FROM external_effect_authorizations a JOIN provider_concurrency_leases l ON l.workspace_id=a.workspace_id AND l.external_effect_id=a.effect_id JOIN external_effect_lifecycle_transitions t ON t.workspace_id=a.workspace_id AND t.effect_id=a.effect_id WHERE a.workspace_id=$1 AND a.effect_id=$2 AND t.status='dispatching' AND t.cause='dispatch_started' ORDER BY t.ordinal DESC LIMIT 1").bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.external_effect_id).fetch_one(pool).await.unwrap();
    vestrace_application::ProviderDispatchAuthority {
        effect_id: ExternalEffectId::from_uuid(f.accepted.external_effect_id),
        authorization_id: vestrace_domain::id::PolicyDecisionId::from_uuid(row.0),
        connection_id: vestrace_domain::ConnectionId::from_uuid(f.accepted.connection_id),
        connection_revision_id: vestrace_domain::ConnectionRevisionId::from_uuid(
            f.accepted.connection_revision_id,
        ),
        concurrency_lease_id: row.1,
        credential_lease_id: None,
        dispatch_transition_id: vestrace_domain::ExternalEffectLifecycleTransitionId::from_uuid(
            row.2,
        ),
        dispatch_expires_at: row.3,
    }
}
#[sqlx::test(migrations = false)]
async fn preparation_replay_and_publication_serialize_in_both_orders_without_reseal(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    for preparation_first in [true, false] {
        let f = result_fixture(&pool).await;
        let p = Uuid::now_v7();
        commit_result(&f, p, Uuid::now_v7()).await;
        let commitments = bind_all_and_compute(&f, p).await;
        let dispatch = preparation_dispatch_authority(&pool, &f).await;
        let before = facts(&pool, &f, p).await;
        let prep_pool = named_actor(&f.runtime, "14e-preparation-race").await;
        let repo = vestrace_infrastructure::postgres::PgEmbeddingResultRepository::new(
            PgStore::from_pool(prep_pool.clone()),
            Arc::new(common::dispatch_repository(&prep_pool)),
        );
        let sealer = Arc::new(NoReplaySealer(std::sync::atomic::AtomicUsize::new(0)));
        let service = vestrace_application::EmbeddingResultPreparationService::new(
            Arc::new(repo),
            Arc::new(NeverVault),
            sealer.clone(),
        );
        let context = f.accepted.context.clone();
        let prep_authority = vestrace_application::EmbeddingResultDispatchAuthority {
            job_id: f.accepted.job_id,
            effect_id: ExternalEffectId::from_uuid(f.accepted.external_effect_id),
            dispatch,
        };
        let identities = vestrace_application::EmbeddingResultPreparationIdentities {
            preparation_id: EmbeddingResultPreparationId::new(),
            receipt_id: vestrace_domain::ExternalEffectReceiptId::new(),
            attachments: f
                .outputs
                .iter()
                .map(
                    |o| vestrace_application::EmbeddingResultPreparedAttachment {
                        output_ordinal: o.output_ordinal,
                        intent_id: o.intent_id,
                        attachment_id: vestrace_domain::PreparedMaterialAttachmentId::new(),
                    },
                )
                .collect(),
        };
        let vectors = (0..2)
            .map(|ordinal| {
                vestrace_application::GovernedEmbeddingVector::from_provider_components(
                    ordinal,
                    vec![0.25; 768],
                )
                .unwrap()
            })
            .collect();
        let response = vestrace_application::GovernedEmbeddingsResponse::new(
            RESULT_MODEL,
            RESULT_MODEL.into(),
            vectors,
            2,
        )
        .unwrap();
        let preparation = async move {
            service
                .prepare(context, prep_authority, identities, response)
                .await
        };
        let pub_repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(
            named_actor(&f.runtime, "14e-publication-race").await,
        ));
        let context = f.accepted.context.clone();
        let a = authority(&f, p);
        let publication = async move {
            pub_repo
                .publish(&context, &a, Uuid::now_v7(), Uuid::now_v7(), commitments)
                .await
        };
        let mut gate = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM connection_execution_guards WHERE workspace_id=$1 AND connection_id=$2 FOR UPDATE").bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.connection_id).fetch_one(&mut *gate).await.unwrap();
        let (prep_task, pub_task) = if preparation_first {
            let prep_task = tokio::spawn(preparation);
            observe_blocked_actor(&pool, "14e-preparation-race").await;
            let pub_task = tokio::spawn(publication);
            observe_blocked_actor(&pool, "14e-publication-race").await;
            (prep_task, pub_task)
        } else {
            let pub_task = tokio::spawn(publication);
            observe_blocked_actor(&pool, "14e-publication-race").await;
            let prep_task = tokio::spawn(preparation);
            observe_blocked_actor(&pool, "14e-preparation-race").await;
            (prep_task, pub_task)
        };
        assert_eq!(facts(&pool, &f, p).await, before);
        gate.rollback().await.unwrap();
        assert_eq!(
            prep_task.await.unwrap().unwrap(),
            vestrace_application::EmbeddingResultPreparationOutcome::ConvergedExisting {
                preparation_id: EmbeddingResultPreparationId::from_uuid(p)
            }
        );
        assert_eq!(pub_task.await.unwrap().unwrap().output_count, 2);
        assert_eq!(sealer.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        let dispatch_counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM external_effect_lifecycle_transitions WHERE effect_id=$1 AND status='dispatching'),(SELECT count(*) FROM external_effect_receipts WHERE effect_id=$1),(SELECT count(*) FROM embedding_job_result_preparations WHERE external_effect_id=$1)").bind(f.accepted.external_effect_id).fetch_one(&pool).await.unwrap();
        assert_eq!(
            dispatch_counts,
            (1, 1, 1),
            "replay cannot create another provider dispatch or response tuple"
        );
        let after = facts(&pool, &f, p).await;
        assert_eq!(after.publications, 1);
        assert_eq!(after.events, 1);
        assert_eq!(after.job_version, before.job_version + 1);
    }
}

#[sqlx::test(migrations = false)]
async fn legacy_rotation_is_refused_without_embedding_transition_authority(pool: PgPool) {
    provision_result_behavior_database_through(&pool, 194).await;
    let f = result_fixture_with_pinned_credential(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let old = f.accepted.credential.as_ref().unwrap();
    let occupancy = Uuid::now_v7();
    let revision = Uuid::now_v7();
    let intent = Uuid::now_v7();
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1,$2,$3,$4)")
        .bind(occupancy)
        .bind(f.accepted.context.workspace_id.as_uuid())
        .bind(f.accepted.connection_id)
        .bind(old.slot_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_key_creation_intent($1,$2,$3,$4,$5,$6,$7,$8,'credential_v2')").bind(intent).bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.connection_id).bind(old.slot_id).bind(occupancy).bind(revision).bind(Uuid::now_v7()).bind(Uuid::now_v7()).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
        .bind(intent)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1,$2)")
        .bind(intent)
        .bind(Uuid::now_v7())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_create_credential_prepared_material($1,$2,$3)")
        .bind(intent)
        .bind(Uuid::now_v7())
        .bind(b"rotation-fixture-ciphertext".as_slice())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1,$2)")
        .bind(intent)
        .bind(Uuid::now_v7())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
        .bind(intent)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let guard: Uuid = sqlx::query_scalar(
        "SELECT id FROM connection_execution_guards WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(f.accepted.context.workspace_id.as_uuid())
    .bind(f.accepted.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let audit = Uuid::now_v7();
    sqlx::query("INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) VALUES($1,$2,$3,'credential.fixture.rotation','credential',$4,'{}',NOW())").bind(audit).bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.context.principal_id.as_uuid()).bind(revision).execute(&pool).await.unwrap();
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    let error =
        sqlx::query("SELECT vestrace_rotate_credential($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1::BIGINT)")
            .bind(f.accepted.context.workspace_id.as_uuid())
            .bind(f.accepted.connection_id)
            .bind(guard)
            .bind(old.activation_guard_id)
            .bind(old.slot_id)
            .bind(old.revision_id)
            .bind(revision)
            .bind(intent)
            .bind(f.accepted.connection_qualification_id)
            .bind(audit)
            .execute(&mut *tx)
            .await
            .unwrap_err();
    tx.rollback().await.unwrap();
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert!(
        database
            .message()
            .contains("rotation requires P04 embedding transition evidence")
    );
    let unchanged:(Uuid,String)=sqlx::query_as("SELECT s.current_revision_id,i.state FROM credential_slots s JOIN credential_key_creation_intents i ON i.workspace_id=s.workspace_id AND i.credential_slot_id=s.id WHERE s.id=$1 AND i.id=$2").bind(old.slot_id).bind(intent).fetch_one(&pool).await.unwrap();
    assert_eq!(unchanged, (old.revision_id, "candidate".into()));
    // A successful rotation-first adoption scenario is unavailable until the
    // embedding transition authority exists; this probe preserves that gate.
}

#[sqlx::test(migrations = false)]
async fn publication_defensively_refuses_a_corrupted_non_live_source(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let commitments = bind_all_and_compute(&f, p).await;
    let before = facts(&pool, &f, p).await;
    let workspace = f.accepted.context.workspace_id.as_uuid();
    let source = f.sources[0].as_uuid();
    // This is malformed-state defensive evidence, NOT a legal erasure winner.
    // As in the 14D corruption probe, the real lifecycle command first refuses.
    let mut erasure = f.runtime.begin().await.unwrap();
    scoped(&mut erasure, workspace).await;
    let refused = sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
        .bind(source)
        .execute(&mut *erasure)
        .await
        .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514")
    );
    erasure.rollback().await.unwrap();
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, workspace).await;
    sqlx::query("ALTER TABLE content_materials DISABLE TRIGGER USER")
        .execute(&mut *owner)
        .await
        .unwrap();
    let changed = sqlx::query("UPDATE content_materials SET state='erasure_prepared' WHERE workspace_id=$1 AND id=$2 AND state='live'")
        .bind(workspace).bind(source).execute(&mut *owner).await.unwrap();
    assert_eq!(changed.rows_affected(), 1);
    sqlx::query("ALTER TABLE content_materials ENABLE TRIGGER USER")
        .execute(&mut *owner)
        .await
        .unwrap();
    owner.commit().await.unwrap();
    let disabled: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_trigger WHERE tgrelid='content_materials'::regclass AND NOT tgisinternal AND tgenabled='D'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        disabled, 0,
        "all source guards restored before runtime publication"
    );
    let source_state: String =
        sqlx::query_scalar("SELECT state FROM content_materials WHERE id=$1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source_state, "erasure_prepared");
    // Real repository commits on success; no guards are bypassed during this call.
    let repo = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(f.runtime.clone()));
    let result = repo
        .publish(
            &f.accepted.context,
            &authority(&f, p),
            Uuid::now_v7(),
            Uuid::now_v7(),
            commitments,
        )
        .await;
    assert_eq!(
        facts(&pool, &f, p).await,
        before,
        "unsafe persisted publication over a corrupted non-Live source"
    );
    assert!(
        result.is_err(),
        "non-Live source must refuse guarded publication"
    );
}

#[sqlx::test(migrations = false)]
async fn all_bound_publishers_recheck_publication_after_the_connection_guard(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let f = result_fixture(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let commitments = bind_all_and_compute(&f, p).await;
    let before = facts(&pool, &f, p).await;
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM connection_execution_guards WHERE workspace_id=$1 AND connection_id=$2 FOR UPDATE")
        .bind(f.accepted.context.workspace_id.as_uuid()).bind(f.accepted.connection_id)
        .fetch_one(&mut *gate).await.unwrap();
    let first = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(
        named_actor(&f.runtime, "14e-bound-publish-first").await,
    ));
    let context = f.accepted.context.clone();
    let auth = authority(&f, p);
    let first_commitments = commitments.clone();
    let one = tokio::spawn(async move {
        first
            .publish(
                &context,
                &auth,
                Uuid::now_v7(),
                Uuid::now_v7(),
                first_commitments,
            )
            .await
    });
    observe_blocked_actor(&pool, "14e-bound-publish-first").await;
    let second = PgEmbeddingResultFinalizationRepository::new(PgStore::from_pool(
        named_actor(&f.runtime, "14e-bound-publish-second").await,
    ));
    let context = f.accepted.context.clone();
    let auth = authority(&f, p);
    let two = tokio::spawn(async move {
        second
            .publish(&context, &auth, Uuid::now_v7(), Uuid::now_v7(), commitments)
            .await
    });
    observe_blocked_actor(&pool, "14e-bound-publish-second").await;
    assert_eq!(facts(&pool, &f, p).await, before);
    gate.rollback().await.unwrap();
    let first = one.await.unwrap();
    let second = two.await.unwrap();
    let after = facts(&pool, &f, p).await;
    assert_eq!(after.publications, 1);
    assert_eq!(after.events, 1);
    assert_eq!(after.live_materials, 2);
    assert_eq!(after.job_version, before.job_version + 1);
    assert_eq!(after.corpus_revision, before.corpus_revision + 1);
    assert_eq!(after.live_member_count, before.live_member_count + 2);
    assert_eq!(after.generation_epoch, before.generation_epoch + 1);
    assert_eq!(
        first.unwrap(),
        second.unwrap(),
        "both already-bound publishers must return the immutable winner after waiting"
    );
}

#[sqlx::test(migrations = false)]
async fn published_public_adoption_replays_after_guarded_credential_revocation(pool: PgPool) {
    provision_result_behavior_database_through(&pool, 194).await;
    let f = result_fixture_with_pinned_credential(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    vestrace_infrastructure::HISTORICAL_MIGRATOR
        .run(&f.runtime)
        .await
        .unwrap();
    let publication = finish_fixture(&f, p).await;
    let owned: Uuid = sqlx::query_scalar("SELECT owned_blocker_id FROM embedding_result_credential_blocker_adoptions WHERE preparation_id=$1")
        .bind(p).fetch_one(&pool).await.unwrap();
    let state: String =
        sqlx::query_scalar("SELECT state FROM material_erasure_blockers WHERE id=$1")
            .bind(owned)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "terminal");
    revoke_pinned_credential(&pool, &f).await;
    let before = facts(&pool, &f, p).await;
    let marker = history(&pool, p).await;
    let mut tx = f.runtime.begin().await.unwrap();
    scoped(&mut tx, f.accepted.context.workspace_id.as_uuid()).await;
    let replay: Uuid = sqlx::query_scalar(
        "SELECT vestrace_adopt_embedding_result_credential_blocker($1,$2,$3,$4)",
    )
    .bind(f.accepted.context.workspace_id.as_uuid())
    .bind(p)
    .bind(f.accepted.job_id.as_uuid())
    .bind(f.accepted.external_effect_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(replay, owned);
    assert_eq!(facts(&pool, &f, p).await, before);
    assert_eq!(history(&pool, p).await, marker);
    assert_eq!(finish_fixture(&f, p).await, publication);
}

#[sqlx::test(migrations = false)]
async fn published_job_recovery_survives_guarded_credential_revocation(pool: PgPool) {
    use vestrace_application::{EmbeddingJobAttemptRecovery, ProviderDispatchRepository};
    provision_result_behavior_database(&pool).await;
    let f = result_fixture_with_pinned_credential(&pool).await;
    let p = Uuid::now_v7();
    commit_result(&f, p, Uuid::now_v7()).await;
    let publication = finish_fixture(&f, p).await;
    revoke_pinned_credential(&pool, &f).await;
    let before = facts(&pool, &f, p).await;
    let immutable_history = history(&pool, p).await;
    let footprint_sql = "SELECT jsonb_build_object('effects',(SELECT count(*) FROM external_effect_intents WHERE workspace_id=$1),'admissions',(SELECT count(*) FROM connection_dispatch_admissions WHERE workspace_id=$1),'leases',(SELECT count(*) FROM provider_concurrency_leases WHERE workspace_id=$1),'dispatches',(SELECT count(*) FROM external_effect_lifecycle_transitions WHERE effect_id=$2 AND status='dispatching'),'receipts',(SELECT count(*) FROM external_effect_receipts WHERE workspace_id=$1 AND effect_id=$2))";
    let footprint: serde_json::Value = sqlx::query_scalar(footprint_sql)
        .bind(f.accepted.context.workspace_id.as_uuid())
        .bind(f.accepted.external_effect_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let recovery = common::dispatch_repository(&f.runtime);
    for _ in 0..2 {
        assert_eq!(
            recovery
                .recover_embedding_job_attempt(
                    &f.accepted.context,
                    f.accepted.job_id,
                    chrono::Utc::now()
                )
                .await
                .unwrap(),
            EmbeddingJobAttemptRecovery::Succeeded
        );
        assert_eq!(facts(&pool, &f, p).await, before);
        assert_eq!(history(&pool, p).await, immutable_history);
        let after: serde_json::Value = sqlx::query_scalar(footprint_sql)
            .bind(f.accepted.context.workspace_id.as_uuid())
            .bind(f.accepted.external_effect_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            after, footprint,
            "Published recovery must not create another dispatch, admission, lease, effect or receipt"
        );
    }
    assert_eq!(finish_fixture(&f, p).await, publication);
}
