//! Health inspection: what this deployment claims to check, and how a finding
//! comes to exist.
//!
//! # Why this module was three lines until now
//!
//! It held only [`HealthRepository`] — a single `check()` that answers the
//! readiness probe, and which despite its name has nothing to do with the
//! health subsystem. Meanwhile the findings an operator actually saw came from
//! `PgDiagnosticsRepository`: eight SQL methods, each returning a
//! `DiagnosticFinding` it had built itself with a hand-written code, severity,
//! message and remediation. The invariant registry in the domain — with
//! versions, fingerprints, lifecycles and dispositions — had no caller at all.
//!
//! That is what HLT-001 forbids: "health findings must be produced by an
//! invariant registry, not ad-hoc checks." The check *and* its classification
//! lived in the adapter, so there was no list of what the system claims to
//! verify, no version on any check, and nothing preventing a ninth check from
//! appearing with its own vocabulary.
//!
//! # The shape now
//!
//! - [`standard_invariants`] is the catalogue. It is the only place a check's
//!   identity, version, severity, repairability and remediation are decided.
//! - [`InvariantObserver`] **measures**. An adapter reports what it saw and
//!   nothing about what it means — no severity, no remediation, no code of its
//!   own invention.
//! - [`HealthInspectionService`] joins the two, and refuses an observation
//!   naming an invariant the registry does not define. A finding that cannot be
//!   traced to a registered invariant is exactly the ad-hoc check the
//!   requirement is about, so it is an error rather than a passthrough.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::HealthFindingId;
use vestrace_domain::DomainError;
use vestrace_domain::health::{
    FindingDisposition, FindingLifecycleStatus, HealthFinding, HealthOccurrence, HealthScope,
    HealthSeverity, HealthState, InvariantDefinition, InvariantRegistry, Repairability,
};
use vestrace_domain::time::{Timestamp, now};

use crate::{ApplicationError, RequestContext};

/// Whether the process can serve traffic at all.
///
/// This is the readiness probe and nothing more: it answers "is the database
/// reachable", which is why it takes no context and returns no findings. The
/// name predates the health subsystem in this module and is misleading beside
/// it — what an operator means by "health" is [`HealthInspectionService`], not
/// this.
#[async_trait]
pub trait HealthRepository: Send + Sync {
    async fn check(&self) -> Result<(), ApplicationError>;
}

/// One measurement of one invariant.
///
/// The adapter fills this in and decides nothing else. `detail` is the human
/// sentence for this particular violation — "7 outbox messages pending for more
/// than 60 seconds" — while what that *means* comes from the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantObservation {
    pub invariant_id: String,
    pub scope: HealthScope,
    /// Stable identity for "this violation of this invariant", so the same
    /// problem seen twice is one finding with two occurrences rather than two
    /// findings.
    pub fingerprint: String,
    pub state: HealthState,
    pub detail: String,
    pub evidence_refs: Vec<String>,
}

#[async_trait]
pub trait InvariantObserver: Send + Sync {
    /// Measure every invariant this observer can evaluate for the workspace in
    /// `context`, reporting only those it found violated.
    async fn observe(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<InvariantObservation>, ApplicationError>;
}

pub type SharedInvariantObserver = Arc<dyn InvariantObserver>;

/// A finding together with the invariant that produced it.
///
/// Callers need the definition to render a violation — its severity and its
/// remediation — and passing the pair keeps them from having to look it up in a
/// registry they might not have.
#[derive(Clone, Debug)]
pub struct InspectedFinding {
    pub finding: HealthFinding,
    pub definition: InvariantDefinition,
    pub detail: String,
}

impl InspectedFinding {
    pub fn severity(&self) -> HealthSeverity {
        self.finding.severity()
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self.severity(),
            HealthSeverity::Error | HealthSeverity::Critical
        )
    }
}

pub struct HealthInspectionService {
    registry: InvariantRegistry,
}

impl HealthInspectionService {
    pub fn new(registry: InvariantRegistry) -> Self {
        Self { registry }
    }

    /// The catalogue this service inspects against.
    pub fn registry(&self) -> &InvariantRegistry {
        &self.registry
    }

    /// Turn measurements into findings.
    ///
    /// An observation naming an unregistered invariant is refused rather than
    /// dropped or passed through: dropping it would hide a check whose author
    /// believed it was running, and passing it through would reintroduce the
    /// ad-hoc finding this service exists to prevent.
    pub fn inspect(
        &self,
        observations: Vec<InvariantObservation>,
        at: Timestamp,
    ) -> Result<Vec<InspectedFinding>, ApplicationError> {
        let mut inspected = Vec::with_capacity(observations.len());
        for observation in observations {
            let definition = self
                .registry
                .get(&observation.invariant_id)
                .ok_or_else(|| {
                    ApplicationError::Policy(format!(
                        "observation names invariant {} which is not in the registry, so its \
                     severity and remediation would have to be invented",
                        observation.invariant_id
                    ))
                })?;

            let finding = HealthFinding::new(
                definition,
                observation.scope,
                observation.fingerprint,
                observation.state,
                observation.evidence_refs,
                at,
            )?;

            inspected.push(InspectedFinding {
                finding,
                definition: definition.clone(),
                detail: observation.detail,
            });
        }

        // Worst first: an operator reading a truncated list should see the
        // errors.
        inspected.sort_by(|left, right| right.severity().cmp(&left.severity()));
        Ok(inspected)
    }
}

/// Durable findings and their occurrences.
///
/// A finding is identified by `(workspace, invariant_id, fingerprint)`, so the
/// same problem seen on two runs is one finding with two occurrences. Without
/// that, `occurrence_count` is always zero, recurrence and flapping have no
/// history to assess, and a disposition has nothing to attach to.
#[async_trait]
pub trait HealthFindingRepository: Send + Sync {
    /// The finding for this invariant and fingerprint, if one is already open.
    async fn find_by_fingerprint(
        &self,
        context: &RequestContext,
        invariant_id: &str,
        fingerprint: &str,
    ) -> Result<Option<HealthFinding>, ApplicationError>;

    /// Store a finding and, when given, the occurrence that produced this
    /// observation of it — in one transaction, because a finding whose count
    /// moved without its occurrence being written would be a count nothing
    /// supports.
    async fn save(
        &self,
        context: &RequestContext,
        finding: &HealthFinding,
        occurrence: Option<&HealthOccurrence>,
    ) -> Result<(), ApplicationError>;

    /// One finding by its identifier, whatever its lifecycle status.
    ///
    /// Needed to answer a finding: an operator names the one they have read,
    /// not the invariant and fingerprint that produced it.
    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: HealthFindingId,
    ) -> Result<Option<HealthFinding>, ApplicationError>;

    /// Every finding in the workspace, whatever its lifecycle status.
    ///
    /// Suppressed and accepted-risk findings are included: filtering them here
    /// would make a silenced finding indistinguishable from one that never
    /// existed, and deciding what to show is the caller's business.
    async fn list(&self, context: &RequestContext) -> Result<Vec<HealthFinding>, ApplicationError>;

    /// The occurrences of one finding, oldest first.
    async fn occurrences(
        &self,
        context: &RequestContext,
        finding_id: HealthFindingId,
    ) -> Result<Vec<HealthOccurrence>, ApplicationError>;
}

pub type SharedHealthFindingRepository = Arc<dyn HealthFindingRepository>;

/// Observes, reconciles against what is already known, and persists.
///
/// This is the piece that turns a run's measurements into a history. Given an
/// observation it either opens a finding or records an occurrence on the one
/// that already exists — and the difference between those two is the difference
/// between "the outbox is backed up" and "the outbox has been backed up for
/// eleven runs".
pub struct HealthMonitorService {
    observer: SharedInvariantObserver,
    repository: SharedHealthFindingRepository,
    inspection: HealthInspectionService,
}

impl HealthMonitorService {
    pub fn new(
        observer: SharedInvariantObserver,
        repository: SharedHealthFindingRepository,
        inspection: HealthInspectionService,
    ) -> Self {
        Self {
            observer,
            repository,
            inspection,
        }
    }

    /// Run every invariant, fold the results into the stored findings, and
    /// return what an operator should be shown.
    ///
    /// The returned list is every *known* finding, not only the ones observed
    /// on this run: a problem that stopped being observed is still open until
    /// something verifies it away, and hiding it because this run did not see it
    /// would resolve findings by forgetting them.

    /// Answer a finding: silence it, or accept the risk it describes.
    ///
    /// # Why this exists
    ///
    /// `outbox.no_dead_letters` fires at error severity, and an error-severity
    /// finding pins the health summary to unhealthy for as long as it is open.
    /// Fixing the cause was not enough — the surface reported
    /// `observed_on_this_run: false` and stayed open — because a finding is a
    /// durable record and closing one is a decision, not a side effect.
    ///
    /// `FindingDisposition` was written for exactly this and nothing could
    /// produce one. The decision now has a way in, and it carries who made it,
    /// why, under which policy version, against which audit reference, and when
    /// it lapses — because a silence nobody is still choosing is the thing this
    /// mechanism exists to prevent.
    pub async fn disposition(
        &self,
        context: &RequestContext,
        id: HealthFindingId,
        disposition: FindingDisposition,
    ) -> Result<MonitoredFinding, ApplicationError> {
        let Some(mut finding) = self.repository.find_by_id(context, id).await? else {
            return Err(ApplicationError::Domain(DomainError::NotFound(format!(
                "health finding {id} is not in this workspace"
            ))));
        };

        finding
            .set_disposition(disposition)
            .map_err(ApplicationError::Domain)?;
        self.repository.save(context, &finding, None).await?;

        let definition = self
            .inspection
            .registry()
            .get(finding.invariant_id())
            .cloned()
            .ok_or_else(|| {
                ApplicationError::Policy(format!(
                    "finding {id} names invariant {} which is no longer registered",
                    finding.invariant_id()
                ))
            })?;

        let at = now();
        Ok(MonitoredFinding {
            detail: format!("last observed {}", finding.last_seen_at()),
            observed_on_this_run: false,
            status: finding.lifecycle_status_at(at),
            definition,
            finding,
        })
    }

    pub async fn inspect(
        &self,
        context: &RequestContext,
        at: Timestamp,
    ) -> Result<Vec<MonitoredFinding>, ApplicationError> {
        let observations = self.observer.observe(context).await?;
        let inspected = self.inspection.inspect(observations, at)?;

        for entry in &inspected {
            let existing = self
                .repository
                .find_by_fingerprint(
                    context,
                    entry.finding.invariant_id(),
                    entry.finding.fingerprint(),
                )
                .await?;

            match existing {
                Some(mut finding) => {
                    // The version travels with the observation, so a check that
                    // was edited between runs says so without splitting the
                    // finding's history.
                    finding.observed_by_version(entry.finding.invariant_version());
                    let occurrence = finding.record_occurrence(
                        entry.finding.observed_state(),
                        entry.finding.evidence_refs().to_vec(),
                        at,
                    )?;
                    self.repository
                        .save(context, &finding, Some(&occurrence))
                        .await?;
                }
                None => {
                    // A new finding is opened with its first occurrence, so a
                    // finding never exists with a count of zero and no history.
                    let mut finding = entry.finding.clone();
                    let occurrence = finding.record_occurrence(
                        entry.finding.observed_state(),
                        entry.finding.evidence_refs().to_vec(),
                        at,
                    )?;
                    self.repository
                        .save(context, &finding, Some(&occurrence))
                        .await?;
                }
            }
        }

        let mut monitored = Vec::new();
        for finding in self.repository.list(context).await? {
            let Some(definition) = self.inspection.registry().get(finding.invariant_id()) else {
                // A stored finding whose invariant has since left the registry.
                // Reporting it would mean inventing a severity and remediation,
                // which is what the registry exists to prevent, so it is named
                // and skipped rather than rendered.
                tracing::warn!(
                    invariant = finding.invariant_id(),
                    "a stored health finding names an invariant that is no longer registered"
                );
                continue;
            };

            let observed_now = inspected.iter().find(|entry| {
                entry.finding.invariant_id() == finding.invariant_id()
                    && entry.finding.fingerprint() == finding.fingerprint()
            });

            monitored.push(MonitoredFinding {
                detail: observed_now
                    .map(|entry| entry.detail.clone())
                    .unwrap_or_else(|| {
                        format!(
                            "last observed {}, not seen on this run",
                            finding.last_seen_at()
                        )
                    }),
                observed_on_this_run: observed_now.is_some(),
                status: finding.lifecycle_status_at(at),
                definition: definition.clone(),
                finding,
            });
        }

        monitored.sort_by(|left, right| {
            right
                .definition
                .default_severity()
                .cmp(&left.definition.default_severity())
        });
        Ok(monitored)
    }
}

/// A stored finding, resolved against the registry and the current instant.
#[derive(Clone, Debug)]
pub struct MonitoredFinding {
    pub finding: HealthFinding,
    pub definition: InvariantDefinition,
    pub detail: String,
    pub observed_on_this_run: bool,
    /// The status as it stands now, with a lapsed silence already accounted for.
    pub status: FindingLifecycleStatus,
}

impl MonitoredFinding {
    pub fn severity(&self) -> HealthSeverity {
        self.definition.default_severity()
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self.severity(),
            HealthSeverity::Error | HealthSeverity::Critical
        ) && !self.is_silenced()
    }

    /// Whether a live disposition is keeping this finding out of the operator's
    /// way. A lapsed one does not silence anything.
    pub fn is_silenced(&self) -> bool {
        matches!(
            self.status,
            FindingLifecycleStatus::Suppressed | FindingLifecycleStatus::AcceptedRisk
        )
    }
}

/// The invariants this deployment claims to check.
///
/// Each entry replaces one hand-written check in `PgDiagnosticsRepository`. The
/// ids are namespaced by the subsystem that owns the invariant rather than by
/// the code that measures it, because the measurement can move and the
/// invariant should not have to be renamed when it does.
///
/// Versions start at `v1` and are part of the finding, so a check whose
/// definition changes can be told apart from one that did not — which is what
/// HLT-018 asks for and what a bare code string cannot express.
pub fn standard_invariants() -> InvariantRegistry {
    let definitions = [
        InvariantDefinition::new(
            "database.migrations_applied",
            "v1",
            "every migration in the embedded set has been applied successfully",
            "inspect the _sqlx_migrations table and re-run `vestrace migrate`",
            HealthSeverity::Error,
            Repairability::Manual,
            vec![],
        ),
        InvariantDefinition::new(
            "database.extensions_installed",
            "v1",
            "the extensions the schema depends on are present",
            "create the missing extension in the target database",
            HealthSeverity::Error,
            Repairability::Manual,
            vec![],
        ),
        InvariantDefinition::new(
            "memory.active_memory_has_source",
            "v1",
            "an active memory has at least one provenance source",
            "inspect the memory and re-link a source, or archive it",
            HealthSeverity::Warning,
            Repairability::Manual,
            vec![],
        ),
        InvariantDefinition::new(
            "memory.active_memory_is_indexed",
            "v1",
            "an active memory has the search document the text channel reads",
            "run `vestrace rebuild search-documents`",
            HealthSeverity::Warning,
            // Reconstructible from the active revision, which is the more
            // authoritative layer — so this one may be repaired automatically
            // (HLT-007).
            Repairability::Auto,
            vec!["memory.active_memory_has_source".to_string()],
        ),
        InvariantDefinition::new(
            "memory.active_memory_is_embedded",
            "v1",
            "an active memory has a vector in the configured embedding space",
            "run: vestrace rebuild embeddings; an unembedded memory is invisible to the vector channel while remaining findable by text",
            HealthSeverity::Warning,
            // Reconstructible from the active revision through the embedding
            // provider, so it can be repaired without a human deciding anything.
            Repairability::Auto,
            vec!["memory.active_memory_is_indexed".to_string()],
        ),
        InvariantDefinition::new(
            "outbox.backlog_within_budget",
            "v1",
            "no outbox message has been waiting longer than the lag budget",
            "check that a worker process is running and reaching this workspace; the detail names the topics that are behind, and a topic no handler claims stays pending by design rather than being discarded",
            HealthSeverity::Warning,
            Repairability::Manual,
            vec![],
        ),
        InvariantDefinition::new(
            "model.recent_failures_within_budget",
            "v1",
            "model invocations are not failing repeatedly within the last day",
            "review provider configuration, credentials and health",
            HealthSeverity::Warning,
            Repairability::Manual,
            vec![],
        ),
        InvariantDefinition::new(
            "run.lease_not_expired",
            "v1",
            "no run holds a lease that has already expired",
            "let startup recovery reclaim the run, or release the lease",
            HealthSeverity::Warning,
            Repairability::Manual,
            vec![],
        ),
        InvariantDefinition::new(
            "jobs.no_dead_letters",
            "v1",
            "no job has been dead-lettered",
            "inspect the dead-lettered jobs and retry or discard them",
            HealthSeverity::Warning,
            Repairability::Manual,
            vec![],
        ),
        InvariantDefinition::new(
            "outbox.no_dead_letters",
            "v1",
            "no outbox message has exhausted its delivery attempts",
            "read last_error on the dead-lettered rows, fix what refused them, then clear \
             dead_lettered_at to return them to the queue; the rows are kept precisely so this \
             is possible",
            // An error, not a warning, and deliberately more severe than the
            // backlog it grows out of: a backlog is delivery running late, a
            // dead letter is delivery abandoned. The system recorded a promise
            // in the same transaction as the write that made it, and did not
            // keep it.
            HealthSeverity::Error,
            // The cause is outside the system — an unreachable provider, a
            // malformed payload — so nothing here can decide the repair.
            Repairability::Manual,
            vec!["outbox.backlog_within_budget".to_string()],
        ),
    ];

    let mut registry = InvariantRegistry::default();
    for definition in definitions {
        let definition = definition.expect("the standard invariant catalogue is well-formed");
        registry
            .register(definition)
            .expect("the standard invariant catalogue has unique ids");
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::{PrincipalId, WorkspaceId, time::now};

    fn context() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    fn observation(invariant_id: &str) -> InvariantObservation {
        InvariantObservation {
            invariant_id: invariant_id.to_string(),
            scope: HealthScope::workspace(WorkspaceId::new()),
            fingerprint: format!("{invariant_id}:probe"),
            state: HealthState::Degraded,
            detail: "something was off".to_string(),
            evidence_refs: vec!["evidence:probe".to_string()],
        }
    }

    #[test]
    fn a_finding_inherits_severity_and_remediation_from_its_invariant() {
        let service = HealthInspectionService::new(standard_invariants());
        let inspected = service
            .inspect(vec![observation("database.migrations_applied")], now())
            .expect("a registered invariant produces a finding");

        assert_eq!(inspected.len(), 1);
        let entry = &inspected[0];
        assert_eq!(entry.finding.invariant_id(), "database.migrations_applied");
        assert_eq!(entry.finding.invariant_version(), "v1");
        assert_eq!(entry.severity(), HealthSeverity::Error);
        assert!(
            !entry.definition.remediation().trim().is_empty(),
            "a finding reached an operator with nothing to do about it"
        );
        let _ = context();
    }

    #[test]
    fn an_unregistered_invariant_is_refused_rather_than_reported() {
        // The whole point of HLT-001: a finding that cannot be traced to a
        // registered invariant is an ad-hoc check, and passing it through would
        // make the registry decorative.
        let service = HealthInspectionService::new(standard_invariants());
        let error = service
            .inspect(vec![observation("something.nobody.registered")], now())
            .expect_err("an unregistered invariant must not produce a finding");
        assert!(matches!(error, ApplicationError::Policy(_)));
    }

    #[test]
    fn errors_are_reported_before_warnings() {
        let service = HealthInspectionService::new(standard_invariants());
        let inspected = service
            .inspect(
                vec![
                    observation("jobs.no_dead_letters"),
                    observation("database.migrations_applied"),
                ],
                now(),
            )
            .expect("both invariants are registered");

        assert_eq!(inspected[0].severity(), HealthSeverity::Error);
        assert!(inspected[0].is_error());
        assert!(!inspected[1].is_error());
    }

    #[test]
    fn every_standard_invariant_declares_what_to_do_about_it() {
        for definition in standard_invariants().definitions() {
            assert!(
                !definition.remediation().trim().is_empty(),
                "{} has no remediation",
                definition.invariant_id()
            );
            // A remediation is printed on one line by the doctor and rendered in
            // one cell by the console. An embedded newline broke both, which is
            // how a wrapped source string turned into wrapped output.
            assert!(
                !definition.remediation().contains('\n')
                    && !definition.description().contains('\n'),
                "{} carries a line break in text meant to be shown on one line",
                definition.invariant_id()
            );
            assert!(
                !definition.description().trim().is_empty(),
                "{} has no description",
                definition.invariant_id()
            );
        }
    }
}
