# R1 Domain Kernel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the pure event-sourced run aggregate kernel without changing current HTTP or PostgreSQL behavior.

**Architecture:** Keep `AgentRun` as the current projection DTO while adding `RunState`, command/event envelopes, a pure decision function, and pure reducer/replay functions under `vestrace-domain::run`. All non-deterministic values are supplied in envelopes; the kernel performs no I/O.

**Tech Stack:** Rust 1.85, edition 2024, serde, schemars, thiserror, chrono, UUID v7.

## Global Constraints

- Do not modify application, infrastructure, HTTP, CLI, migrations, or console behavior in R1.1.
- Preserve `AgentRun`, `RunStatus`, and `RunVersion` imports used by existing crates.
- Write a failing test before each production behavior.
- Do not add dependencies.
- Keep every public event name explicit and stable.
- No I/O, clock access, UUID generation, logging, or policy calls inside `decide`, `apply`, or `replay`.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-targets --all-features` before completion.

---

### Task 1: Public create/apply/replay contract

**Files:**
- Create: `crates/vestrace-domain/tests/run_state_engine.rs`
- Create: `crates/vestrace-domain/src/run/command.rs`
- Create: `crates/vestrace-domain/src/run/event.rs`
- Create: `crates/vestrace-domain/src/run/state.rs`
- Create: `crates/vestrace-domain/src/run/error.rs`
- Create: `crates/vestrace-domain/src/run/decision.rs`
- Create: `crates/vestrace-domain/src/run/reducer.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`

**Interfaces:**
- Produces: `RunCommandEnvelope`, `RunCommand`, `RunEventEnvelope`, `RunEvent`, `PendingRunEvent`, `RunState`, `RunActor`, `decide`, `apply`, `replay`.

- [ ] **Step 1: Write the failing create/replay test**

```rust
#[test]
fn create_event_builds_version_one_state_and_replays_deterministically() {
    let fixture = Fixture::new();
    let pending = decide(None, &fixture.create_command("  First run  ")).unwrap();
    assert_eq!(pending.len(), 1);

    let event = fixture.envelope(RunVersion::INITIAL, pending[0].clone());
    let state = apply(None, &event).unwrap();

    assert_eq!(state.title, "First run");
    assert_eq!(state.status, RunStatus::Created);
    assert_eq!(state.version, RunVersion::INITIAL);
    assert_eq!(replay([event.clone()]).unwrap(), Some(state.clone()));
    assert_eq!(replay([event]).unwrap(), Some(state));
}
```

- [ ] **Step 2: Push the test and verify CI fails because the new API is absent**

Expected compiler failures mention unresolved imports from `vestrace_domain::run`.

- [ ] **Step 3: Implement minimal public types and create reduction**

`RunVersion` gains `ZERO`; `RunStatus` gains the approved R1 statuses. `Create` validates and trims title. `apply(None, run.created)` constructs `RunState` with version 1.

- [ ] **Step 4: Verify the focused test passes**

Run through GitHub Actions: `cargo test -p vestrace-domain --test run_state_engine`.

- [ ] **Step 5: Commit**

Commit message: `feat(domain): add run event-sourcing kernel`

---

### Task 2: Version and identity integrity

**Files:**
- Modify: `crates/vestrace-domain/tests/run_state_engine.rs`
- Modify: `crates/vestrace-domain/src/run/decision.rs`
- Modify: `crates/vestrace-domain/src/run/reducer.rs`
- Modify: `crates/vestrace-domain/src/run/error.rs`

**Interfaces:**
- Produces typed `RunDecisionError::VersionConflict` and reduction errors for sequence, workspace, run, event type, and event version mismatches.

- [ ] **Step 1: Add failing tests**

```rust
#[test]
fn decide_rejects_stale_expected_version() { /* state v1, command expected v0 */ }

#[test]
fn apply_rejects_sequence_gap() { /* state v1, event sequence v3 */ }

#[test]
fn apply_rejects_wrong_workspace_or_run() { /* mutate envelope identity */ }
```

- [ ] **Step 2: Verify failures are caused by missing validation**

- [ ] **Step 3: Implement exact typed validation**

Validation order: expected version, aggregate existence, transition/data checks for commands; sequence, identity, stable type, schema version, terminal guard for events.

- [ ] **Step 4: Verify focused and existing domain tests pass**

- [ ] **Step 5: Commit**

Commit message: `test(domain): enforce run stream integrity`

---

### Task 3: Lifecycle transitions and waiting states

**Files:**
- Modify: `crates/vestrace-domain/tests/run_state_engine.rs`
- Modify: `crates/vestrace-domain/src/run/command.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/state.rs`
- Modify: `crates/vestrace-domain/src/run/decision.rs`
- Modify: `crates/vestrace-domain/src/run/reducer.rs`

**Interfaces:**
- Produces `MarkReady`, `Start`, `WaitForInput`, `WaitForApproval`, `Resume`, `Complete`, `Fail`, `Cancel`, and `MarkStalled` behavior.

- [ ] **Step 1: Add a table-driven failing transition test**

Cover `created -> ready -> running`, both waiting states and resume, terminal commands, and invalid transitions.

- [ ] **Step 2: Verify invalid/missing transition behavior fails**

- [ ] **Step 3: Implement minimal transition rules and state invariants**

Terminal statuses set `finished_at` and completion data; waiting statuses set a matching `RunWait`; resume clears wait.

- [ ] **Step 4: Verify lifecycle tests pass**

- [ ] **Step 5: Commit**

Commit message: `feat(domain): add run lifecycle transitions`

---

### Task 4: Step lifecycle invariants

**Files:**
- Modify: `crates/vestrace-domain/tests/run_state_engine.rs`
- Modify: `crates/vestrace-domain/src/run/command.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/state.rs`
- Modify: `crates/vestrace-domain/src/run/decision.rs`
- Modify: `crates/vestrace-domain/src/run/reducer.rs`

**Interfaces:**
- Produces `StartStep`, `CompleteStep`, `FailStep`, `RunStepState`, and `RunStepStatus`.

- [ ] **Step 1: Add failing tests**

Cover start only while running, no second active step, matching step ID on completion/failure, and no successful run completion while a step remains active.

- [ ] **Step 2: Verify tests fail for missing step behavior**

- [ ] **Step 3: Implement minimal step behavior**

- [ ] **Step 4: Verify focused tests pass**

- [ ] **Step 5: Commit**

Commit message: `feat(domain): enforce active run step invariants`

---

### Task 5: Serialization and deterministic replay

**Files:**
- Modify: `crates/vestrace-domain/tests/run_state_engine.rs`
- Modify: all new run domain modules as needed for derives.

**Interfaces:**
- Ensures command/event/state JSON round trips and stable event-type mapping.

- [ ] **Step 1: Add failing serialization and replay tests**

Serialize and deserialize each public envelope and verify equality. Replay the same multi-event stream twice and compare final states. Verify an empty stream returns `None` and an event after terminal state is rejected.

- [ ] **Step 2: Verify failures**

- [ ] **Step 3: Add only required derives and stable type mapping checks**

- [ ] **Step 4: Verify tests pass**

- [ ] **Step 5: Commit**

Commit message: `test(domain): prove deterministic run replay`

---

### Task 6: Workspace regression and documentation verification

**Files:**
- Modify: `docs/specs/r1-event-sourced-state-engine.md` only if implementation names differ.
- Modify: `docs/superpowers/plans/2026-08-04-r1-domain-kernel.md` checkbox state.

**Interfaces:**
- Produces a verified R1.1 branch without changing external behavior.

- [ ] **Step 1: Run format**

`cargo fmt --all --check`

- [ ] **Step 2: Run Clippy**

`cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] **Step 3: Run all Rust tests**

`cargo test --workspace --all-targets --all-features`

- [ ] **Step 4: Inspect PR diff**

Confirm only domain kernel, its tests, and approved documentation changed.

- [ ] **Step 5: Update the draft PR summary with exact verification evidence**
