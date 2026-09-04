//! One invocation: crash the credential-key creation lifecycle at one boundary,
//! then say what is still there.
//!
//! Same parent/child contract as the material scenario. The lifecycles differ
//! in one way that matters here: the guarded pre-live abort admits every
//! pre-live state, so every boundary before the bind has a lawful terminal.

use secrecy::SecretString;
use uuid::Uuid;
use vestrace_application::{
    CredentialIntentResumption, CredentialResumptionOutcome, MaterialKeyVault, RequestContext,
};
use vestrace_domain::external_effects::EffectFaultPoint;
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{
    CredentialKeyCreationIntentId, IntentNonce, MaterialKeyId, PrincipalId, WorkspaceId,
};
use vestrace_fault_scenario::ScenarioSettings;
use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};
use vestrace_infrastructure::{DatabaseConfig, PgCredentialIntentRepository, PgStore};

use crate::intent_support::{
    self, BOOTSTRAP_ALGORITHM, BOOTSTRAP_KEY_ID, BOOTSTRAP_SCOPE, MARKER_PREFIX,
};

fn marker(intent_id: Uuid, point: EffectFaultPoint) -> String {
    format!(
        "{MARKER_PREFIX}credential intent={intent_id} stage={}",
        point.as_str()
    )
}

fn vault() -> Result<HostMaterialKeyVault, String> {
    let (vault_root, bootstrap_root) = intent_support::vault_roots()?;
    HostMaterialKeyVault::new(
        vault_root,
        bootstrap_root,
        KeyReference::new(
            MOUNTED_SECRET_STORE_PROVIDER,
            BOOTSTRAP_KEY_ID,
            "v1",
            KeyPurpose::Storage,
            BOOTSTRAP_SCOPE,
            BOOTSTRAP_ALGORITHM,
        )
        .map_err(|error| format!("the scenario bootstrap reference is invalid: {error}"))?,
        SecretResolutionRequest::new(
            WorkspaceId::new(),
            BOOTSTRAP_SCOPE,
            "scenario://credential-intent-crash",
        ),
    )
    .map_err(|error| format!("the scenario host vault is unavailable: {error}"))
}

async fn store(settings: &ScenarioSettings) -> Result<PgStore, String> {
    let store = PgStore::connect(&DatabaseConfig {
        url: SecretString::from(settings.database_url().to_owned()),
        max_connections: 4,
    })
    .await
    .map_err(|error| format!("the scenario database is unreachable: {error}"))?;
    store
        .migrate()
        .await
        .map_err(|error| format!("the scenario database could not be migrated: {error}"))?;
    Ok(store)
}

/// Diverges: always ends in `abort()`.
pub async fn run_child(settings: &ScenarioSettings) -> ! {
    let point = settings
        .intent_point()
        .expect("the credential intent child runs only for an intent boundary");
    if let Err(error) = drive_to(settings, point).await {
        eprintln!("vestrace-fault-scenario: {error}");
        std::process::exit(2);
    }
    std::process::abort();
}

async fn drive_to(settings: &ScenarioSettings, point: EffectFaultPoint) -> Result<(), String> {
    let store = store(settings).await?;
    let vault = vault()?;
    let pool = store.pool();

    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let slot_id = Uuid::now_v7();
    let occupancy_id = Uuid::now_v7();
    let intent_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let material_key_id = Uuid::now_v7();
    let nonce = Uuid::now_v7();
    let connector_id = Uuid::now_v7();

    intent_support::tenancy(pool, workspace_id, principal_id, "credential-intent-crash").await?;
    eprintln!("{}", marker(intent_id, point));

    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!("credential-intent-crash-{connector_id}"))
    .bind("openai")
    .execute(pool)
    .await
    .map_err(|error| format!("the scenario connector could not be created: {error}"))?;
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_id)
    .bind(principal_id)
    .bind(format!("credential-intent-crash-{connection_id}"))
    .execute(pool)
    .await
    .map_err(|error| format!("the scenario connection could not be created: {error}"))?;

    for query in [
        sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
            .bind(Uuid::now_v7())
            .bind(workspace_id)
            .bind(connection_id),
        sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, $4, $5)")
            .bind(slot_id)
            .bind(workspace_id)
            .bind(connection_id)
            .bind("provider")
            .bind("primary"),
        sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
            .bind(Uuid::now_v7())
            .bind(workspace_id)
            .bind(connection_id)
            .bind(slot_id),
        sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
            .bind(occupancy_id)
            .bind(workspace_id)
            .bind(connection_id)
            .bind(slot_id),
    ] {
        intent_support::guarded(pool, workspace_id, principal_id, query).await?;
    }

    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query(
            "SELECT vestrace_reserve_credential_key_creation_intent(\
             $1, $2, $3, $4, $5, $6, $7, $8, 'credential_v2')",
        )
        .bind(intent_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind(slot_id)
        .bind(occupancy_id)
        .bind(revision_id)
        .bind(material_key_id)
        .bind(nonce),
    )
    .await?;
    if point == EffectFaultPoint::AfterReserved {
        return Ok(());
    }

    let receipt = vault
        .create_if_absent(
            MaterialKeyId::from_uuid(material_key_id),
            IntentNonce::from_uuid(nonce),
        )
        .map_err(|error| format!("the scenario vault key could not be created: {error}"))?;
    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
            .bind(intent_id),
    )
    .await?;
    if point == EffectFaultPoint::AfterVaultCreateBeforeReceipt {
        return Ok(());
    }

    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
            .bind(intent_id)
            .bind(receipt.as_uuid()),
    )
    .await?;
    if point == EffectFaultPoint::AfterReceiptBeforePrepared {
        return Ok(());
    }

    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
            .bind(intent_id)
            .bind(Uuid::now_v7())
            .bind(b"scenario credential material".as_slice()),
    )
    .await?;
    if point == EffectFaultPoint::AfterPreparedBeforeBound {
        return Ok(());
    }

    if matches!(
        point,
        EffectFaultPoint::AfterAbortBeforeWitnessedErase
            | EffectFaultPoint::AfterEraseReceiptBeforeTerminalAppend
    ) {
        intent_support::guarded(
            pool,
            workspace_id,
            principal_id,
            sqlx::query("SELECT vestrace_prepare_credential_pre_live_abandon($1, $2)")
                .bind(intent_id)
                .bind(1_i64),
        )
        .await?;
        if point == EffectFaultPoint::AfterAbortBeforeWitnessedErase {
            return Ok(());
        }
        let key = MaterialKeyId::from_uuid(material_key_id);
        vault
            .prepare_erasure(key)
            .map_err(|error| format!("the scenario erasure fence failed: {error}"))?;
        let erased = vault
            .erase(key)
            .map_err(|error| format!("the scenario vault erase failed: {error}"))?;
        intent_support::guarded(
            pool,
            workspace_id,
            principal_id,
            sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
                .bind(intent_id)
                .bind(erased.as_uuid()),
        )
        .await?;
        return Ok(());
    }

    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(intent_id)
            .bind(Uuid::now_v7()),
    )
    .await?;
    Ok(())
}

pub async fn run_parent(settings: &ScenarioSettings) -> Result<String, String> {
    let point = settings
        .intent_point()
        .expect("the credential intent parent runs only for an intent boundary");
    let store = store(settings).await?;
    let pool = store.pool();

    let stderr = intent_support::run_the_child(settings, point).await?;
    let (intent_id, workspace_id, principal_id) =
        intent_support::identify(pool, &stderr, "credential").await?;

    let survived =
        intent_support::state_of(pool, "credential_key_creation_intents", intent_id).await?;
    let context = RequestContext::new(
        WorkspaceId::from_uuid(workspace_id),
        PrincipalId::from_uuid(principal_id),
    );
    let outcome = CredentialIntentResumption::new(
        PgCredentialIntentRepository::new(store.clone()),
        std::sync::Arc::new(vault()?),
    )
    .resume(
        &context,
        CredentialKeyCreationIntentId::from_uuid(intent_id),
    )
    .await
    .map_err(|error| format!("the credential resumption failed: {error}"))?;

    let resolved =
        intent_support::state_of(pool, "credential_key_creation_intents", intent_id).await?;
    let identities =
        intent_support::identities_for_key(pool, "credential_key_creation_intents", intent_id)
            .await?;

    Ok(serde_json::json!({
        "scenario": "credential_intent_crash",
        "point": point.as_str(),
        "survived_state": survived,
        "resumption": outcome_name(&outcome),
        "resolved_state": resolved,
        "intent_identities": identities,
    })
    .to_string())
}

fn outcome_name(outcome: &CredentialResumptionOutcome) -> &'static str {
    match outcome {
        CredentialResumptionOutcome::AlreadyTerminal(_) => "already_terminal",
        CredentialResumptionOutcome::Terminal(_) => "terminal",
        CredentialResumptionOutcome::Parked { .. } => "parked",
    }
}
