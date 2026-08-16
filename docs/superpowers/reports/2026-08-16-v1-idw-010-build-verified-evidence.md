# IDW-010 Build-Verified Evidence Report

## Scope and commits

- Approved design/base: `7ca671f` (`docs: design IDW-010 build-verified evidence`), which adds `docs/superpowers/specs/2026-08-16-v1-idw-010-build-verified-evidence-design.md`.
- Plan: `cdc2b5f` (`docs: plan IDW-010 build verification`), which adds `docs/superpowers/plans/2026-08-16-v1-idw-010-build-verified-evidence.md`.
- Final implementation HEAD before this evidence report: `e1db998506387c71970bbc85015da33e185e57d8`.

| Commit | Changed paths |
| --- | --- |
| `e17cd5d` | `crates/vestrace-domain/src/conformance/gate.rs`; `crates/vestrace-domain/src/conformance/mod.rs` |
| `a2eb52c` | `Cargo.toml`; `Cargo.lock`; `crates/vestrace-domain/Cargo.toml`; `crates/vestrace-domain/src/conformance/cases.rs`; `crates/vestrace-domain/src/enterprise/sharing.rs` |
| `e1db998` | `crates/vestrace-cli/src/commands/conformance.rs`; `crates/vestrace-cli/tests/idw_010_build_verified.rs` |

The exact `7ca671f..e1db998` range has four commits and ten implementation/plan paths. `git diff --check 7ca671f..HEAD` exited 0.

## Build-verified invariant evidence

Task 2 first compiled the negative invariant test before adding its runner or production assertion. `cargo test -p vestrace-domain idw_010_is_reported_only_as_build_verified -- --nocapture` exited 1 with `cannot find function build_verified_cases in this scope`. After the minimal runner and normal-build static assertions were added, that focused test exited 0 with 1 passed and 0 failed; the conformance cases module exited 0 with 3 passed and 0 failed.

The mutation proof temporarily added `impl From<SharedMemoryRef> for MemoryId`. `cargo check -p vestrace-domain` then exited 1 at the required `assert_not_impl_any!` boundary with the assertion-linked `E0283` diagnostic. Removing only that temporary implementation restored GREEN: `cargo check -p vestrace-domain` and the focused IDW-010 test both exited 0. The mutation was never staged or committed.

## Fresh final verification

Every command below ran fresh from `E:\Soft\vestrace`; all Rust gates completed independently.

| Command | Exit | Result |
| --- | ---: | --- |
| `cargo fmt --all -- --check` | 0 | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | pass |
| `cargo test --workspace --all-targets --all-features --no-run` | 0 | all workspace test targets compiled |
| `cargo test --test v1_release_evidence -- --nocapture` | 0 | 4 passed; 0 failed |
| `cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture` | 0 | 3 passed; 0 failed |
| `C:\Program Files\Git\bin\bash.exe ./scripts/foundation-cli-truth.sh` | 0 | `CLI truthfulness checks passed` |

The first no-run compilation orchestration attempt reached its 120-second command timeout while still compiling and reported tool exit 124; it is not accepted as a gate result. The independent 600-second retry above completed with exit 0.

The built CLI command `target\\debug\\vestrace.exe conformance check trusted --json` wrote stdout and stderr separately. The command itself exited 1, as required while cases remain skipped. JSON parsing, rather than text matching, asserted exactly:

```text
199 total / 196 passed / 0 failed / 3 skipped / 0 not-applicable
passed_build_verified=1
IDW-010=pass/build_verified
skips=IDW-014, QUAL-010, REC-016
```

Docker was not run: IDW-010 is a compile-time/domain and CLI conformance-evidence slice with no runtime database surface. These local gates do not claim production qualification.

## Preservation and scope

The established generated/cache inventory procedure was reused: capture `git status --short -- apps/console/dist apps/console/node_modules`, obtain `git diff --binary` for those paths, and hash the joined binary diff with `git hash-object --stdin`. The inventory remains exactly 43 entries with SHA-1 `771c271c6fcc611a3405860235cc2ecdac411fb5`. Cached generated/cache paths count is 0.

Before the report commit, the repository has 44 status entries. The only cached entry is the preserved `R100 apps/console/nginx.conf apps/console/nginx.conf.template` rename. No change was made to that rename or to `apps/console/dist`, `apps/console/node_modules`, `target`, or `graphify-out`.

TRUSTED and Vestrace v1.0 remain open. This report neither implements nor closes IDW-014, QUAL-010, or REC-016.

IDW-010 build-verified evidence complete; TRUSTED now reports 196 passed and remains open on IDW-014, QUAL-010, and REC-016.
