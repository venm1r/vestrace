# Feedback, health, trust, and qualification

## Feedback is not automatic truth

Test results, user ratings, and model-judge conclusions differ in provenance and weight. Keep raw observations separate from learned projections. Frequency of use may inform ranking but does not establish truth.

A learning proposal does not authorize changes to policy, capabilities, assets, or canonical assertions. Those follow their own versioned mutation and review gates. Preserve measurements so conclusions can be reconsidered.

## Health and trust

A process/database may be healthy while an external outcome remains unknown, restored data remains unverified, or dangerous capabilities remain suspended. Recovery and revalidation are different stages.

A finding records a violation or suspicion. Removing a symptom does not always resolve it. Suppressed/accepted-risk is disposition, not integrity state; [ADR-0009](../adr/0009-finding-disposition-is-not-integrity-state.md) specifies the distinction.

## Repair

Automatic repair is justified only when a result is unambiguously reconstructible from higher-authority data and current authority permits it. Semantic conflict is not deterministic rebuild. Assigning healthy/trusted without observations is not repair.

CLI doctor, plan, repair, rebuild, and conformance commands do not by their existence qualify every target profile or repair path.

## Qualification

A profile qualifies specific properties on a specific target. Identify source, environment, configuration, models, and executed checks. Missing, skipped, or unverified checks do not become PASS because other tests are green.

Documentation validation covers text and examples only. It does not contribute product qualification or close P12.

**Sources:** [health contract](../specs/en/vestrace-health-repair-incident-contract-v0.2.md), [qualification contract](../specs/en/vestrace-qualification-conformance-spec-v0.2.md), [CLI](../../crates/vestrace-cli/src/main.rs).
