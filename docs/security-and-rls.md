# Security and workspace isolation

This restores a missing entry point. It explains source-defined foundations and normative boundaries, not blanket security qualification.

## Identity and defense in depth

Bearer authentication resolves principal/workspace server-side. Scoped database transactions establish workspace/principal context. RLS protects scoped rows, while application-level authorization and workspace predicates remain mandatory. Administrative/superuser fixtures are not evidence of restricted-runtime isolation.

Roles are administrative templates. Effective runtime authority is the intersection of capabilities and applicable policy. Identity, health, trust, and permission are distinct.

## Read versus disclosure

Permission to read is not permission to send content to a model, provider, export, or another workspace. Apply classification/destination rules before disclosure, including snippets, diagnostics, history, and caches.

Do not infer classification ordering from label strings. The domain's sensitivity vocabulary and a particular installation's classification-label rules are separate concerns. Ordinary memory edits preserve labels under the current API.

## Secrets and materials

Secret plaintext is not ordinary Memory. Durable state stores safe references/metadata. Resolve secret bytes through controlled backends only for use; do not log them or retain them in receipts/context. Material preparation is not Live publication.

## Sharing and restoration

Cross-workspace sharing requires an explicit source grant plus independently accepted target mount. `global` remains workspace-local; mounts do not become local authoritative memories or implicit transitive grants.

Revocation stops future disclosure but does not erase prior audit facts or recall bytes already delivered externally. Restart/recovery does not automatically restore TRUSTED or dangerous capabilities; evidence-backed revalidation is required.

**Sources:** [auth](../crates/vestrace-http/src/auth.rs), [Trust & Authority](specs/en/vestrace-trust-authority-model-v0.2.md), [Governance](specs/en/vestrace-crypto-data-governance-contract-v0.2.md), [status](status.md).
