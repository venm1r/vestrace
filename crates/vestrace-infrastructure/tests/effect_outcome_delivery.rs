//! A settled effect outcome reaches the run that asked for it.
//!
//! # What this is for
//!
//! An effect that times out gets an `unknown` receipt and the run carries on
//! without knowing. The reconciliation sweep later asks the provider and finds
//! out — and until this existed, wrote the answer to a table nobody joined
//! against. `NotApplied` means the system dispatched something that did not
//! happen, and no run was ever told.
//!
//! Every assertion here runs against real PostgreSQL, because the property is
//! about two tables and an optimistic append, none of which a fake would model
//! honestly.

use std::str::FromStr;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::PgPool;
use vestrace_application::{
    EffectOutcomeDeliveryService, ExternalEffectRepository, RequestContext, RunEventStore,
};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectPrecondition, EffectReversibility, EvidenceStrength,
    ExternalEffectIntent, ExternalEffectReceipt, IdempotencyProfile, ObservedEffectState,
    ReconciliationOutcome, reconcile_effect,
};
use vestrace_domain::id::{AgentRunId, PrincipalId, WorkspaceId};
use vestrace_domain::run::LegacyRunEvent;
use vestrace_domain::{Capability, RiskCategory};
use vestrace_infrastructure::{
    PgExternalEffectRepository, PgRunEventStore, PgRunRecoveryStore, PgStore,
};

const WORKSPACE_ID: &str = "54000000-0000-0000-0000-000000000001";
const PRINCIPAL_ID: &str = "54000000-0000-0000-0000-000000000003";
const RUN_ID: &str = "54000000-0000-0000-0000-000000000004";

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn context() -> RequestContext {
    RequestContext::new(
        WorkspaceId::from_str(WORKSPACE_ID).unwrap(),
        PrincipalId::from_str(PRINCIPAL_ID).unwrap(),
    )
}

async fn seed_run(pool: &PgPool) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1::uuid, 'effect-outcome-delivery')")
        .bind(WORKSPACE_ID)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES ($1::uuid, $2::uuid, 'effect-outcome-delivery-principal')",
    )
    .bind(PRINCIPAL_ID)
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_runs (
             id, workspace_id, principal_id, title, objective, status, run_version
         ) VALUES ($1::uuid, $2::uuid, $3::uuid, 'deliver', 'deliver', 'created', 1)",
    )
    .bind(RUN_ID)
    .bind(WORKSPACE_ID)
    .bind(PRINCIPAL_ID)
    .execute(pool)
    .await
    .unwrap();
}

/// An effect belonging to `execution_ref`.
fn intent_for(execution_ref: &str) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        execution_ref,
        WorkspaceId::from_str(WORKSPACE_ID).unwrap(),
        PrincipalId::from_str(PRINCIPAL_ID).unwrap(),
        "webhook-v1",
        "send",
        "https://alpha.effects.test/hook",
        "sha256:arguments",
        "deliver notification",
        vec![EffectPrecondition::new("resource-version", "v1").unwrap()],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        EffectReversibility::Compensatable,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        None::<String>,
        None::<String>,
        at(10),
    )
    .unwrap()
}

fn unknown_receipt(intent: &ExternalEffectIntent) -> ExternalEffectReceipt {
    ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap()
}

fn service(pool: &PgPool) -> EffectOutcomeDeliveryService {
    let store = PgStore::from_pool(pool.clone());
    EffectOutcomeDeliveryService::new(
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunRecoveryStore::new(store)),
    )
}

/// Settle an effect against `execution_ref` and store the evidence.
async fn settle(
    repository: &PgExternalEffectRepository,
    context: &RequestContext,
    execution_ref: &str,
    applied: bool,
) -> ExternalEffectIntent {
    let intent = intent_for(execution_ref);
    let receipt = unknown_receipt(&intent);
    repository.insert_intent(context, &intent).await.unwrap();
    repository.insert_receipt(context, &receipt).await.unwrap();
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(applied),
            "external:observed",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    repository
        .insert_reconciliation(context, &reconciliation)
        .await
        .unwrap();
    intent
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_settled_outcome_is_recorded_in_the_run_that_asked_for_it(pool: PgPool) {
    seed_run(&pool).await;
    let store = PgStore::from_pool(pool.clone());
    let repository = PgExternalEffectRepository::new(store.clone());
    let events = PgRunEventStore::new(store);
    let context = context();
    let run_id = AgentRunId::from_str(RUN_ID).unwrap();

    // An effect this run asked for did not happen.
    let intent = settle(&repository, &context, &format!("run://{RUN_ID}"), false).await;
    let owed = repository
        .find_undelivered_outcomes(&context, 10)
        .await
        .unwrap();
    assert_eq!(owed.len(), 1);
    let reconciliation_id = owed[0].reconciliation.id();

    let report = service(&pool).deliver_once(&context, at(40)).await.unwrap();
    assert_eq!(report.delivered, 1, "the run was not told");
    assert_eq!(report.unattributable, 0);
    assert_eq!(report.deferred, 0);

    let stream = events.load_stream(&context, run_id).await.unwrap();
    let settled: Vec<_> = stream
        .iter()
        .filter_map(|envelope| match &envelope.payload {
            LegacyRunEvent::ExternalEffectSettled {
                effect_id, outcome, ..
            } => Some((*effect_id, *outcome)),
            _ => None,
        })
        .collect();
    assert_eq!(
        settled,
        vec![(intent.id(), ReconciliationOutcome::NotApplied)],
        "the run's history does not say the effect it asked for never happened"
    );
    let delivered_transition: (String, String, String) = sqlx::query_as(
        "SELECT status, cause, cause_ref FROM external_effect_lifecycle_transitions \
         WHERE effect_id = $1 AND workspace_id = $2 AND cause = 'outcome_delivered'",
    )
    .bind(intent.id().as_uuid())
    .bind(context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        delivered_transition,
        (
            "failed".into(),
            "outcome_delivered".into(),
            reconciliation_id.to_string()
        )
    );
    repository
        .mark_outcome_delivered(&context, reconciliation_id, at(45))
        .await
        .unwrap();

    // The debt is paid: a second pass tells the run nothing more.
    let second = service(&pool).deliver_once(&context, at(50)).await.unwrap();
    assert_eq!(
        second.delivered, 0,
        "the same outcome was appended to the run twice"
    );
    let stream = events.load_stream(&context, run_id).await.unwrap();
    assert_eq!(
        stream
            .iter()
            .filter(|envelope| matches!(
                envelope.payload,
                LegacyRunEvent::ExternalEffectSettled { .. }
            ))
            .count(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions \
             WHERE effect_id = $1 AND workspace_id = $2 AND cause = 'outcome_delivered'"
        )
        .bind(intent.id().as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "repeated delivery appended another lifecycle transition"
    );
    assert_eq!(
        repository
            .find_lifecycle_status(&context, intent.id())
            .await
            .unwrap(),
        Some(vestrace_domain::external_effects::EffectLifecycleStatus::Failed),
        "repeated delivery regressed the NotApplied lifecycle outcome"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_settled_lost_dispatch_with_no_receipt_is_told_to_its_run(pool: PgPool) {
    seed_run(&pool).await;
    let store = PgStore::from_pool(pool.clone());
    let repository = PgExternalEffectRepository::new(store.clone());
    let events = PgRunEventStore::new(store);
    let context = context();
    let run_id = AgentRunId::from_str(RUN_ID).unwrap();
    let intent = intent_for(&format!("run://{RUN_ID}"));
    repository.insert_intent(&context, &intent).await.unwrap();
    repository
        .record_dispatch_started(&context, intent.id(), at(20))
        .await
        .unwrap();
    let reconciliation = reconcile_effect(
        &intent,
        None,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:observed",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    repository
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();

    let report = service(&pool).deliver_once(&context, at(40)).await.unwrap();
    assert_eq!(report.delivered, 1);
    let stream = events.load_stream(&context, run_id).await.unwrap();
    assert!(stream.iter().any(|envelope| matches!(
        envelope.payload,
        LegacyRunEvent::ExternalEffectSettled {
            effect_id,
            receipt_id: None,
            outcome: ReconciliationOutcome::Confirmed,
            ..
        } if effect_id == intent.id()
    )));
}

/// An effect nobody can attribute closes its debt without inventing a run.
#[sqlx::test(migrations = "../../migrations")]
async fn an_outcome_belonging_to_no_run_is_not_owed_forever(pool: PgPool) {
    seed_run(&pool).await;
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let context = context();

    // An operator acting directly: there is no run behind this.
    settle(&repository, &context, "workspace://", true).await;

    let report = service(&pool).deliver_once(&context, at(40)).await.unwrap();
    assert_eq!(report.delivered, 0);
    assert_eq!(
        report.unattributable, 1,
        "an outcome with no run to tell was counted as delivered, or left owed forever"
    );

    // And it is not reconsidered on the next pass.
    let second = service(&pool).deliver_once(&context, at(50)).await.unwrap();
    assert_eq!(second.unattributable, 0, "the debt was retried forever");
}

/// An unsettled outcome is not a fact about the run.
#[sqlx::test(migrations = "../../migrations")]
async fn an_inconclusive_outcome_is_not_delivered(pool: PgPool) {
    seed_run(&pool).await;
    let store = PgStore::from_pool(pool.clone());
    let repository = PgExternalEffectRepository::new(store.clone());
    let events = PgRunEventStore::new(store);
    let context = context();
    let run_id = AgentRunId::from_str(RUN_ID).unwrap();

    let intent = intent_for(&format!("run://{RUN_ID}"));
    let receipt = unknown_receipt(&intent);
    repository.insert_intent(&context, &intent).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    let inconclusive = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            None,
            "external:indeterminate",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    assert_eq!(inconclusive.outcome(), ReconciliationOutcome::Inconclusive);
    repository
        .insert_reconciliation(&context, &inconclusive)
        .await
        .unwrap();

    let report = service(&pool).deliver_once(&context, at(40)).await.unwrap();
    assert_eq!(
        (report.delivered, report.unattributable, report.deferred),
        (0, 0, 0),
        "\"we asked and could not tell\" was written into a run's history as a fact"
    );

    let stream = events.load_stream(&context, run_id).await.unwrap();
    assert!(!stream.iter().any(|envelope| matches!(
        envelope.payload,
        LegacyRunEvent::ExternalEffectSettled { .. }
    )));
}
