import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { changeScopePaths, protectedAuthorityPaths } from '../scripts/p02-scope.mjs';
import { verifyDirtyBaselineInScope } from '../scripts/verify-dirty-baseline.mjs';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const git = (repoRoot, args) => execFileSync('git', ['-c', `safe.directory=${repoRoot}`, '-c', 'core.excludesFile=', ...args], { cwd: repoRoot });

test('P02 scope declares disjoint change and protected authority paths', () => {
  assert.ok(Array.isArray(changeScopePaths));
  assert.ok(Array.isArray(protectedAuthorityPaths));
  assert.equal(new Set(changeScopePaths).size, changeScopePaths.length);
  assert.equal(new Set(protectedAuthorityPaths).size, protectedAuthorityPaths.length);
  for (const path of protectedAuthorityPaths) assert.ok(!changeScopePaths.includes(path));
  assert.deepEqual(protectedAuthorityPaths, [
    'docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md',
    'docs/superpowers/plans/2026-08-26-vestrace-v1-g0-01-protocol-lock.md',
    'docs/superpowers/plans/2026-08-27-vestrace-v1-g0-02-security-material-foundation.md',
    'docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md',
  ]);
});

test('P02 scope rejects a protected authority mutation', () => {
  const repoRoot = mkdtempSync(join(tmpdir(), 'vestrace-p02-scope-'));
  const protectedPath = protectedAuthorityPaths[0];
  const preflightPath = 'docs/development-evidence/v1-g0-02-preflight.json';
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
    captured_at_utc: '2026-08-27T00:00:00.000Z',
    change_scope_paths: changeScopePaths,
    dirty_files: [{ bytes: authorityBytes.length, path: protectedPath, sha256: sha256(authorityBytes), status: '??' }],
    head: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    protected_authority_digests: [{ bytes: authorityBytes.length, path: protectedPath, sha256: sha256(authorityBytes) }],
    protected_authority_paths: protectedAuthorityPaths,
    schema_version: 1,
    status_porcelain_v1_z_base64: rawStatus.toString('base64'),
    status_porcelain_v1_z_sha256: sha256(rawStatus),
  }, null, 2));
  writeFileSync(join(repoRoot, protectedPath), 'mutated authority\n');
  let failure;
  try {
    execFileSync(process.execPath, [
      'scripts/verify-dirty-baseline.mjs', '--check', repoRoot, join(repoRoot, preflightPath), '--scope', 'p02-scope.mjs',
    ], { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  } catch (error) {
    failure = error;
  }
  assert.ok(failure, 'expected verifier rejection');
  assert.match(failure.stderr, /protected authority changed/);
}, { timeout: 20_000 });

test('P02 baseline amendments excuse only the authorized absent baseline file', () => {
  const repoRoot = mkdtempSync(join(tmpdir(), 'vestrace-p02-baseline-amendment-'));
  const amendedPath = 'deleted-by-operator.md';
  const unamendedPath = 'other-baseline-file.md';
  writeFileSync(join(repoRoot, 'README.md'), 'fixture\n');
  git(repoRoot, ['init']);
  git(repoRoot, ['config', 'user.email', 'fixture@example.invalid']);
  git(repoRoot, ['config', 'user.name', 'fixture']);
  git(repoRoot, ['config', 'core.autocrlf', 'false']);
  git(repoRoot, ['add', 'README.md']);
  git(repoRoot, ['commit', '-m', 'fixture']);
  writeFileSync(join(repoRoot, amendedPath), 'captured amendment target\n');
  writeFileSync(join(repoRoot, unamendedPath), 'captured unamended target\n');
  const rawStatus = git(repoRoot, ['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  const record = (path) => {
    const bytes = readFileSync(join(repoRoot, path));
    return { bytes: bytes.length, path, sha256: sha256(bytes), status: '??' };
  };
  const amendedRecord = record(amendedPath);
  const preflight = {
    baseline_amendments: [{
      authorized_at_utc: '2026-08-28T00:00:00.000Z',
      authorized_by: 'fixture operator',
      bytes: amendedRecord.bytes,
      path: amendedPath,
      reason: 'operator deliberately deleted this fixture file',
      sha256: amendedRecord.sha256,
    }],
    change_scope_paths: [],
    dirty_files: [amendedRecord, record(unamendedPath)],
    head: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    protected_authority_digests: [],
    protected_authority_paths: [],
    schema_version: 1,
    status_porcelain_v1_z_base64: rawStatus.toString('base64'),
    status_porcelain_v1_z_sha256: sha256(rawStatus),
  };
  const fixtureScope = { changeScopePaths: [], protectedAuthorityPaths: [] };

  rmSync(join(repoRoot, amendedPath));
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, preflight, fixtureScope), []);

  const missingAuthorizer = structuredClone(preflight);
  delete missingAuthorizer.baseline_amendments[0].authorized_by;
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, missingAuthorizer, fixtureScope), [
    'malformed preflight baseline_amendments entry',
    `baseline dirty path is no longer dirty: ${amendedPath}`,
  ]);

  writeFileSync(join(repoRoot, amendedPath), 'replacement bytes\n');
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, preflight, fixtureScope), [
    `baseline dirty path changed: ${amendedPath}`,
  ]);

  rmSync(join(repoRoot, amendedPath));
  rmSync(join(repoRoot, unamendedPath));
  assert.deepEqual(verifyDirtyBaselineInScope(repoRoot, preflight, fixtureScope), [
    `baseline dirty path is no longer dirty: ${unamendedPath}`,
  ]);
}, { timeout: 20_000 });
