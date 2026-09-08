# Execution, external effects, and recovery

## Run is not a process

Run is domain execution; a worker is a process handling available work. Process exit 0 does not prove every Run succeeded, and an idle worker does not establish a failed Run.

HTTP lifecycle commands and protocol adapters describe transitions in one canonical history. AG-UI/A2A projections and client task cards cannot independently assign an external-effect outcome.

## Uncertain outcomes

Persist intent and check authority before dispatch. Record the result against that operation. A crash between sending and retaining the receipt leaves an outcome to reconcile: no response does not prove the provider did nothing.

Retry depends on the operation and provider contract. Acknowledging possible duplicate charges is a separate authorized decision, not a default retry branch. Compensation is a new effect; it neither erases history nor rolls back the external world like SQL.

## Provider identity

The governed path binds requests to connection/model revisions, policy, and ModelRequestEvidence. After immutable binding, restarting a process must not silently choose another provider from current environment variables.

Availability follows the relevant package and evidence. A domain enum does not establish production-ready behavior for all its states.

## P04: distinguish historical preparation from later publication

The historical 14D verdict stopped at delivery-only ResultPrepared. It did not accept Live publication, corpus/generation changes, successful jobs, or all worker composition.

The supplied `3e05dfbd` snapshot contains the later finalization/publication implementation and a final lead verdict accepting **Task 14E only**. The accepted rotation-before-adoption deferral remains binding, and the record explicitly leaves P04, G0, and v1.0 incomplete. This refactor reads that record; it does not rerun its tests or broaden its scope.

## Diagnose before repair

Identify durable intent, dispatch, response acknowledgement, preparation, and publication. Do not delete history or recreate jobs merely because a UI shows pending. When durable evidence does not justify an action, stop automatic repair and preserve safe observations for review.

**Sources:** [effect contract](../specs/en/vestrace-execution-external-effects-contract-v0.2.md), [finalization](../../crates/vestrace-application/src/embedding/finalization.rs), [recorded P04 evidence](../development-evidence/v1-g0-04-embedding-transition-foundation.md).
