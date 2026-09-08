# Deployment boundaries and readiness

## First target

The roadmap proposes one reproducible local self-hosted environment. That is a qualification goal, not a newly certified platform. CI tags and schema compatibility alone do not qualify filesystem, vault, network policy, or model behavior.

Components include PostgreSQL and role provisioning/migration, server, worker, Console proxy, persistent vault roots, and a separate external read-only bootstrap-secret store. Identify who provisions identities/grants, migrates, and runs the restricted processes.

| Layer | Check | Does not establish |
| --- | --- | --- |
| Process | Liveness and response | Database or useful-task availability |
| Schema/storage | Reachability and migration compatibility | Model, permissions, or ready generation |
| Authority | Token, grants, disclosure policy, vault roots | A particular external call's success |
| Feature | Complete selected memory/retrieval/provider flow | Other features' qualification |
| Release | All required evidence on the exact target | Support for another untested environment |

## Before valuable data

Record target identities and backup/restore procedures; verify key custody and network boundaries. Define retention/logging policy. An isolated developer Compose deployment is not a ready enterprise topology.

Processes need the same accepted source/image and compatible storage contract. Do not mix a newer worker with an untested older server merely because both start.

## Changes

New frontend routes do not expand admission by default. Approved destination/type/model settings use explicit Connection revisions. Imported URLs must not become arbitrary server-side fetch instructions.

Hosted deployment is a separate [P4 proposal](../roadmap/p4-expansion.md), after validating the core scenarios.

**Sources:** [Compose](../../docker-compose.yml), [nginx](../../apps/console/nginx.conf.template), [gate program](../superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).
