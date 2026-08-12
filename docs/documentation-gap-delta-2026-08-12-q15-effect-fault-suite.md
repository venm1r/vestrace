# Documentation Gap Delta — Q15 Effect Fault Suite

**Date:** 2026-08-12
**Scope:** deterministic application fault-suite boundary after Q14 effect evidence persistence
**Authority:** External Effects Contract, Health / Repair / Incident Contract, Qualification / Conformance Specification, and v0.2 → v1.0 roadmap.

## Implemented bounded contract

- Added `EffectFaultScenarioExecutor`, an injected async executor boundary.
- Added `ExternalEffectFaultSuiteService` and a typed report containing all observations and the domain decision.
- Every required `EffectFaultPoint` executes exactly once in deterministic order.
- Point/observation mismatches fail closed before a decision is emitted.
- Unsafe retry evidence remains a failed decision; it is never normalized into success.
- Executor failures propagate as application errors and do not produce partial reports.
- Added focused tests for complete safe evidence, unsafe retry evidence, executor failure, and the ambiguous-dispatch UNKNOWN barrier.

## Explicit boundary

This slice does not inject failures into production adapters, simulate process crashes, persist fault-suite evidence, run chaos tests, reconcile provider state, restore trust/capabilities, or qualify AUTONOMY/TRUSTED.
