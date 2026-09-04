# Vestrace v1 G0-01 protocol-lock evidence

Captured from root-observed verification: 2026-08-26T15:57:29.466Z through 2026-08-26T15:58:24.208Z.

Source revision: `6aba953c684fc60ad822cafa283f3cda8966e799`.

## Immutable inputs and digests

- Preflight file SHA-256: `3894e421966e53b4c1aca5c9544497241376d5172ed7e0e12fd850b9ccd27283`.
- Preflight raw NUL-delimited porcelain status SHA-256: `60d39130936735bb07549232814766436d4081a4716fd610fa5f714a9806464f`.
- External corpus aggregate SHA-256: `762c25f3781f1ba30783e218db64aa4dc9536add490f61255056bfe8635d8594`.
- External corpus manifest SHA-256: `f8100e1bcb070da6c6f540644d2fcae0862dd5c7cc3337cfa7c5bd29d10aac02`.
- Protocol-lock SHA-256: `dc643aa2f974e122d4272efa0e34053b9cf73ce98523833dbef39475b4a68e16`.
- Detailed plan SHA-256: `ca02673b85f85641d131a96af704b0f075740a097eb2a0e6115e0b7ffd6c0097`.
- Gate program SHA-256: `9b917a2c589d860e3c4e199581adecbde927bf4d4db2a2810998a4d23b489dd3`.
- Frozen design spec SHA-256: `b31b5be62504e1a65f411cd31b446cd41b3d032b7282f1aa42907706ef9c1473`.

## Root-observed command evidence

All commands below exited `0`.

- 2026-08-26T15:57:29.466Z..2026-08-26T15:57:38.096Z:
  - `node scripts/generate-openai-q1-marker.mjs --check .`
  - `node --test tests/protocol_q1_marker.test.mjs` - 1/1 passed.
  - `node scripts/external-corpus-manifest.mjs --check .`
  - `node scripts/extract-ag-ui-runtime-schemas.mjs --check .`
  - `node scripts/protocol-lock.mjs --check .`
  - `node scripts/verify-p01-text-hygiene.mjs --check .`
  - `node --test tests/p01_text_hygiene.test.mjs` - 1/1 passed.
  - `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-01-preflight.json`
  - `npm --prefix apps/console run test:protocol` - 8/8 passed.
  - `npm --prefix apps/console run typecheck`
- 2026-08-26T15:57:43.902Z..2026-08-26T15:57:45.737Z:
  - `cargo test --test external_corpus_manifest --test protocol_q1_manifest --test protocol_a2a_lock --locked -- --nocapture` - 4/4 passed.
- 2026-08-26T15:57:45.737Z..2026-08-26T15:57:47.504Z:
  - `cargo check --workspace --locked`
- 2026-08-26T15:57:56.1867369Z..2026-08-26T15:57:56.7383380Z:
  - `git -c safe.directory=E:/Soft/vestrace diff --check -- docs/external-corpus/vestrace-docss-2026-08-19.manifest.json schemas/a2a/vestrace-v1-profile.json schemas/ag-ui/0.0.58/runtime-schemas.json schemas/ag-ui/vestrace-v1-profile.json schemas/openai-compatible/openai-chat-completions-v1-q1.json schemas/protocol-lock.json`
  - `node scripts/verify-p01-text-hygiene.mjs --check .`
  - `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-01-preflight.json`
  - Complete status contained 134 porcelain lines.
- 2026-08-26T15:58:03.565Z..2026-08-26T15:58:09.694Z:
  - `node scripts/verify-protocol-provenance.mjs --online .`
- 2026-08-26T15:58:09.694Z..2026-08-26T15:58:19.874Z:
  - `npm --prefix apps/console audit signatures` - 89 packages have verified registry signatures and 42 packages have verified attestations.
- 2026-08-26T15:58:19.874Z..2026-08-26T15:58:24.208Z:
  - `git ls-remote https://github.com/a2aproject/A2A.git refs/tags/v1.0.1` - `3303592588e388e62e0f69f701af531d2f4e3991	refs/tags/v1.0.1`.

## Scoped path set

The exact P01 change scope from `scripts/p01-scope.mjs` is:

- `Cargo.lock`
- `Cargo.toml`
- `apps/console/package-lock.json`
- `apps/console/package.json`
- `apps/console/tests/agUiProtocolProfile.test.mjs`
- `apps/console/tests/protocolLock.test.mjs`
- `crates/vestrace-http/Cargo.toml`
- `crates/vestrace-infrastructure/Cargo.toml`
- `docs/development-evidence/v1-g0-01-preflight.json`
- `docs/development-evidence/v1-g0-01-protocol-lock.md`
- `docs/external-corpus/vestrace-docss-2026-08-19.manifest.json`
- `schemas/a2a/vestrace-v1-profile.json`
- `schemas/ag-ui/0.0.58/runtime-schemas.json`
- `schemas/ag-ui/vestrace-v1-profile.json`
- `schemas/openai-compatible/openai-chat-completions-v1-q1.json`
- `schemas/protocol-lock.json`
- `scripts/external-corpus-manifest.mjs`
- `scripts/extract-ag-ui-runtime-schemas.mjs`
- `scripts/generate-openai-q1-marker.mjs`
- `scripts/p01-scope.mjs`
- `scripts/protocol-lock.mjs`
- `scripts/protocol-provenance.mjs`
- `scripts/verify-dirty-baseline.mjs`
- `scripts/verify-p01-text-hygiene.mjs`
- `scripts/verify-protocol-provenance.mjs`
- `tests/external_corpus_manifest.rs`
- `tests/fixtures/openai-q1/marker.png`
- `tests/p01_text_hygiene.test.mjs`
- `tests/protocol_a2a_lock.rs`
- `tests/protocol_q1_manifest.rs`
- `tests/protocol_q1_marker.test.mjs`

## Dirty-work and limitations acknowledgement

The repository had pre-existing dirty work. It was preserved; the dirty-baseline verifier passed against the recovered preflight baseline. The scoped status observation reported 134 porcelain lines.

The approved npm install reported three dependency-tree vulnerabilities: one moderate and two high. No audit fix was run.

## Non-claims

This evidence records the final independent review verdict of `APPROVE`. It does not claim G0 completion or v1.0 completion.

P01 adds no AG-UI or A2A endpoint, provider execution, PostgreSQL/Compose/LM Studio/remote API/browser/TCK/fault/accessibility/release qualification. The commands above verify only the recorded protocol-lock inputs, deterministic artifacts, provenance checks, and offline integration checks.
