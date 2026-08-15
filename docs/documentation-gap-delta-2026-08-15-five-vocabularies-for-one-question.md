# Five vocabularies for one question

**Date:** 2026-08-15
**Scope:** capability grant resource scopes — the strings an operator writes to
say *what* a permission is about.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
gate before:  192 passed (184 executed, 8 attested), 7 skipped
gate after:   192 passed (184 executed, 8 attested), 7 skipped
suite:        884 → 899 passed, 0 failed
```

No case count changed. This slice fixed a defect and strengthened an existing
case rather than closing a skip.

## What was found

`resource_scope` was a `String`, validated as non-blank and nothing else. Every
place that asked for authorization invented its own way of naming what it was
asking about:

| where | what it said |
|---|---|
| HTTP router | `/v1/memories/0198…` |
| memory purge | `memory://0198…` |
| health disposition | `finding://0198…` |
| run worker | `run:0198…` |
| MCP server | `memory_id:0198…`, or `workspace` |
| grant policy engine | `unscoped` |
| external effects | `endpoint:alpha`, `customer://42/card` |

Two consequences, both live.

### One resource had three names

The same memory is `/v1/memories/0198…` through HTTP, `memory://0198…` when it
is purged, and `memory_id:0198…` through MCP. A grant written in one of those
authorises nothing in the other two. An operator has no way to discover this
except by being denied, and the denial says `ResourceMismatch` — which reads as
"you granted the wrong resource", not "you granted it in a spelling nothing
asks in".

### Two of those spellings could not express a workspace-wide grant at all

`scope_is_within` treats `/` as the boundary between a scope and what is inside
it. That rule is right and was defended at length when `memory://` was fixed —
but it was fixed for `/`-shaped scopes only, and the `:`-shaped ones were
missed. Measured directly:

```text
       memory://  contains  memory://0198abc         => true
      finding://  contains  finding://0198abc        => true
             /v1  contains  /v1/memories/0198abc     => true
            run:  contains  run:0198abc              => false
```

So **"may work any run in this workspace" was unwriteable.** Runs had to be
granted one by one, or — the thing an operator actually does when they cannot
say what they mean — permitted through something broader elsewhere. The same
was true of every MCP scope, since all of them were `<key>:<value>`.

## What changed

A scope is now a parsed thing with a grammar, in
[`security/scope.rs`](../crates/vestrace-domain/src/security/scope.rs):

- `<kind>://` — every resource of a kind. The trailing `//` is what makes it a
  container under the boundary rule.
- `<kind>://<id>` — one resource.
- `/<path>` — an HTTP surface.
- `<scheme>://<target>` — something at a provider.

The kinds are a **closed set**, one per thing a capability can be about.
Parsing happens in two places, and both matter:

- **When a grant is issued.** A scope that could never match anything is refused
  at the moment somebody writes it, rather than being stored, appearing in a
  listing as though it were a permission, and denying every request forever.
- **When an authorization is requested.** A request in a private vocabulary is a
  bug in the caller, not a denial. Reporting it as `ResourceMismatch` blames the
  operator for the caller's spelling. This is also what stops the *next* call
  site inventing a sixth vocabulary and finding out in production.

Call sites now speak it: the run worker asks about `run://{id}`, the MCP server
names resources by kind rather than by argument key, and the grant policy engine
asks about `workspace://` instead of the bare word `unscoped`.

External effect intents are validated too, because `intent.target()` *becomes*
the resource scope of the authorization the effect dispatches under — so a
target nobody can write a grant for is an effect that can never be permitted,
and the failure belongs where the caller can still fix it.

## The decision worth defending: the closed set stops at the boundary

A payments adapter charges `customer://42/card`. This system does not know which
namespaces an adapter speaks, and a closed list of external schemes would mean
every new adapter needed a domain change before it could be granted. So external
schemes are open.

The cost is exact and worth stating plainly: **`memroy://0198…` is refused as an
internal scope only because `memroy` is not a kind, and is then admitted as an
external target.** A misspelled internal kind survives as a grant that matches
nothing.

What contains it is the check on the other side. Every authorization request
this system makes is internal, and requests are parsed too — so a caller cannot
ask about `memroy://`. A misspelled grant is *unreachable*, not dangerous: a
permission that does nothing, rather than a permission that does something
unintended. That is a weaker guarantee than the one for internal scopes, and the
module says so where somebody will read it.

## Evidence

CAP-002 was already the case about scope hierarchy, and this defect *is* a
hierarchy defect — a shape of name that could not nest. It now also asserts:

- a grant on `run://` covers `run://0198…`, and one on `run://0198…` does not
  cover `run://`;
- a scope carrying `*` is refused **when the grant is written**, not merely
  ignored when it is compared. The old case issued such a grant successfully and
  relied on the comparison failing — which in a listing is indistinguishable
  from a grant that is simply narrow;
- `run:0198…`, `memory_id:0198…`, `unscoped` and `workspace` — all four of them
  live scopes in this system a day ago — are refused at issue.

Mutation-proved by disabling the container rule in `scope_is_within`. The case
failed with *"a grant for every run did not cover one run, so 'may work any run
in this workspace' cannot be written and each run must be granted by name"*, and
passed again once restored.

That proof was got wrong the first time and is worth recording: the restore was
done with `mv`, which preserves the backup's mtime, so cargo saw the source as
older than the build artifact and did not rebuild. The "restored" run was the
mutated binary, and it reported a failure I would have read as real. Both
directions were redone with an explicit `touch` and a checked build. This is the
third variant of the same trap — after two stale Docker images — and the rule
that catches it is unchanged: **check that the artifact you are reading was
built from the source you think it was.**

## What this does not do

- **It does not unify the two axes.** A grant on `/v1/memories` is about which
  door a caller may knock on; a grant on `memory://` is about which resource may
  be touched once inside. The router asks the first and the purge asks the
  second, which is why **a purge still needs two grants**. That was a standing
  gap before this slice and remains one; collapsing the axes here would have
  silently widened one of them, and which one should win is a design question I
  did not answer.
- **Stored grants are not re-validated.** The check is at issue time. A `run:`
  grant already sitting in a database stays exactly as unmatchable as it was,
  and nothing reports it. Migration 0147's constraint is still only
  `btrim(resource_scope) <> ''`; no migration was added, and a database-level
  check would be the honest place for one.
- **Nothing tells an operator why a denial happened.** `ResourceMismatch` still
  names no scopes. The vocabulary now makes the *mistake* harder to write; it
  does not make the *denial* easier to read.
- **The closed set is not enforced end-to-end.** It constrains what can be
  written and what can be asked. It does not prove every entry point asks — that
  is CAP-005, which remains a skip for the same reason it always did.

## Test results

Full workspace suite against a live PostgreSQL 17: **899 passed, 0 failed**
(884 before; the increase is this module's unit tests). Conformance gate:
**199 total, 192 passed (184 executed, 8 attested), 0 failed, 7 skipped**.

Remaining skips are unchanged: CAP-005, CAP-012, IDW-010, IDW-014, QUAL-010,
REC-016, RET-004.
