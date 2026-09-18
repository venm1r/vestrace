# 07. Security, cleanup, and observability

**Status:** Proposed extension requirements. Existing authorization/material authorities remain in force.

## 7.1 Threat model

Consider malicious MCP/HTTP callers, stale browsers, concurrent writers, hostile imported
Markdown/JSON, incorrect scanners, worker death, unknown POST outcomes, and revocation between
preview and Apply/download. This package does not solve host-root or PostgreSQL-superuser
compromise, nor make a model a universal security oracle.

Trusted middleware establishes identity. Validate workspace selection against the token.
Workers cannot take actor authority from JSON or use their broader service principal to bypass
a denied user request. Names, paths, and source titles can themselves be sensitive.

## 7.2 Disclosure

List, detail, history, context, source diff, and export share MemoryReadService/policy projection.
Check classification before sending content to a ranker/model or generating snippets. Browser
roles do not enforce security. Check each historical revision's own label, not only the current
memory label. Both operation capability and content read/destination predicates must pass.

Safe-looking hints must not reveal denied references/names. Audit/metrics contain operation
IDs and aggregate status only within authorized scope. No forbidden counts or titles are exposed
through diagnostics.

## 7.3 Storage

Source/upload/export payloads use existing MaterialIntentCommands, vault, and erasure. The narrow
SourcePayloadStore adapts this authority; it does not introduce another cryptographic state machine.

MW-04 must establish a lawful ordinary-content owner, read path, and cleanup. Do not reuse
embedding-specific 14C/D provisional keys or fabricate VaultReceipts. When owner/read APIs
cannot express a binding, explicitly extend the same owner model with raw-SQL/runtime-role
tests instead of choosing another store.

Bind keys/receipts to workspace and owner tuple. Perform vault work outside SQL transactions.
Plaintext is limited to bounded request/process buffers and cleared as the types allow; do
not promise complete JavaScript/OS-buffer zeroization. Loggers must not accept content bodies.
System temporary directories are not staging storage.

## 7.4 Classification and secrets

Do not derive a severity order from label spelling. An ordering such as internal < confidential
must come from accepted policy, not string comparison. Ordinary editing/sync preserves labels;
other labels require refusal and separate governance. Imported labels are not executable policy.

Default scanner exclusions reduce accidents, not a DLP guarantee. Plaintext export requires
explicit intent and a permitted destination. Warn that downloaded JSON/Markdown may be sensitive.

## 7.5 Replay and outcomes

New Idempotency-Key scope includes operation/principal/workspace. Receipt disclosure requires
current rights. Fingerprints exclude random result IDs and must not become public equality oracles.

result-unknown is a valid client state. Timeout does not mean nothing was saved: retry identical
body/key or read the known operation. At-least-once can invoke a handler repeatedly; canonical
receipts and unique guards establish one business mutation, not a claim of one handler invocation.

## 7.6 Operational signals

Within permitted scope, expose counts of preview_ready/applying/blocked/failed, staging/apply
duration, unfinished-operation age, dead-letter items, pending projections, and unsupported
counter/storage reasons—without raw text. Metrics do not establish correctness or Trusted state.

After restart, show actual receipts and whether work committed, needs retransmission, was
cancelled, or is blocked. Doctor/repair uses guarded commands, not a separate DML bypass.

## 7.7 Readiness

Require schema/migration compatibility, narrow provisioning, ordinary-material support, real
worker registration of memory.source_import.apply and memory.export.prepare, and durable actor
resolution. Qualified tokenizers are required when claiming a token cap. Write/edit readiness
requires the shared atomic writer; context/indexing additionally requires generation readiness.

Disabled features show unavailable/not_enabled. Flags cannot bypass checks, alter legacy semantics,
or enable permissive defaults. Backup/restore and new owner compatibility are qualified separately
from knowledge import.
