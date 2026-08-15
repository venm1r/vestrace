# V1 Integration Baseline Report

**Design:** `docs/superpowers/specs/2026-08-16-v1-integration-baseline-design.md`
**Plan:** `docs/superpowers/plans/2026-08-16-v1-integration-baseline.md`
**Approved source point:** design commit `7f6a49a39d7af42a42eb6b52e1d9b00195c7584e`
**Execution-start HEAD:** `444fa6106523f94ac2c2881ad53a9df410c0b3ae` (contains this plan)
**Starting branch:** `main`, ahead 28 and behind 3 relative to `origin/main`

## Baseline inventory

- Approved dirty paths at plan time: 588.
- Captured dirty paths at execution start: 588.
- Pre-existing staged path: `R100 apps/console/nginx.conf apps/console/nginx.conf.template`.
- Generated/cache exclusions: `apps/console/node_modules`, `apps/console/dist`, `target`, `graphify-out`.

## Starting gate evidence

- `cargo fmt --all -- --check`: FAIL; formatting drift exists.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: FAIL; domain lint errors and workspace warnings exist.
- `cargo check --workspace --all-targets --all-features`: PASS with warnings.
- `cargo test --test v1_release_evidence -- --nocapture`: PASS, 4 tests.
- `cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture`: PASS, 3 tests.
- `npm run typecheck`: PASS.
- TRUSTED conformance: expected-open result, 199 total / 195 passed / 0 failed / 4 skipped / 0 not-applicable.
- Fresh PostgreSQL and Docker acceptance: not yet run in this slice.

## Checkpoint execution record

### Step 1: captured starting state

UTC: `2026-08-15T20:30:49Z`

Commands and outputs:

```text
git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace rev-parse HEAD
444fa6106523f94ac2c2881ad53a9df410c0b3ae

git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace status --short --branch
## main...origin/main [ahead 28, behind 3]
... 588 porcelain-v1 entries; see the count command below.

git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace diff --cached --name-status
R100    apps/console/nginx.conf    apps/console/nginx.conf.template

git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace status --porcelain=v1 | Measure-Object
Count : 588
```

The observed HEAD, divergence, dirty count, and sole staged rename exactly matched the approved checkpoint assumptions. Git emitted only non-fatal warnings that its global ignore file was inaccessible.

### Step 3: staged classifications

The scoped `git add` completed successfully. Its exact staged classification summary was:

```text
A     308
D      80
M     188
R100    1
TOTAL 577
```

The sole `R100` was the pre-existing `apps/console/nginx.conf` to `apps/console/nginx.conf.template` rename. The other 576 paths are all within the approved source categories: root Cargo/Docker files; `apps/console/Dockerfile` and `apps/console/src`; `crates`; `migrations`; `tests`; and `docs` (including this report). The exclusion scan returned no `apps/console/node_modules`, `apps/console/dist`, `target`, or `graphify-out` path.

Commands and outputs:

```text
git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace add -- [approved categories]
exit 0

git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace diff --cached --name-status
577 entries: A=308, D=80, M=188, R100=1
out-of-scope generated/cache entries: none
nginx entry: R100 apps/console/nginx.conf apps/console/nginx.conf.template
```

### Step 4: checkpoint commit and staged-preservation verification

The approved source categories were committed successfully with:

```text
git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace commit --only -m "chore: checkpoint v1 integration baseline" -- [approved categories]
```

The resulting commit is `64347860e40fa24dc84b0ab1eb4fa2e80f931319` (`chore: checkpoint v1 integration baseline`) and contains the 576 non-nginx source paths: 308 additions, 80 deletions, and 188 modifications. It deliberately excludes both nginx rename sides and every generated/cache exclusion.

Post-commit cached-diff verification succeeded. The sole cached entry is:

```text
R100    apps/console/nginx.conf    apps/console/nginx.conf.template
```

The generated/cache exclusion scan returned no cached path under `apps/console/node_modules`, `apps/console/dist`, `target`, or `graphify-out`. Their pre-existing worktree changes remain unstaged.

## Final gate evidence

### Task 1 checkpoint completion — 2026-08-15T20:38:52Z

Environment: local `E:\Soft\vestrace` checkout on `main`; Git invoked with `safe.directory=E:/Soft/vestrace`.

```text
git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace rev-parse HEAD
exit 0
64347860e40fa24dc84b0ab1eb4fa2e80f931319

git -c safe.directory=E:/Soft/vestrace -C E:\Soft\vestrace diff --cached --name-status
exit 0
R100    apps/console/nginx.conf    apps/console/nginx.conf.template

cached generated/cache exclusion scan
exit 0
(no matching paths)
```

No tests were run for this checkpoint-only Task 1. The starting-gate evidence above is retained as historical evidence, not newly produced final-gate test evidence.

## Remaining v1 gates and non-claims

- Open: `IDW-010`, `IDW-014`, `QUAL-010`, `REC-016`.
- This report does not claim TRUSTED, v1.0, production evidence collection, production crypto custody, or exact-environment qualification.

## Self-review and concerns

- Self-review: the starting state matched the approved values before any index write; only the explicit source categories were staged; the staged-scope exclusion scan was empty; commit `64347860e40fa24dc84b0ab1eb4fa2e80f931319` contains the approved 576 source paths; and the post-commit cached diff retains only the pre-existing nginx rename.
- No tests are run by this checkpoint-only task; the starting-gate results above are historical baseline evidence, not newly produced results.
- Git warned that `C:\Users\venmi/.config/git/ignore` was inaccessible and emitted line-ending conversion warnings while staging. Neither warning changed the staged classification or scope.
