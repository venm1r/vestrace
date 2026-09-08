# Development guide

Start with [architecture](../architecture.md), [implementation status](../status.md), the [roadmap](../roadmap/README.md), and the exact package for the task. A class reference does not prove a working scenario. When a source baseline changes, reread affected code and dependency evidence.

## Deliver a complete change

A task needs an aligned contract, implementation, observable behavior, and documentation. Test the user's observable obligation rather than merely an internal call. Fixtures establish a legitimate environment; end-to-end state must come through real entry points rather than fabricated completion records.

## Protect unrelated work

Do not change frozen specifications, historical evidence, or applied migrations as an incidental feature change. Root PLAN.md belongs to separate work. Before writing, record the exact allowlist, existing dirty/untracked bytes, and independent review process. MW documents do not grant blanket permission over protected P04 paths.

## Sequence and modularity

By default, run one major code package alongside one independent documentation/measurement task. Do not assign competing migration/writer changes simultaneously to one persistent builder. Parallel work needs disjoint scopes and accepted interfaces.

Keep domain, application, infrastructure, HTTP, CLI, and MCP responsibilities separate. UI and scanners are clients, not new authorities. Split modules for responsibility and readability, not arbitrary line counts. Do not create a second event log, execution runtime, or policy engine for convenience.

## Finish with evidence

Review contracts and production composition rather than rely on a builder's account. Report commands actually executed, exit codes, negative tests, and unmet conditions. Commit, merge, and deployment require their own authorization; a documentation page does not perform them.

[Testing](testing.md) · [Agent workflow](agent-workflow.md) · [Release rules](release.md) · [Normative contracts](../specs/README.md)
