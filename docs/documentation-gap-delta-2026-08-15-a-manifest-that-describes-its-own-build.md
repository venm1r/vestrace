# A manifest that describes its own build

**Date:** 2026-08-15
**Scope:** the capability manifest's identity fields — where they come from, and
why it matters that nobody types them.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The v1.0 answer two slices ago listed "no capability manifest exists" as
mechanical work. It was not mechanical, and the reason is worth writing down.

`vestrace conformance manifest` existed and took **every** field as a required
argument, including `--source-revision`, `--build-digest`,
`--configuration-digest` and `--environment-manifest`. A qualification bundle
binds to a `target_digest` computed over exactly those, and
`QualificationBaseline::matches_bundle` refuses a bundle whose digest moved —
that is the whole enforcement behind "a material change invalidates the
baseline".

So the mechanism that decides whether a qualification still applies was reading
four strings a human typed. Anything could be made to match anything.

## Where identity comes from now

- **`build_digest`** — a sha256 of the running executable, read by the binary
  producing the manifest. The only version of "this is the artefact that was
  qualified" nobody has to be trusted for.
- **`configuration_digest`** — `AppConfig::identity_digest()`, over an
  explicitly named list of settings that decide behaviour: policy engine and
  version, risk ceiling, capabilities, auth on/off, key version, model and
  embedding configuration, effect adapters, workspaces.
- **`environment_manifest`** — a readable summary rather than a digest, because
  it is the field somebody actually looks at.
- **`source_revision`** — compiled in from `VESTRACE_SOURCE_REVISION`, and
  **refused** if absent: a manifest that cannot say which source it describes
  certifies nothing in particular.

Each can still be overridden by argument, for the case where an operator knows
something the process cannot. The difference is that the default is now the
truth.

### Why the configuration digest is a hand-written list

`AppConfig` holds the master key and the admin token. A digest built by walking
the struct would one day include them in a document published beside a release.
Naming each field means secrets are never reached rather than filtered out, and
it makes "what counts as a material change" a decision rather than a side effect
of which types happened to derive `Serialize`.

Deliberately excluded: bind address and log filter. They change where output
goes, not what the system does.

## Live

```text
product:              vestrace
source_revision:      working-tree-2026-08-15-effects
build_digest:         sha256:1073f30e11fa3f4a2c3f013925e5b1592f563e441f075b13506c3ed5fe8f1661
configuration_digest: sha256:6b5c8f1655ccb28ed3b40ee141d7d9743c5951b7b66b6c191d34e055714d6943
environment_manifest: postgres; policy=CapabilityGrants; auth=true; secrets=local-file;
                      model=disabled; embedding=text-embedding-nomic-embed-text-v1.5;
                      effects=[local-webhook]; workspaces=1
```

The same binary, with one setting changed:

```text
generated-manifest.json   configuration_digest: sha256:6b5c8f16…
changed.json              configuration_digest: sha256:9e0794d4…
```

That is QUAL-014 outside its fixture: changing the embedding model moves the
configuration digest, which moves the target digest, which stops the baseline
matching. Nothing had to be remembered or re-typed for that to be true.

A bundle built against the generated manifest still fails, on the eleven skips
and nothing else — which is the correct answer and the same one as before, now
reached from a manifest that describes the build it came from.

## A Docker lesson, since it cost the most time here

Adding `ENV VESTRACE_SOURCE_REVISION=...` above the `RUN cargo build` left the
compile **cached**, so the binary kept whatever revision it had been built with —
and the manifest command kept refusing, correctly, while I assumed the build had
taken. A layer rebuilds when something it *references* changes, so the argument
is now referenced by the build command itself:

```dockerfile
ARG VESTRACE_SOURCE_REVISION=unknown-source-revision
RUN VESTRACE_SOURCE_REVISION="${VESTRACE_SOURCE_REVISION}" cargo build --release …
```

This is the second stale-image trap in two slices. The check that catches it is
the same one both times: grep the deployed binary for a string only the new code
contains.

## What this does not do

- **No manifest is checked into the repository, deliberately.** Three of its
  fields describe a particular build; a file in git would be wrong for every
  build but one. What belongs in git is the command, and that is now there.
- **`product_version` is `0.0.0`** — the workspace version, untouched. The
  manifest reports it honestly rather than inventing a release number.
- **Nothing signs the manifest or the bundle.** `attach_signature` and
  `validate_signature` exist; no key does.
- **No baseline is published.** `QualificationBaseline::from_bundle` refuses an
  incomplete bundle, and this one fails, so there is nothing to publish yet —
  which is the mechanism working, not a gap.
- **`identity_digest` is only as good as its list.** A setting added to
  `AppConfig` that changes behaviour and is not added there will not move the
  digest, and the baseline will keep vouching for a deployment that changed.
  Nothing enforces that; a reviewer has to.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0**.
