# Slice 17a — the embedding channel's transport tells the truth

**Phase:** Trust / T4 (data governance), transport half.
**Status:** plan, narrowed after one adversarial review. Nothing here qualifies a
profile or makes a release claim.

## Why this is half a slice

The plan that went into review gated the embedding channel with a data policy.
The review found the design unsound on four counts, all verified here directly:

- There are **two unrelated `EmbeddingProvider` traits** (`providers/ports.rs`
  and `retrieval/embedding.rs`). A decorator on one closes nothing.
- `embed` carries **no `RequestContext`**, so a decorator could write only a
  channel, a destination and a timestamp — indistinguishable across retries and
  concurrent batches.
- Reusing `[policy.data].classification`, documented as covering "every run
  objective", would silently convert an existing completion declaration into
  consent for bulk tenant-memory disclosure.
- `MemoryRevision.classification` **already exists** and the embedding source
  drops it, reading only content. Governing the corpus by a configured floor
  while its own per-item classification sits unread is the wrong design.

Those belong to a policy slice, which needs a port change and a per-item
classification. This slice takes only the half that needs none of that, and
removes a live exposure today.

## What is wrong now

`OpenAiCompatibleEmbeddingClient` is built as
`Client::builder().timeout(DEFAULT_TIMEOUT).build()` — no `.redirect(..)`, no
`.no_proxy()`. reqwest follows up to ten redirects and honours ambient system
proxies. What travels this channel is `item.content` for every memory awaiting a
vector, `active_text(memory_id)` on delivery, and the user's `query` through the
vector retriever. A configured endpoint can answer `307` and send the corpus,
and any credential, somewhere else.

**The repository already settled this question.** `webhook.rs` builds both of its
clients with `.redirect(reqwest::redirect::Policy::none())`:

> Redirects are refused rather than followed: the destination is configuration,
> and a 302 would let the far side choose a different one.

Correct, written down, and never reached the two adapters that did not have it.
Slice 16 fixed completions. This fixes the third.

## Decisions I am making as lead

1. **Redirects and proxies are disabled on the embedding client**, matching
   webhooks and completions. No configuration switch: a deployment that wants a
   proxy to its embedding provider is not talking to a local model, and the
   locality derivation already says so.

2. **The locality derivation is extracted, not copied.** It is currently private
   inside `openai_compatible.rs`. It moves to a shared `providers` module and
   both clients call it. A second copy could disagree with the first, which is
   the exact defect this repository already carries in `should_redact`, and
   introducing a second instance of a known defect is not acceptable.

3. **The descriptor becomes channel-neutral.**
   `TextGenerationProviderEgress` is named for one channel and now describes two.
   Renaming is part of the extraction, not a follow-up.

4. **No policy, no gate, no migration, no config key, no startup refusal.**
   Nothing here changes what any deployment must declare, so `docker-compose.yml`
   keeps working unchanged. The gate is slice 17b's subject.

5. **The descriptor is produced but not yet consulted for embeddings.** That is
   stated plainly rather than hidden: this slice makes the channel describable
   and safe in transport; it does not make anyone ask about it. A descriptor
   nobody reads is honest groundwork, and calling it enforcement would be the
   claim this project exists to refuse.

## Acceptance criteria

- **Redirect test against the real embedding client**: a loopback listener
  answering `307` with a `Location` to an unroutable host. The client must
  surface the redirect to the caller as an error rather than follow it. `307` is
  used rather than `302` because a 302 turns POST into GET and drops the body,
  which weakens what a body-based assertion can prove.
- Deleting `.redirect(Policy::none())` must make that test fail. Verify by doing
  it, then restore.
- **Proxy test against the real embedding client**: `HTTP_PROXY`/`HTTPS_PROXY`
  pointed at a black hole, loopback request still succeeds. Deleting
  `.no_proxy()` must make it fail. Verify by doing it, then restore.
- The embedding client exposes an egress descriptor whose destination is derived
  by the **shared** function: loopback resolves to `LocalModel`, a private-range
  host and `host.docker.internal` resolve to `RemoteProvider`.
- The extracted derivation has exactly one definition. A test or a grep in the
  report demonstrates there is no second copy.
- Slice 16's completion behaviour is unchanged: its existing redirect, proxy,
  descriptor and LM Studio tests still pass untouched.
- Live: LM Studio at `http://localhost:12345/v1` serves
  `text-embedding-nomic-embed-text-v1.5`. A real embedding round trip through the
  hardened client succeeds and reports `LocalModel`.
- `cargo fmt`, clippy clean; conformance 199/199 and fault suite `failures=2`
  unchanged; the full `vestrace-infrastructure` suite still passes on real
  PostgreSQL.

## Explicitly not in this slice

- The data-policy gate on the embedding channel, the port's missing
  `RequestContext`, per-memory classification from `MemoryRevision`, the evidence
  migration and the `docker-compose.yml` policy — all slice 17b.
- `EmbeddingConfig::secret_name`, which `build_embedding_provider` never passes
  to the client, leaving remote embedding authentication inert. Found during
  review, real, and not this slice's subject.
- Redaction and minimisation, and the duplicate rule in `should_redact`.
- Moving slice 16's completion gate to a different seam.
