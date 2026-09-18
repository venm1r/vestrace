# 10. Migrations, compatibility, and storage

**Status:** Proposed logical migration plan, not executable migration SQL.

## 10.1 Existing boundary and candidate collision

The original review included 0192–0194 and the 14E proposal naming 0195. The supplied archive
now contains 0195_embedding_job_result_finalization.sql **and 0196_retired_credential_erasure.sql**.
This package changes none of their checksums. MW-M01's historical candidate number is therefore
occupied; MW-00 must reassign affected candidate paths consistently before implementation.
No replacement number is reserved by this English edition.

| Logical migration | Historical candidate path | Responsibility |
| --- | --- | --- |
| MW-M01 | migrations/0196_memory_workspace_receipts.sql | MW-01 creates exact revision links, epoch triggers, and receipt schema; MW-02 consumes them without editing applied SQL. |
| MW-M02 | migrations/0197_source_import_authority.sql | Collections/sources/revisions, initial bindings, operations/items, ordinary payload owner. |
| MW-M03 | migrations/0198_source_sync_conflicts.sql | Immutable conflict triples, resolution authority, transitions on existing manual-override bindings. |
| MW-M04 | migrations/0199_memory_export_authority.sql | Pinned export selection, result bindings, portable mappings. |

Check all names against current aggregates and extend suitable authority instead of creating
parallel owners. Chapter 02 is a logical contract, not permission to bypass InstallationMutationPermit.

## 10.2 M01 and shared writes

M01 applies in MW-01. Transactional database triggers advance the browse epoch for canonical
memory/revision/link mutations, including legacy writers. MW-00 enumerates every lawful writer.
MW-02 must neither edit applied M01 nor advance the epoch twice. M02 adds source-related triggers
through a forward migration. All epoch locks occupy an agreed position in transaction order;
no opposing epoch/memory lock orders are permitted.

Add same-workspace revision-source FKs for new attribution. Do not infer legacy links from
timestamps. Provision existing-workspace epoch rows through the authorized migration/provisioner,
not runtime-wide enumeration. Schema readiness precedes new routes.

Reuse idempotency_keys authority and namespace rather than a second idempotency service. A typed
receipt may project a committed result but cannot elect another winner. Any uniqueness change
must preserve unambiguous legacy-key ownership, not silently bind old keys to another principal.
Preserve unexpired legacy exact replay; a new namespace cannot unexpectedly reapply an old
operation. Adapt legacy writes to the shared participant without changing wire shapes. Expired
receipts cannot be reconstructed from hashes. New receipt retention follows lifecycle without
sensitive response_payload.

## 10.3 M02/M03 integrity and privileges

Use composite workspace FKs, closed states, unique collection/external_id and source ordinals,
and at most one memory/source binding. Source payload belongs to an approved ordinary-content
owner in the same workspace, never a temporary file or unrelated embedding output.

Preview pins bases/payload identity; mapping is immutable after PreviewReady. Apply selection,
state, receipt, and enqueue are atomic. Item receipt commits with source/memory changes. A source
head/locator can advance on conflict only with an immutable conflict record; effective memory
remains M until resolution.

Scoped owner/context is required for every mutating SQL entrypoint. RLS does not replace actor
permission, and privilege does not replace invariants. Grant EXECUTE only to exact required
signatures. Update docker/postgres/init-runtime-role.sh allowlists consistently. Derive the
catalog in tests and assert required names, not only a minimum count.

## 10.4 M04 and foreign mappings

Namespace foreign IDs by workspace/import_operation/package_id/foreign_kind/foreign_id; do not
use them as trusted local IDs. Allocate local revision mappings before linking. Incomplete
relationships remain partial; reject future unknown formats before mutation.

Select export refs consistently and bind material to the operation/dependencies. No generic
artifact URL may bypass current download authorization, even when preparation had wider grants.

## 10.5 Retention compatibility

Include new links in existing purge/erasure inventory. FKs cannot make lawful purge an unexplained
error; CASCADE cannot delete other memories' shared sources. Test the ordinary purge command
on disposable data with bindings, previews, and exports; observe every retained/erased outcome.
Safe relationship/receipt IDs may survive as policy-permitted tombstones, but not plaintext or
denied locators. Server cleanup cannot revoke a client archive.

## 10.6 Upgrade and fallback

Test clean installation and a populated baseline containing r1/r2 memories, effects, outbox,
material state, and legacy receipts. Preserve old routes and P02/P03/P04 scenarios; show legacy
provenance honestly in the new library.

Product rollback requires stopping admission, draining/cancelling work, and checking old-version
compatibility with new schema/data. No automatic down-migration/table deletion is proposed.
Knowledge JSON does not replace full backup. A new owner model requires recovery/restore
verification before claiming upgrade support.
