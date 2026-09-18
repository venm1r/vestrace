//! One invocation: crash the material-key creation lifecycle at one boundary,
//! then say what is still there.
//!
//! The parent/child split is the same as the external-effect scenario's and for
//! the same reason: the half that dies and the half that reads back what
//! survived are the same build of the same binary. Every field of the printed
//! observation is read out of PostgreSQL after the child is gone. The parent
//! knows which boundary it asked for and that the child aborted; neither may
//! become a field.

use secrecy::SecretString;
use uuid::Uuid;
use vestrace_application::{
    MaterialIntentResumption, MaterialKeyVault, RequestContext, ResumptionOutcome,
};
use vestrace_domain::external_effects::EffectFaultPoint;
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{
    IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId, PrincipalId, WorkspaceId,
};
use vestrace_fault_scenario::ScenarioSettings;
use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};
use vestrace_infrastructure::{DatabaseConfig, PgMaterialIntentRepository, PgStore};

use crate::intent_support::{
    self, BOOTSTRAP_ALGORITHM, BOOTSTRAP_KEY_ID, BOOTSTRAP_SCOPE, MARKER_PREFIX,
};

/// The one marker the child writes before it dies.
///
/// The intent id is generated inside the child, so the parent cannot hand it a
/// fixture id and has to learn it. This line is an identifier and a stage name,
/// not an observation: every reported field is still read back from the world.
fn marker(intent_id: Uuid, point: EffectFaultPoint) -> String {
    format!(
        "{MARKER_PREFIX}material intent={intent_id} stage={}",
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
            "scenario://material-intent-crash",
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

/// Drive the lifecycle to the requested boundary, announce it, and die there.
///
/// Diverges: this always ends in `abort()`, so nothing after the boundary can
/// run and no destructor can tidy anything away — which is the point.
pub async fn run_child(settings: &ScenarioSettings) -> ! {
    let point = settings
        .intent_point()
        .expect("the material intent child runs only for an intent boundary");
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
    let intent_id = Uuid::now_v7();
    let material_id = Uuid::now_v7();
    let material_key_id = Uuid::now_v7();
    let nonce = Uuid::now_v7();

    intent_support::tenancy(pool, workspace_id, principal_id, "material-intent-crash").await?;
    // Announced before the first transition so a child that dies early is still
    // attributable, and refused by the parent for stopping short rather than
    // reported as the boundary it never reached.
    eprintln!("{}", marker(intent_id, point));

    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query(
            "SELECT vestrace_reserve_material_key_creation_intent($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(intent_id)
        .bind(workspace_id)
        .bind(material_id)
        .bind(material_key_id)
        .bind(nonce)
        .bind("content")
        .bind(principal_id)
        .bind(0_i64),
    )
    .await?;
    if point == EffectFaultPoint::AfterReserved {
        return Ok(());
    }

    // The vault write lands before the database row. A crash here is the one
    // boundary where the host holds a key the database has never heard of.
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
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)").bind(intent_id),
    )
    .await?;
    if point == EffectFaultPoint::AfterVaultCreateBeforeReceipt {
        return Ok(());
    }

    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
            .bind(intent_id)
            .bind(receipt.as_uuid()),
    )
    .await?;
    if point == EffectFaultPoint::AfterReceiptBeforePrepared {
        return Ok(());
    }

    // The lifecycle constrains ciphertext length to the disclosed size class and
    // nothing else; encryption is not what a crash boundary exercises, so this
    // writes padded bytes rather than pretending to seal content it never had.
    intent_support::guarded(
        pool,
        workspace_id,
        principal_id,
        sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
            .bind(intent_id)
            .bind(Uuid::now_v7())
            .bind(vec![0x41_u8; 4096])
            .bind(4096_i64),
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
            sqlx::query("SELECT vestrace_prepare_content_abandon($1)").bind(intent_id),
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
            sqlx::query("SELECT vestrace_record_unbound_material_key_erasure($1, $2)")
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
        sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1, $2)")
            .bind(intent_id)
            .bind(Uuid::now_v7()),
    )
    .await?;
    Ok(())
}

/// Run the child, then read back what its death left behind.
pub async fn run_parent(settings: &ScenarioSettings) -> Result<String, String> {
    let point = settings
        .intent_point()
        .expect("the material intent parent runs only for an intent boundary");
    let store = store(settings).await?;
    let pool = store.pool();

    let stderr = intent_support::run_the_child(settings, point).await?;
    let (intent_id, workspace_id, principal_id) =
        intent_support::identify(pool, &stderr, "material").await?;

    let survived =
        intent_support::state_of(pool, "material_key_creation_intents", intent_id).await?;
    let context = RequestContext::new(
        WorkspaceId::from_uuid(workspace_id),
        PrincipalId::from_uuid(principal_id),
    );
    // The resumption a deployment would run, over the same repository and the
    // same host vault, rather than a rehearsal of it written here.
    let outcome = MaterialIntentResumption::new(
        PgMaterialIntentRepository::new(store.clone()),
        std::sync::Arc::new(vault()?),
    )
    .resume(&context, MaterialKeyCreationIntentId::from_uuid(intent_id))
    .await
    .map_err(|error| format!("the material resumption failed: {error}"))?;

    let resolved =
        intent_support::state_of(pool, "material_key_creation_intents", intent_id).await?;
    let identities =
        intent_support::identities_for_key(pool, "material_key_creation_intents", intent_id)
            .await?;

    Ok(serde_json::json!({
        "scenario": "material_intent_crash",
        "point": point.as_str(),
        "survived_state": survived,
        "resumption": outcome_name(&outcome),
        "resolved_state": resolved,
        "intent_identities": identities,
    })
    .to_string())
}

/// The wire name of a resumption outcome. No `_ =>` arm: a variant added later
/// must be named here rather than quietly rendered as something else.
fn outcome_name(outcome: &ResumptionOutcome) -> &'static str {
    match outcome {
        ResumptionOutcome::AlreadyTerminal(_) => "already_terminal",
        ResumptionOutcome::Terminal(_) => "terminal",
        ResumptionOutcome::Parked { .. } => "parked",
    }
}
