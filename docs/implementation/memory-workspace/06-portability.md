# 06. Knowledge export and reimport

**Status:** Proposed contract. Knowledge transfer is not installation backup or disaster recovery.

## 6.1 Purpose and formats

Export a bounded permitted selection of memories, revision history, and provenance. Do not
transfer PostgreSQL/vault backup, identities, capabilities, tokens, or qualification. Deleting
source memory cannot recall copies already downloaded by a client.

The machine format is single-file UTF-8 JSON `vestrace.memory-package/1`. Markdown is a readable
view, not a lossless round-trip format. ZIP upload is excluded to avoid archive traversal/
decompression risks in the first version.

## 6.2 Pinned selection and current authorization

ExportRequest explicitly names 1–100 memory_ids, include_history, and format. Pin revisions
under a consistent scoped read and current principal. Bound the uncompressed package to 8 MiB;
excess is a typed failure, never truncated “complete” output.

Persist selection refs and policy version. Existing `memory.export.prepare` outbox delivery
builds the result through protected material storage. No plaintext /tmp, public artifact, or
CDN fallback is allowed. On every content download, reauthorize **each** pinned memory,
revision, and source. Revocation withholds the whole result and lawfully retires its material.
Do not reuse a download URL that bypasses those checks.

Authorization concerns the response's linearization/check point. Already transmitted bytes
cannot be recalled. Streaming chunks are not independent authorization decisions; construct
the bounded MVP response after one consistent check. Disconnect does not prove nothing was received.

## 6.3 Package allowlist

Top-level fields are schema_version, package_id, generated_at, memories, sources, revision_links,
and completeness. Foreign UUIDs identify objects inside the package namespace. Memory contains
kind, selected active_foreign_revision_id, and permitted revisions. Revision data includes
content, declared classification, original times, change_reason, and provenance annotations.

Exclude active tokens, keys/key IDs, server credentials, raw request logs, permissions,
qualification verdicts, and ready vectors. The closed schema rejects such extra fields.
Treat exported plaintext according to the most restrictive source obligations.

Omit denied provenance names/IDs and mark completeness.provenance=partial for hidden/unknown
origins. complete means closure of the selected revisions, not all historical data.
history=selected_current may coexist with complete provenance for that selection. A future
origin signature is outside this version and cannot make the file a trusted issuer automatically.

## 6.4 Shared portable-import pipeline

Use preview/apply with mode=portable. Validate schema/version/limits/references first, then
select the target collection and permitted classification. Local policy grants authority;
imported metadata cannot grant rights or silently downgrade labels.

Allocate new local memory/source/revision IDs. The immutable mapping binds package_id + foreign_id
to local_id within **that operation**. Exact replay returns the same mappings. A separate import
of the same package requires new explicit intent and a duplicate warning, not an overwrite.

Set local recorded_at to import time; preserve sender-claimed time in origin_recorded_at.
The local audit actor is the actual importer. Foreign actors are unverified annotations, not
principals to impersonate. Active identifies the selected local revision, not confirmed truth.

Validate all intra-package links before Apply. Cross-item links are written only after both
local endpoints exist; keep pending/unresolved status until then. Import entities/mappings first,
then execute a second idempotent linking pass through the same handler/topic. Report partial
batches explicitly and claim complete provenance only after closure.

## 6.5 Retention and erasure

Proposed export-material expiry is 24 hours. Afterwards return 410 and invoke existing erasure.
Operation history retains only policy-permitted safe IDs/outcomes, not content copies.
Lawful erasure invalidates managed staged previews, exports, and derived context containing
the data. Permission changes do not revoke client-local copies; state this in the UI. Do not
keep denied content in idempotency response_payload for replay convenience.

## 6.6 Acceptance

Export → target preview → Apply → read preserves selected content, permitted provenance,
and origin-time annotations. Local IDs, actor, and grants differ predictably. Compare graph
and content, not sender server IDs. Refuse unauthorized label mapping, unknown versions,
secret fields, oversize input, and missing references before canonical application.

## 6.7 Entity bounds and mapping details

Portable import allows at most **100 canonical entities total** (memories + sources),
**64 KiB per content revision**, and **8 MiB UTF-8 across all content values**. The wire JSON
also has a transport limit; repeated text fields still count toward decoded size.

One entity is one import item. Allocate revision mappings when committing its item; finish
links in a persisted final phase of the same memory.source_import.apply handler, not a new scheduler.

```text
portable_import_mappings(workspace_id, operation_id, package_id,
  foreign_kind, foreign_id, local_id)
```

Require unique foreign tuples and unique local IDs for each kind/operation. Diagnostics use
`portable/memory/<foreign_id>` only as a synthetic display locator, never a filesystem path.
Selected foreign ordinals may have gaps; build a new local sequence and retain origin numbers
as annotations. keep_manual returns the existing memory revision with an audit/binding receipt;
it does not require an empty text revision.
