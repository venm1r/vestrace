pub mod cases;
pub mod gate;
pub mod registry;
pub mod runner;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementLevel {
    Must,
    MustNot,
    Should,
    ShouldNot,
    May,
}

impl std::fmt::Display for RequirementLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Must => write!(f, "MUST"),
            Self::MustNot => write!(f, "MUST NOT"),
            Self::Should => write!(f, "SHOULD"),
            Self::ShouldNot => write!(f, "SHOULD NOT"),
            Self::May => write!(f, "MAY"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationClass {
    Static,
    Domain,
    Stateful,
    Security,
    Fault,
    Recovery,
    Interop,
    Evidence,
}

impl std::fmt::Display for VerificationClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static => write!(f, "STATIC"),
            Self::Domain => write!(f, "DOMAIN"),
            Self::Stateful => write!(f, "STATEFUL"),
            Self::Security => write!(f, "SECURITY"),
            Self::Fault => write!(f, "FAULT"),
            Self::Recovery => write!(f, "RECOVERY"),
            Self::Interop => write!(f, "INTEROP"),
            Self::Evidence => write!(f, "EVIDENCE"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementFamily {
    Arc,
    Mem,
    Tmp,
    Mut,
    Ret,
    Lrn,
    Cap,
    Idw,
    Hlt,
    Ext,
    Rec,
    Gov,
    Qual,
}

impl std::fmt::Display for RequirementFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arc => write!(f, "ARC"),
            Self::Mem => write!(f, "MEM"),
            Self::Tmp => write!(f, "TMP"),
            Self::Mut => write!(f, "MUT"),
            Self::Ret => write!(f, "RET"),
            Self::Lrn => write!(f, "LRN"),
            Self::Cap => write!(f, "CAP"),
            Self::Idw => write!(f, "IDW"),
            Self::Hlt => write!(f, "HLT"),
            Self::Ext => write!(f, "EXT"),
            Self::Rec => write!(f, "REC"),
            Self::Gov => write!(f, "GOV"),
            Self::Qual => write!(f, "QUAL"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RequirementId {
    pub family: RequirementFamily,
    pub number: u16,
}

impl RequirementId {
    pub const fn new(family: RequirementFamily, number: u16) -> Self {
        Self { family, number }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let dash = s.find('-')?;
        let family_str = &s[..dash];
        let number: u16 = s[dash + 1..].parse().ok()?;
        let family = match family_str {
            "ARC" => RequirementFamily::Arc,
            "MEM" => RequirementFamily::Mem,
            "TMP" => RequirementFamily::Tmp,
            "MUT" => RequirementFamily::Mut,
            "RET" => RequirementFamily::Ret,
            "LRN" => RequirementFamily::Lrn,
            "CAP" => RequirementFamily::Cap,
            "IDW" => RequirementFamily::Idw,
            "HLT" => RequirementFamily::Hlt,
            "EXT" => RequirementFamily::Ext,
            "REC" => RequirementFamily::Rec,
            "GOV" => RequirementFamily::Gov,
            "QUAL" => RequirementFamily::Qual,
            _ => return None,
        };
        Some(Self { family, number })
    }
}

impl std::fmt::Display for RequirementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{:03}", self.family, self.number)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Requirement {
    pub id: RequirementId,
    pub level: RequirementLevel,
    pub class: VerificationClass,
    /// The requirement as the normative invariants catalogue states it,
    /// verbatim.
    ///
    /// # Why the authoritative wording is carried, not paraphrased
    ///
    /// [`Self::statement`] is an English gloss, and glosses drift. An audit
    /// comparing the two found that only 61 of 199 registry statements are even
    /// the closest match for their own catalogue entry, and several describe a
    /// different requirement outright — registry CAP-012 states the Brain/Face/
    /// organ authority intersection, which comes from a **post-v0.2 extension**
    /// document, while catalogue CAP-012 is about a material change invalidating
    /// an approval.
    ///
    /// The catalogue is item 3 of the frozen v0.2 normative hierarchy and owns
    /// the requirement IDs, so this field carries its text unaltered and a guard
    /// keeps it byte-identical. A reader of the gate can then see what was
    /// actually required, rather than what somebody once wrote down about it.
    pub spec_statement: &'static str,
    /// An English gloss, for output and search. Not authoritative.
    pub statement: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualificationProfile {
    Core,
    Memory,
    Cognition,
    Autonomy,
    Federation,
    Trusted,
}

impl std::fmt::Display for QualificationProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "CORE"),
            Self::Memory => write!(f, "MEMORY"),
            Self::Cognition => write!(f, "COGNITION"),
            Self::Autonomy => write!(f, "AUTONOMY"),
            Self::Federation => write!(f, "FEDERATION"),
            Self::Trusted => write!(f, "TRUSTED"),
        }
    }
}

impl QualificationProfile {
    pub fn dependency_chain() -> &'static [Self] {
        &[
            Self::Core,
            Self::Memory,
            Self::Cognition,
            Self::Autonomy,
            Self::Federation,
            Self::Trusted,
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseCategory {
    Static,
    Behavioral,
    Stateful,
    Fault,
    Security,
    Recovery,
    Interop,
    Evidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Pass,
    Fail,
    Skip,
    NotApplicable,
}

/// How a case result was arrived at.
///
/// The distinction is the whole difference between a qualification report and a
/// list of intentions. `Executed` means code ran and asserted the property.
/// `BuildVerified` means compilation established a type-level invariant that
/// could not be false in the resulting binary.
/// `Attested` means a human wrote down that the property holds and named where
/// to look — useful for `VerificationClass::Static`, which describes an
/// architectural shape no unit test can observe, and worthless as proof of a
/// `Domain` or `Stateful` requirement.
///
/// Defaults to `Attested` on deserialization: a report from an older build
/// carries no origin, and assuming the weaker one is the only safe reading.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CaseOrigin {
    Executed,
    BuildVerified,
    #[default]
    Attested,
}

impl std::fmt::Display for CaseOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Executed => write!(f, "executed"),
            Self::BuildVerified => write!(f, "build_verified"),
            Self::Attested => write!(f, "attested"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConformanceCaseResult {
    pub case_id: String,
    pub requirement_ids: Vec<RequirementId>,
    pub status: CaseStatus,
    pub message: String,
    pub evidence: Option<String>,
    /// Whether anything actually ran. See [`CaseOrigin`].
    #[serde(default)]
    pub origin: CaseOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConformanceReport {
    pub profile: Option<QualificationProfile>,
    pub results: Vec<ConformanceCaseResult>,
    pub summary: ConformanceSummary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConformanceSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub not_applicable: usize,
    /// How many of `passed` were reached by running something. Reported
    /// separately because a summary that adds execution, build verification,
    /// and attestation together can make this repository believe it has
    /// verified requirements when it has none.
    #[serde(default)]
    pub passed_executed: usize,
    /// How many of `passed` were established by compilation of a type-level
    /// invariant rather than by runtime execution or attestation.
    #[serde(default)]
    pub passed_build_verified: usize,
}

impl ConformanceReport {
    pub fn from_results(
        profile: Option<QualificationProfile>,
        results: Vec<ConformanceCaseResult>,
    ) -> Self {
        let total = results.len();
        let passed = results
            .iter()
            .filter(|r| r.status == CaseStatus::Pass)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == CaseStatus::Fail)
            .count();
        let skipped = results
            .iter()
            .filter(|r| r.status == CaseStatus::Skip)
            .count();
        let not_applicable = results
            .iter()
            .filter(|r| r.status == CaseStatus::NotApplicable)
            .count();
        let passed_executed = results
            .iter()
            .filter(|r| r.status == CaseStatus::Pass && r.origin == CaseOrigin::Executed)
            .count();
        let passed_build_verified = results
            .iter()
            .filter(|r| r.status == CaseStatus::Pass && r.origin == CaseOrigin::BuildVerified)
            .count();
        Self {
            profile,
            results,
            summary: ConformanceSummary {
                total,
                passed,
                failed,
                skipped,
                not_applicable,
                passed_executed,
                passed_build_verified,
            },
        }
    }

    pub fn is_pass(&self) -> bool {
        self.summary.failed == 0 && self.summary.skipped == 0
    }

    /// Requirements whose class demands execution but which are backed only by
    /// an attestation.
    ///
    /// `Static` is exempt: "the architecture distinguishes authoritative from
    /// derived state" is a claim about shape, and no assertion at runtime
    /// observes it. Every other class describes behaviour, and behaviour that
    /// was never run is not evidence of anything.
    pub fn attested_but_should_execute(&self) -> Vec<RequirementId> {
        self.results
            .iter()
            .filter(|result| {
                result.status == CaseStatus::Pass && result.origin == CaseOrigin::Attested
            })
            .flat_map(|result| result.requirement_ids.iter().copied())
            .filter(|id| {
                registry::find(id)
                    .map(|requirement| requirement.class != VerificationClass::Static)
                    .unwrap_or(false)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaseOrigin, CaseStatus, ConformanceCaseResult, ConformanceReport, RequirementFamily,
        RequirementId,
    };

    #[test]
    fn skipped_case_does_not_qualify_a_report() {
        let report = ConformanceReport::from_results(
            None,
            vec![ConformanceCaseResult {
                case_id: "case-1".into(),
                requirement_ids: vec![RequirementId::new(RequirementFamily::Mem, 1)],
                status: CaseStatus::Skip,
                message: "no executable case".into(),
                evidence: None,
                origin: CaseOrigin::Attested,
            }],
        );

        assert!(!report.is_pass());
    }

    #[test]
    fn pass_origins_are_counted_separately() {
        let result = |case_id: &str, family: RequirementFamily, number: u16, origin: CaseOrigin| {
            ConformanceCaseResult {
                case_id: case_id.into(),
                requirement_ids: vec![RequirementId::new(family, number)],
                status: CaseStatus::Pass,
                message: "passed".into(),
                evidence: Some(format!("evidence:{case_id}")),
                origin,
            }
        };
        let report = ConformanceReport::from_results(
            None,
            vec![
                result("executed", RequirementFamily::Tmp, 2, CaseOrigin::Executed),
                result(
                    "build-verified",
                    RequirementFamily::Idw,
                    10,
                    CaseOrigin::BuildVerified,
                ),
                result("attested", RequirementFamily::Arc, 5, CaseOrigin::Attested),
            ],
        );

        assert_eq!(report.summary.passed, 3);
        assert_eq!(report.summary.passed_executed, 1);
        assert_eq!(report.summary.passed_build_verified, 1);
    }

    #[test]
    fn legacy_reports_default_new_evidence_fields_safely() {
        let json = r#"{
            "profile":null,
            "results":[{
                "case_id":"legacy",
                "requirement_ids":[{"family":"arc","number":5}],
                "status":"pass",
                "message":"legacy claim",
                "evidence":"docs/legacy.md"
            }],
            "summary":{
                "total":1,
                "passed":1,
                "failed":0,
                "skipped":0,
                "not_applicable":0,
                "passed_executed":0
            }
        }"#;

        let report: ConformanceReport = serde_json::from_str(json).unwrap();
        assert_eq!(report.results[0].origin, CaseOrigin::Attested);
        assert_eq!(report.summary.passed_build_verified, 0);
    }

    #[test]
    fn a_behavioural_requirement_backed_only_by_a_claim_is_named() {
        // TMP-004 is `Stateful`: a sentence about it is not evidence. ARC-005
        // is `Static`, where an attestation is the only available form, so it
        // must not be reported as a shortfall.
        let report = ConformanceReport::from_results(
            None,
            vec![
                ConformanceCaseResult {
                    case_id: "attested-stateful".into(),
                    requirement_ids: vec![RequirementId::new(RequirementFamily::Tmp, 4)],
                    status: CaseStatus::Pass,
                    message: "claimed".into(),
                    evidence: None,
                    origin: CaseOrigin::Attested,
                },
                ConformanceCaseResult {
                    case_id: "attested-static".into(),
                    requirement_ids: vec![RequirementId::new(RequirementFamily::Arc, 5)],
                    status: CaseStatus::Pass,
                    message: "claimed".into(),
                    evidence: None,
                    origin: CaseOrigin::Attested,
                },
            ],
        );

        let shortfall = report.attested_but_should_execute();
        assert_eq!(
            shortfall,
            vec![RequirementId::new(RequirementFamily::Tmp, 4)]
        );
    }
}
