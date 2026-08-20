# External Effect Fault Scenario Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A scenario program that kills a real process mid-lifecycle at each of the five external-effect fault points, lets the deployment recover, reads what survived, and reports it — so the fault suite has something truthful to evaluate.

**Architecture:** A new workspace crate holds one binary, excluded from the shipped image by construction. `ProcessFaultInjectionRuntime` invokes it once per fault point with a cleared environment. It reads the database URL from a file, refuses any isolation but `ephemeral`, spawns a child of itself that drives the real lifecycle and calls `std::process::abort()` at the requested point, then runs the deployment's own recovery path, reads the persisted state back, and prints a JSON observation.

**Tech Stack:** Rust 2024, `tokio`, `sqlx` via `PgStore`, `serde_json`, a small `tokio`-based HTTP stub for the adapter side.

**Spec:** `docs/superpowers/specs/2026-08-20-external-effect-fault-scenario-design.md`

## Global Constraints

- **No branch of this crate may construct `FaultObservation::expected(...)`.** A guard test asserts the string appears nowhere in it.
- Every observation field is derived from persisted state or from the adapter stub, never from what the scenario believes it did.
- The program refuses to run unless `VESTRACE_FAULT_ISOLATION` is exactly `ephemeral`.
- The database URL is read from a file named by `--database-url-file <path>`. Never from `argv`, never from the inherited environment.
- The child calls `std::process::abort()`, never `exit()`.
- The scenario binary must not appear in `Dockerfile`; a guard test asserts it.
- Nothing in `crates/vestrace-cli`, `crates/vestrace-http` or the shipped server gains a fault hook.
- The suite's verdict is the finding. If the suite fails, **do not adjust the scenario to make it pass** — report it.
- Output on stdout is exactly one JSON object with the fields `point`, `status`, `retry_attempted`, `reconciliation_started`, `receipt_persisted`, matching the `ProcessFaultObservation` reader in `crates/vestrace-application/src/fault_runtime.rs`.

---

## File Structure

| File | Responsibility |
|---|---|
| Create: `crates/vestrace-fault-scenario/Cargo.toml` | The crate; a `[[bin]]` named `vestrace-fault-scenario` |
| Create: `crates/vestrace-fault-scenario/src/main.rs` | Argument and environment contract, parent/child dispatch |
| Create: `crates/vestrace-fault-scenario/src/settings.rs` | `ScenarioSettings`: isolation check, URL file, requested point |
| Create: `crates/vestrace-fault-scenario/src/adapter_stub.rs` | The HTTP stub that records dispatches and answers read-backs |
| Create: `crates/vestrace-fault-scenario/src/child.rs` | Drives the real lifecycle, aborts at the requested point |
| Create: `crates/vestrace-fault-scenario/src/observe.rs` | Derives the four observation fields from persisted state |
| Create: `crates/vestrace-fault-scenario/tests/contract.rs` | Isolation refusal, URL file handling, the two guard tests |
| Create: `crates/vestrace-fault-scenario/tests/observation.rs` | Field derivation against a known database state |
| Modify: `Cargo.toml` | Add the crate to `members` |

---

### Task 1: The crate, its contract, and the two guards

**Files:**
- Create: `crates/vestrace-fault-scenario/Cargo.toml`, `src/main.rs`, `src/settings.rs`
- Modify: `Cargo.toml` (workspace `members`)
- Test: `crates/vestrace-fault-scenario/tests/contract.rs`

**Interfaces:**
- Produces: `ScenarioSettings` with `pub fn from_env_and_args(args: impl Iterator<Item = String>, env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String>`, `pub fn point(&self) -> EffectFaultPoint`, `pub fn database_url(&self) -> &str`, `pub fn is_child(&self) -> bool`

- [ ] **Step 1: Write the failing tests**

Create `crates/vestrace-fault-scenario/tests/contract.rs`:

```rust
use std::collections::HashMap;
use vestrace_fault_scenario::ScenarioSettings;

fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn settings_from(
    args: &[&str],
    env: &HashMap<String, String>,
) -> Result<ScenarioSettings, String> {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    ScenarioSettings::from_env_and_args(owned.into_iter(), &|name| env.get(name).cloned())
}

/// This program kills processes mid-transaction against whatever database it
/// is handed. The isolation claim is the only thing between that and somebody's
/// data, so it is checked rather than assumed.
#[test]
fn any_isolation_but_ephemeral_is_refused() {
    let url_file = std::env::temp_dir().join(format!("vfs-url-{}.txt", std::process::id()));
    std::fs::write(&url_file, "postgres://u:p@127.0.0.1:5432/db").unwrap();
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_intent_persistence"),
        ("VESTRACE_FAULT_ISOLATION", "designated-non-production"),
    ]);

    let error = settings_from(
        &["--database-url-file", url_file.to_str().unwrap()],
        &env,
    )
    .expect_err("a non-ephemeral isolation must be refused");

    assert!(error.contains("ephemeral"), "unhelpful refusal: {error}");
    std::fs::remove_file(&url_file).ok();
}

/// The invoking contract clears the environment, so the URL cannot arrive that
/// way; it must not arrive in argv either, where any process on the host can
/// read it.
#[test]
fn the_database_url_is_read_from_a_file_not_an_argument() {
    let url_file = std::env::temp_dir().join(format!("vfs-url2-{}.txt", std::process::id()));
    std::fs::write(&url_file, "  postgres://u:p@127.0.0.1:5432/db\n").unwrap();
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_intent_persistence"),
        ("VESTRACE_FAULT_ISOLATION", "ephemeral"),
    ]);

    let settings = settings_from(
        &["--database-url-file", url_file.to_str().unwrap()],
        &env,
    )
    .expect("a well-formed invocation must be accepted");

    assert_eq!(settings.database_url(), "postgres://u:p@127.0.0.1:5432/db");
    std::fs::remove_file(&url_file).ok();
}

#[test]
fn a_missing_url_file_is_refused() {
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_intent_persistence"),
        ("VESTRACE_FAULT_ISOLATION", "ephemeral"),
    ]);

    let error = settings_from(&["--database-url-file", "/nonexistent/vfs"], &env)
        .expect_err("a missing url file must be refused");

    assert!(error.contains("database url file"), "unhelpful refusal: {error}");
}

#[test]
fn an_unknown_fault_point_is_refused() {
    let url_file = std::env::temp_dir().join(format!("vfs-url3-{}.txt", std::process::id()));
    std::fs::write(&url_file, "postgres://u:p@127.0.0.1:5432/db").unwrap();
    let env = env_of(&[
        ("VESTRACE_FAULT_POINT", "after_lunch"),
        ("VESTRACE_FAULT_ISOLATION", "ephemeral"),
    ]);

    let error = settings_from(
        &["--database-url-file", url_file.to_str().unwrap()],
        &env,
    )
    .expect_err("an unknown fault point must be refused");

    assert!(error.contains("after_lunch"), "the refusal must name what it got: {error}");
    std::fs::remove_file(&url_file).ok();
}

/// The cheapest way for this whole crate to become worthless is for somebody to
/// fill an awkward field with the answer the suite wants. The suite's own
/// `expected` constructor is that answer.
#[test]
fn the_crate_never_constructs_the_expected_observation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            if text.contains("FaultObservation::expected") {
                offenders.push(path.display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "an observation must be found, not stated; offenders: {offenders:?}"
    );
}

/// A hook that aborts the process must never reach the shipped image. The image
/// builds one package and copies one path, so this is a guard on that staying
/// true.
#[test]
fn the_shipped_image_does_not_build_this_binary() {
    let dockerfile = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../Dockerfile"),
    )
    .unwrap();
    assert!(
        !dockerfile.contains("vestrace-fault-scenario"),
        "the scenario binary must not enter the image"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vestrace-fault-scenario --test contract`
Expected: FAIL — the package does not exist yet.

- [ ] **Step 3: Create the crate**

`crates/vestrace-fault-scenario/Cargo.toml`:

```toml
[package]
name = "vestrace-fault-scenario"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[[bin]]
name = "vestrace-fault-scenario"
path = "src/main.rs"

[lib]
name = "vestrace_fault_scenario"
path = "src/lib.rs"

[dependencies]
serde_json.workspace = true
tokio = { workspace = true, features = ["full"] }
vestrace-application = { path = "../vestrace-application" }
vestrace-domain = { path = "../vestrace-domain" }
vestrace-infrastructure = { path = "../vestrace-infrastructure" }
```

Add `"crates/vestrace-fault-scenario",` to `members` in the root `Cargo.toml`, after `"crates/vestrace-cli",`.

- [ ] **Step 4: Write the settings**

Create `crates/vestrace-fault-scenario/src/settings.rs`:

```rust
use vestrace_domain::external_effects::EffectFaultPoint;

/// What one invocation was asked to do, and whether it is allowed to.
///
/// The invoking contract clears the environment and passes only the three
/// `VESTRACE_FAULT_*` variables, so everything else has to arrive through
/// arguments — and the database URL cannot, because argv is readable by any
/// process on the host. It arrives as a path to a file instead.
pub struct ScenarioSettings {
    point: EffectFaultPoint,
    database_url: String,
    is_child: bool,
}

impl ScenarioSettings {
    pub fn from_env_and_args(
        args: impl Iterator<Item = String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let isolation = env("VESTRACE_FAULT_ISOLATION").unwrap_or_default();
        if isolation != "ephemeral" {
            return Err(format!(
                "refusing to run: this scenario kills processes mid-transaction and \
                 requires an ephemeral isolation, but VESTRACE_FAULT_ISOLATION is \
                 '{isolation}'"
            ));
        }

        let requested = env("VESTRACE_FAULT_POINT").unwrap_or_default();
        let point = match requested.as_str() {
            "after_intent_persistence" => EffectFaultPoint::AfterIntentPersistence,
            "after_authorization_before_dispatch" => {
                EffectFaultPoint::AfterAuthorizationBeforeDispatch
            }
            "after_dispatch_before_receipt" => EffectFaultPoint::AfterDispatchBeforeReceipt,
            "after_receipt_before_outcome_confirmation" => {
                EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation
            }
            "after_outcome_before_run_commit" => EffectFaultPoint::AfterOutcomeBeforeRunCommit,
            other => return Err(format!("unknown fault point '{other}'")),
        };

        let mut url_file = None;
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            if arg == "--database-url-file" {
                url_file = args.next();
            }
        }
        let url_file =
            url_file.ok_or_else(|| "database url file is required: pass --database-url-file".to_owned())?;
        let database_url = std::fs::read_to_string(&url_file)
            .map_err(|error| format!("database url file {url_file} is unreadable: {error}"))?
            .trim()
            .to_owned();
        if database_url.is_empty() {
            return Err(format!("database url file {url_file} is empty"));
        }

        Ok(Self {
            point,
            database_url,
            is_child: env("VESTRACE_FAULT_CHILD").is_some(),
        })
    }

    pub fn point(&self) -> EffectFaultPoint {
        self.point
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn is_child(&self) -> bool {
        self.is_child
    }
}
```

Create `crates/vestrace-fault-scenario/src/lib.rs`:

```rust
//! The program that breaks the external-effect lifecycle on purpose.
//!
//! Its whole value is that the observation it prints is *found* rather than
//! *stated*: every field is read back from persisted state or counted by a
//! party outside the process under test. A build of this crate that constructs
//! the suite's expected answer would clear the release gate and prove nothing,
//! which is why a test forbids it by name.

pub mod settings;

pub use settings::ScenarioSettings;
```

Create `crates/vestrace-fault-scenario/src/main.rs`:

```rust
use vestrace_fault_scenario::ScenarioSettings;

fn main() {
    let settings =
        match ScenarioSettings::from_env_and_args(std::env::args().skip(1), &|name| {
            std::env::var(name).ok()
        }) {
            Ok(settings) => settings,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        };
    // Task 3 gives the child its work and Task 4 gives the parent its own.
    eprintln!(
        "scenario not yet implemented for {:?} (child: {})",
        settings.point(),
        settings.is_child()
    );
    std::process::exit(3);
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vestrace-fault-scenario --test contract`
Expected: PASS, 6 tests.

- [ ] **Step 6: Mutation-prove the isolation guard**

Change the isolation comparison to `if false`, run the suite, and confirm `any_isolation_but_ephemeral_is_refused` fails and no other case does. Restore it.

- [ ] **Step 7: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-fault-scenario
git add crates/vestrace-fault-scenario Cargo.toml Cargo.lock
git commit -m "feat(fault-scenario): refuse to run outside an ephemeral isolation"
```

---

### Task 2: The adapter stub that counts dispatches

**Files:**
- Create: `crates/vestrace-fault-scenario/src/adapter_stub.rs`
- Modify: `crates/vestrace-fault-scenario/src/lib.rs`
- Test: `crates/vestrace-fault-scenario/tests/adapter_stub.rs`

**Interfaces:**
- Produces: `pub struct AdapterStub` with `pub async fn start() -> Result<Self, String>`, `pub fn dispatch_url(&self) -> String`, `pub fn read_back_url(&self) -> String`, `pub fn dispatch_count(&self) -> usize`, `pub async fn shutdown(self)`

**Why this exists:** `retry_attempted` is the one field the process under test cannot be trusted to report about itself. A second dispatch for one intent is a retry, and only the party being dispatched *to* can say it happened. The stub is that party, and it survives the child's death because it lives in the parent.

- [ ] **Step 1: Write the failing test**

Create `crates/vestrace-fault-scenario/tests/adapter_stub.rs`:

```rust
use vestrace_fault_scenario::AdapterStub;

/// The count is the evidence for `retry_attempted`, so it has to be exact and
/// it has to survive the death of whatever was dispatching.
#[tokio::test]
async fn the_stub_counts_every_dispatch_it_receives() {
    let stub = AdapterStub::start().await.unwrap();
    assert_eq!(stub.dispatch_count(), 0);

    let client = reqwest::Client::new();
    client.post(stub.dispatch_url()).send().await.unwrap();
    client.post(stub.dispatch_url()).send().await.unwrap();

    assert_eq!(stub.dispatch_count(), 2, "a retry is a second dispatch");
    stub.shutdown().await;
}

/// Read-back is how reconciliation asks what happened. A stub that could not
/// answer would make every UNKNOWN unresolvable for a reason belonging to the
/// harness rather than to the system.
#[tokio::test]
async fn the_stub_answers_read_back_for_what_it_received() {
    let stub = AdapterStub::start().await.unwrap();
    let client = reqwest::Client::new();
    client.post(stub.dispatch_url()).send().await.unwrap();

    let body = client
        .get(stub.read_back_url())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(body.contains("\"dispatches\":1"), "unexpected read-back body: {body}");
    stub.shutdown().await;
}
```

Add `reqwest` and `tokio` (with `macros`, `rt-multi-thread`) to the crate's `[dev-dependencies]` from the workspace.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-fault-scenario --test adapter_stub`
Expected: FAIL — `AdapterStub` does not exist.

- [ ] **Step 3: Implement the stub**

Create `src/adapter_stub.rs` with a `tokio::net::TcpListener` bound to `127.0.0.1:0`, an `Arc<AtomicUsize>` counter, and a spawned accept loop answering two paths: `POST /dispatch` increments the counter and returns `200` with a JSON body naming an acknowledgement, and `GET /effects` returns `{"dispatches":N}`. Keep the parser minimal — read until `\r\n\r\n`, match the request line — since this serves exactly two shapes and a dependency on a web framework would be the larger risk.

Export it from `lib.rs`: `pub mod adapter_stub;` and `pub use adapter_stub::AdapterStub;`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vestrace-fault-scenario --test adapter_stub`
Expected: PASS, 2 tests.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-fault-scenario
git add crates/vestrace-fault-scenario Cargo.toml Cargo.lock
git commit -m "feat(fault-scenario): count dispatches from outside the process under test"
```

---

### Task 3: The child that dies at the requested point

**Files:**
- Create: `crates/vestrace-fault-scenario/src/child.rs`
- Modify: `crates/vestrace-fault-scenario/src/lib.rs`, `src/main.rs`
- Test: `crates/vestrace-fault-scenario/tests/child_points.rs`

**Interfaces:**
- Consumes: `ScenarioSettings` from Task 1, `AdapterStub` from Task 2
- Produces: `pub async fn run_child(settings: &ScenarioSettings, dispatch_url: &str) -> !` and `pub enum ChildStage { IntentPersisted, Authorized, Dispatched, ReceiptPersisted, OutcomeSettled }` with `pub fn aborts_at(point: EffectFaultPoint) -> ChildStage`

**Read before writing:** the lifecycle this drives is `PerformExternalEffectService::perform` in `crates/vestrace-application/src/external_effects.rs:88-131`, whose five numbered comments are the five stages. A real call site is `crates/vestrace-http/src/api/effects.rs:137`. An `ExternalEffectIntent` fixture with every required argument is `tests/e1_e4_connect.rs:65-88` — copy its shape, changing the target to the stub's dispatch URL. Do not invent a shorter constructor.

- [ ] **Step 1: Write the failing test**

Create `crates/vestrace-fault-scenario/tests/child_points.rs`:

```rust
use vestrace_domain::external_effects::EffectFaultPoint;
use vestrace_fault_scenario::{ChildStage, aborts_at};

/// The abort site is the whole experiment. If it drifts by one stage the
/// observation is about a different fault than the one reported, and nothing
/// downstream can tell.
#[test]
fn each_point_aborts_at_its_own_stage() {
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterIntentPersistence),
        ChildStage::IntentPersisted
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterAuthorizationBeforeDispatch),
        ChildStage::Authorized
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterDispatchBeforeReceipt),
        ChildStage::Dispatched
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation),
        ChildStage::ReceiptPersisted
    );
    assert_eq!(
        aborts_at(EffectFaultPoint::AfterOutcomeBeforeRunCommit),
        ChildStage::OutcomeSettled
    );
}

/// Every point must have a stage. A `_ =>` arm added later would silently send
/// a new point to whatever stage happened to be last.
#[test]
fn every_required_point_has_a_stage() {
    for point in EffectFaultPoint::required_points() {
        let _ = aborts_at(point);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-fault-scenario --test child_points`
Expected: FAIL — `ChildStage` and `aborts_at` do not exist.

- [ ] **Step 3: Implement the stage map and the child**

In `src/child.rs`, define `ChildStage` (deriving `Clone, Copy, Debug, Eq, PartialEq`) and `aborts_at` as an exhaustive `match` over `EffectFaultPoint` with **no wildcard arm**, mirroring the table above.

Then write `run_child`, which:

1. connects `PgStore` to `settings.database_url()`,
2. builds the same service set the HTTP layer builds at `crates/vestrace-http/src/api/effects.rs:137` — `PerformExternalEffectService::new(effects, authorization)` over `PgExternalEffectRepository`,
3. builds an intent shaped like `tests/e1_e4_connect.rs:65-88` whose target is `dispatch_url`,
4. walks the lifecycle stage by stage, and after completing the stage that `aborts_at(settings.point())` names, calls `std::process::abort()`.

`abort()` and not `exit()`: an orderly exit flushes buffers and completes transactions, which is a shutdown path nobody asked about. The point is what survives a process that stopped without being asked.

For the stages beyond `perform` — `ReceiptPersisted` and `OutcomeSettled` — drive the reconciliation and `EffectOutcomeDeliveryService` steps that follow it; `find_undelivered_outcomes` and `mark_outcome_delivered` in `crates/vestrace-application/src/effect_outcome_delivery.rs` are the boundary point 5 sits on.

Wire `main.rs` to call `run_child` when `settings.is_child()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vestrace-fault-scenario --test child_points`
Expected: PASS, 2 tests.

- [ ] **Step 5: Mutation-prove the stage map**

Change `AfterDispatchBeforeReceipt` to map to `ChildStage::ReceiptPersisted`, run the suite, confirm `each_point_aborts_at_its_own_stage` fails, restore.

- [ ] **Step 6: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-fault-scenario
git add crates/vestrace-fault-scenario
git commit -m "feat(fault-scenario): abort the real lifecycle at the requested point"
```

---

### Task 4: Deriving the observation from what survived

**Files:**
- Create: `crates/vestrace-fault-scenario/src/observe.rs`
- Modify: `crates/vestrace-fault-scenario/src/lib.rs`
- Test: `crates/vestrace-fault-scenario/tests/observation.rs` (requires a database)

**Interfaces:**
- Consumes: `AdapterStub::dispatch_count` from Task 2
- Produces: `pub async fn observe(store: &PgStore, context: &RequestContext, intent_id: ExternalEffectIntentId, point: EffectFaultPoint, dispatch_count: usize) -> Result<FaultObservation, String>`

**Why each field comes from where it does:**

| Field | Source | Why not elsewhere |
|---|---|---|
| `status` | the persisted effect record | what the child believed it set died with it |
| `receipt_persisted` | `find_receipt` returns a row or does not | a boolean the parent sets is a claim, not a reading |
| `reconciliation_started` | `find_reconciliation` / the candidate query | calling recovery is not the same as recovery having started one |
| `retry_attempted` | the stub's dispatch count exceeding one | only the dispatched-to party can say the world was touched twice |

- [ ] **Step 1: Write the failing test**

Create `crates/vestrace-fault-scenario/tests/observation.rs`. It requires `DATABASE_URL`; skip with a printed notice when absent, matching how the repository's other database-backed tests behave. Seed an effect record in a known state, then assert `observe` reports exactly that state: an intent with no receipt gives `receipt_persisted: false`; a stub count of `2` gives `retry_attempted: true`; a stub count of `1` gives `false`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-fault-scenario --test observation`
Expected: FAIL — `observe` does not exist.

- [ ] **Step 3: Implement `observe`**

Read each field from the repository per the table above. `retry_attempted` is `dispatch_count > 1`. Return `Err` rather than a default when the intent cannot be found: an observation about an effect that is not there describes nothing.

- [ ] **Step 4: Run test to verify it passes**

Run with a database URL set.
Expected: PASS.

- [ ] **Step 5: Mutation-prove each derivation**

Replace each of the four derivations with a constant in turn, run the suite, and record which case fails for each. A derivation whose mutation kills nothing is unpinned — report it rather than adjusting the expectation.

- [ ] **Step 6: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-fault-scenario
git add crates/vestrace-fault-scenario
git commit -m "feat(fault-scenario): read the observation back out of what survived"
```

---

### Task 5: The parent, and the runtime that reads it

**Files:**
- Modify: `crates/vestrace-fault-scenario/src/main.rs`
- Test: `crates/vestrace-fault-scenario/tests/output_contract.rs`

**Interfaces:**
- Consumes: everything above
- Produces: the process contract — one JSON object on stdout, exit 0 on success

- [ ] **Step 1: Write the failing test**

Create `crates/vestrace-fault-scenario/tests/output_contract.rs`, asserting that a JSON object with the five fields this program prints is accepted by the reader in `crates/vestrace-application/src/fault_runtime.rs`. Build the JSON exactly as the program emits it and parse it with `serde_json` into a struct with the same field names and `deny_unknown_fields`, so an extra or renamed field fails here rather than in an environment that takes minutes to reach.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vestrace-fault-scenario --test output_contract`
Expected: FAIL — the emitter does not exist.

- [ ] **Step 3: Implement the parent**

In `main.rs`, when `!settings.is_child()`:

1. start the `AdapterStub`,
2. prepare a clean effect fixture and remember its id,
3. spawn this same executable with `VESTRACE_FAULT_CHILD=1` and the same `VESTRACE_FAULT_*` variables, passing the same `--database-url-file`,
4. wait for it and require that it did **not** exit successfully — a child that finished normally did not crash, and reporting an observation from it would describe a run that never faulted,
5. run the deployment's own recovery: `ExternalEffectRecoveryService::new(repository, read_back)` — the construction the worker uses at `crates/vestrace-cli/src/commands/worker.rs:369` — followed by outcome delivery,
6. call `observe(...)` and print its JSON on stdout, exit 0.

Point 3's requirement is why the stub must answer read-backs: reconciliation asks the world what happened, and a stub that could not answer would make the effect unresolvable for a reason belonging to the harness.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vestrace-fault-scenario --test output_contract`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p vestrace-fault-scenario
git add crates/vestrace-fault-scenario
git commit -m "feat(fault-scenario): drive the child, recover, and report what is there"
```

---

### Task 6: The suite against a real ephemeral database, and the record

**Files:**
- Create: `tests/effect_fault_scenario_e2e.rs`
- Create: `docs/documentation-gap-delta-2026-08-20-a-fault-nobody-had-injected.md`
- Modify: `docs/documentation-status-v0.2.md`

- [ ] **Step 1: Write the end-to-end harness**

Create `tests/effect_fault_scenario_e2e.rs`, `#[ignore]` by default because it needs Docker and kills processes. It provisions an ephemeral PostgreSQL 17 the way `tests/idw_014_shared_read_postgres.rs` does, writes the URL to a temporary file, builds `ProcessFaultInjectionRuntime` pointed at the scenario binary with `--database-url-file`, wraps it in `ConfiguredEffectFaultScenarioExecutor` and `ExternalEffectFaultSuiteService`, runs all five points, and prints the resulting `FaultSuiteDecision` with every failure it names.

- [ ] **Step 2: Run it**

Run: `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e -- --ignored --nocapture`

**Record whatever it says.** A failing suite is the expected first result and is the finding. Do not change the scenario to make the suite pass; if a point fails, capture the decision's failure strings verbatim for the write-up.

- [ ] **Step 3: Write the delta**

Create `docs/documentation-gap-delta-2026-08-20-a-fault-nobody-had-injected.md` in the house style of `docs/documentation-gap-delta-2026-08-19-a-gate-nobody-could-run.md`: what existed, what did not, the trap of the twenty-line liar, the abort-site table, where each observation field comes from and why, the mutation results, the suite's actual verdict, and a **What this does not do** section covering at minimum — only process death is exercised, not container or node failure; Postgres is assumed to keep its own promises; the suite does not run in CI; and the remaining three evidence sources still have no producers.

- [ ] **Step 4: Add the status paragraph**

Insert one dense paragraph into `docs/documentation-status-v0.2.md` immediately before the line beginning "The pinned implementation baseline remains", carrying the suite's real verdict and the same non-claims.

- [ ] **Step 5: Verify and commit**

```bash
bash ./scripts/foundation-doc-truth.sh
git add tests/effect_fault_scenario_e2e.rs docs/
git diff --cached --check
git commit -m "test(fault): run the fault suite against an ephemeral deployment"
```

---

## Self-Review

**Spec coverage:** §3 separate crate and image exclusion → Task 1 (crate) and its Dockerfile guard. §4 invocation contract, URL file, isolation refusal → Task 1. §5 parent/child sequence and `abort()` → Tasks 3 and 5. §5.1 the point table, including that 4 and 5 lie outside `perform` → Task 3. §5.2 recovery must include the worker's construction → Task 5 Step 3, which names the worker's line. §6 honest derivation and the adapter stub → Tasks 2 and 4, with the no-expected guard in Task 1. §7 test strategy → each task's own steps plus Task 6. §8 non-goals → carried into Task 6's required write-up content.

**Placeholder scan:** Tasks 2, 4 and 6 describe their implementations rather than transcribing them, and each names the exact existing file to model on (`tests/idw_014_shared_read_postgres.rs` for provisioning, `crates/vestrace-http/src/api/effects.rs:137` and `tests/e1_e4_connect.rs:65-88` for the lifecycle, `crates/vestrace-cli/src/commands/worker.rs:369` for recovery). This is deliberate: these constructions take dozens of arguments that already exist correctly in the repository, and transcribing them from memory into a plan is how they drift.

**Type consistency:** `ScenarioSettings::{from_env_and_args, point, database_url, is_child}` defined in Task 1 and used in Tasks 3 and 5. `AdapterStub::{start, dispatch_url, read_back_url, dispatch_count, shutdown}` defined in Task 2 and used in Tasks 4 and 5. `ChildStage`/`aborts_at` defined in Task 3. `observe` defined in Task 4 and called in Task 5.

**Known risk the plan carries:** Task 3 is the largest and least transcribed step, because the lifecycle it drives spans three services and its intent constructor takes eighteen arguments. If it proves larger than one task in execution, split it at the `perform` boundary — points 1-3 in one task, points 4-5 in another — rather than letting a single task grow past its own test cycle.
