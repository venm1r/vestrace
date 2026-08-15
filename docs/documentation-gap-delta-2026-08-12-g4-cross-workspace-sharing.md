# G4 Cross-Workspace Sharing v2 Delta

Date: 2026-08-12

Reviewed worktree: `HEAD 568f3d5` plus the uncommitted G4 slice. The repository's pinned implementation documents remain historical baseline records; this delta is the current slice ledger.

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: G4 follows G3 and requires grant+mount, revoke, and stale evidence at the Govern gate.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: IDW-004..014 require explicit two-sided consent, exact targets and revisions, independent source/target policy, fail-closed stale/revoked mounts, namespaced references, provenance-safe history, and preserved RLS isolation.
- `docs/specs/vestrace-trust-authority-model-v0.2.md` §13: source grant and target acceptance are separate authorities; operation permissions are not transitive; revoke denies future reads while preserving historical disclosure audit.
- `docs/specs/vestrace-domain-model-v0.2.md` §16: `MemoryShareGrant`, immutable grant revisions, `MemoryMount`, and `SharedMemoryRef` are distinct domain objects.
- `docs/adr/0006-cross-workspace-sharing-is-grant-plus-mount.md`: the accepted architecture is exact source grant → target acceptance → read-only mount; legacy one-sided grants do not define the target model.

## Scope implemented

1. Added typed `MemoryShareGrantRevisionId` and `MemoryMountId` identifiers.
2. Added `MemoryShareGrantRevision` with exact source workspace, exact target workspace, exact memory and memory revision, source generation, operation-specific permissions, and bounded validity.
3. Added `MemoryShareGrant` lifecycle with explicit revoke and a legacy bridge that rejects automatic promotion of `CrossWorkspaceMemoryGrant`.
4. Added target-owned `TargetSharePolicy` and `MemoryMountAcceptance`. A mount is accepted only for the exact grant and immutable revision, with operations attenuated to the intersection of source and target policy.
5. Added `SharedMemoryRef` carrying source workspace, source memory, exact source memory revision, grant revision, and source generation. It has no local-memory conversion path.
6. Added independent `evaluate_share_access` checks and fail-closed `MemoryMount::sync_with_source` / `shared_ref` behavior for revoke, expiry, stale identity, and mount expiry.
7. Added immutable `ShareDisclosure` history. Revocation prevents new disclosures but does not rewrite previously recorded disclosure references.
8. Rejected wildcard targets, `ReShare` operations, and transitive mount-of-mount authority at the domain boundary.

## Exit evidence

- RED was observed before G4 APIs existed: `cargo test --test g4_cross_workspace_sharing -- --nocapture` failed on unresolved domain types and IDs.
- `cargo test --test g4_cross_workspace_sharing -- --nocapture`: 11 passed.
- The focused suite covers exact two-sided acceptance, wildcard and operation attenuation, exact revision/target matching, constructor-generated authoritative IDs, independent source/target policy denial, revoke/expiry/stale denial, mismatched-source stale propagation, expiry before explicit sync, immutable historical disclosure, anti-transitive sharing, and legacy non-upgrade.

## Explicit non-claims

This is an additive in-memory domain slice, not complete cross-workspace runtime support. It does not claim:

- durable grant revisions, mount persistence, disclosure audit persistence, or migration/backfill;
- HTTP, MCP, worker, retrieval, cache, index, provider, export, or local-derivation wiring;
- PostgreSQL runtime RLS evidence or any removal/relaxation of normal workspace isolation;
- automatic cache/index invalidation or active-run revalidation after revoke;
- federation trust evaluation or GOVERNANCE/QualificationBundle closure.

The next documented gate is G5: federation trust boundary where remote identity/trust is not local data permission.
