# A grant that could not say "any"

**Date:** 2026-08-15
**Scope:** `scope_is_within`, and the grants it made unexpressible.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Yesterday's delta ended with a limitation I described and postponed:

> `scope_is_within(child, parent)` is `child == parent || child.starts_with("{parent}/")`.
> That is correct for path scopes … and it does **not** work for the URI-shaped
> scopes the resource checks use. … The real options are to teach the matcher
> about `://` or to stop using URI-shaped resource scopes for grants, and both
> are larger than this slice.

They were not larger. This is the first of them.

## The consequence, stated plainly

A grant scoped to `memory://` could not cover `memory://0198…`, because the
prefix test looked for `memory:///`. So the authority "may purge any memory in
this workspace" **could not be written down**. Only "may purge this one".

An operator who cannot say what they mean says something broader instead. The
available way to express "any memory" was a grant on `/v1` with the `http`
operation — which covers every route under `/v1`, for every capability the
principal holds. The scope language pushed people from a precise grant toward the
broadest one available, which is the opposite of what resource scopes are for.

## The rule now

`child` is inside `parent` when it continues past it **at a boundary**:

- a parent that already ends in `/` — `memory://`, `finding://`, `/v1/` — needs
  only something to follow it;
- a parent that does not needs the next character to be `/`.

The property the prefix test existed for is kept, and is the part worth testing
hardest: `memory://abc` is **not** inside `memory://ab`, `/v1-admin` is **not**
inside `/v1`, a different scheme is a different world, and a wildcard on either
side is refused rather than interpreted. An empty parent contains nothing, which
it previously did not — `"".starts_with("/")` was false for every child, so this
is a codified accident rather than a change.

Five unit tests, three of them about the dangerous direction.

## Live

**One grant, any memory.** A single `memory.purge` grant scoped to `memory://`,
plus the surface grant on `/v1/memories`, and two freshly created memories are
both destroyed under it:

```text
memory 1: HTTP 200
memory 2: HTTP 200
```

Yesterday this took one grant per memory, issued after the memory existed.

**One grant, any finding.** A `workspace.admin` grant on `health.disposition`
scoped to `finding://` now answers a finding it was not written for:

```text
memory.active_memory_is_embedded | accepted_risk
```

**And nothing was widened by accident.** A finding id that does not exist returns
404 — authorized by the prefix grant, then not found, which is the right order —
and the neighbouring-identifier cases are covered by tests rather than by a live
demonstration, because constructing one requires two resources whose ids share a
prefix.

Health, with both findings answered:

```text
healthy: True
 - outbox.no_dead_letters           | error   | suppressed
 - outbox.backlog_within_budget     | warning | open
 - memory.active_memory_is_embedded | warning | accepted_risk
```

## What this does not do

- **It does not touch `selector_is_within`.** Operation selectors are
  dot-separated and `http` already contains `http.delete`; that matcher has the
  same shape and no equivalent gap, because nobody writes an operation ending in
  a separator.
- **Two grants are still needed to purge a memory** — one for the route, one for
  the resource. This makes each of them expressible in general terms; it does not
  merge them. Whether the surface check and the resource check should be one
  question remains open, and I still think they should not.
- **A trailing slash is now load-bearing.** `memory://` and `memory:/` are
  different parents, and only the first reads like something a person meant.
  Nothing validates that a grant's scope is well-formed for the resources it is
  supposed to cover; a typo produces a grant that silently covers nothing.
- **The scope language is still ad hoc.** Two syntaxes — paths and URIs — share
  one matcher, and the matcher now knows about both by accident of their both
  using `/`. A grant scoped to `memory://x` and a route scoped to `/v1/memories/x`
  describe the same resource in two vocabularies, and nothing checks they agree.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed**.

Two tests in `effect_fault_runtime.rs` — which spawn child processes with
timeouts — failed on the first run of the suite while the machine was compiling
in parallel, and passed on their own immediately afterwards
(`14 passed; 0 failed`). They are timing-sensitive under load rather than broken,
and that is worth recording rather than reporting a clean run and moving on.
