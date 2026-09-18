# Retrieval and ContextPack API

**Scope:** Source-defined inputs and output limitations in the supplied snapshot.

## Request

`POST /v1/retrieval/search` takes `query` and optional `intent`, `token_budget`, `limit`, `time_perspective`, and `as_of`.

The parser accepts intents `current_state`, `decision_recall`, `timeline`, `task_resume`, `procedure_lookup`, `user_preferences`, and `semantic_recall`. Omitted intent defaults to semantic_recall; unknown values are rejected.

Time perspectives are `current`, `as_of`, `timeline`, and `all_history`. `as_of` requires a timestamp. This enumeration does not establish full recorded-as-known temporal semantics; that is a separate F201 concern.

## Response

Candidates contain memory/revision IDs, status, revision number, validity, revision timestamp, source generation, score/channel/rank, explanation, and optional classification. **The current candidate DTO does not return content.** The response also includes withheld results, policy version, temporal perspective, degraded channels, and warnings.

With `token_budget`, the server also builds a ContextPack, but its HTTP DTO exposes a summary rather than section text. `section_count > 0` does not mean that the client received a model-ready prompt. Rendered sections are a proposed new surface.

## Exact history

The revision hydrator requests exact memory/revision pairs without substituting latest. Mismatched pairs are filtered; missing references cannot be repaired by inventing sources. Historical reads remain subject to current policy and retention.

## Budget and content limitations

The current helper uses ceil(UTF-8 bytes / 4), an estimate rather than a universal hard-token bound. MW distinguishes byte-only output from a qualified tokenizer path. Do not describe the former as complete implementation of the latter.

The current builder can use an explanation when content is empty. The proposed API must not present that technical string as source knowledge. Deterministic shortening must not delete a negation and then be advertised as a trustworthy summary.

## Diagnostics

Do not combine incomparable channel scores through an arbitrary sum. Report participating and degraded channels. Degradation must remain within an accepted safe path; omitting vectors alone does not authorize bypassing generation fences.

**Sources:** [HTTP](../../crates/vestrace-http/src/api/retrieval.rs), [builder](../../crates/vestrace-application/src/retrieval/context_builder.rs), [revision hydrator](../../crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs).
