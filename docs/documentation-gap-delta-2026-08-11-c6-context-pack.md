# Documentation Gap Delta — C6 ContextPack 2.0 Assembly

**Date:** 2026-08-11  
**Scope:** bounded C6 implementation slice in the dirty `E:\Soft\vestrace` checkout  
**Authority:** v0.2 retrieval requirements and the C6 implementation plan

This source-based delta records the ContextPack assembly behavior now present in
the checkout. It does not replace the frozen v0.2 architecture baseline, does
not qualify a profile, and does not claim completion of C5-C8 or later phases.

## Implemented in this slice

- `RetrievalCandidate` now carries the authoritative `MemoryKind`; the
  PostgreSQL text retriever hydrates it from `memories.kind` with explicit enum
  mapping.
- `ContextPackBuilder` routes candidates deterministically into the documented
  section order: `constraints`, `current_facts`, `decisions`, `tasks`,
  `procedures`, `supporting`, `recent_events`, `history`.
- Every emitted `ContextItem` retains the candidate memory and exact revision
  through `EvidenceRef::MemoryRevisionRef`.
- Inclusion explanations record both the target section and the selected
  representation.
- Representation selection is deterministic and provider-free:
  `Full -> Summary -> Atomic -> Reference`. Summary uses the first sentence,
  Atomic uses the first clause/line, and Reference uses stable memory/revision
  identifiers. UTF-8-safe truncation is used only when the stable reference
  itself cannot fit.
- Non-empty rendered text uses a conservative ceiling byte count, so accounted
  tokens remain bounded by the declared budget.

## Evidence

- Focused C6 builder tests cover section routing, exact revision provenance,
  all four representation levels, and the hard token ceiling.
- `cargo check --workspace` passes after the candidate-contract extension.
- The independent read-only review attempt timed out; no review approval is
  recorded for this slice.
- Remaining workspace and acceptance checks are recorded with the execution
  plan; unrelated dirty files and generated evidence remain untouched.

## Remaining boundary

This slice does not implement or qualify:

- `RET-001` capability/share authorization beyond the existing C5 trusted
  workspace boundary;
- `RET-002` temporal/as-of selection;
- `RET-003` conflict projection and conflict warnings;
- `RET-007` classification/model-destination policy;
- `RET-014` mounted retrieval;
- C7 temporal and multi-channel retrieval, C8 profile qualification, or later
  G/H/E/T phases.

PostgreSQL runtime integration remains an environment-dependent gate and must
be reported separately from compile and unit-test evidence.
