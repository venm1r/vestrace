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

## Task 6 local CI and console evidence

### 2026-08-15T21:42:02Z through 2026-08-15T21:47:17Z

All commands ran in `E:\Soft\vestrace` at `2ea1f9219ddba701094ece76edc1c6058220ecfb`. Git invocations used `-c safe.directory=E:/Soft/vestrace`. The default Windows `bash` launcher could not start its WSL backend (`Bash/Service/CreateInstance/E_ACCESSDENIED`), so the three repository scripts were run unchanged with the installed `C:\Program Files\Git\bin\bash.exe`.

| Command | UTC start--end | Exit | Result |
| --- | --- | ---: | --- |
| `cargo fmt --all -- --check` | 21:42:02--21:42:04 | 0 | pass |
| `bash ./scripts/foundation-doc-truth.sh` | 21:42:23--21:42:23 | 0 | pass (`documentation truthfulness checks passed`) |
| `bash ./scripts/foundation-boundary-truth.sh` | 21:42:28--21:42:28 | 0 | pass (`HTTP boundary checks passed`) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 21:42:33--21:42:34 | 0 | pass |
| `cargo test --workspace --all-targets --all-features --no-run` | 21:42:40--21:44:35 | 0 | pass |
| `cargo build -p vestrace-cli --bin vestrace` | 21:44:41--21:44:43 | 0 | pass |
| `bash ./scripts/foundation-cli-truth.sh` | 21:44:48--21:44:49 | 1 | **fail**: `worker returned an unexpected error: ... Error: database is unavailable` |
| `cargo test --test v1_release_evidence -- --nocapture` | 21:45:07--21:45:11 | 0 | 4 passed |
| `cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture` | 21:45:16--21:45:17 | 0 | 3 passed |
| `cargo test -p vestrace-integration-tests --test v01_acceptance -- --nocapture` | 21:45:22--21:45:24 | 0 | 9 passed |

The CLI truth script deliberately points its commands at `postgres://p0-user:***@127.0.0.1:9/vestrace_p0_unavailable`. It expects `worker`, `mcp`, `doctor`, and `rebuild` to report `... command is not implemented`; the current binary reached the worker and instead reported `database is unavailable`. This is a real local CI gate failure. The interpretation that the script expectation is stale relative to the implemented worker is an inference from its current assertions and output, not a source change in this evidence-only task.

Console validation reran after replacing the task snippet's unsupported `New-Item -LiteralPath` (this host exposes only `New-Item -Path`) with the equivalent `New-Item -Path`. `npm run typecheck` and `npm exec -- vite build --outDir <resolved-temp-path> --emptyOutDir` both exited 0 between 2026-08-15T21:46:32Z and 2026-08-15T21:46:44Z. Output resolved under the system temp root at `C:\Users\venmi\AppData\Local\Temp\vestrace-console-v1-70f1bdd9b7e047f0a8f00883466fac8c`; it was left for OS cleanup. The generated-path status count stayed `43` before and after, and the binary-diff hash stayed `771c271c6fcc611a3405860235cc2ecdac411fb5` before and after. Thus `apps/console/dist` and `apps/console/node_modules` were byte-for-byte preserved.

`cargo run -q -p vestrace-cli -- conformance check trusted --json` ran from 2026-08-15T21:47:16Z to 2026-08-15T21:47:17Z and exited `1` as expected for an open gate. Its isolated JSON report parsed to exactly `199` total, `195` passed, `0` failed, `4` skipped, and `0` not-applicable. The skips were exactly `IDW-010`, `IDW-014`, `QUAL-010`, and `REC-016`.

Historical self-review: source was not edited; the console build used a validated disposable system-temp path; the before/after generated status and binary-diff hashes matched; and the existing staged nginx rename was not included in the original evidence commit. The former CLI-script failure is superseded by the corrections below; this report makes no qualification claim.

### Task 6 CLI truthfulness correction — 2026-08-16

Human approval added one test-first, verification-script-only correction to Task 6. Root cause: `scripts/foundation-cli-truth.sh` was added in `8689e90` before `729d456` implemented the `worker`, `mcp`, `doctor`, and `rebuild` CLI paths; it still expected those commands to be unimplemented. Fresh probes against the script's deliberate unavailable database URL showed `worker`, `mcp`, `doctor`, `rebuild search-documents`, and `migrate` each exit 1 without hanging and without leaking credentials. Worker emits startup plus `database is unavailable`; doctor and rebuild additionally emit the redacted connection URL; all five report the unavailable database specifically.

RED: the unchanged script reran at 2026-08-16T05:30:05Z--05:30:06Z, exit 1, because worker emitted its real unavailable-database behavior instead of the stale unimplemented-command text. GREEN: the script now accepts required argument vectors and multiple expected fragments, invokes `rebuild search-documents`, requires the current observable failure contracts, and retains non-zero and credential-leak checks. It passed at 2026-08-16T05:31:37Z--05:31:43Z.

Fresh deterministic verification then passed: `cargo fmt --all -- --check`; workspace Clippy with warnings denied; workspace all-target/all-feature no-run compilation; CLI build; release evidence (4 tests); registry specification (3 tests); and v0.1 acceptance (9 tests). A fresh isolated console typecheck/build wrote only to validated system-temp output `C:\Users\venmi\AppData\Local\Temp\vestrace-console-v1-da22d59dc88c47baa9423e7637c0cf77`; generated status stayed at 43 entries and the before/after binary-diff hash stayed `771c271c6fcc611a3405860235cc2ecdac411fb5`.

TRUSTED reran with the expected open result: exit 1, 199 total / 195 passed / 0 failed / 4 skipped / 0 not-applicable, exactly `IDW-010`, `IDW-014`, `QUAL-010`, and `REC-016`. Rust product source remains unchanged. This corrects the local CLI gate but does not claim TRUSTED, v1.0, production evidence, crypto custody, or exact-environment qualification.

Before staging this correction, `git diff --cached --name-status` still contained only `R100 apps/console/nginx.conf apps/console/nginx.conf.template`; no generated or cache path was staged. The committed scope is limited to this durable report, `scripts/foundation-cli-truth.sh`, and the paired Task 6 design/plan amendments.

The scoped commit initially failed at 2026-08-16T05:35:42Z because Git could not create `E:/Soft/vestrace/.git/index.lock` (`Permission denied`); this historical permission issue was resolved by `25043a5` (`test: align CLI truth gate with implemented commands`). The sole cached rename remains preserved, and no generated or cache path entered that commit.

### Task 6 review correction — credential-safe CLI truth diagnostics

Two disposable binaries outside tracked source provided RED evidence. The pre-fix script accepted a generic fake that returned only `database is unavailable` for MCP and migrate, so it could not distinguish their routing. A separate fake emitted the scripted password alongside a wrong result; the pre-fix mismatch diagnostic included that password before its credential-leak check. The actual CLI help output was first observed as `Usage: vestrace.exe mcp [OPTIONS]` and `Usage: vestrace.exe migrate [OPTIONS]`, both successful.

The script now checks for a password or full unavailable URL before reporting any mismatch and reports only bounded diagnostics. It additionally requires successful, subcommand-specific help usage for MCP and migrate using portable exact-subcommand patterns, while retaining their unavailable-database assertions. GREEN: the generic fake now failed on `mcp --help`; the leak fake failed with `worker leaked database credentials` without echoing either secret form; and the real script passed. `foundation-doc-truth.sh` and `foundation-boundary-truth.sh` both freshly passed, as did fmt, workspace Clippy, all-target/all-feature no-run compilation, CLI build, the 4/3/9 focused suites, and the isolated console typecheck/Vite build. Generated status remained 43 entries and its binary-diff hash remained `771c271c6fcc611a3405860235cc2ecdac411fb5` before and after. TRUSTED remains the expected open result: exit 1, 199/195/0/4/0, exactly `IDW-010`, `IDW-014`, `QUAL-010`, and `REC-016`.

This section is included in the two-file commit that corrects the verification script. The minor malformed-helper-delimiter finding is explicitly deferred for final triage. No Rust product code, generated console output, or qualification claim is included.

## Task 7 isolated PostgreSQL and Compose runtime evidence — BLOCKED

### Fresh standalone PostgreSQL gate — 2026-08-16T05:54:19Z through 2026-08-16T05:56:59Z

All commands ran from `E:\Soft\vestrace` at HEAD `9e51bee64b5a63660d585e7633249126283dd527`. The test container was uniquely named `vestrace-v1-pg-2ff81842e3e94fd0bd6bca7d9279a981`, validated against `^vestrace-v1-pg-[0-9a-f]{32}$`, and confirmed absent before creation. Image `pgvector/pgvector:pg17` resolved to `sha256:cf134a767f474095eeba57e0117be8e568e011a63f33fbf252f14c9b760f8e6f` (registry digest of the same value). Docker assigned non-default binding `127.0.0.1:6614` to container port `5432`; readiness passed on attempt 3.

| Command | UTC start--end | Exit | Result |
| --- | --- | ---: | --- |
| `docker pull pgvector/pgvector:pg17` | 05:54:19--05:54:37 | 0 | image resolved at the digest above |
| `docker run --detach --name vestrace-v1-pg-2ff81842e3e94fd0bd6bca7d9279a981 --env POSTGRES_DB=vestrace_test --env POSTGRES_USER=vestrace --env POSTGRES_PASSWORD=vestrace --publish 127.0.0.1::5432 pgvector/pgvector:pg17` | 05:54:37--05:54:38 | 0 | container `45521331adc021e4d89fed5c235c966cd88878ae89fec1c415ed69b2d6e2818f` |
| `docker exec <validated-name> pg_isready -U vestrace -d vestrace_test` | 05:54:39--05:54:41 | 0 | 3 attempts; accepting connections |
| `DATABASE_URL=postgres://vestrace:vestrace@127.0.0.1:6614/vestrace_test cargo test --workspace --all-targets --all-features --no-fail-fast -- --nocapture` | 05:54:41--05:56:55 | 0 | 128 result lines; 912 passed / 0 failed / 4 ignored / 0 measured / 0 filtered |
| `cargo build -p vestrace-cli --bin vestrace` | 05:56:55--05:56:56 | 0 | pass |
| `VESTRACE_DATABASE__URL=<same isolated URL> target/debug/vestrace migrate` | 05:56:56--05:56:58 | 0 | first migration pass; no stdout/stderr |
| `VESTRACE_DATABASE__URL=<same isolated URL> target/debug/vestrace migrate` | 05:56:58--05:56:58 | 0 | idempotent second pass; no stdout/stderr |
| `docker rm --force <validated-name>` | 05:56:58--05:56:59 | 0 | emitted only the exact validated name; subsequent exact-name query found 0 containers |

The four ignored workspace tests were exactly the four Docker-dependent `compose_smoke` cases. The log also contains the intentional panic from `restricted_role_is_dropped_when_a_test_body_panics`; its enclosing cleanup-path test passed and the aggregate failure count remained zero. A first invocation at 05:53:31Z stopped before container creation because Windows PowerShell promoted Docker's normal `Unable to find image ... locally` pull notice to a terminating pipeline record. That invocation created no container and required no cleanup; the explicit pull above removed the invocation artifact.

### Isolated Compose acceptance — blocked 2026-08-16T06:01:49Z

The Compose project was uniquely named `vestrace-v1-baseline-7595ffdf46ac4389ae48042da6693e07`, validated against `^vestrace-v1-baseline-[0-9a-f]{32}$`, and confirmed to have zero containers, networks, and volumes before use. Non-default, initially free host ports were selected: server `127.0.0.1:6830` and console `127.0.0.1:6831`. The default `vestrace` project's six container IDs, network `7e1d3b86f7e2|vestrace_default`, and volume `vestrace_postgres-data` were snapshotted before the isolated run.

Built image evidence:

- server: `sha256:f434871a3c946158d8ee21aa82f01744d937c5d97d43db38e1c3597f1a3d79a6`
- worker: `sha256:3b518fdfcc11db7102545ebd49e3ad28f4678a96f0ae70934a520b6f8e6f531b`
- console: `sha256:4d51a1ea88a46c8d4776b6bf65ef9d8fcb9076a879878b0e2131c3bd07639706`
- Compose PostgreSQL `pgvector/pgvector:pg17-bookworm`: `sha256:7ae6051efd0e60444282c27c7e141af07f322ce033300e727a49c3dd11075e38`

| Command | UTC start--end | Exit | Result |
| --- | --- | ---: | --- |
| `COMPOSE_PROJECT_NAME=<validated-project> VESTRACE_HTTP_PORT=6830 VESTRACE_CONSOLE_PORT=6831 docker compose config --quiet` | 05:58:44--05:58:45 | 0 | pass |
| `docker compose up --build --detach --wait --wait-timeout 180` under the same environment | 05:58:45--06:01:49 | 1 | images built and PostgreSQL became healthy; `vestrace-server` exited 1, so dependent health failed |
| validated `docker compose down --volumes --remove-orphans` | 06:01:49--06:01:56 | 0 | removed exactly 5 isolated containers, 1 isolated network, and 1 isolated volume; 0 matching resources remained |

A bounded diagnostic rerun used the same validated project only after confirming its isolated resource counts were zero. `docker compose up --detach` reproduced exit 1 at 06:02:36Z--06:02:45Z. `docker compose ps --all` and `docker compose logs --no-color --timestamps postgres vestrace-server` both exited 0 and captured:

```text
Error: bootstrap credential could not be seeded: error returned from database: insert or update on table "access_tokens" violates foreign key constraint "access_tokens_workspace_id_fkey"
DETAIL: Key is not present in table "workspaces".
```

This is a source/Compose startup-order defect, not an environment blocker: the server seeds the configured bootstrap credential before its workspace exists, while the `dev-seed` service that creates that workspace depends on the server becoming healthy. Per the Task 7 stop condition, `foundation-smoke.sh`, `foundation-run-smoke.sh`, `foundation-runtime-rls.sh`, and the four ignored Compose tests were **not run** after the health dependency failed; no result is inferred for them.

Diagnostic cleanup again exited 0, removed exactly the five isolated containers, one isolated network, and one isolated volume, and left zero matching resources. The before/after default-project container, network, and volume snapshots were identical in both cleanup passes. No existing/default project, volume, container, or default host port was operated on.

### Task 7 state, non-claims, and self-review

- Status: **BLOCKED** on the fresh Compose server startup defect above; standalone PostgreSQL gates are green.
- Environment blockers: none. Docker permission, image pulls, PostgreSQL 17, Rust workspace tests, CLI build, migrations, Compose config, image builds, and isolated cleanup all ran.
- Source edits: none. Only this durable evidence report and the untracked task report were written.
- Commit: deliberately not created because the runtime gate is blocked and the task authorizes the report-only commit only after all acceptance gates pass.
- Cached state before report editing remained exactly `R100 apps/console/nginx.conf apps/console/nginx.conf.template`; cached generated/cache scan was empty. Console generated/cache status was 43 entries after the runtime work and its then-current binary-diff hash was `690ab1d436e059b160c06b36b443cbd4e8af0afe`. That hash differs from Task 6's recorded `771c271c6fcc611a3405860235cc2ecdac411fb5`. Task 7 did not target those paths, but no pre-run hash was captured, so byte-for-byte preservation across Task 7 is not claimed.
- Self-review: names matched their strict prefixes before every destructive command; isolated resources were absent before creation and absent after both cleanup paths; non-default host ports were used; the default Compose snapshot was unchanged; no smoke/RLS/ignored-test pass is claimed; TRUSTED, v1.0, production evidence, crypto custody, and exact-environment qualification remain unclaimed.
