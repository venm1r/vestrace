import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { verifyDirtyBaselineInScope } from '../scripts/verify-dirty-baseline.mjs';

import {
  CHANGE_SCOPE_PATHS as p01ChangeScopePaths,
  PROTECTED_AUTHORITY_PATHS as p01ProtectedAuthorityPaths,
} from '../scripts/p01-scope.mjs';
import {
  changeScopePaths as p02ChangeScopePaths,
  protectedAuthorityPaths as p02ProtectedAuthorityPaths,
} from '../scripts/p02-scope.mjs';
import {
  changeScopePaths,
  protectedAuthorityPaths,
} from '../scripts/p03-scope.mjs';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const git = (repoRoot, args) => execFileSync(
  'git',
  ['-c', `safe.directory=${repoRoot}`, '-c', 'core.excludesFile=', ...args],
  { cwd: repoRoot },
);

function verifierFixture({ changePaths, label, preflightPath, protectedPaths, scope }) {
  const repoRoot = mkdtempSync(join(tmpdir(), `vestrace-${label.toLowerCase()}-scope-`));
  const protectedPath = protectedPaths[0];
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
    return {
      bytes: bytes.length,
      path,
      sha256: sha256(bytes),
      status: '??',
    };
  });
  writeFileSync(join(repoRoot, preflightPath), JSON.stringify({
    captured_at_utc: '2026-08-29T00:00:00.000Z',
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

  const args = [
    'scripts/verify-dirty-baseline.mjs',
    '--check',
    repoRoot,
    join(repoRoot, preflightPath),
  ];
  if (scope) args.push('--scope', scope);
  execFileSync(process.execPath, args, {
    cwd: process.cwd(),
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  writeFileSync(join(repoRoot, protectedPath), 'mutated authority\n');
  let failure;
  try {
    execFileSync(process.execPath, args, {
      cwd: process.cwd(),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (error) {
    failure = error;
  }
  assert.ok(failure, `${label} expected verifier rejection`);
  assert.match(failure.stderr, /protected authority changed/);
}

test('legacy P01 verifier default dispatch preserves positive and negative behavior', () => {
  verifierFixture({
    changePaths: p01ChangeScopePaths,
    label: 'P01',
    preflightPath: 'docs/development-evidence/v1-g0-01-preflight.json',
    protectedPaths: p01ProtectedAuthorityPaths,
  });
}, { timeout: 20_000 });

test('legacy P02 verifier explicit dispatch preserves positive and negative behavior', () => {
  verifierFixture({
    changePaths: p02ChangeScopePaths,
    label: 'P02',
    preflightPath: 'docs/development-evidence/v1-g0-02-preflight.json',
    protectedPaths: p02ProtectedAuthorityPaths,
    scope: 'p02-scope.mjs',
  });
}, { timeout: 20_000 });

test('scope dispatch rejects absent, malformed, and traversal scope names before import', () => {
  for (const [scope, message] of [
    ['p99-scope.mjs', 'scope module does not exist: p99-scope.mjs'],
    ['p4-scope.mjs', 'invalid scope module name: p4-scope.mjs'],
    ['../../evil.mjs', 'invalid scope module name: ../../evil.mjs'],
  ]) {
    let failure;
    try {
      execFileSync(process.execPath, [
        'scripts/verify-dirty-baseline.mjs',
        '--check',
        '.',
        'missing-preflight.json',
        '--scope',
        scope,
      ], { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (error) {
      failure = error;
    }
    assert.ok(failure, `${scope} must be rejected`);
    assert.equal(failure.status, 1);
    assert.match(failure.stderr, new RegExp(message.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
});

test('scope dispatch derives the P04 label from a valid future scope module', () => {
  const repoRoot = mkdtempSync(join(tmpdir(), 'vestrace-p04-scope-'));
  const scriptsRoot = join(repoRoot, 'scripts');
  const preflightPath = 'preflight.json';
  mkdirSync(scriptsRoot, { recursive: true });
  writeFileSync(join(repoRoot, 'README.md'), 'fixture\n');
  writeFileSync(join(scriptsRoot, 'verify-dirty-baseline.mjs'), readFileSync(join(process.cwd(), 'scripts/verify-dirty-baseline.mjs')));
  writeFileSync(join(scriptsRoot, 'p01-scope.mjs'), readFileSync(join(process.cwd(), 'scripts/p01-scope.mjs')));
  writeFileSync(join(scriptsRoot, 'p04-scope.mjs'), "export const changeScopePaths = ['preflight.json'];\nexport const protectedAuthorityPaths = [];\n");
  writeFileSync(join(repoRoot, preflightPath), '{}\n');
  git(repoRoot, ['init']);
  git(repoRoot, ['config', 'user.email', 'fixture@example.invalid']);
  git(repoRoot, ['config', 'user.name', 'fixture']);
  git(repoRoot, ['config', 'core.autocrlf', 'false']);
  git(repoRoot, ['add', '.']);
  git(repoRoot, ['commit', '-m', 'fixture']);
  const rawStatus = git(repoRoot, ['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  writeFileSync(join(repoRoot, preflightPath), JSON.stringify({
    captured_at_utc: '2026-08-29T00:00:00.000Z',
    change_scope_paths: ['preflight.json'],
    dirty_files: [],
    head: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    protected_authority_digests: [],
    protected_authority_paths: [],
    schema_version: 1,
    status_porcelain_v1_z_base64: rawStatus.toString('base64'),
    status_porcelain_v1_z_sha256: sha256(rawStatus),
  }, null, 2));
  writeFileSync(join(repoRoot, 'outside.txt'), 'outside scope\n');

  let failure;
  try {
    execFileSync(process.execPath, [
      join(scriptsRoot, 'verify-dirty-baseline.mjs'),
      '--check',
      repoRoot,
      join(repoRoot, preflightPath),
      '--scope',
      'p04-scope.mjs',
    ], { cwd: repoRoot, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  } catch (error) {
    failure = error;
  }
  assert.ok(failure, 'expected an out-of-scope dirty path');
  assert.equal(failure.status, 1);
  assert.match(failure.stderr, /new dirty path outside P04 scope: outside.txt/);
}, { timeout: 20_000 });

test('P03 scope declares unique, sorted, disjoint change and protected paths', () => {
  assert.deepEqual(changeScopePaths, [...changeScopePaths].sort());
  assert.deepEqual(protectedAuthorityPaths, [...protectedAuthorityPaths].sort());
  assert.equal(new Set(changeScopePaths).size, changeScopePaths.length);
  assert.equal(new Set(protectedAuthorityPaths).size, protectedAuthorityPaths.length);
  assert.equal(changeScopePaths.length, 144);
  assert.equal(protectedAuthorityPaths.length, 17);
  for (const path of protectedAuthorityPaths) assert.ok(!changeScopePaths.includes(path));
  for (const path of [
    '.dockerignore',
    'Dockerfile',
    'apps/console/src/routes/ConnectionsPage.tsx',
    'apps/console/src/routes/EvaluationsPage.tsx',
    'apps/console/src/routes/ModelsPage.tsx',
    'crates/vestrace-application/src/ports.rs',
    'crates/vestrace-application/src/run/commands.rs',
    'crates/vestrace-application/src/run/coordinator.rs',
    'crates/vestrace-application/src/run/handlers/advance_run.rs',
    'crates/vestrace-application/tests/model_data_policy.rs',
    'crates/vestrace-application/tests/run_coordinator.rs',
    'crates/vestrace-http/src/api/ag_ui.rs',
    'crates/vestrace-http/src/api/runs.rs',
    'crates/vestrace-http/tests/router_contract.rs',
    'crates/vestrace-http/tests/run_routes.rs',
    'crates/vestrace-infrastructure/src/postgres/material_intent.rs',
    'crates/vestrace-infrastructure/tests/lm_studio_model_data_policy.rs',
    'docs/superpowers/plans/2026-09-01-vestrace-p03-task11-completion.md',
    'docs/superpowers/specs/2026-09-01-vestrace-p03-task11-governed-run-input-design.md',
  ]) assert.ok(changeScopePaths.includes(path), `${path} must be change-authorized`);
  assert.deepEqual(protectedAuthorityPaths, [
    'crates/vestrace-http/src/api/memory.rs',
    'crates/vestrace-http/src/api/retrieval.rs',
    'docs/development-evidence/v1-g0-01-preflight.json',
    'docs/development-evidence/v1-g0-01-protocol-lock.md',
    'docs/development-evidence/v1-g0-02-preflight.json',
    'docs/development-evidence/v1-g0-02-security-material-foundation.md',
    'docs/external-corpus/vestrace-docss-2026-08-19.manifest.json',
    'docs/superpowers/plans/2026-08-26-vestrace-v1-g0-01-protocol-lock.md',
    'docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md',
    'docs/superpowers/plans/2026-08-27-vestrace-v1-g0-02-security-material-foundation.md',
    'docs/superpowers/plans/2026-08-28-vestrace-v1-g0-03-provider-execution-foundation.md',
    'docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md',
    'schemas/openai-compatible/openai-chat-completions-v1-q1.json',
    'schemas/protocol-lock.json',
    'scripts/protocol-lock.mjs',
    'scripts/protocol-provenance.mjs',
    'tests/fixtures/openai-q1/marker.png',
  ]);
});

test('P03 scope rejects a protected authority mutation', () => {
  const repoRoot = mkdtempSync(join(tmpdir(), 'vestrace-p03-scope-'));
  const protectedPath = protectedAuthorityPaths[0];
  const preflightPath = 'docs/development-evidence/v1-g0-03-preflight.json';
  mkdirSync(dirname(join(repoRoot, protectedPath)), { recursive: true });
  mkdirSync(dirname(join(repoRoot, preflightPath)), { recursive: true });
  writeFileSync(join(repoRoot, 'README.md'), 'fixture\n');
  git(repoRoot, ['init']);
  git(repoRoot, ['config', 'user.email', 'fixture@example.invalid']);
  git(repoRoot, ['config', 'user.name', 'fixture']);
  git(repoRoot, ['config', 'core.autocrlf', 'false']);
  git(repoRoot, ['add', 'README.md']);
  git(repoRoot, ['commit', '-m', 'fixture']);
  writeFileSync(join(repoRoot, protectedPath), 'original authority\n');
  const rawStatus = git(repoRoot, ['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  const authorityBytes = readFileSync(join(repoRoot, protectedPath));
  writeFileSync(join(repoRoot, preflightPath), JSON.stringify({
    captured_at_utc: '2026-08-29T00:00:00.000Z',
    change_scope_paths: changeScopePaths,
    dirty_files: [{
      bytes: authorityBytes.length,
      path: protectedPath,
      sha256: sha256(authorityBytes),
      status: '??',
    }],
    head: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    protected_authority_digests: [{
      bytes: authorityBytes.length,
      path: protectedPath,
      sha256: sha256(authorityBytes),
    }],
    protected_authority_paths: protectedAuthorityPaths,
    schema_version: 1,
    status_porcelain_v1_z_base64: rawStatus.toString('base64'),
    status_porcelain_v1_z_sha256: sha256(rawStatus),
  }, null, 2));
  writeFileSync(join(repoRoot, protectedPath), 'mutated authority\n');

  let failure;
  try {
    execFileSync(process.execPath, [
      'scripts/verify-dirty-baseline.mjs',
      '--check',
      repoRoot,
      join(repoRoot, preflightPath),
      '--scope',
      'p03-scope.mjs',
    ], { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  } catch (error) {
    failure = error;
  }
  assert.ok(failure, 'expected verifier rejection');
  assert.match(failure.stderr, /protected authority changed/);
}, { timeout: 20_000 });

test('an authorized protected authority revision is honoured only for its exact recorded digest', () => {
  const repoRoot = mkdtempSync(join(tmpdir(), 'vestrace-p03-revision-'));
  const protectedPath = 'docs/authority.md';
  const dirtyPath = 'src/out-of-scope.rs';
  mkdirSync(dirname(join(repoRoot, protectedPath)), { recursive: true });
  mkdirSync(dirname(join(repoRoot, dirtyPath)), { recursive: true });
  writeFileSync(join(repoRoot, 'README.md'), 'fixture\n');
  git(repoRoot, ['init']);
  git(repoRoot, ['config', 'user.email', 'fixture@example.invalid']);
  git(repoRoot, ['config', 'user.name', 'fixture']);
  git(repoRoot, ['config', 'core.autocrlf', 'false']);
  git(repoRoot, ['add', 'README.md']);
  git(repoRoot, ['commit', '-m', 'fixture']);
  writeFileSync(join(repoRoot, protectedPath), 'original authority\n');
  writeFileSync(join(repoRoot, dirtyPath), 'original dirty\n');
  const rawStatus = git(repoRoot, ['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  const record = (path) => {
    const bytes = readFileSync(join(repoRoot, path));
    return { bytes: bytes.length, path, sha256: sha256(bytes), status: '??' };
  };
  const capturedProtected = record(protectedPath);
  const capturedDirty = record(dirtyPath);
  const preflight = {
    captured_at_utc: '2026-08-29T00:00:00.000Z',
    change_scope_paths: [],
    dirty_files: [capturedDirty, capturedProtected].sort((a, b) => a.path.localeCompare(b.path)),
    head: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    protected_authority_digests: [{
      bytes: capturedProtected.bytes,
      path: protectedPath,
      sha256: capturedProtected.sha256,
    }],
    protected_authority_paths: [protectedPath],
    schema_version: 1,
    status_porcelain_v1_z_base64: rawStatus.toString('base64'),
    status_porcelain_v1_z_sha256: sha256(rawStatus),
  };
  const fixtureScope = { changeScopePaths: [], protectedAuthorityPaths: [protectedPath] };

  // An unrecorded change to either kind of path is refused.
  writeFileSync(join(repoRoot, protectedPath), 'corrected authority\n');
  writeFileSync(join(repoRoot, dirtyPath), 'corrected dirty\n');
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, preflight, fixtureScope), [
    `protected authority changed: ${protectedPath}`,
    `baseline dirty path changed: ${protectedPath}`,
    `baseline dirty path changed: ${dirtyPath}`,
  ]);

  // Recording the exact authorized outcome clears both, and nothing else.
  const revision = (path) => {
    const bytes = readFileSync(join(repoRoot, path));
    const captured = path === protectedPath ? capturedProtected : capturedDirty;
    return {
      authorized_at_utc: '2026-08-30T00:00:00.000Z',
      authorized_by: 'fixture operator',
      bytes: bytes.length,
      captured_bytes: captured.bytes,
      captured_sha256: captured.sha256,
      path,
      reason: 'operator-accepted correction made after this baseline was captured',
      sha256: sha256(bytes),
    };
  };
  const recorded = structuredClone(preflight);
  recorded.protected_authority_revisions = [revision(protectedPath), revision(dirtyPath)];
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, recorded, fixtureScope), []);

  // A revision pins one exact outcome; any further drift re-trips the guard.
  writeFileSync(join(repoRoot, protectedPath), 'drifted again\n');
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, recorded, fixtureScope), [
    `protected authority changed: ${protectedPath}`,
    `baseline dirty path changed: ${protectedPath}`,
  ]);
  writeFileSync(join(repoRoot, protectedPath), 'corrected authority\n');

  // A revision that misstates what it moved from is not a record of anything.
  const wrongOrigin = structuredClone(recorded);
  wrongOrigin.protected_authority_revisions[0].captured_sha256 = sha256(Buffer.from('not the baseline'));
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, wrongOrigin, fixtureScope), [
    `preflight protected_authority_revisions does not match the captured baseline: ${protectedPath}`,
    `protected authority changed: ${protectedPath}`,
    `baseline dirty path changed: ${protectedPath}`,
  ]);

  // Authorization metadata is mandatory.
  const unauthorized = structuredClone(recorded);
  delete unauthorized.protected_authority_revisions[0].authorized_by;
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, unauthorized, fixtureScope), [
    'malformed preflight protected_authority_revisions entry',
    `protected authority changed: ${protectedPath}`,
    `baseline dirty path changed: ${protectedPath}`,
  ]);

  // A duplicate, or an override for a path under no guard, is dead authority.
  const duplicated = structuredClone(recorded);
  duplicated.protected_authority_revisions.push(revision(protectedPath));
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, duplicated, fixtureScope), [
    `duplicate preflight protected_authority_revisions path: ${protectedPath}`,
  ]);

  const unused = structuredClone(recorded);
  unused.protected_authority_revisions.push({
    ...revision(protectedPath),
    path: 'docs/never-guarded.md',
  });
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, unused, fixtureScope), [
    'unused preflight protected_authority_revisions path: docs/never-guarded.md',
  ]);
}, { timeout: 20_000 });
