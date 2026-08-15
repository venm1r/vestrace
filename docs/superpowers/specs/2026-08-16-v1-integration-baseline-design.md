# V1 Integration Baseline Design

**Date:** 2026-08-16
**Status:** approved design
**Scope:** stabilize the active dirty `main` checkout as the source baseline for the path to v1.0

## 1. Goal

Turn the current implementation state into a reproducible, reviewable baseline that passes the repository's ordinary source and CI gates. This slice prepares later v1.0 capability work; it does not close a TRUSTED requirement, qualify a deployment, or make a v1.0 release claim.

## 2. Baseline authority

The active working tree is the intended v1.0 starting point, including its current Rust, SQL migration, integration-test, console-source, and documentation changes. Work remains in the existing `main` checkout by explicit operator choice.

The slice must preserve all pre-existing work:

- no reset, checkout-based restoration, rebase, stash, or deletion of existing changes;
- no modification of the pre-existing staged `apps/console/nginx.conf` to `apps/console/nginx.conf.template` rename;
- no assumption that an untracked file is disposable;
- every commit uses an explicit path list and is inspected before creation.

Generated and cache material is not part of the source baseline:

- `apps/console/node_modules/`;
- `apps/console/dist/`;
- `target/`;
- `graphify-out/`.

Existing changes under those paths remain untouched unless a later explicitly approved cleanup addresses them.

## 3. Slice boundary

This slice may change only what is necessary to restore the baseline gates:

- Rust formatting;
- compiler and Clippy diagnostics;
- tests required to protect any behavior affected while resolving a diagnostic;
- narrowly related documentation recording verification results and remaining blockers.

This slice must not:

- implement `IDW-010`, `IDW-014`, `QUAL-010`, or `REC-016`;
- add a production `V1ReleaseEvidenceProbe`;
- add KMS, HSM, Vault, or OS-keyring custody;
- change product behavior merely to silence a lint;
- regenerate or commit console build output;
- claim TRUSTED, v1.0, or exact-environment qualification.

## 4. Work sequence

### 4.1 Inventory

Capture the branch, HEAD, divergence from `origin/main`, staged paths, working-tree paths, and the exact commands and failures that define the starting point. Classify every file selected for a commit as source, test, migration, documentation, or generated material.

### 4.2 Mechanical stabilization

Apply Rust formatting and resolve diagnostics without changing behavior. Mechanical edits are inspected as diffs before any behavioral work begins.

If a diagnostic cannot be fixed without changing behavior, stop the mechanical pass for that item and use a separate test-driven change:

1. write a focused test that exposes the required behavior;
2. run it and observe the expected failure;
3. implement the minimal correction;
4. rerun the focused test and its nearest affected suite;
5. inspect the exact diff.

### 4.3 Layered verification

Verification advances from cheap and local to environment-dependent:

1. `cargo fmt --all -- --check`;
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
3. `cargo test --workspace --no-run`;
4. focused v1 release-evidence and registry/catalogue tests;
5. `npm run typecheck` and a production console build directed to a validated disposable output directory outside `apps/console/dist/`;
6. workspace tests;
7. PostgreSQL and Docker acceptance gates when the required environment is available;
8. `vestrace conformance check trusted --json`.

An environment-blocked gate is reported separately and is never inferred from a unit or contract test.

## 5. Conformance control

The expected pre-v1 result is:

```text
199 total
195 passed
0 failed
4 skipped
0 not applicable
```

The known skips are `IDW-010`, `IDW-014`, `QUAL-010`, and `REC-016`. This slice succeeds only if it introduces no additional failure, skip, or not-applicable result. The command is still expected to exit non-zero because the four known skips remain open.

## 6. Failure handling

- If formatting changes an unexpected file, stop and inspect before continuing.
- If a focused fix expands into an unrelated subsystem, defer it as a separate slice.
- If an existing test fails outside the selected change, record the failure and determine whether it is baseline drift before modifying code.
- If Docker, PostgreSQL, or another runtime dependency is unavailable, keep the local result truthful and leave the runtime gate open.
- Never use a broad commit, `git add -A`, or an implicit staged set.

## 7. Exit criteria

The V1 Integration Baseline slice is complete only when:

- Rust formatting passes;
- Clippy passes with `-D warnings` across the workspace and all targets/features;
- the workspace compiles for tests;
- focused v1 evidence and registry/catalogue tests pass;
- console typecheck and a disposable-output production build pass without modifying or committing existing generated output;
- available workspace, PostgreSQL, and Docker tests have explicit fresh results;
- TRUSTED conformance reports exactly the four known skips and no failures;
- the final diff is scoped and reviewed;
- verification evidence and environment-blocked gates are documented;
- commits include only explicitly listed files and exclude the pre-existing staged nginx rename.

Passing this slice means the repository has a stable implementation baseline. It does not mean the roadmap or v1.0 release gate is complete.

## 8. Subsequent dependency order

After this baseline closes, implementation proceeds through separate reviewed designs and plans:

1. `IDW-010` evidence-source boundary;
2. `IDW-014` cross-workspace sharing adapter and isolation evidence;
3. `QUAL-010` cognitive qualification scenarios;
4. `REC-016` destructive-recovery forensic preservation;
5. production release-evidence collection and crypto custody;
6. exact-environment TRUSTED qualification and release evidence.

Each item retains its own test-first cycle, evidence gate, non-claims, and atomic commit boundary.
