# Documentation Gap Delta — Q13 Runtime Reconciliation

**Date:** 2026-08-12
**Scope:** workspace-scoped reconciliation of ambiguous external effects after Q12 startup recovery orchestration
**Authority:** External Effects Contract, Health / Repair / Incident Contract, Qualification / Conformance Specification, and v0.2 → v1.0 roadmap.

## Implemented bounded contract

- Added `ExternalEffectReconciliationService` to the application boundary.
- Recovery is scoped to the immutable effect intent workspace.
- Only `UNKNOWN` receipts may enter this recovery reconciliation path.
- At least one observation is required; the domain reconciler selects the strongest evidence.
- `Confirmed`, `NotApplied`, and `Inconclusive` remain explicit domain outcomes.
- The service exposes no automatic dispatch or retry operation; `UNKNOWN` remains retry-denied.
- Added focused tests for confirmed reconciliation, workspace isolation, and evidence/retry barriers.

## Explicit boundary

This slice does not persist effect intents/receipts/reconciliations, discover them during server/worker startup, provide provider read-back adapters, execute deterministic crash/fault scenarios, or restore trust/capabilities after revalidation.
