#![forbid(unsafe_code)]
#![recursion_limit = "1024"]

mod commands;

use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
use commands::{
    conformance::{
        ConformanceArtifactArg, ConformanceLifecycleArg, ConformanceProfileArg,
        FaultSuiteIsolationArg, RecoveryQualificationIsolationArg,
    },
    rebuild::RebuildTarget,
    schema::SchemaFormat,
};
use vestrace_infrastructure::{AppConfig, ConfigOverrides};

#[derive(Debug, Parser)]
#[command(name = "vestrace", about = "Vestrace service operations")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[arg(long, global = true)]
    http_bind: Option<SocketAddr>,

    #[command(subcommand)]
    command: Command,
}

// Keep the direct Clap command shape during stabilization; parsing remains unchanged.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
enum Command {
    Server,
    Worker {
        /// Run one bounded poll cycle and report whether it worked, was idle, or failed.
        #[arg(long)]
        once: bool,
    },
    Mcp,
    Migrate {
        #[arg(long, conflicts_with = "only_version")]
        through_version: Option<i64>,
        #[arg(long, conflicts_with = "through_version")]
        only_version: Option<i64>,
    },
    SafetySupervisor {
        #[command(subcommand)]
        action: SafetySupervisorAction,
    },
    Doctor,
    Plan {
        #[arg(long = "finding-id", required = true)]
        finding_ids: Vec<uuid::Uuid>,
    },
    Repair {
        #[arg(long)]
        plan_id: uuid::Uuid,
        #[arg(long)]
        current_state_ref: String,
    },
    Rebuild {
        #[arg(value_enum)]
        target: RebuildTarget,
    },
    Schema {
        #[arg(value_enum)]
        format: SchemaFormat,
    },
    Conformance {
        #[command(subcommand)]
        action: ConformanceAction,
    },
}

#[derive(Clone, Debug, Subcommand)]
enum SafetySupervisorAction {
    Initialize {
        #[arg(long)]
        installation_id: uuid::Uuid,
        #[arg(long)]
        fingerprint_key_id: uuid::Uuid,
        #[arg(long)]
        continuity_proof_hex: String,
        #[arg(long)]
        generation_id: uuid::Uuid,
        /// Abort after the durable witness receipt and before guarded SQL.
        #[arg(long)]
        fault_after_witness_advance: bool,
    },
    Backup {
        #[command(subcommand)]
        action: BackupSupervisorAction,
    },
    Restore {
        #[command(subcommand)]
        action: RestoreSupervisorAction,
    },
    Reconcile,
    /// Report whether the protected host roots and the database agree.
    ///
    /// Deliberately takes no arguments. Roots are environment-only, and a flag
    /// that named one would let an operator point readiness at a directory
    /// nobody designated; a flag that repaired a divergence would make this a
    /// mutation wearing a read's name.
    Readiness,
}

#[derive(Clone, Debug, Subcommand)]
enum BackupSupervisorAction {
    Begin,
    CaptureBase {
        #[arg(long)]
        set_id: uuid::Uuid,
    },
    AppendBase {
        /// Host-local base archive. The supervisor refuses relative paths.
        #[arg(long)]
        segment: PathBuf,
        #[arg(long)]
        set_id: uuid::Uuid,
        #[arg(long)]
        timeline: u32,
        #[arg(long)]
        start_lsn: u64,
        #[arg(long)]
        end_lsn: u64,
    },
    AppendWal {
        /// Host-local WAL segment. The supervisor refuses relative paths.
        #[arg(long)]
        segment: PathBuf,
        #[arg(long)]
        set_id: uuid::Uuid,
        #[arg(long)]
        timeline: u32,
        #[arg(long)]
        start_lsn: u64,
        #[arg(long)]
        end_lsn: u64,
    },
    AcquireHold {
        #[arg(long)]
        set_id: uuid::Uuid,
        #[arg(long)]
        hold_id: uuid::Uuid,
    },
    BeginSeal {
        #[arg(long)]
        set_id: uuid::Uuid,
    },
    CommitSeal {
        #[arg(long)]
        set_id: uuid::Uuid,
    },
    /// Bind the current sealed signed manifest to one deletion preparation.
    PrepareDelete {
        #[arg(long)]
        set_id: uuid::Uuid,
    },
    /// Erase only the envelope named by a signed deletion preparation. The
    /// command resumes the same intent after a host crash.
    EraseKey {
        #[arg(long)]
        set_id: uuid::Uuid,
    },
    /// Remove only objects from the guarded manifest after key erasure, then
    /// record the terminal deleted state.
    FinalizeDelete {
        #[arg(long)]
        set_id: uuid::Uuid,
    },
}

#[derive(Clone, Debug, Subcommand)]
enum RestoreSupervisorAction {
    /// Record a restore attempt and create one new, protected target root.
    Prepare {
        #[arg(long)]
        attempt_id: uuid::Uuid,
        #[arg(long)]
        backup_set_id: uuid::Uuid,
        #[arg(long)]
        hold_id: uuid::Uuid,
        #[arg(long)]
        target_id: uuid::Uuid,
        #[arg(long)]
        target_generation_id: uuid::Uuid,
        /// Absolute, formerly absent target directory selected for this attempt.
        #[arg(long)]
        target_root: PathBuf,
    },
    /// Record only the fixed source-freeze receipt written by the source
    /// quiescer beneath VESTRACE_RESTORE_SOURCE_ROOT.
    FreezeSource,
    /// Decrypt the exact frozen manifest into the already prepared target and
    /// run the configured target-only PostgreSQL restore tool.
    Materialize {
        #[arg(long)]
        target_root: PathBuf,
    },
    /// Pin the immutable target generation and exact frozen archive head.
    PlanActivation,
    /// Record the target receipt, activate its pinned generation, and release
    /// only the matching restore hold.
    Activate {
        #[arg(long)]
        target_root: PathBuf,
    },
    /// Destroy the locator-bound failed target, resume the source through its
    /// configured tool, and release only the resulting terminal hold.
    Refuse {
        #[arg(long)]
        target_root: PathBuf,
    },
}

#[derive(Clone, Debug, Subcommand)]
enum ConformanceAction {
    List {
        #[arg(value_enum)]
        profile: Option<ConformanceProfileArg>,
    },
    Check {
        #[arg(value_enum)]
        profile: Option<ConformanceProfileArg>,
        #[arg(long)]
        json: bool,
    },
    /// Run the destructive external-effect qualification against an ephemeral
    /// deployment and persist exactly what it observed.
    FaultSuite {
        /// Path to the separately built fault-scenario program.
        #[arg(long)]
        program: PathBuf,
        /// Digest of the exact deployment target being observed.
        #[arg(long)]
        target_digest: String,
        /// This program aborts processes mid-transaction, so the CLI exposes
        /// only the isolation the scenario can prove.
        #[arg(long, value_enum)]
        isolation: FaultSuiteIsolationArg,
    },
    /// Exercise every startup-recovery target in a disposable database and
    /// persist the actions the recovery service actually took.
    RecoveryQualification {
        /// Workspace that owns the canonical run streams used by the scenario.
        #[arg(long)]
        workspace_id: uuid::Uuid,
        /// Principal that authors the canonical run streams used by the scenario.
        #[arg(long)]
        principal_id: uuid::Uuid,
        /// Qualification evidence is append-only, so only a disposable
        /// database can be used for this complete sweep.
        #[arg(long, value_enum)]
        isolation: RecoveryQualificationIsolationArg,
    },
    /// Publish a durable baseline from an existing qualification bundle.
    PublishBaseline {
        #[arg(long)]
        bundle_file: PathBuf,
        #[arg(long, value_enum)]
        profile: ConformanceProfileArg,
    },
    Bundle {
        #[arg(long, value_enum)]
        profile: ConformanceProfileArg,
        #[arg(long, value_enum)]
        lifecycle: Option<ConformanceLifecycleArg>,
        #[arg(long)]
        persist: bool,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        target_manifest: Option<String>,
        #[arg(long)]
        source_revision: Option<String>,
        #[arg(long)]
        build_digest: Option<String>,
        #[arg(long)]
        configuration_digest: Option<String>,
        #[arg(long)]
        environment_manifest: Option<String>,
        #[arg(long)]
        target_manifest_file: Option<PathBuf>,
        #[arg(long)]
        suite_version: String,
        #[arg(long = "known-limitation")]
        known_limitations: Vec<String>,
        #[arg(long)]
        post_incident_evidence_file: Option<PathBuf>,
    },
    /// Ask the v1.0 release gate whether this exact build in this exact
    /// environment may be released.
    ///
    /// The gate names every evidence source it does not have, so a release
    /// that cannot be made says which evidence is missing rather than failing
    /// for a reason nobody recorded.
    Release {
        #[arg(long)]
        manifest_file: PathBuf,
        #[arg(long)]
        bundle_file: PathBuf,
        #[arg(long, value_enum)]
        profile: ConformanceProfileArg,
        /// Collect runtime qualification from the configured database.
        ///
        /// Without it the gate reports runtime evidence as missing, which is
        /// what it is: nobody looked.
        #[arg(long)]
        runtime_evidence: bool,
        /// Collect crypto adapter qualification from a mounted secret store.
        #[arg(long)]
        crypto_evidence: bool,
        /// Requires `--crypto-evidence` or `--release-approval`: naming a
        /// store without collecting evidence from it is not meaningful.
        #[arg(long)]
        key_store_root: Option<PathBuf>,
        #[arg(long)]
        key_id: Option<String>,
        #[arg(long, default_value = "v1")]
        key_version: String,
        #[arg(long, default_value = "release")]
        key_scope: String,
        /// Collect release approval from persisted baseline and trust evidence.
        #[arg(long)]
        release_approval: bool,
        /// Evaluate progressive restoration for every configured capability.
        #[arg(long)]
        capability_restoration: bool,
        /// Independently configured signer identity trusted for release approval.
        #[arg(long)]
        trusted_signer: Option<String>,
        /// Explicit workspace scope for the persisted trust-state lookup.
        /// Release artifacts themselves do not determine a health scope.
        #[arg(long)]
        trust_workspace_id: Option<uuid::Uuid>,
        /// Load target-bound persisted fault-suite observations and let the
        /// current evaluator derive their release verdict.
        #[arg(long)]
        fault_suite_evidence: Option<uuid::Uuid>,
        /// Evaluate every persisted startup-recovery observation.
        ///
        /// Missing and duplicate targets remain evaluator failures; collection
        /// does not invent or deduplicate observations.
        #[arg(long)]
        recovery_qualification: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Verify {
        #[arg(long, value_enum)]
        profile: ConformanceProfileArg,
        #[arg(long, value_enum)]
        lifecycle: ConformanceLifecycleArg,
        #[arg(long)]
        target_manifest_file: PathBuf,
        #[arg(long)]
        bundle_file: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        public_key_file: Option<PathBuf>,
        #[arg(long)]
        require_signature: bool,
        #[arg(long)]
        trusted_signer: Option<String>,
        #[arg(long)]
        trusted_key_provider: Option<String>,
        #[arg(long)]
        trusted_key_id: Option<String>,
        #[arg(long)]
        trusted_key_version: Option<String>,
        #[arg(long)]
        trusted_key_scope: Option<String>,
        #[arg(long)]
        require_trusted_signer: bool,
    },
    Sign {
        #[arg(long, value_enum)]
        artifact: ConformanceArtifactArg,
        #[arg(long)]
        artifact_file: PathBuf,
        #[arg(long)]
        private_key_file: Option<PathBuf>,
        #[arg(long)]
        key_store_root: Option<PathBuf>,
        #[arg(long)]
        signer_identity: String,
        #[arg(long, default_value = "local-file")]
        key_provider: String,
        #[arg(long)]
        key_id: String,
        #[arg(long, default_value = "v1")]
        key_version: String,
        #[arg(long, default_value = "release")]
        key_scope: String,
        #[arg(long)]
        output: PathBuf,
    },
    VerifySignature {
        #[arg(long, value_enum)]
        artifact: ConformanceArtifactArg,
        #[arg(long)]
        artifact_file: PathBuf,
        #[arg(long)]
        public_key_file: PathBuf,
        #[arg(long)]
        trusted_signer: Option<String>,
        #[arg(long)]
        trusted_key_provider: Option<String>,
        #[arg(long)]
        trusted_key_id: Option<String>,
        #[arg(long)]
        trusted_key_version: Option<String>,
        #[arg(long)]
        trusted_key_scope: Option<String>,
        #[arg(long)]
        require_trusted_signer: bool,
    },
    Manifest {
        #[arg(long, default_value = "manifest-v1")]
        manifest_version: String,
        #[arg(long, default_value = "vestrace")]
        product: String,
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        product_version: String,
        /// The revision this binary was built from. Compiled in when the build
        /// sets `VESTRACE_SOURCE_REVISION`; the manifest refuses to be written
        /// without one, because a qualification that cannot say what it
        /// qualified is a certificate for nothing in particular.
        #[arg(long)]
        source_revision: Option<String>,
        /// Defaults to a digest of the running executable.
        #[arg(long)]
        build_digest: Option<String>,
        /// Defaults to a digest of the effective configuration.
        #[arg(long)]
        configuration_digest: Option<String>,
        /// Defaults to a summary of what this deployment runs against.
        #[arg(long)]
        environment_manifest: Option<String>,
        #[arg(long = "schema-version", required = true)]
        schema_versions: Vec<String>,
        #[arg(long = "profile", value_enum, required = true)]
        supported_profiles: Vec<ConformanceProfileArg>,
        /// Where to read the configuration whose digest is taken. Defaults to
        /// the same resolution every other command uses.
        #[arg(long)]
        config_for_identity: Option<PathBuf>,
        #[arg(long = "feature")]
        optional_features: Vec<String>,
        #[arg(long = "storage-backend")]
        storage_backends: Vec<String>,
        #[arg(long = "crypto-provider")]
        crypto_providers: Vec<String>,
        #[arg(long = "model-provider")]
        model_provider_adapters: Vec<String>,
        #[arg(long = "external-effect-adapter")]
        external_effect_adapters: Vec<String>,
        #[arg(long = "federation-capability")]
        federation_capabilities: Vec<String>,
        #[arg(long = "known-limitation")]
        known_limitations: Vec<String>,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    // The supervisor surface contains the complete archive and restore command
    // tree. Clap builds that tree while parsing, which exceeds the small native
    // Windows main-thread stack once all guarded subcommands are present. Keep
    // the operational entry point on an explicitly sized Rust thread so help
    // and refusal paths have the same reliable command surface as mutations.
    std::thread::Builder::new()
        .name("vestrace-cli".to_owned())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Runtime::new()
                .map_err(anyhow::Error::from)?
                .block_on(run())
        })
        .map_err(anyhow::Error::from)?
        .join()
        .map_err(|_| anyhow::anyhow!("vestrace CLI execution thread panicked"))?
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Schema { format } = &cli.command {
        return commands::schema::run(format);
    }

    if let Command::Conformance { action } = &cli.command {
        return commands::conformance::run(
            action.clone(),
            cli.config.as_deref(),
            ConfigOverrides {
                http_bind: cli.http_bind,
            },
        )
        .await;
    }

    if let Command::SafetySupervisor { action } = &cli.command {
        return match action {
            SafetySupervisorAction::Initialize {
                installation_id,
                fingerprint_key_id,
                continuity_proof_hex,
                generation_id,
                fault_after_witness_advance,
            } => {
                commands::safety_supervisor::initialize(
                    *installation_id,
                    *fingerprint_key_id,
                    continuity_proof_hex,
                    *generation_id,
                    *fault_after_witness_advance,
                )
                .await
            }
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::Begin,
            } => commands::safety_supervisor::backup_begin().await,
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::CaptureBase { set_id },
            } => commands::safety_supervisor::backup_capture_base(*set_id).await,
            SafetySupervisorAction::Backup {
                action:
                    BackupSupervisorAction::AppendBase {
                        segment,
                        set_id,
                        timeline,
                        start_lsn,
                        end_lsn,
                    },
            } => {
                commands::safety_supervisor::backup_append_base(
                    *set_id, segment, *timeline, *start_lsn, *end_lsn,
                )
                .await
            }
            SafetySupervisorAction::Backup {
                action:
                    BackupSupervisorAction::AppendWal {
                        segment,
                        set_id,
                        timeline,
                        start_lsn,
                        end_lsn,
                    },
            } => {
                commands::safety_supervisor::backup_append_wal(
                    *set_id, segment, *timeline, *start_lsn, *end_lsn,
                )
                .await
            }
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::AcquireHold { set_id, hold_id },
            } => commands::safety_supervisor::backup_acquire_hold(*set_id, *hold_id).await,
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::BeginSeal { set_id },
            } => commands::safety_supervisor::backup_begin_sealing(*set_id).await,
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::CommitSeal { set_id },
            } => commands::safety_supervisor::backup_commit_sealed(*set_id).await,
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::PrepareDelete { set_id },
            } => commands::safety_supervisor::backup_prepare_deletion(*set_id).await,
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::EraseKey { set_id },
            } => commands::safety_supervisor::backup_erase_key(*set_id).await,
            SafetySupervisorAction::Backup {
                action: BackupSupervisorAction::FinalizeDelete { set_id },
            } => commands::safety_supervisor::backup_finalize_delete(*set_id).await,
            SafetySupervisorAction::Restore {
                action:
                    RestoreSupervisorAction::Prepare {
                        attempt_id,
                        backup_set_id,
                        hold_id,
                        target_id,
                        target_generation_id,
                        target_root,
                    },
            } => {
                commands::safety_supervisor::restore_prepare(
                    *attempt_id,
                    *backup_set_id,
                    *hold_id,
                    *target_id,
                    *target_generation_id,
                    target_root,
                )
                .await
            }
            SafetySupervisorAction::Restore {
                action: RestoreSupervisorAction::FreezeSource,
            } => commands::safety_supervisor::restore_freeze_source().await,
            SafetySupervisorAction::Restore {
                action: RestoreSupervisorAction::Materialize { target_root },
            } => commands::safety_supervisor::restore_materialize(target_root).await,
            SafetySupervisorAction::Restore {
                action: RestoreSupervisorAction::PlanActivation,
            } => commands::safety_supervisor::restore_plan_activation().await,
            SafetySupervisorAction::Restore {
                action: RestoreSupervisorAction::Activate { target_root },
            } => commands::safety_supervisor::restore_activate(target_root).await,
            SafetySupervisorAction::Restore {
                action: RestoreSupervisorAction::Refuse { target_root },
            } => commands::safety_supervisor::restore_refuse(target_root).await,
            SafetySupervisorAction::Reconcile => commands::safety_supervisor::reconcile().await,
            SafetySupervisorAction::Readiness => commands::safety_supervisor::readiness().await,
        };
    }

    match &cli.command {
        Command::Plan { finding_ids } => return commands::operator::plan(finding_ids),
        Command::Repair {
            plan_id,
            current_state_ref,
        } => return commands::operator::repair(*plan_id, current_state_ref),
        _ => {}
    }

    let config = AppConfig::load_from_with_overrides(
        cli.config.as_deref(),
        ConfigOverrides {
            http_bind: cli.http_bind,
        },
    )?;

    match cli.command {
        // Process identity is created once at the command boundary and injected
        // into dispatch. Storage must record the process that actually exists,
        // never fabricate an owner when the call reaches the repository.
        Command::Server => {
            commands::server::run(&config, vestrace_domain::id::WorkerId::new()).await
        }
        Command::Worker { once } => {
            let did_work = commands::worker::run(&config, once).await?;
            if once && !did_work {
                std::process::exit(3);
            }
            Ok(())
        }
        Command::Mcp => commands::mcp::run(&config).await,
        Command::Migrate {
            through_version,
            only_version,
        } => commands::migrate::run(&config, through_version, only_version).await,
        Command::Doctor => commands::doctor::run(&config).await,
        Command::SafetySupervisor { .. } => unreachable!(),
        Command::Plan { .. } | Command::Repair { .. } => unreachable!(),
        Command::Rebuild { target } => commands::rebuild::run(&config, &target).await,
        Command::Schema { .. } => unreachable!(),
        Command::Conformance { .. } => unreachable!(),
    }
}
