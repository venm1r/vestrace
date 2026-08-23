# An answer that stayed in one file

**Date:** 2026-08-23
**Scope:** the embedding channel's transport. Half of a planned slice; the other
half was cut by review and is named below.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim.

```text
infrastructure suite:  30 targets, 230 passed, 0 failed  (real PostgreSQL 17 + pgvector)
live model tests:      3 passed — the new embedding one and both from slice 16
conformance gate:      199 passed, 0 failed, 0 skipped — unchanged
fault suite:           FAILED — 2 failures, unchanged
```

## A different kind of debt

The previous nine gaps this session were between *modelled* and *used*: a type
existed and nothing called it. This one is different, and worse in a specific
way.

`webhook.rs` has built both of its clients with redirects refused for as long as
it has existed, and states the reason:

> Redirects are refused rather than followed: the destination is configuration,
> and a 302 would let the far side choose a different one.

That reasoning is correct. It was arrived at, written down, and argued in place.
It then stayed inside the file that arrived at it. The completion client did not
have it — slice 16 found that out from an adversarial review — and the embedding
client did not have it either.

Nothing looks wrong when you read any one of these files. The knowledge simply
did not propagate. A gap between *modelled* and *used* is visible to a grep; a
gap between *settled* and *applied everywhere* is not.

## What was exposed

`OpenAiCompatibleEmbeddingClient` was built as
`Client::builder().timeout(DEFAULT_TIMEOUT).build()`. No `.redirect(..)`, no
`.no_proxy()`. reqwest follows up to ten redirects by default and honours
ambient system proxies.

What travels this channel is not a prompt:

- `EmbeddingBackfill` sends `item.content` for **every memory awaiting a
  vector** — the tenant's corpus, in bulk;
- the delivery path sends `active_text(memory_id)`;
- the vector retriever sends the user's `query`.

A configured endpoint could answer `307` and receive all of it, along with any
credential, at an address the deployment never named.

## Extracted, not copied

The locality derivation was private inside `openai_compatible.rs`. Copying it
into `embeddings.rs` would have been the obvious move and the wrong one: this
repository already carries one duplicated rule — `security/redaction.rs::
should_redact` answers the same question as `DataPolicy` from a hardcoded matrix
— and it is free to disagree with its twin. Adding a second instance of a known
defect while fixing a first is not a trade worth making.

There is now exactly one definition, in `providers/egress.rs`, and both clients
call it. `TextGenerationProviderEgress` is renamed `ProviderEgress` because it
now describes two channels.

## 307, not 302

The redirect test answers `307 Temporary Redirect`. A `302` turns POST into GET
and drops the body, so an assertion of the form "the far side received no
content" can pass even with redirects enabled.

The assertion used is stronger and does not depend on that at all: the client
must surface the redirect **to the caller as an error**. That can only happen if
the redirect was refused; had it been followed, the result would have been a send
failure against the unroutable target instead.

The same reasoning applies to slice 16's completion test, which was reviewed for
this and holds: it asserts `InvalidResponse` containing `302 Found`, which a
following client could never produce.

## Proof by breaking

| faked | what failed |
|---|---|
| `.redirect(Policy::none())` deleted | 307 test — client followed the redirect, error changed to a send failure |
| `.no_proxy()` deleted | proxy test — failed after 2.02s where the restored test passes in 0.01s |

## What this does not do

**It produces a descriptor that nothing yet consults.** The embedding channel
can now say where it goes; no policy asks. That is groundwork, and calling it
enforcement would be exactly the claim this project exists to refuse.

**The data-policy gate on this channel is not here.** The reviewed plan included
it and the review was right to reject it, on four counts verified directly:

- there are **two unrelated `EmbeddingProvider` traits**
  (`providers/ports.rs` and `retrieval/embedding.rs`), so a decorator on one
  closes nothing;
- `embed` carries no `RequestContext`, so a decorator could record only a
  channel, a destination and a timestamp — indistinguishable across retries and
  concurrent batches;
- reusing `[policy.data].classification`, documented as covering "every run
  objective", would have silently turned an existing completion declaration into
  consent for bulk memory disclosure;
- **`MemoryRevision.classification` already exists** and the embedding source
  drops it, reading only `content`. Governing a corpus by a configured floor
  while its own per-item classification sits unread is the wrong design.

That slice needs a port change and a per-item classification. It is not this one.

## Found in passing

**`EmbeddingConfig::secret_name` is inert.** It is documented as naming an
optional workspace secret holding the provider key, and
`build_embedding_provider` always passes `None` to the client. Remote embedding
authentication therefore cannot work, while the configuration says it can. The
eleventh gap of this session between what is declared and what runs.

**`docker-compose.yml` calls `host.docker.internal` local.** It resolves to the
host gateway, which is not loopback, so the derivation classifies it
`RemoteProvider` — correctly, and contrary to the comment beside it. Covered by
a test here so the disagreement is recorded rather than argued later.
