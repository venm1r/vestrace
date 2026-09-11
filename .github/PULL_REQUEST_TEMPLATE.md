## Summary

<!-- What problem does this change solve? Keep the description focused on observable behavior and project obligations. -->

## Related issue

<!-- Link the issue or proposal when one exists. Use "None" if this is a small maintenance change. -->

## Scope

<!-- What components, interfaces, or documents are intentionally changed? What is explicitly out of scope? -->

## Change type

- [ ] Bug fix
- [ ] Feature
- [ ] Refactor
- [ ] Documentation
- [ ] Test / tooling / CI
- [ ] Operations / deployment
- [ ] Other

## Contract and compatibility impact

Check all that apply and explain any checked item below.

- [ ] Public HTTP/API behavior
- [ ] MCP behavior or schema
- [ ] CLI behavior
- [ ] Console behavior
- [ ] Database schema or migration
- [ ] Authentication / authorization / security boundary
- [ ] Authority, provenance, governance, or lifecycle semantics
- [ ] Import/export or interoperability
- [ ] Backward compatibility
- [ ] No contract or compatibility impact

### Impact notes

<!-- Explain checked impacts, migration requirements, compatibility expectations, and relevant design/spec references. -->

## Verification

List the commands actually run against this change and their result. Do not list checks that were not executed.

```text
# example
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked --no-fail-fast
```

## Evidence

<!-- Add relevant test output, measurements, screenshots, reproduction results, or links to evidence. Avoid credentials and private data. -->

## Documentation

- [ ] User/developer documentation was updated where observable behavior changed.
- [ ] Implementation-status wording still distinguishes implemented behavior from proposals.
- [ ] Frozen specifications, historical evidence, and applied migrations were not changed incidentally.
- [ ] Documentation changes are not required for this PR.

## Security and privacy

- [ ] I considered authentication, authorization, tenant/workspace isolation, secrets, and sensitive logging where relevant.
- [ ] This PR does not disclose live credentials, tokens, private datasets, or vulnerability details that require coordinated disclosure.

## Final checklist

- [ ] The change is focused and avoids unrelated cleanup.
- [ ] Tests cover the affected obligation, or the reason tests are not applicable is explained.
- [ ] Relevant verification was run, or missing verification is called out explicitly.
- [ ] New behavior does not introduce a second source of truth, policy authority, or event path without an explicit architectural decision.
- [ ] Any known follow-up work or remaining limitation is documented.