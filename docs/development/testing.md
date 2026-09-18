# Testing: layers, sensitivity, and resources

| Layer | Establishes | Does not replace |
| --- | --- | --- |
| Domain/unit | Local logic and permitted transitions | Runtime wiring, database behavior, authorization |
| Contract/schema | Format and compatibility | Execution of the operation |
| PostgreSQL | Constraints, unit of work, grants, and races on the tested database | Provider/browser interoperability |
| Composition/end-to-end | Real entry points and the selected complete path | Untested topologies |
| Fault/restart | Selected process-death boundaries | Every failure or arbitrary power loss |
| Product quality | Context usefulness on a defined corpus | Security/qualification |
| Release | All declared gates on an exact target | Another target's stability |

## Commands

Run these in a code checkout with the necessary database and environment. They are instructions, not results of this documentation task.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked --no-fail-fast
cargo test --workspace --doc --all-features --locked
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

Doctests are explicit: `--all-targets` does not replace `--doc`. The supplied CI defines an all-targets test job but no separate doctest job; adding one remains a proposed CI change, not part of this refactor. MW-specific targets become runnable only after their implementation.

## RED and mutation sensitivity

Establish a valid fixture first, then observe the intended failing assertion. A missing database URL, nonexistent target, or compilation failure is not behavioral RED.

For a critical guard, weakening it should make the unchanged test fail. Restore exact implementation bytes and observe GREEN. Record both. A test that passes with the implementation removed is not adequate evidence.

## Runtime role

An administrator may prepare a disposable database and exact grants under deployment rules. Run the product portion as the restricted runtime user. Otherwise administrator privileges can hide missing grants or missing denials.

## Resource cost

Measure cold builds, edit/test cycles, peak RAM/disk, and test-binary sizes. Do not run two Cargo builds concurrently by default. Treat profile changes or test-target consolidation as measured experiments without silently removing negative-test classes.

## Documentation validation

[Documentation checks](../maintenance/README.md) verify links, JSON, dependencies, scope, examples, and edition manifests. They do not produce Rust/SQL/browser/provider qualification. Store their results under maintenance, not product evidence.

**Sources:** [CI](../../.github/workflows/ci.yml), [Console scripts](../../apps/console/package.json), [qualification contract](../specs/en/vestrace-qualification-conformance-spec-v0.2.md).
