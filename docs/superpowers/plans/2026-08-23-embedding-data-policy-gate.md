# Slice 17b — the data-policy gate on the embedding channel

**Phase:** Trust / T4 (data governance), policy half.
**Status:** plan, revised after one adversarial review. Nothing here qualifies a
profile or makes a release claim.

17a hardened this channel's transport and produced a descriptor nobody asks
anything. This is the asking.

## A correction I am making to my own previous plan

The earlier draft said `MemoryRevision.classification` is interpreted by nothing
and therefore cannot govern anything. **That was wrong.**
`retrieval::hydration::ClassificationPolicy` already interprets it: exact-label
admission, plus a separate explicit flag for whether content carrying *no*
classification may be disclosed, because — in the domain's own words — "a
revision with `NULL` classification has not been assessed", which is not the
same as unclassified. It has no allow-everything variant that a deployment can
drift into, and it is kept in the domain deliberately

> so a future adapter cannot quietly implement a laxer version of it.

That is precisely what the embedding path is doing today. Retrieval withholds a
revision whose label is not admissible; the embedding sources read only
`content` and send the same revision to a provider. The rule exists, is
argued, and one path ignores it.

So this slice does not invent a mapping from label to `Sensitivity`, and it does
not govern the corpus by a configured floor alone. **It reuses
`ClassificationPolicy`.**

## Decisions I am making as lead

1. **Two checks, not one.** Per item, `ClassificationPolicy` decides whether the
   label is admissible for embedding — the same rule retrieval uses, in one
   place. Per channel, `DataPolicy` decides whether the destination may receive
   the data at all. Both must pass. A denial names which one refused.

2. **`[policy.data.embedding]` declares both**: the admissible label set and
   whether unclassified revisions may be embedded, alongside its own
   classification floor, ceiling and allowed destinations. It is a policy of its
   own, not a widened reading of the completion key: an existing declaration
   about run objectives must not become consent for bulk memory disclosure. No
   ordering between the two channels is enforced — requiring
   `embedding >= completion` would presume memory is always at least as
   sensitive as an objective.

3. **No `workspace_id` on the evidence table.** The repository enforces
   `a_table_holding_a_workspace_id_has_row_level_security`: a `workspace_id`
   column declares tenant ownership and demands forced RLS. Calling one
   "attribution, not tenancy" is a contradiction the codebase already refuses,
   and renaming the column to evade the invariant would be worse — the same
   semantics wearing a disguise.

4. **The row carries a causal reference instead**, which is what the attribution
   was actually for. A workspace id would not have distinguished anything: the
   worker builds one `RequestContext` per workspace and reuses it across every
   delivery and retry, and rebuild reuses one across every batch and probe. The
   reference is the outbox message id and attempt for delivery, the retrieval
   request id for a query, and the rebuild invocation id and batch ordinal for
   backfill and probe. Two rows for the same disclosure must be distinguishable
   from one row for two disclosures.

5. **Construction is governed, not merely wired.** Wrapping the three
   composition roots proves today's wiring and nothing more:
   `OpenAiCompatibleEmbeddingClient` is public and re-exported, and
   `PgVectorRetriever::new`, `EmbeddingBackfill::new` and the delivery
   constructor all accept a raw `SharedEmbeddingProvider`. Consumers take a
   governed type that only the gate can produce; the raw client stops being
   reachable for this use.

6. **Purpose is expressed by method, not by argument.** Purpose-specific entry
   points rather than an enum a careless caller can mislabel. A row saying
   `dimension_probe` must be one only the probe could have written.

7. **The probe is gated, with no exemption.** It discloses nothing, so exempting
   it is tempting and wrong: an ungated route to the same endpoint is how a gate
   becomes decorative.

8. **The dead pair is deleted, and its remains with it.**
   `providers/ports.rs::EmbeddingProvider` has no callers, and
   `impl EmbeddingProvider for OpenAiCompatibleClient` is its only
   implementation. Deleting both leaves `OpenAiCompatibleClient.base_url`
   unread and `EmbeddingRequest`/`EmbeddingResponse` orphaned, which would fail
   `clippy -D warnings`; they go too. This removes exported items, which is an
   API break with no in-repository caller, and that is stated rather than
   discovered later.

9. **`embedding.enabled` with no `[policy.data.embedding]` refuses startup.**
   Absence is not consent. A completion policy alone does not satisfy it.

10. **`docker-compose.yml` gets an honest policy in the same commit.** Both
    server and worker enable embeddings today, so both would otherwise refuse to
    start. `host.docker.internal` resolves to the host gateway and derives as
    `RemoteProvider`, contrary to the comment beside it, so the policy must
    allow `remote_provider` or the default deployment does not work. The
    misleading comment is corrected in the same change.

11. **The decision commits before the provider is called**, and a repository
    failure prevents the call.

## Acceptance criteria

- `embedding.enabled = true` with no `[policy.data.embedding]`: startup refused,
  keys named. A completion policy alone does not satisfy it.
- **A revision whose label retrieval would withhold is refused for embedding**,
  and the denial names the label check rather than the destination. This is the
  criterion the slice exists for.
- A revision with `NULL` classification follows the declared unclassified flag,
  in both directions, and the two cases are distinguishable in the evidence.
- Allowing `local_model` with a loopback endpoint: embedding proceeds, one
  allowance recorded per call before the request, carrying purpose, causal
  reference and input count.
- Non-loopback endpoint: the operation fails, nothing is sent, denial recorded.
  Proven by a provider that records whether it was called.
- **The vector retriever's query is gated with no change at its call site.**
- **The probe is gated** and its row says `dimension_probe`, written by a path
  no other caller can reach.
- Two deliveries of the same outbox message produce two distinguishable rows.
- `observe` mode: the call proceeds and the denial is recorded.
- **A repository failure prevents the provider call.** Proven with a double.
- No ungated construction path remains: the raw embedding client cannot be
  injected into the retriever, backfill or delivery. Demonstrated by the type
  system, and the report says how.
- `row_level_security` tests still pass, including
  `a_table_holding_a_workspace_id_has_row_level_security`.
- **Compose smoke asserts the worker is still running**, not only that the
  server is healthy — today it observes server readiness alone, so a worker that
  rejects its policy and exits would go unnoticed.
- Faking the gate to always allow makes a test fail.
- Slices 16 and 17a unchanged: their redirect, proxy, descriptor and LM Studio
  tests still pass untouched.
- Live: a real embedding round trip through LM Studio
  (`text-embedding-nomic-embed-text-v1.5`) with the gate allowing it.
- `cargo fmt`, clippy clean; conformance 199/199 and fault suite `failures=2`
  unchanged; full `vestrace-infrastructure` suite passing on real PostgreSQL.

## Explicitly not in this slice

- Constraining `MemoryRevision.classification` to a vocabulary. This slice reuses
  the admission rule that already exists; giving the field a validated domain is
  separate.
- `memory_repository`'s `row.try_get("classification").ok()`, which turns a
  decode failure into an absent classification — a failure manufactured into an
  absence. Real, small, not this slice.
- `EmbeddingConfig::secret_name`, still inert.
- Redaction and minimisation, and the duplicate rule in `should_redact`.
- Any change to the release gate or its evidence families.
