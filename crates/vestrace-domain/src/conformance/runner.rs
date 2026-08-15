use super::{
    CaseCategory, CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
    RequirementId,
};
use std::collections::BTreeMap;

pub trait ConformanceCase: Send + Sync {
    fn case_id(&self) -> &str;
    fn requirement_ids(&self) -> &[RequirementId];
    fn category(&self) -> CaseCategory;
    fn description(&self) -> &str;
    fn run(&self) -> ConformanceCaseResult;
}

pub struct ConformanceRunner {
    cases: Vec<Box<dyn ConformanceCase>>,
}

impl ConformanceRunner {
    pub fn new() -> Self {
        Self { cases: Vec::new() }
    }

    pub fn register(&mut self, case: Box<dyn ConformanceCase>) {
        self.cases.push(case);
    }

    pub fn run_all(&self, profile: Option<QualificationProfile>) -> ConformanceReport {
        let results: Vec<ConformanceCaseResult> = self.cases.iter().map(|c| c.run()).collect();
        ConformanceReport::from_results(profile, results)
    }

    pub fn run_for_profile(&self, profile: QualificationProfile) -> ConformanceReport {
        let allowed = profile_requirements(profile);
        let results: Vec<ConformanceCaseResult> = self
            .cases
            .iter()
            .map(|c| {
                let mut result = c.run();
                let matches = result.requirement_ids.iter().any(|id| allowed.contains(id));
                if !matches {
                    result.status = CaseStatus::NotApplicable;
                }
                result
            })
            .collect();
        ConformanceReport::from_results(Some(profile), results)
    }

    pub fn case_count(&self) -> usize {
        self.cases.len()
    }
}

impl Default for ConformanceRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// The requirements a profile must close.
///
/// # Every family must appear somewhere
///
/// A family absent from every profile is unreachable: `run_for_profile` marks
/// any case outside the closure `NotApplicable`, so its requirements can never
/// pass and can never fail — they simply are not asked. RET (15) and HLT (20)
/// were in exactly that position, which meant a passing TRUSTED report, the
/// v1.0 gate, would have certified a release while 35 mandatory requirements
/// had never been put to any gate at all.
///
/// Their placement follows the roadmap in `docs/plans/`: RET is produced by
/// C5–C7, which sit before the v0.2 gate and build on memory, so it closes with
/// MEMORY. HLT is produced by H1–H5, which depend on CAP; the profile chain has
/// no "Understand" level to match the v0.5 gate, so it closes with TRUSTED —
/// the latest correct answer rather than a guessed intermediate one. Moving it
/// earlier is a refinement; leaving it nowhere was a hole.
pub fn profile_requirements(profile: QualificationProfile) -> Vec<RequirementId> {
    use super::RequirementFamily as F;
    match profile {
        QualificationProfile::Core => {
            let mut ids: Vec<RequirementId> = Vec::new();
            for n in 1..=10u16 {
                ids.push(RequirementId::new(F::Arc, n));
            }
            for n in 1..=10u16 {
                ids.push(RequirementId::new(F::Tmp, n));
            }
            for n in 1..=8u16 {
                ids.push(RequirementId::new(F::Mut, n));
            }
            ids
        }
        QualificationProfile::Memory => {
            let mut ids = profile_requirements(QualificationProfile::Core);
            for n in 1..=20u16 {
                ids.push(RequirementId::new(F::Mem, n));
            }
            // Retrieval reads what memory stores; C5–C7 produce its evidence
            // and land before the v0.2 gate.
            for n in 1..=15u16 {
                ids.push(RequirementId::new(F::Ret, n));
            }
            ids
        }
        QualificationProfile::Cognition => {
            let mut ids = profile_requirements(QualificationProfile::Memory);
            for n in 1..=8u16 {
                ids.push(RequirementId::new(F::Lrn, n));
            }
            ids
        }
        QualificationProfile::Autonomy => {
            let mut ids = profile_requirements(QualificationProfile::Cognition);
            for n in 1..=14u16 {
                ids.push(RequirementId::new(F::Cap, n));
            }
            for n in 1..=18u16 {
                ids.push(RequirementId::new(F::Ext, n));
            }
            ids
        }
        QualificationProfile::Federation => {
            let mut ids = profile_requirements(QualificationProfile::Autonomy);
            for n in 1..=14u16 {
                ids.push(RequirementId::new(F::Idw, n));
            }
            ids
        }
        QualificationProfile::Trusted => {
            let mut ids = profile_requirements(QualificationProfile::Federation);
            for n in 1..=18u16 {
                ids.push(RequirementId::new(F::Rec, n));
            }
            for n in 1..=26u16 {
                ids.push(RequirementId::new(F::Gov, n));
            }
            for n in 1..=18u16 {
                ids.push(RequirementId::new(F::Qual, n));
            }
            // Health, repair and incident handling. The roadmap gates these at
            // v0.5, but no profile corresponds to that gate; closing them here
            // at least puts them inside v1.0.
            for n in 1..=20u16 {
                ids.push(RequirementId::new(F::Hlt, n));
            }
            ids
        }
    }
}

#[cfg(test)]
mod tests {
    use super::profile_requirements;
    use crate::conformance::{QualificationProfile, registry};
    use std::collections::HashSet;

    #[test]
    fn every_registered_requirement_is_reachable_from_the_trusted_profile() {
        // TRUSTED is the v1.0 gate. A requirement outside its closure is
        // marked `NotApplicable` by `run_for_profile`, so it can never pass and
        // never fail — the gate simply never asks. RET and HLT sat there, 35
        // mandatory requirements that a passing v1.0 report would not have
        // covered. This test is what keeps a new family from landing the same
        // way: adding one to the registry without placing it in a profile now
        // fails here rather than silently widening the hole.
        let closure: HashSet<_> = profile_requirements(QualificationProfile::Trusted)
            .into_iter()
            .collect();

        let orphaned: Vec<String> = registry::all()
            .iter()
            .filter(|requirement| !closure.contains(&requirement.id))
            .map(|requirement| requirement.id.to_string())
            .collect();

        assert!(
            orphaned.is_empty(),
            "these requirements belong to no profile and no gate can evaluate them: {}",
            orphaned.join(", ")
        );
    }

    #[test]
    fn the_profile_chain_only_ever_grows() {
        // Each profile must contain everything the previous one required.
        // A narrower successor would let a release qualify at a higher level
        // while dropping a requirement it had already met.
        let chain = QualificationProfile::dependency_chain();
        for pair in chain.windows(2) {
            let (earlier, later) = (pair[0], pair[1]);
            let earlier_ids: HashSet<_> = profile_requirements(earlier).into_iter().collect();
            let later_ids: HashSet<_> = profile_requirements(later).into_iter().collect();
            let dropped: Vec<String> = earlier_ids
                .difference(&later_ids)
                .map(|id| id.to_string())
                .collect();
            assert!(
                dropped.is_empty(),
                "{later} drops requirements that {earlier} already required: {}",
                dropped.join(", ")
            );
        }
    }
}

pub fn group_by_family(
    results: &[ConformanceCaseResult],
) -> BTreeMap<String, Vec<&ConformanceCaseResult>> {
    let mut groups: BTreeMap<String, Vec<&ConformanceCaseResult>> = BTreeMap::new();
    for r in results {
        for id in &r.requirement_ids {
            let family = format!("{}", id.family);
            groups.entry(family).or_default().push(r);
        }
    }
    groups
}
