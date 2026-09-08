# Retrieval and context construction

## Pipeline and disclosure

Retrieval takes a task and constraints, chooses permitted candidates, combines channels, applies temporal semantics, and builds a bounded view. Its goal is sufficient, current, explainable context rather than the longest possible prompt.

Workspace, capability, classification, and destination checks must happen before protected text reaches a reranker or model. A policy port's existence does not prove every production path invokes it.

## Current representation

HTTP candidates expose memory/revision identity, status, revision number, temporal fields, generation, score, channel, and explanation. ContextPackDto exposes budget, counts, withheld results, degraded channels, and policy version, but not section text.

The internal ContextPackBuilder groups constraints, facts, decisions, and tasks and uses `Full → Summary → Atomic → Reference`. Current Summary/Atomic forms are deterministic string shortening, not independently verified LLM summaries.

## Budget is part of the contract

`conservative_token_count` computes ceil(text.len() / 4), with len measured in UTF-8 bytes. Its name is not proof of a universal tokenizer upper bound. Wrappers, separators, and citations also consume space.

Distinguish bytes, approximate tokens, and a qualified model-specific bound. A hard-token request must not silently become a byte heuristic. MW documents that compatibility decision; this refactor does not implement a new counter.

## History and compression

The reviewed builder deduplicates by memory_id. A timeline needing several revisions of the same memory requires separate verification. Current-state deduplication is not automatically correct for history.

Compression must preserve negation, corrections, and conflict. A truncated identifier is not a valid provenance reference. When no acceptable representation fits, prefer an explicit, safely explained omission.

## Cache reuse

Context is derived. Permissions, policy, revisions, and generations may change after construction. Recording an issued result and authorizing redisclosure are different decisions. A cached client copy cannot become a policy bypass.

**Sources:** [HTTP](../../crates/vestrace-http/src/api/retrieval.rs), [builder](../../crates/vestrace-application/src/retrieval/context_builder.rs), [Architecture Contract](../specs/en/vestrace-architecture-contract-v0.2.md).
