# Context inspection and explanations

**Status:** Proposed functional design for F205.

## Three evidence types

Vestrace may record context it issued. A client may report using it. A trusted integration hook may observe the final model request. These are different forms of evidence. Neither matching IDs nor a client boolean independently proves what reached the model.

## Inspection view

Show query intent/time/scope → permitted candidates → selected revisions → rendered sections/budget → bounded warnings → optional observed request. Identify sources and processing versions at each step. Authorize the debug view before showing content, counts, or deleted/withheld metadata.

Compare requests with the same corpus snapshot, user scope, and budget. Show added/removed permitted revisions and cost. A model change without pinned version/tokenizer is not a retrieval-only improvement.

## Retention and acceptance

Raw prompts/context can be sensitive material. Where allowed, preserve a bounded rendered payload under governed lifecycle; otherwise keep safe structural identities and explain replay limits. A prohibition on sensitive storage must not become a false guarantee of full reproduction.

For a late correction, operators must see the new basis and why prior context differs. Unauthorized callers must not extract candidates through debug endpoints. Without observed requests, an external runtime must not be labeled “the model definitely saw this.” Forensic UI must respect retention and erasure.

**Sources:** [retrieval](../../crates/vestrace-http/src/api/retrieval.rs), [builder](../../crates/vestrace-application/src/retrieval/context_builder.rs), [Architecture Contract](../specs/en/vestrace-architecture-contract-v0.2.md).
