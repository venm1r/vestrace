# Vestrace v1.0 G0-01 Protocol Lock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the reproducible protocol baseline required by G0: exact AG-UI and A2A dependency pins with provenance, executable AG-UI runtime-schema/profile fixtures, and the canonical OpenAI-compatible `q1` manifest and image fixture, all sealed by one deterministic protocol lock.

**Architecture:** Keep protocol inputs under `schemas/` and executable conformance fixtures under `tests/fixtures/`. A deterministic Node generator reads repository files and lockfiles, computes byte-level SHA-256 digests, and emits one canonical `schemas/protocol-lock.json`; check mode never accesses the network and fails on any drift. AG-UI conformance executes the public 0.0.58 Zod schemas. A2A dependency pins compile behind the existing HTTP/server and infrastructure/client adapter boundaries but do not add routes or behavior in this package.

**Tech Stack:** Node 22 built-ins, `@ag-ui/core` 0.0.58, `@ag-ui/client` 0.0.58, `zod-to-json-schema` 3.25.2, Rust 1.85, Cargo lock format v4, `a2a-lf` 0.3.0, `a2a-client-lf` 0.2.1, `a2a-server-lf` 0.4.1, JSON Schema draft-07.

**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` sections 2, 6.4.1, 7, 8, 14.1, and G0; frozen SHA-256 `B31B5BE62504E1A65F411CD31B446CD41B3D032B7282F1AA42907706EF9C1473`.

**External corpus impact:** `compatibility seam` against registered digest `762C25F3781F1BA30783E218DB64AA4DC9536ADD490F61255056BFE8635D8594`. P01 adopts only a repo-local manifest and the frozen spec's existing canonical-Run/projection and model-request-reconstruction seams. The RFC, roadmap, generalized runtime, Organ, cognition, and supply-chain proposals are `defer post-v1`; derived JSON/HTML files are provenance companions, not independent authorities; missing referenced documents remain recorded as missing.

## Global Constraints

- Scope is P01 only. Do not add or change HTTP routes, migrations, application services, provider calls, workers, console screens, Docker/installer behavior, or release status.
- Preserve `PLAN.md`, `docs/superpowers/plans/2026-08-25-console-interface-redesign.md`, the frozen spec, and all unrelated dirty files byte-for-byte.
- Do not commit, push, deploy, request API credentials, or contact LM Studio or a third-party model endpoint.
- Registry access is allowed only for the exact public packages named here. It requires no API key. If a registry unexpectedly requires credentials, stop that install instead of adding placeholders or asking for a secret.
- Exact versions use `=` in Cargo workspace dependencies and bare versions without `^` or `~` in npm dependencies. Lockfile changes are limited to the named packages and their required transitive dependency resolution.
- Never fabricate integrity, checksum, provenance, schema, or fixture digests. Generate them from exact bytes and fail closed when observed metadata differs from this plan.
- `schemas/protocol-lock.json` is generated, but reviewed and committed as a release input in a later clean release checkout. `--check` must work offline after dependencies are installed.
- Canonical JSON outputs use UTF-8 without BOM, two-space indentation, lexicographically ordered object keys, preserved array order, and one trailing LF.
- No runtime payload, prompt, response, secret, credential, or model output belongs in this package.
- Replace the usual per-task commit step with a scoped diff/evidence checkpoint because the operator prohibited commits.

## Pinned Provenance Inputs

The generator treats these values as fixed reviewed inputs and verifies the corresponding lockfile/archive fields. A mismatch is a protocol-change failure, not an invitation to update the plan in place.

| Input | Version/release | Integrity/checksum | Repository and immutable provenance evidence |
| --- | --- | --- | --- |
| `@ag-ui/core` | `0.0.58` | `sha512-XgGb7YmhV+yMBaEmlrpsd5S+nUxq0JgSegss2t4gIFR1j7w3w0ibtKfRgcQHWeMvwZxcT5S28VEEarqtgxYYHw==` | package metadata `https://registry.npmjs.org/@ag-ui%2fcore/0.0.58`; repository `git+https://github.com/ag-ui-protocol/ag-ui.git`; attestation `https://registry.npmjs.org/-/npm/v1/attestations/@ag-ui%2fcore@0.0.58`; SLSA resolved URI `git+https://github.com/ag-ui-protocol/ag-ui@refs/heads/main`; resolved git commit `0c0b88a3fe087a631decd6225efaf40e068e4449` |
| `@ag-ui/client` | `0.0.58` | `sha512-9tAUJ6Ot0y2f5Va7xGFhUSO5OPAjilMsPdmKNmAUR774LKWcvNpriJ6mqH3p/ewUue1zfoUo4vpX+9Xo/zsuzA==` | package metadata `https://registry.npmjs.org/@ag-ui%2fclient/0.0.58`; repository `git+https://github.com/ag-ui-protocol/ag-ui.git`; attestation `https://registry.npmjs.org/-/npm/v1/attestations/@ag-ui%2fclient@0.0.58`; SLSA resolved URI `git+https://github.com/ag-ui-protocol/ag-ui@refs/heads/main`; resolved git commit `0c0b88a3fe087a631decd6225efaf40e068e4449` |
| A2A specification | `v1.0.1`, wire `1.0` | release identity is the pinned tag plus commit; P01 vendors no separate spec archive | repository `https://github.com/a2aproject/A2A.git`; release `https://github.com/a2aproject/A2A/releases/tag/v1.0.1`; full tag commit `3303592588e388e62e0f69f701af531d2f4e3991` whose approved abbreviation is `3303592` |
| `a2a-lf` | `0.3.0` | crates.io SHA-256 `7fb24275cca126dc3301d272eef07bd4cefd87f9a7dd5d6f27200fe87e8a83d0` | normalized archive repository `https://github.com/a2aproject/a2a-rs`; archive `https://crates.io/api/v1/crates/a2a-lf/0.3.0/download`; `.cargo_vcs_info.json` commit `73c72eeed997fddf5a00be44068575437e7f3f82` |
| `a2a-client-lf` | `0.2.1` | crates.io SHA-256 `f68a06a40df172bb5ae0f25e3d49e922f0d33f8af49d0b1276307857dbd28ad2` | normalized archive repository `https://github.com/a2aproject/a2a-rs`; archive `https://crates.io/api/v1/crates/a2a-client-lf/0.2.1/download`; `.cargo_vcs_info.json` commit `0b19af0e2805455c01f8f2b7fb52c5d5ec1bce95` |
| `a2a-server-lf` | `0.4.1` | crates.io SHA-256 `c4df08dff9607c4045c892b58f3824bb215262a37bce33f1ab42a72a5c9acd51` | normalized archive repository `https://github.com/a2aproject/a2a-rs`; archive `https://crates.io/api/v1/crates/a2a-server-lf/0.4.1/download`; `.cargo_vcs_info.json` commit `0b19af0e2805455c01f8f2b7fb52c5d5ec1bce95` |

---

### Task 1: Capture the dirty baseline and register the external planning corpus

**Files:**
- Create first: `docs/development-evidence/v1-g0-01-preflight.json`
- Create: `scripts/p01-scope.mjs`
- Create: `scripts/verify-dirty-baseline.mjs`
- Create: `scripts/external-corpus-manifest.mjs`
- Create: `docs/external-corpus/vestrace-docss-2026-08-19.manifest.json`
- Create: `tests/external_corpus_manifest.rs`

**Interfaces:**
- The preflight JSON records HEAD, UTC capture time, Base64 and SHA-256 of raw NUL-delimited porcelain-v1 status bytes, exact `change_scope_paths`, exact `protected_authority_paths`, and one sorted entry per pre-existing dirty file: status, repository-relative path, byte count, and lowercase working-tree SHA-256 or `absent`.
- `scripts/p01-scope.mjs` exports the exact two arrays. `change_scope_paths` includes only implementation/evidence outputs named by P01. `protected_authority_paths` includes both new plan files and the frozen spec; none may appear in `change_scope_paths`.
- `verifyDirtyBaseline(repoRoot, preflight)` compares every protected authority and every baseline path outside `change_scope_paths` byte-for-byte, rejects any new dirty path outside that change scope, and requires the preflight arrays to equal `p01-scope.mjs`. It never restores, deletes, stages, or rewrites a path.
- `buildExternalCorpusManifest(sourceRoot)` accepts only the exact twelve-name snapshot below, computes source bytes/digests, attaches the fixed role/provenance/decision/derivation map, and returns canonical manifest schema version `1`.
- CLI `--write <repo-root> <source-root>` writes only the repo-local manifest; `--check <repo-root>` validates its internal structure/digests without the machine-local corpus; `--verify-source <repo-root> <source-root>` compares the registered manifest to the mutable source path without writing.
- Unknown/missing CLI arguments, a thirteenth/missing file, a byte/digest mismatch, or an unknown filename fail nonzero before any write.
- The Rust test recomputes the aggregate digest from sorted UTF-8 lines `name<TAB>bytes<TAB>lowercase-file-sha256<LF>` and validates roles, decisions, provenance, missing references, and the absence of a runtime/build dependency on the external path.

The exact registered entries are:

| Name | Bytes | SHA-256 | Role | Decision |
| --- | ---: | --- | --- | --- |
| `AMENDMENT-2026-08-19.md` | 2987 | `798dea48d277c545050e9637adc568a7a6c47aeff836749dfcdbc7c86b298df6` | proposed post-v1 contract | `defer_post_v1` |
| `RFC-Model-Request-Reconstruction-and-Context-Surface-2026-08-19.md` | 23397 | `f90163409eae865c818e572936f936215f955537e56f5d4a5b4b440bdd4ef423` | proposed post-v1 contract | `defer_post_v1` |
| `Vestrace-post-v1-gate-roadmap-2026-08-12.md` | 62896 | `b7a5b959ad1f055dea52b00a1dd9326a69d401199ced3c2a4d919188e018fdeb` | strategic post-v1 roadmap | `defer_post_v1` |
| `Vestrace-post-v1-gate-roadmap-artifact-2026-08-12.json` | 87553 | `e430fa9c20519d43398f2504561d19a8b6fccfb3b10b595543b1658b86f12b0b` | derived roadmap data companion | `defer_post_v1` |
| `Vestrace-post-v1-gate-roadmap-report-2026-08-12.html` | 517604 | `d48231290e0c6ffbb7c81e6e2af283af616e5ee4d68ab2e96bfc9de7876cdf99` | derived roadmap render companion | `defer_post_v1` |
| `Vestrace-post-v1-gate-roadmap-source-notes-2026-08-12.md` | 2533 | `6e6622c914c9ecd20fbea3cd7d8d9b066934e74a1b10faac7c59c140928c5c3c` | roadmap source notes with missing-reference warning | `defer_post_v1` |
| `Vestrace-prospective-technologies-artifact-2026-08-12.json` | 90605 | `62550dfc4e41a371b626166803a6d427a5d2a4330f645dfc2e69c261c25e070d` | derived research data companion | `defer_post_v1` |
| `Vestrace-prospective-technologies-report-2026-08-12.html` | 624448 | `a9f2f93c9846654f8e0029c03880b26e248ce7e5eaa7943c063521e7a837f835` | derived research render companion | `defer_post_v1` |
| `Vestrace-prospective-technologies-research-2026-08-12.md` | 66180 | `ca4c9e0f8dea834d4b5d430cac7fb77d1b17c22003c58254aeaabaf94bd103ff` | non-normative donor research | `compatibility_seam` |
| `Vestrace-reference-systems-artifact-2026-08-11.json` | 72429 | `0aa249d87edb176e8fba43a86933f438995259139c6a9017084fd2670db18de2` | derived research data companion | `defer_post_v1` |
| `Vestrace-reference-systems-report-2026-08-11.html` | 485361 | `f1a58da57c079703f9c478e448682b73cd0cc53de93265e80170dc3d84a01ed9` | derived research render companion | `defer_post_v1` |
| `Vestrace-reference-systems-research-2026-08-11.md` | 55105 | `db2b404618a08720b4a8a7b89bc07998e72ddea4ea2119d23795949c86481c9f` | non-normative donor research | `compatibility_seam` |

Every entry records provenance `operator-supplied snapshot at E:\Junk\AI\vestrace-docss, registered from the approved spec on 2026-08-26`. Each derived JSON/HTML companion has `derived_from` pointing to its matching roadmap or research Markdown file and `independent_authority: false`. The RFC/amendment decision note says post-v1 architecture-contract reconciliation is required. The roadmap note says its v1/`TRUSTED`-complete premise is unverified and supplies no evidence. The two donor-research decisions record the borrowed invariants `canonical truth remains distinct from projections` and `single-writer/CAS/lease/fencing may preserve declared ownership`, and reject `second durable runtime`, `message-bus or graph truth`, `cloud/microservice requirement`, and `owner assertion as release evidence`. The manifest also records these unresolved references exactly: `vestrace-brain-face-organ-system-model.md` and `ADR-0011 — Brain–Face–Organ System Decomposition`. It must state that neither file is present and no contents were inferred.

- [ ] **Step 1: Capture preflight before the first implementation write/install**

Run read-only commands first:

```powershell
git -c safe.directory=E:/Soft/vestrace rev-parse HEAD
git -c safe.directory=E:/Soft/vestrace status --porcelain=v1 -z --untracked-files=all
```

For every returned path, read only that working-tree path and calculate the baseline record described above; use `absent` for a deleted path. Preserve the exact raw NUL-delimited status bytes as Base64 and calculate their SHA-256. `change_scope_paths` enumerates every exact implementation/evidence file named by P01 and contains no directory-wide wildcard. It explicitly excludes `docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md`, `docs/superpowers/plans/2026-08-26-vestrace-v1-g0-01-protocol-lock.md`, and the frozen spec; those three are `protected_authority_paths` and retain their captured byte digests. Persist the observed values in `docs/development-evidence/v1-g0-01-preflight.json` using `apply_patch` as the first implementation write. Do not stage files. If status changes between capture and persistence, discard the capture and repeat before continuing.

- [ ] **Step 2: Write the failing external-corpus contract test**

Assert the exact twelve entries/table values, aggregate digest `762c25f3781f1ba30783e218db64aa4dc9536add490f61255056bfe8635d8594`, allowed decision enum `adopt_now | compatibility_seam | defer_post_v1 | reject`, fixed provenance/derivation/borrowed/rejected fields, two explicit missing references, and `source_path_is_runtime_dependency == false`.

- [ ] **Step 3: Run the corpus test and observe RED**

```powershell
cargo test --test external_corpus_manifest -- --nocapture
```

Expected: failure because the repo-local manifest/generator do not exist.

- [ ] **Step 4: Implement and generate the manifest**

Use Node built-ins only. First implement `p01-scope.mjs` and `verify-dirty-baseline.mjs`; test protected-authority mutation plus in-scope/out-of-scope changed/new/deleted fixtures. Encode the exact corpus role/decision mapping above in the manifest generator, derive bytes/digests from the supplied source for `--write`, canonicalize JSON, and refuse a non-exact file set. Then run:

```powershell
node scripts/external-corpus-manifest.mjs --write . 'E:\Junk\AI\vestrace-docss'
node scripts/external-corpus-manifest.mjs --verify-source . 'E:\Junk\AI\vestrace-docss'
node scripts/external-corpus-manifest.mjs --check .
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-01-preflight.json
```

- [ ] **Step 5: Run the corpus contract and observe GREEN**

```powershell
cargo test --test external_corpus_manifest -- --nocapture
```

Expected: exact entry and aggregate checks pass. A CLI test runs `--check` against a temporary repo containing only the committed manifest and no external corpus path, proving the machine-local directory is not a later planning/build/runtime dependency.

- [ ] **Step 6: Review the scoped Task 1 diff**

Confirm the preflight predates every install/write, the repo-local manifest is complete, no external bytes were copied into the repository, no external statement became v1 release evidence, and the machine-local path appears only as provenance/explicit verification input.

---

### Task 2: Canonical OpenAI-compatible q1 manifest and deterministic image fixture

**Files:**
- Create: `tests/protocol_q1_manifest.rs`
- Create: `tests/protocol_q1_marker.test.mjs`
- Create: `scripts/generate-openai-q1-marker.mjs`
- Create: `tests/fixtures/openai-q1/marker.png`
- Create: `schemas/openai-compatible/openai-chat-completions-v1-q1.json`

**Interfaces:**
- `generateMarkerPng()` returns the one canonical PNG byte buffer without filesystem or clock input.
- CLI `--write <repo-root>` writes only the marker; `--check <repo-root>` compares regenerated bytes without writing. Unknown/missing arguments fail nonzero before filesystem mutation.
- The q1 manifest exposes `profile_id`, `schema_version`, `bounds`, `connection_probes`, `model_probes`, and `fixtures`.
- Probe arrays preserve wire order; object-key ordering is canonical formatting only and never changes probe order.
- The Rust contract test independently checks the file digest relationship, exact ordinals, bounds, prerequisites, required/optional roles, request templates, and safe structural oracles.

- [ ] **Step 1: Write the failing q1 contract test**

Create `tests/protocol_q1_manifest.rs`. Read the manifest at runtime through `env!("CARGO_MANIFEST_DIR")` so a missing file is a test failure rather than an `include_bytes!` compile failure. Assert:

- `profile_id == "openai-chat-completions-v1/q1"` and `schema_version == 1`;
- connection ordinals are exactly `00`, `10`; model ordinals are exactly `15`, `20`, `30`, `35`, `40`, `50`, `60`, `70`, `80`, `90`;
- ordinal `10` is `GET /models`, has the 1 MiB/4096-entry/1..256-byte limits and fifteen-minute validity;
- ordinals `20..80` target `/chat/completions`; ordinal `90` targets `/embeddings`;
- non-stream, SSE, tool-argument, event-count, idle/total-time, and embedding-dimension bounds match section 6.4.1;
- `35` depends on `30`; `50` and `60` depend on `40`;
- `20` is required for chat, `90` is required for embeddings, and optional probe outcomes remain explicit;
- each request template contains its named nonce marker and each oracle is structural, bounded, and free of recorded provider body content;
- the fixture path is exactly `tests/fixtures/openai-q1/marker.png`, its media type is `image/png`, and its recorded lowercase SHA-256 equals the actual fixture bytes.

- [ ] **Step 2: Run the q1 test and observe RED**

```powershell
cargo test --test protocol_q1_manifest -- --nocapture
```

Expected: the test fails because the canonical q1 manifest/fixture do not exist.

- [ ] **Step 3: Implement the deterministic PNG generator**

Implement a dependency-free Node module that constructs a bounded RGB PNG using fixed dimensions, fixed white background, black 5x7 bitmap glyphs, fixed scale/padding, zlib level, PNG filter, chunk order, and CRC32. Render `VESTRACE_Q1_IMAGE` visibly. Export `generateMarkerPng()` for tests and implement the two CLI modes above.

The generator must produce byte-identical output twice in one process and across two processes. `tests/protocol_q1_marker.test.mjs` exercises both modes in a temporary root, proves `--check` does not change file timestamps/bytes, and proves missing/unknown arguments fail without a file. The generator must not read fonts, locale, time, randomness, environment-specific image libraries, or network resources.

- [ ] **Step 4: Write the canonical q1 manifest**

Transcribe the approved section 6.4.1 semantics into structured JSON; do not abbreviate a request or oracle as prose such as `standard chat request`. Use exact closed fields for:

- nonce derivation: first 96 SHA-256 bits encoded as 24 lowercase hex characters from `(QualificationJobId || profile_digest || probe_ordinal)`;
- request templates for `models_list`, exact-text chat, SSE, streamed usage, forced tool call, continued tool result, parallel tools, strict structured output, inline data-URL image input, and two-string embeddings;
- pass/oracle shapes, prerequisite outcomes, required capability mapping, size/time/count/dimension bounds, response classifications, and the no-automatic-retry rule;
- the generated image's actual SHA-256 digest.

The manifest must not contain a self-digest; its byte digest belongs in `schemas/protocol-lock.json`.

- [ ] **Step 5: Run the q1 contract and observe GREEN**

```powershell
node scripts/generate-openai-q1-marker.mjs --check .
node --test tests/protocol_q1_marker.test.mjs
cargo test --test protocol_q1_manifest -- --nocapture
```

Expected: deterministic fixture check and every q1 contract assertion pass.

- [ ] **Step 6: Review the scoped Task 2 diff**

Confirm the manifest contains all twelve ordered ordinals, no provider output, no implicit Responses API fallback, no retry-after-dispatch behavior, and no credential-bearing request example.

---

### Task 3: Exact AG-UI dependency pins and executable runtime-schema/profile extraction

**Files:**
- Modify: `apps/console/package.json`
- Modify: `apps/console/package-lock.json`
- Create: `scripts/extract-ag-ui-runtime-schemas.mjs`
- Create: `schemas/ag-ui/0.0.58/runtime-schemas.json`
- Create: `schemas/ag-ui/vestrace-v1-profile.json`
- Create: `apps/console/tests/agUiProtocolProfile.test.mjs`

**Interfaces:**
- Runtime schema extraction imports only public exports from `@ag-ui/core`: `RunAgentInputSchema`, `MessageSchema`, `StateSchema`, `ContextSchema`, `ToolSchema`, `InterruptSchema`, `InputContentSchema`, and `EventSchemas`.
- `extractRuntimeSchemas(repoRoot)` resolves packages from `apps/console/package.json`, converts those exact Zod roots to deterministic JSON Schema draft-07 documents, and returns one canonical bundle with upstream package/version metadata.
- `buildVestraceAgUiProfile()` derives the supported event names from pinned `EventType`, explicitly excludes `RAW` and `REASONING_ENCRYPTED_VALUE`, and records the complete request/state/context/tool/multimedia/interrupt surface.
- The profile test executes upstream `.safeParse()` for valid and mutation-invalid request/event fixtures; it does not treat generated JSON Schema as a replacement authority for the upstream runtime validators.

- [ ] **Step 1: Write the failing AG-UI profile test**

Cover these behaviors with Node's built-in test runner:

- exact package versions are `0.0.58`;
- a minimal valid `RunAgentInput` parses and removing each required identity field fails;
- user text, image, audio, video, binary, and document inputs parse through the pinned public schemas;
- message, state, context, tool, interrupt/resume, lifecycle, text, tool-call, state, messages-snapshot, activity, custom, and visible-reasoning events parse through `EventSchemas`;
- one-field mutations of type discriminators, required IDs, tool arguments, state patch shape, and multimedia source shape fail;
- the Vestrace profile contains every pinned event type except `RAW` and `REASONING_ENCRYPTED_VALUE`, and marks those two as unsupported rather than silently accepting them;
- re-extraction is byte-identical to the committed schema/profile files.

- [ ] **Step 2: Run the AG-UI test and observe RED**

```powershell
node --test apps/console/tests/agUiProtocolProfile.test.mjs
```

Expected: module/package/file-not-found failure because pinned dependencies and generated artifacts are absent.

- [ ] **Step 3: Pin npm dependencies and refresh only the console lockfile**

Add exact runtime dependencies:

```json
"@ag-ui/client": "0.0.58",
"@ag-ui/core": "0.0.58"
```

Add exact development dependency `"zod-to-json-schema": "3.25.2"`. Run:

```powershell
npm --prefix apps/console install --save-exact @ag-ui/core@0.0.58 @ag-ui/client@0.0.58
npm --prefix apps/console install --save-dev --save-exact zod-to-json-schema@3.25.2
```

Inspect `package-lock.json` and require the two AG-UI integrity values from the provenance table. Reject any range in direct dependencies or any registry/auth substitution.

- [ ] **Step 4: Implement deterministic schema/profile extraction**

Use `createRequire()` anchored at `apps/console/package.json`, resolve the ESM entry points to file URLs, and use `zodToJsonSchema(schema, { name, target: "jsonSchema7", $refStrategy: "root" })` independently for each named public root. Normalize only object key order; preserve every schema array order and literal. Include no absolute path, timestamp, Node version, or host data.

The Vestrace profile records:

- request root `RunAgentInputSchema`;
- supported roots for messages, context, tools, state, multimedia, interrupts/resume, and events;
- the exact supported event-name array in upstream enum order;
- `RAW` and `REASONING_ENCRYPTED_VALUE` in an explicit `unsupported_event_types` array with the approved profile reason;
- transport profile `POST/SSE`, while leaving route paths to P08.

Support `--write <repo-root>` and offline `--check <repo-root>` modes. `--check` regenerates in memory and reports the first differing artifact without rewriting it.

- [ ] **Step 5: Run AG-UI extraction and observe GREEN**

```powershell
node scripts/extract-ag-ui-runtime-schemas.mjs --write .
node scripts/extract-ag-ui-runtime-schemas.mjs --check .
node --test apps/console/tests/agUiProtocolProfile.test.mjs
npm --prefix apps/console run typecheck
```

Expected: deterministic extraction, conformance test, and existing TypeScript typecheck all pass.

- [ ] **Step 6: Review the scoped Task 3 diff**

Confirm package versions are exact, the lockfile integrity matches registry provenance, generated artifacts contain no machine-specific data, and unsupported events are absent from the supported set rather than removed from upstream evidence.

---

### Task 4: Exact A2A specification/SDK pins behind existing adapter boundaries

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/vestrace-http/Cargo.toml`
- Modify: `crates/vestrace-infrastructure/Cargo.toml`
- Create: `schemas/a2a/vestrace-v1-profile.json`
- Create: `tests/protocol_a2a_lock.rs`

**Interfaces:**
- Workspace dependency aliases are `a2a = { package = "a2a-lf", version = "=0.3.0" }`, `a2a-client = { package = "a2a-client-lf", version = "=0.2.1" }`, and `a2a-server = { package = "a2a-server-lf", version = "=0.4.1" }`.
- `vestrace-http` consumes only `a2a` and `a2a-server`; `vestrace-infrastructure` consumes only `a2a` and `a2a-client`.
- `schemas/a2a/vestrace-v1-profile.json` is Vestrace's closed advertised profile, not a copied SDK model and not an implementation claim.

- [ ] **Step 1: Write the failing A2A lock/profile test**

Create a Rust integration test that reads `Cargo.toml`, `Cargo.lock`, both target crate manifests, and the A2A profile. Assert:

- exact workspace package aliases/versions and correct server/client ownership;
- exact three Cargo.lock package versions and registry checksums from the provenance table;
- specification release `v1.0.1`, release commit `3303592`, and wire header `A2A-Version: 1.0`;
- only one advertised `JSONRPC` HTTP/SSE interface;
- required server operations are exactly `SendMessage`, `SendStreamingMessage`, `GetTask`, `ListTasks`, `CancelTask`, and `SubscribeToTask`;
- push notification methods and `GetExtendedAgentCard` are explicitly unsupported;
- REST, gRPC, push notifications, and an extended card are not advertised.

- [ ] **Step 2: Run the A2A test and observe RED**

```powershell
cargo test --test protocol_a2a_lock -- --nocapture
```

Expected: the test fails because the exact SDK dependencies/profile do not exist.

- [ ] **Step 3: Add the exact workspace and crate dependency pins**

Add the three aliases to `[workspace.dependencies]`. Reference the server aliases from `crates/vestrace-http/Cargo.toml` and the client aliases from `crates/vestrace-infrastructure/Cargo.toml`. Do not expose SDK types through domain/application public interfaces and do not add routes or adapter code in P01.

Resolve only the required graph:

```powershell
cargo update -p a2a-lf --precise 0.3.0
cargo update -p a2a-client-lf --precise 0.2.1
cargo update -p a2a-server-lf --precise 0.4.1
```

If initial resolution requires `cargo check` before a package exists in `Cargo.lock`, run `cargo check --workspace` once, then repeat the three precise updates and inspect the final lock. Stop if the resolved graph requires a Rust version newer than `1.85`; do not patch, fork, or downgrade the approved crates inside this package.

- [ ] **Step 4: Add the closed Vestrace A2A profile**

Encode the exact approved profile fields enumerated by the test. Include the six operations and explicit unsupported capabilities. Do not claim the operations are implemented, qualified, or mounted; the file is a pinned input for P09/P10.

- [ ] **Step 5: Run the A2A contract and compile checks to GREEN**

```powershell
cargo test --test protocol_a2a_lock -- --nocapture
cargo check --workspace --locked
```

Expected: exact lock/profile assertions pass and the full Rust workspace compiles without leaking SDK types across domain/application boundaries.

- [ ] **Step 6: Review the scoped Task 4 diff**

Confirm no A2A route, Agent Card, task state, client call, feature advertisement, or release evidence was added; this task locks inputs only.

---

### Task 5: Deterministic aggregate protocol lock and drift detector

**Files:**
- Create: `scripts/protocol-provenance.mjs`
- Create: `scripts/verify-protocol-provenance.mjs`
- Create: `scripts/protocol-lock.mjs`
- Create: `scripts/verify-p01-text-hygiene.mjs`
- Create: `schemas/protocol-lock.json`
- Create: `apps/console/tests/protocolLock.test.mjs`
- Create: `tests/p01_text_hygiene.test.mjs`
- Modify: `apps/console/package.json`

**Interfaces:**
- `PINNED_PROTOCOL_PROVENANCE` in `scripts/protocol-provenance.mjs` is the sole code source for every exact version, integrity/checksum, repository URL, evidence URL, full commit, wire version, and adapter owner listed in the reviewed provenance table.
- `verifyOnlineProvenance()` fetches only the exact npm/crates.io HTTPS evidence URLs. For npm it validates package-metadata `repository.url`, SLSA subject, exact `resolvedDependencies[].uri`, and resolved commit. For each crate it validates archive SHA-256, normalized `Cargo.toml` repository, and `.cargo_vcs_info.json`. CLI `--online <repo-root>` is read-only and fails nonzero on any mismatch. The A2A tag-to-full-commit relation is verified separately by the exact `git ls-remote` command below.
- `buildProtocolLock(repoRoot)` returns the canonical lock object without filesystem writes or network access.
- `serializeProtocolLock(lock)` returns canonical UTF-8 JSON text with one trailing LF.
- `verifyProtocolLock(repoRoot)` returns an ordered list of typed drift findings; the CLI maps an empty list to exit `0` and any finding to nonzero.
- CLI modes are `--write <repo-root>` and `--check <repo-root>`; unknown/missing arguments fail nonzero without writing.
- `verifyP01TextHygiene(repoRoot)` imports the exact created-text path list from `p01-scope.mjs`, decodes every file as fatal UTF-8, and rejects BOM, U+FFFD, CR or CRLF, trailing space/tab, missing final LF, and noncanonical generated JSON. CLI `--check <repo-root>` is read-only; unknown/missing arguments fail nonzero.
- Add console script `"test:protocol": "node --test tests/agUiProtocolProfile.test.mjs tests/protocolLock.test.mjs"`.

- [ ] **Step 1: Write the failing aggregate-lock test**

Assert the generated lock includes:

- lock schema version `1` and the frozen design-spec SHA-256;
- both npm package names, exact versions, package-lock integrity values, attestation URL, repository, and full SLSA resolved git commit;
- A2A spec release/commit/wire version;
- all three Rust package names, versions, Cargo.lock registry checksums, repositories, `.cargo_vcs_info.json` source commits, and adapter ownership;
- the registered external-corpus aggregate digest plus SHA-256 of its repo-local manifest;
- SHA-256 digests of the q1 manifest, q1 marker PNG, AG-UI runtime schema bundle, AG-UI Vestrace profile, and A2A Vestrace profile;
- sorted fixture entries but preserved protocol array order inside each referenced artifact.

Use a temporary directory assembled from minimal fixture files to prove these mutation-sensitive failures independently:

1. one-byte q1 manifest mutation;
2. one-byte marker PNG mutation;
3. changed AG-UI npm integrity;
4. changed A2A Cargo checksum;
5. missing or changed provenance commit, repository URL, attestation URL, or archive URL;
6. generated lock with CRLF or no final LF;
7. range-based direct dependency instead of an exact version;
8. stale committed lock after any referenced artifact changes.

In `tests/p01_text_hygiene.test.mjs`, independently prove rejection of a UTF-8 BOM, invalid UTF-8, explicit replacement character, CRLF, bare CR, trailing tab/space, missing final LF, and parseable but noncanonical JSON, plus acceptance of canonical LF text and JSON. Assert every expected P01-created text path appears exactly once and the PNG is excluded as binary.

- [ ] **Step 2: Run the aggregate-lock test and observe RED**

```powershell
node --test apps/console/tests/protocolLock.test.mjs
node --test tests/p01_text_hygiene.test.mjs
```

Expected: import/file-not-found failures because the generators, scope module, and aggregate lock are absent.

- [ ] **Step 3: Implement the offline lock builder/verifier**

Parse npm JSON directly. Parse Cargo.lock package blocks with a strict local parser limited to `name`, `version`, `source`, and `checksum`; reject duplicate or missing exact package tuples. Put every value from the reviewed table, including the full A2A tag commit and exact canonical URLs, in `PINNED_PROTOCOL_PROVENANCE`; both the offline builder and online verifier import that object rather than duplicating constants. Derive every repository-file digest from current bytes.

Implement the online verifier with Node built-ins. Require exact npm package-metadata `repository.url`, then decode the npm DSSE payload and require the exact subject SHA-512, resolved dependency URI, and `digest.gitCommit`. Parse downloaded gzip/tar crate archives in memory and require the exact archive SHA-256, normalized `Cargo.toml` `package.repository`, and `.cargo_vcs_info.json`. Unit tests feed the parsers fixed local metadata/payload/archive fixtures and independently mutate every repository identity; only the explicit `--online` command performs network I/O. Authenticity of installed npm provenance/signatures is additionally checked by npm's own `audit signatures` command, and the A2A tag relation by `git ls-remote`, in Step 4.

The generator must refuse to write when:

- the frozen spec digest differs;
- the registered external-corpus aggregate or repo-local manifest digest differs;
- a direct dependency is not exact;
- a lockfile version/integrity/checksum differs;
- any referenced schema/fixture is absent;
- AG-UI `--check` extraction is stale;
- q1 fixture regeneration differs;
- the lock object cannot be canonically serialized.

Network verification of attestations/archive provenance is a separate explicit maintenance command, not part of normal `--check`; normal verification must be deterministic and offline.

Implement the hygiene verifier from the interface above. JSON canonicality uses the same key-ordering serializer as the lock generator; protocol arrays remain in their file order. The exact created-text list is data in `p01-scope.mjs`, not an implicit directory walk, so unrelated untracked/user files are inspected only by the dirty-baseline verifier and never reformatted.

- [ ] **Step 4: Generate and verify the aggregate lock**

```powershell
node scripts/protocol-lock.mjs --write .
node scripts/protocol-lock.mjs --check .
node scripts/verify-p01-text-hygiene.mjs --check .
node --test tests/p01_text_hygiene.test.mjs
npm --prefix apps/console run test:protocol
node scripts/verify-protocol-provenance.mjs --online .
npm --prefix apps/console audit signatures
git ls-remote https://github.com/a2aproject/A2A.git refs/tags/v1.0.1
```

Expected: the committed lock exactly matches the in-memory reconstruction, all mutation tests pass, the online verifier observes every reviewed value, npm verifies installed signatures/provenance, and `git ls-remote` returns `3303592588e388e62e0f69f701af531d2f4e3991` for the pinned tag. Record the command output in Task 6 evidence; do not rewrite constants from a changed response.

- [ ] **Step 5: Review the scoped Task 5 diff**

Confirm every digest is derived, every provenance value matches the reviewed table, check mode has no network/write path, and no lock field asserts protocol implementation or qualification.

---

### Task 6: P01 integrated verification and evidence boundary

**Files:**
- Create: `docs/development-evidence/v1-g0-01-protocol-lock.md`
- Inspect only: every P01 file named above
- Inspect only: repository-wide status to prove unrelated dirty work was preserved

**Interfaces:**
- The evidence note records observed commands/outcomes, source revision, scoped file list, pre-existing dirty-file acknowledgement, protocol-lock SHA-256, and explicit non-claims.
- It contains no secret, environment token, user payload, registry credential, or model response.

- [ ] **Step 1: Run focused verification from a fresh process**

```powershell
node scripts/generate-openai-q1-marker.mjs --check .
node --test tests/protocol_q1_marker.test.mjs
node scripts/external-corpus-manifest.mjs --check .
node scripts/extract-ag-ui-runtime-schemas.mjs --check .
node scripts/protocol-lock.mjs --check .
node scripts/verify-p01-text-hygiene.mjs --check .
node --test tests/p01_text_hygiene.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-01-preflight.json
npm --prefix apps/console run test:protocol
npm --prefix apps/console run typecheck
cargo test --test external_corpus_manifest --test protocol_q1_manifest --test protocol_a2a_lock --locked -- --nocapture
cargo check --workspace --locked
```

Require every command to exit `0`. Report an unavailable registry only if dependencies were never installed; once lockfiles and caches exist, every command in this block must be offline-capable.

- [ ] **Step 2: Run repository hygiene checks**

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check -- Cargo.toml Cargo.lock crates/vestrace-http/Cargo.toml crates/vestrace-infrastructure/Cargo.toml apps/console/package.json apps/console/package-lock.json
node scripts/verify-p01-text-hygiene.mjs --check .
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-01-preflight.json
git -c safe.directory=E:/Soft/vestrace status --porcelain=v1 --untracked-files=all
```

The named hygiene command is the untracked-aware complement to `git diff --check`; retain its observed exit code in evidence. Inspect the complete status and compare it with the preflight verifier output. Do not stage, delete, reset, or rewrite unrelated files.

- [ ] **Step 3: Persist truthful scoped evidence**

Record exact timestamps, source revision, preflight digest, external-corpus manifest and aggregate digests, commands, exit codes, online provenance observations from Task 5, observed protocol-lock digest, and scoped paths. State explicitly:

- P01 protocol inputs are reproducibly locked;
- no AG-UI/A2A endpoint or provider execution was implemented by P01;
- no PostgreSQL, Compose, LM Studio, remote API, browser, TCK, fault, accessibility, or release qualification was established;
- G0 and v1.0 remain incomplete.

- [ ] **Step 4: Independent review gate**

An independent reviewer must trace every P01 requirement to live files and fresh command output, verify the provenance constants against the official npm attestations/crates.io archives, and return only material findings plus `VERDICT: APPROVE` or `VERDICT: REVISE`. Correct every material finding with the same persistent cdx builder and repeat focused verification/review.

- [ ] **Step 5: Operator handoff**

Report the scoped outcome and remaining package count from the program index. Do not start P02 until P01 is accepted. Do not call the result G0-complete or v1.0-ready.
