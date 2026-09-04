import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
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
    preflightPath: 'docs/development-evidence/v1-g0-04-preflight.json',
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
    preflightPath: 'docs/development-evidence/v1-g0-04-preflight.json',
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
  // 52 frozen by Task 1, plus six recorded amendments: id.rs (1), the
  // cross-package coupling Task 3 exposed (32), the admission publisher (4),
  // the two files that publisher's route mechanically requires (2), the one
  // file the shared dispatch trait can be implemented in (1), and the shared
  // test fixture two suites would otherwise duplicate (1). A literal is the
  // point: scope that grows without an amendment fails here.
  assert.equal(changeScopePaths.length, 94);
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

test('P04 declares every path its plan names as a task file', () => {
  for (const path of [
    'crates/vestrace-application/src/embedding/barrier.rs',
    'crates/vestrace-application/src/embedding/carry.rs',
    'crates/vestrace-application/src/embedding/transition.rs',
    'crates/vestrace-application/src/retrieval/ports.rs',
    'crates/vestrace-domain/src/embedding/space.rs',
    'crates/vestrace-infrastructure/tests/embedding_carry_classification.rs',
    'crates/vestrace-infrastructure/tests/embedding_transition_barriers.rs',
    'crates/vestrace-infrastructure/tests/retrieval_generation_fence.rs',
    'migrations/0187_embedding_jobs_and_corpus_generations.sql',
    'migrations/0190_embedding_transition_barriers.sql',
    'tests/embedding_fault_scenario_e2e.rs',
  ]) {
    assert.ok(changeScopePaths.includes(path), `expected in change scope: ${path}`);
  }
});
