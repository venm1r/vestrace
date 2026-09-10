import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import {
  changeScopePaths as p03ChangeScopePaths,
  protectedAuthorityPaths as p03ProtectedAuthorityPaths,
} from '../scripts/p03-scope.mjs';
import { changeScopePaths, protectedAuthorityPaths } from '../scripts/p04-scope.mjs';

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function git(cwd, args) {
  return execFileSync('git', args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
}

/// Builds a throwaway repository whose whole dirty set is the protected
/// authority, runs the verifier over it, then mutates one protected path and
/// requires the verifier to reject it.
///
/// `mutate` names the path to break. It is a parameter rather than
/// `protectedPaths[0]` because P04's decisive case is one specific file: if
/// `scripts/verify-dirty-baseline.mjs` can be changed without the verifier
/// noticing, this package's central protection is decorative.
function verifierFixture({ changePaths, label, mutate, preflightPath, protectedPaths, scope }) {
  const repoRoot = mkdtempSync(join(tmpdir(), `vestrace-${label.toLowerCase()}-scope-`));
  const protectedPath = mutate ?? protectedPaths[0];
  assert.ok(
    protectedPaths.includes(protectedPath),
    `${label} fixture must mutate a protected path, got ${protectedPath}`,
  );
  mkdirSync(dirname(join(repoRoot, preflightPath)), { recursive: true });
  writeFileSync(join(repoRoot, 'README.md'), 'fixture\n');
  git(repoRoot, ['init']);
  git(repoRoot, ['config', 'user.email', 'fixture@example.invalid']);
  git(repoRoot, ['config', 'user.name', 'fixture']);
  git(repoRoot, ['config', 'core.autocrlf', 'false']);
  git(repoRoot, ['add', 'README.md']);
  git(repoRoot, ['commit', '-m', 'fixture']);
  for (const path of protectedPaths) {
    mkdirSync(dirname(join(repoRoot, path)), { recursive: true });
    writeFileSync(join(repoRoot, path), `original authority: ${path}\n`);
  }
  const rawStatus = git(repoRoot, ['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  const authorityRecords = protectedPaths.map((path) => {
    const bytes = readFileSync(join(repoRoot, path));
    return { bytes: bytes.length, path, sha256: sha256(bytes), status: '??' };
  });
  writeFileSync(join(repoRoot, preflightPath), JSON.stringify({
    captured_at_utc: '2026-09-03T00:00:00.000Z',
    change_scope_paths: changePaths,
    dirty_files: authorityRecords,
    head: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    protected_authority_digests: authorityRecords.map(({ bytes, path, sha256: digest }) => ({
      bytes,
      path,
      sha256: digest,
    })),
    protected_authority_paths: protectedPaths,
    schema_version: 1,
    status_porcelain_v1_z_base64: rawStatus.toString('base64'),
    status_porcelain_v1_z_sha256: sha256(rawStatus),
  }, null, 2));

  const args = ['scripts/verify-dirty-baseline.mjs', '--check', repoRoot, join(repoRoot, preflightPath)];
  if (scope) args.push('--scope', scope);
  execFileSync(process.execPath, args, { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });

  writeFileSync(join(repoRoot, protectedPath), 'mutated authority\n');
  let failure;
  try {
    execFileSync(process.execPath, args, { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  } catch (error) {
    failure = error;
  }
  assert.ok(failure, `${label} expected verifier rejection after mutating ${protectedPath}`);
  assert.match(failure.stderr, /protected authority changed/);
  assert.match(failure.stderr, new RegExp(protectedPath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
}

test('P04 dispatch verifies its own scope and still rejects a protected mutation', () => {
  verifierFixture({
    changePaths: changeScopePaths,
    label: 'P04',
    preflightPath: 'docs/development-evidence/v1-g0-04c-preflight.json',
    protectedPaths: protectedAuthorityPaths,
    scope: 'p04-scope.mjs',
  });
}, { timeout: 20_000 });

test('P04 refuses a mutation of the baseline verifier itself', () => {
  // The decisive case. Every other protected path is protected by the same
  // mechanism; this one is protected *from* the package that would otherwise
  // be able to weaken the mechanism.
  verifierFixture({
    changePaths: changeScopePaths,
    label: 'P04-verifier',
    mutate: 'scripts/verify-dirty-baseline.mjs',
    preflightPath: 'docs/development-evidence/v1-g0-04c-preflight.json',
    protectedPaths: protectedAuthorityPaths,
    scope: 'p04-scope.mjs',
  });
}, { timeout: 20_000 });

test('P03 dispatch is unchanged by P04 arriving', () => {
  verifierFixture({
    changePaths: p03ChangeScopePaths,
    label: 'P03',
    preflightPath: 'docs/development-evidence/v1-g0-03-preflight.json',
    protectedPaths: p03ProtectedAuthorityPaths,
    scope: 'p03-scope.mjs',
  });
}, { timeout: 20_000 });

test('P04 scope declares unique, sorted, disjoint change and protected paths', () => {
  assert.deepEqual(changeScopePaths, [...changeScopePaths].sort());
  assert.deepEqual(protectedAuthorityPaths, [...protectedAuthorityPaths].sort());
  assert.equal(new Set(changeScopePaths).size, changeScopePaths.length);
  assert.equal(new Set(protectedAuthorityPaths).size, protectedAuthorityPaths.length);
  // Completion contract: exactly 119 task paths; no historical write permissions.
  // 111 frozen by Task 1, plus the two-path Task 5 amendment that lets the
  // shared output-key authority accept rebuild as well as delivery, plus the
  // one-path Task 8 amendment for the conformance fixture that constructs a
  // RetrievalResult literally, plus the three-path Task 11 amendment that lets
  // a content material be owned by a memory revision rather than a principal,
  // plus the two-path amendment that lets production name an embedding job as
  // the cause of its own model-request evidence.
  assert.equal(changeScopePaths.length, 119);
  assert.equal(protectedAuthorityPaths.length, 23);
  for (const path of protectedAuthorityPaths) assert.ok(!changeScopePaths.includes(path));
});

test('P04 protects the baseline verifier and every accepted predecessor authority', () => {
  for (const path of [
    'scripts/verify-dirty-baseline.mjs',
    'scripts/p03-scope.mjs',
    'tests/p03_scope.test.mjs',
    'docs/development-evidence/v1-g0-03-preflight.json',
    'docs/development-evidence/v1-g0-03-provider-execution-foundation.md',
    'docs/superpowers/plans/2026-08-28-vestrace-v1-g0-03-provider-execution-foundation.md',
    'docs/superpowers/plans/2026-09-03-vestrace-v1-g0-04-embedding-transition-foundation.md',
    'docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md',
    'schemas/protocol-lock.json',
    'schemas/openai-compatible/openai-chat-completions-v1-q1.json',
    'tests/fixtures/openai-q1/marker.png',
  ]) {
    assert.ok(protectedAuthorityPaths.includes(path), `expected protected: ${path}`);
    assert.ok(!changeScopePaths.includes(path), `must not be writable: ${path}`);
  }
});

test('P04 completion scope is exactly the approved literal task paths', () => {
  assert.deepEqual(changeScopePaths, [
    'crates/vestrace-application/src/conformance_cases.rs',
  'crates/vestrace-application/src/connections.rs',
    'crates/vestrace-application/src/embedding/adoption.rs',
    'crates/vestrace-application/src/embedding/barrier.rs',
    'crates/vestrace-application/src/embedding/carry.rs',
    'crates/vestrace-application/src/embedding/erasure.rs',
    'crates/vestrace-application/src/embedding/executor.rs',
    'crates/vestrace-application/src/embedding/finalization.rs',
    'crates/vestrace-application/src/embedding/index.rs',
    'crates/vestrace-application/src/embedding/job.rs',
    'crates/vestrace-application/src/embedding/keys.rs',
    'crates/vestrace-application/src/embedding/mod.rs',
    'crates/vestrace-application/src/embedding/result.rs',
    'crates/vestrace-application/src/embedding/retrieval.rs',
    'crates/vestrace-application/src/embedding/transition.rs',
    'crates/vestrace-application/src/embedding/transition_coordinator.rs',
    'crates/vestrace-application/src/embedding/work.rs',
    'crates/vestrace-application/src/material/erasure.rs',
    'crates/vestrace-application/src/model_request_evidence.rs',
  'crates/vestrace-application/src/provider_dispatch.rs',
  'crates/vestrace-application/src/retrieval/ports.rs',
    'crates/vestrace-application/src/retrieval/request.rs',
    'crates/vestrace-application/src/retrieval/service.rs',
    'crates/vestrace-cli/src/commands/mcp.rs',
    'crates/vestrace-cli/src/commands/rebuild.rs',
    'crates/vestrace-cli/src/commands/schema.rs',
    'crates/vestrace-cli/src/commands/server.rs',
    'crates/vestrace-cli/src/commands/worker.rs',
    'crates/vestrace-cli/src/main.rs',
    'crates/vestrace-cli/tests/embedding_worker_once.rs',
    'crates/vestrace-cli/tests/provider_openapi_contract.rs',
    'crates/vestrace-cli/tests/provider_runtime_wiring.rs',
    'crates/vestrace-cli/tests/worker_once_cli.rs',
    'crates/vestrace-domain/src/embedding/adoption.rs',
    'crates/vestrace-domain/src/embedding/generation.rs',
    'crates/vestrace-domain/src/embedding/index.rs',
    'crates/vestrace-domain/src/embedding/job.rs',
    'crates/vestrace-domain/src/embedding/mod.rs',
    'crates/vestrace-domain/src/embedding/retrieval.rs',
    'crates/vestrace-domain/src/embedding/space.rs',
    'crates/vestrace-domain/src/id.rs',
  'crates/vestrace-domain/src/material/intent.rs',
    'crates/vestrace-domain/tests/embedding_contract.rs',
    'crates/vestrace-fault-scenario/src/child.rs',
    'crates/vestrace-fault-scenario/src/main.rs',
    'crates/vestrace-fault-scenario/src/report.rs',
    'crates/vestrace-fault-scenario/src/scenarios/embedding_worker_completion_crash.rs',
    'crates/vestrace-fault-scenario/src/settings.rs',
    'crates/vestrace-http/src/api/embedding_jobs.rs',
    'crates/vestrace-http/src/api/mod.rs',
    'crates/vestrace-http/src/api/models.rs',
    'crates/vestrace-http/src/health.rs',
    'crates/vestrace-http/src/route_inventory.rs',
    'crates/vestrace-http/src/router.rs',
    'crates/vestrace-http/tests/embedding_retrieval_routes.rs',
    'crates/vestrace-http/tests/route_inventory_is_exhaustive.rs',
    'crates/vestrace-infrastructure/src/config.rs',
    'crates/vestrace-infrastructure/src/embedding_index/flat.rs',
    'crates/vestrace-infrastructure/src/embedding_index/mod.rs',
    'crates/vestrace-infrastructure/src/embedding_index/registry.rs',
    'crates/vestrace-infrastructure/src/lib.rs',
    'crates/vestrace-infrastructure/src/postgres/credential_activation.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_adoption_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_erasure_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_index_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_job_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_key_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_result_finalization_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_result_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_retrieval_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_store.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/embedding_work_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/erasure.rs',
    'crates/vestrace-infrastructure/src/postgres/material_intent.rs',
  'crates/vestrace-infrastructure/src/postgres/mod.rs',
    'crates/vestrace-infrastructure/src/postgres/model_binding_repository.rs',
  'crates/vestrace-infrastructure/src/postgres/model_request_evidence_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/qualification_job_repository.rs',
    'crates/vestrace-infrastructure/src/postgres/vector_retriever.rs',
    'crates/vestrace-infrastructure/tests/credential_activation.rs',
    'crates/vestrace-infrastructure/tests/embedding_canonical_generations.rs',
    'crates/vestrace-infrastructure/tests/embedding_carry_classification.rs',
    'crates/vestrace-infrastructure/tests/embedding_effect_recovery.rs',
    'crates/vestrace-infrastructure/tests/embedding_erasure_propagation.rs',
    'crates/vestrace-infrastructure/tests/embedding_executor.rs',
    'crates/vestrace-infrastructure/tests/embedding_index_builds.rs',
    'crates/vestrace-infrastructure/tests/embedding_legacy_adoption.rs',
    'crates/vestrace-infrastructure/tests/embedding_result_finalization.rs',
    'crates/vestrace-infrastructure/tests/embedding_result_preparation.rs',
    'crates/vestrace-infrastructure/tests/embedding_retrieval_results.rs',
    'crates/vestrace-infrastructure/tests/embedding_runtime_role_refusals.rs',
    'crates/vestrace-infrastructure/tests/embedding_schema_contract.rs',
    'crates/vestrace-infrastructure/tests/embedding_transition_activation.rs',
    'crates/vestrace-infrastructure/tests/embedding_transition_barriers.rs',
    'crates/vestrace-infrastructure/tests/embedding_transition_planning.rs',
    'crates/vestrace-infrastructure/tests/embedding_worker_restart.rs',
    'crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs',
    'crates/vestrace-infrastructure/tests/retrieval_generation_fence.rs',
    'crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs',
    'crates/vestrace-mcp/src/server.rs',
    'crates/vestrace-mcp/tests/embedding_retrieval.rs',
    'docker/postgres/init-runtime-role.sh',
    'docs/development-evidence/v1-g0-04-embedding-transition-foundation.md',
    'docs/development-evidence/v1-g0-04c-preflight.json',
    'docs/superpowers/plans/2026-09-08-vestrace-v1-g0-04-completion.md',
    'docs/superpowers/specs/2026-09-08-vestrace-v1-g0-04-completion-design.md',
    'migrations/0197_embedding_canonical_generations.sql',
    'migrations/0198_embedding_index_builds.sql',
    'migrations/0199_embedding_executor_work.sql',
    'migrations/0200_embedding_transition_execution.sql',
    'migrations/0201_embedding_transition_activation.sql',
    'migrations/0202_embedding_retrieval_results.sql',
    'migrations/0203_embedding_erasure_propagation.sql',
    'migrations/0204_embedding_legacy_adoption.sql',
    'scripts/p04-scope.mjs',
    'tests/embedding_fault_scenario_e2e.rs',
    'tests/p04_scope.test.mjs',
  ]);
  assert.deepEqual(protectedAuthorityPaths, [
    'crates/vestrace-http/src/api/memory.rs',
    'crates/vestrace-http/src/api/retrieval.rs',
    'docs/development-evidence/v1-g0-01-preflight.json',
    'docs/development-evidence/v1-g0-01-protocol-lock.md',
    'docs/development-evidence/v1-g0-02-preflight.json',
    'docs/development-evidence/v1-g0-02-security-material-foundation.md',
    'docs/development-evidence/v1-g0-03-preflight.json',
    'docs/development-evidence/v1-g0-03-provider-execution-foundation.md',
    'docs/external-corpus/vestrace-docss-2026-08-19.manifest.json',
    'docs/superpowers/plans/2026-08-26-vestrace-v1-g0-01-protocol-lock.md',
    'docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md',
    'docs/superpowers/plans/2026-08-27-vestrace-v1-g0-02-security-material-foundation.md',
    'docs/superpowers/plans/2026-08-28-vestrace-v1-g0-03-provider-execution-foundation.md',
    'docs/superpowers/plans/2026-09-03-vestrace-v1-g0-04-embedding-transition-foundation.md',
    'docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md',
    'schemas/openai-compatible/openai-chat-completions-v1-q1.json',
    'schemas/protocol-lock.json',
    'scripts/p03-scope.mjs',
    'scripts/protocol-lock.mjs',
    'scripts/protocol-provenance.mjs',
    'scripts/verify-dirty-baseline.mjs',
    'tests/fixtures/openai-q1/marker.png',
    'tests/p03_scope.test.mjs',
  ]);
});

test('P04 completion preserves all 126 historical migrations through 0196', () => {
  const preflight = JSON.parse(readFileSync('docs/development-evidence/v1-g0-04c-preflight.json', 'utf8'));
  const migrations = readdirSync('migrations').filter((name) => /^\d{4}_.*\.sql$/.test(name) && Number(name.slice(0, 4)) <= 196).sort();
  assert.equal(migrations.length, 126);
  assert.equal(migrations.at(-1), '0196_retired_credential_erasure.sql');
  for (const name of migrations) {
    const path = `migrations/${name}`;
    assert.equal(changeScopePaths.includes(path), false, `historical migration writable: ${path}`);
    const digest = preflight.protected_authority_digests.find((entry) => entry.path === path);
    assert.ok(digest, `missing historical digest: ${path}`);
    const bytes = readFileSync(path);
    assert.equal(bytes.length, digest.bytes, path);
    assert.equal(sha256(bytes), digest.sha256, path);
  }
});
