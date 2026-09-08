# Memory Workspace package plans

**Status:** Proposed. No implementation completion or runtime PASS is recorded here.
All plans reuse current memory/material/mutation/outbox authorities. There is no new event
store, scheduler, or direct-SQL client. Reuse the existing Rust/PostgreSQL/HTTP and React/
TypeScript stack; dependency changes require their own approved diff.

| Package | Plan | Prerequisites |
| --- | --- | --- |
| MW-00 | [Baseline and gates](00-preflight.md) | Acceptance of the documentation design. |
| MW-01 | [Reads and context](01-memory-read-context.md) | MW-00. |
| MW-02 | [Atomic corrections](02-atomic-corrections.md) | MW-00, MW-01. |
| MW-03 | [Console](03-console.md) | MW-01, MW-02. |
| MW-04 | [Import](04-import.md) | MW-02, MW-03. |
| MW-05 | [Sync/conflicts](05-sync.md) | MW-04. |
| MW-06 | [Portability](06-portability.md) | MW-04, MW-05. |
| MW-07 | [Feature qualification](07-qualification.md) | MW-01–MW-06. |

## Shared scope rules

The original review pin is `6f610253`; inspect the fresh implementation checkout and its delta.
The supplied archive is `3e05dfbd`, with recorded 14E acceptance and occupied migration 0196.
Candidates 0196–0199 must be reconciled together before SQL; they are not reservations.
Protect PLAN.md, frozen specifications, historical evidence, and unrelated P04 preflight/scope.
New paths outside an individually accepted scope require an amendment before writing.

The [file map](../file-plan.json) states original path status and responsibility; it is not a
blanket allowlist. Reread referenced files, check proposed paths for collisions, and select
an exact task diff. Adjacent package paths are not implicitly writable. Include production
module exports, routes/inventory, schema generator, server/MCP/worker composition, and grants
only as required and explicitly approved.

Enforce the 64 KiB UTF-8 content, 100-item/entity, and 8 MiB decoded package limits. No direct
client SQL, fabricated PASS, raw-content/secret logs, competing authority, or unsafe default
is allowed. Idempotency/CAS/policy and exact sources apply to all mutations and worker delivery.

## Shared task procedure

1. Prepare a valid target and fixture before implementing behavior. Link the full requirement
   and all acceptance cases. Missing targets, compilation failure, and unavailable databases
   are setup errors, not behavioral RED. MW-00 observations need no artificial failure.
2. Run the named RED command; verify the intended assertion fails for the missing guarantee.
3. Implement only the declared path under the approved authority and file scope.
4. Repeat the command for GREEN and exercise **every** linked case, not only illustrative examples.
5. Check sensitivity/compatibility. In disposable copies, disable the named receipt/CAS/policy
   guard and require the unchanged test to fail; restore exact bytes and observe GREEN. UI
   equivalents use reducer/stale-response regressions. Test affected legacy paths. Documentation-only
   MW-00 does not require a fake business mutation test.
6. Obtain independent review of requirements, implementation, and exact diff. Record commands,
   actual exit codes, limits, and missing checks. Check protected paths and git diff --check.
   Commit only when separately authorized and with explicitly named files, not git add -A.
   These documents do not authorize push/deploy.

## Shared verification commands

These are instructions for a prepared code checkout, **not results** of this document task.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked --no-fail-fast
cargo test --workspace --doc --all-features --locked
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

--all-targets does not replace doctests. Proposed test:memory/test:e2e:memory scripts must
exist before use. Supply test credentials safely, not in evidence. Do not run concurrent Cargo
builds by default. Actual browser/HTTP/PostgreSQL composition is separate from typechecking.

## Shared exit gate

No unresolved BLOCKER/MAJOR finding; complete linked scenarios observed; data/API/SDK/docs
agree; source/environment/scope versions are recorded; relevant negative and legacy suites
have run. Unavailable dependencies produce explicit partial results, never completion inferred
from checkboxes. MW qualification does not close P04, G0, P12, or TRUSTED.
