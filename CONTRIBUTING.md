# Contributing to Vestrace

Thanks for considering a contribution to Vestrace.

Vestrace is a Rust-based system for governed long-term memory and context across agents and executions. Contributions should preserve its existing boundaries around authority, provenance, lifecycle, and recoverability rather than introduce parallel sources of truth.

## Before you start

1. Read the [project overview](docs/product/overview.md) and [implementation status](docs/status.md).
2. For code changes, review the [architecture](docs/architecture.md) and [development guide](docs/development/README.md).
3. Check existing issues before opening a new one.
4. For security-sensitive findings, follow [SECURITY.md](SECURITY.md) instead of opening a public issue.

The repository contains implemented behavior, proposals, frozen specifications, and historical evidence. Do not treat a design document as proof that a feature is implemented.

## Ways to contribute

Useful contributions include:

- reproducible bug reports;
- focused fixes with tests;
- documentation corrections backed by the current codebase;
- performance or reliability improvements with measurements;
- compatibility improvements for HTTP, MCP, CLI, Console, storage, or deployment flows;
- well-scoped feature proposals tied to a concrete user problem.

Large architectural changes should start as a feature proposal before implementation.

## Development setup

Start with [Getting started](docs/getting-started.md). The project includes Rust services, PostgreSQL storage, HTTP and MCP interfaces, a CLI, and a React/TypeScript Console.

The local Docker Compose stack does not create a production-ready secret store or bootstrap credentials automatically. Follow the documented environment and deployment requirements instead of assuming `docker compose up` is sufficient for every workflow.

## Project boundaries

Keep domain, application, infrastructure, HTTP, CLI, and MCP responsibilities separate. The Console and other clients should not become alternative authorities for state or policy.

Do not modify these as an incidental part of another change:

- frozen specifications;
- historical evidence;
- applied database migrations;
- unrelated roadmap or planning documents.

If a change requires modifying a protected contract or migration strategy, call that out explicitly in the issue and pull request.

## Testing

Run the checks relevant to your change. The project development guide currently documents these baseline commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked --no-fail-fast
cargo test --workspace --doc --all-features --locked
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

Database, Compose, acceptance, fault/restart, and other integration checks may require additional services or environment setup. See [Testing](docs/development/testing.md) and the current [CI workflow](.github/workflows/ci.yml).

Do not report a check as passing unless it was actually run against the proposed change. If a relevant check could not be run, state that clearly in the pull request.

## Pull requests

Keep pull requests focused and reviewable. A good pull request should:

- explain the user or system problem being solved;
- link the related issue when one exists;
- describe the implementation and important trade-offs;
- identify API, schema, migration, security, compatibility, or documentation impact;
- include tests or explain why no test is appropriate;
- list the verification commands actually executed;
- update documentation when observable behavior changes.

Avoid mixing unrelated cleanup or refactoring into a feature or fix unless it is necessary for the change.

## Commit and review expectations

Prefer clear, scoped commit messages. Reviewers may ask for changes when a contribution weakens documented guarantees, duplicates an existing authority or event path, changes public behavior without corresponding tests/documentation, or makes claims that are not supported by evidence.

A pull request is not considered complete merely because it compiles. Acceptance depends on the obligations affected by the change.

## Documentation contributions

Documentation should distinguish between:

- behavior that exists in the current implementation;
- normative requirements;
- proposals or roadmap items;
- historical evidence and prior acceptance records.

When the implementation and documentation disagree, do not silently rewrite history. Update the current-status documentation and preserve frozen evidence unless the task explicitly requires otherwise.

## Community expectations

Participation in the project is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). For usage and support guidance, see [.github/SUPPORT.md](.github/SUPPORT.md).

By contributing, you agree that your contribution is licensed under the repository's Apache-2.0 license unless explicitly stated otherwise.