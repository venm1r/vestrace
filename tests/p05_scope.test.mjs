import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import {
  changeScopePaths as p04ChangeScopePaths,
  protectedAuthorityPaths as p04ProtectedAuthorityPaths,
} from '../scripts/p04-scope.mjs';
import { changeScopePaths, protectedAuthorityPaths } from '../scripts/p05-scope.mjs';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function git(cwd, args) {
  return execFileSync('git', args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
}

function verifierFixture({ changePaths, label, mutate, preflightPath, protectedPaths, scope }) {
  const repoRoot = mkdtempSync(join(tmpdir(), `vestrace-${label.toLowerCase()}-scope-`));
  const protectedPath = mutate ?? protectedPaths[0];
  assert.ok(protectedPaths.includes(protectedPath));
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
  const records = protectedPaths.map((path) => {
    const bytes = readFileSync(join(repoRoot, path));
    return { bytes: bytes.length, path, sha256: sha256(bytes), status: '??' };
  });
  writeFileSync(join(repoRoot, preflightPath), JSON.stringify({
    captured_at_utc: '2026-09-12T00:00:00.000Z',
    change_scope_paths: changePaths,
    dirty_files: records,
    head: git(repoRoot, ['rev-parse', 'HEAD']).toString('utf8').trim(),
    protected_authority_digests: records.map(({ bytes, path, sha256: digest }) => ({ bytes, path, sha256: digest })),
    protected_authority_paths: protectedPaths,
    schema_version: 1,
    status_porcelain_v1_z_base64: rawStatus.toString('base64'),
    status_porcelain_v1_z_sha256: sha256(rawStatus),
  }, null, 2));

  const args = ['scripts/verify-dirty-baseline.mjs', '--check', repoRoot, join(repoRoot, preflightPath), '--scope', scope];
  execFileSync(process.execPath, args, { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  writeFileSync(join(repoRoot, protectedPath), 'mutated authority\n');
  assert.throws(
    () => execFileSync(process.execPath, args, { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }),
    (error) => /protected authority changed/.test(error.stderr) && error.stderr.includes(protectedPath),
  );
}

test('P05 dispatch verifies its own scope and rejects a P04 authority mutation', () => {
  verifierFixture({
    changePaths: changeScopePaths,
    label: 'P05',
    mutate: 'scripts/p04-scope.mjs',
    preflightPath: 'docs/development-evidence/v1-g0-05-preflight.json',
    protectedPaths: protectedAuthorityPaths,
    scope: 'p05-scope.mjs',
  });
}, { timeout: 30_000 });

test('P04 dispatch remains unchanged when P05 arrives', () => {
  verifierFixture({
    changePaths: p04ChangeScopePaths,
    label: 'P04',
    preflightPath: 'docs/development-evidence/v1-g0-04c-preflight.json',
    protectedPaths: p04ProtectedAuthorityPaths,
    scope: 'p04-scope.mjs',
  });
}, { timeout: 20_000 });

test('P05 scope is sorted, minimal, and disjoint from protected authority', () => {
  assert.deepEqual(changeScopePaths, [...changeScopePaths].sort());
  assert.deepEqual(protectedAuthorityPaths, [...protectedAuthorityPaths].sort());
  assert.equal(changeScopePaths.length, 185);
  assert.equal(new Set(changeScopePaths).size, changeScopePaths.length);
  assert.equal(new Set(protectedAuthorityPaths).size, protectedAuthorityPaths.length);
  for (const path of protectedAuthorityPaths) assert.equal(changeScopePaths.includes(path), false, path);
});

test('P05 protects root planning authority, P04 completion authority, and migrations through 0208', () => {
  for (const path of [
    'PLAN.md',
    'scripts/verify-dirty-baseline.mjs',
    'scripts/p04-scope.mjs',
    'tests/p04_scope.test.mjs',
    'docs/development-evidence/v1-g0-04c-preflight.json',
    'docs/development-evidence/v1-g0-04-embedding-transition-foundation.md',
    'docs/superpowers/plans/2026-09-08-vestrace-v1-g0-04-completion.md',
    'docs/superpowers/specs/2026-09-08-vestrace-v1-g0-04-completion-design.md',
    'migrations/0208_embedding_memory_references.sql',
  ]) assert.ok(protectedAuthorityPaths.includes(path), path);

  const migrations = readdirSync('migrations')
    .filter((name) => /^[0-9]{4}_.*[.]sql$/.test(name) && Number(name.slice(0, 4)) <= 208)
    .sort();
  assert.equal(migrations.length, 138);
  for (const name of migrations) assert.ok(protectedAuthorityPaths.includes(`migrations/${name}`), name);
});

test('P05-B admits its enumerated archive and base-capture paths and freezes the P05-A migration', () => {
  const archivePaths = [
    'crates/vestrace-application/src/backup_archive.rs',
    'crates/vestrace-cli/tests/safety_archive_host_custody.rs',
    'crates/vestrace-cli/tests/safety_archive_recovery.rs',
    'crates/vestrace-domain/src/backup_archive.rs',
    'crates/vestrace-domain/tests/backup_archive_contract.rs',
    'crates/vestrace-infrastructure/src/backup_archive/file_store.rs',
    'crates/vestrace-infrastructure/src/backup_archive/key_custody.rs',
    'crates/vestrace-infrastructure/src/backup_archive/mod.rs',
    'crates/vestrace-infrastructure/src/postgres/backup_archive_repository.rs',
    'crates/vestrace-infrastructure/tests/backup_archive_authority.rs',
    'docs/superpowers/plans/2026-09-13-vestrace-v1-g0-05b-archive-retention.md',
    'docs/superpowers/plans/2026-09-13-vestrace-v1-g0-05b-base-backup-completion.md',
    'migrations/0210_managed_backup_archive_retention.sql',
    'migrations/0211_managed_backup_base_capture.sql',
  ];

  assert.deepEqual(
    changeScopePaths.filter((path) => archivePaths.includes(path)),
    archivePaths,
  );
  assert.deepEqual(
    changeScopePaths.filter((path) => [
      'migrations/0210_managed_backup_archive_retention.sql',
      'migrations/0211_managed_backup_base_capture.sql',
    ].includes(path)),
    [
      'migrations/0210_managed_backup_archive_retention.sql',
      'migrations/0211_managed_backup_base_capture.sql',
    ],
  );
  assert.equal(changeScopePaths.includes('migrations/0209_installation_safety_authority.sql'), false);
  assert.ok(protectedAuthorityPaths.includes('migrations/0209_installation_safety_authority.sql'));
});

test('P05-C admits only the reviewed restore/cutover implementation increment', () => {
  const restorePaths = [
    'crates/vestrace-application/src/restore_cutover.rs',
    'crates/vestrace-cli/tests/safety_restore_host_custody.rs',
    'crates/vestrace-cli/tests/safety_restore_recovery.rs',
    'crates/vestrace-domain/src/restore_cutover.rs',
    'crates/vestrace-domain/tests/restore_cutover_contract.rs',
    'crates/vestrace-infrastructure/src/postgres/restore_cutover_repository.rs',
    'crates/vestrace-infrastructure/src/restore_target/file_target.rs',
    'crates/vestrace-infrastructure/src/restore_target/mod.rs',
    'crates/vestrace-infrastructure/tests/restore_cutover_authority.rs',
    'migrations/0212_managed_restore_cutover.sql',
    'migrations/0213_managed_restore_refusal.sql',
    'migrations/0214_managed_restore_safety_events.sql',
  ];
  assert.deepEqual(
    changeScopePaths.filter((path) => restorePaths.includes(path)),
    restorePaths,
  );
  assert.deepEqual(
    changeScopePaths.filter((path) => path.startsWith('migrations/')),
    [
      'migrations/0210_managed_backup_archive_retention.sql',
      'migrations/0211_managed_backup_base_capture.sql',
      'migrations/0212_managed_restore_cutover.sql',
      'migrations/0213_managed_restore_refusal.sql',
      'migrations/0214_managed_restore_safety_events.sql',
      'migrations/0215_managed_safety_readiness.sql',
      'migrations/0216_installation_drain_request.sql',
    ],
  );
});

test('P05-D admits only the reviewed Compose and G0-evidence plan boundary', () => {
  const p05dPaths = [
    'crates/vestrace-cli/tests/safety_supervisor_readiness.rs',
    // The G0 collector reads a declarative manifest and the digest-bound
    // artifacts it names. Prose is never proof, so the captured command
    // outputs are themselves reviewed paths.
    'docs/development-evidence/v1-g0-05-gate.json',
    'docs/development-evidence/v1-g0-05-gate/compose-config.txt',
    'docs/development-evidence/v1-g0-05-gate/compose-test.txt',
    'docs/development-evidence/v1-g0-05-gate/dirty-baseline.txt',
    'docs/development-evidence/v1-g0-05-gate/migration-ledger.txt',
    'docs/development-evidence/v1-g0-05-gate/readiness-grants.txt',
    'docs/development-evidence/v1-g0-05-gate/readiness-test.txt',
    'docs/development-evidence/v1-g0-05-gate/scope-test.txt',
    'docs/superpowers/plans/2026-09-14-vestrace-v1-g0-05d-compose-g0-evidence.md',
    'migrations/0215_managed_safety_readiness.sql',
    'scripts/p05-g0-gate.mjs',
    'tests/p05_g0_gate.test.mjs',
  ];
  assert.deepEqual(changeScopePaths.filter((path) => p05dPaths.includes(path)), p05dPaths);
});
// The dirty-baseline verifier compares the captured scope entry to the scope
// module element by element, not as a set. Two lists holding the same paths in
// different collations are still a finding, so the capture has to be the module
// verbatim.
test('P05-E admits its own reviewed spec and plan', () => {
  // P05-E is a sub-package of P05: the verifier hard-codes ^p(\d{2})-scope\.mjs$,
  // so there is one scope module per gate number and P05-A through P05-E all
  // share this one. Only the spec and plan are admitted here; the
  // implementation paths are admitted by the plan's own first task.
  for (const path of [
    'docs/superpowers/plans/2026-09-14-vestrace-v1-g0-05e-test-migrator.md',
    'docs/superpowers/specs/2026-09-14-vestrace-v1-g0-05e-test-migrator-design.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});

test('the G0 closure roadmap is admitted and not protected', () => {
  const path = 'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md';
  assert.ok(changeScopePaths.includes(path), path);
  assert.ok(!protectedAuthorityPaths.includes(path), path);
});

test('the captured preflight scope entry is the scope module verbatim', () => {
  const preflight = JSON.parse(
    readFileSync(new URL('../docs/development-evidence/v1-g0-05-preflight.json', import.meta.url)),
  );
  assert.deepEqual(preflight.change_scope_paths, changeScopePaths);
  assert.deepEqual(preflight.protected_authority_paths, protectedAuthorityPaths);
});

// P05-B gave `vestrace-application` a `ring` dev-dependency for the archive
// signing tests in `backup_archive.rs` and never admitted the manifest that
// declares it. The edit is a real P05 implementation path; leaving it out does
// not make it disappear, it only makes the baseline report it as an unexplained
// dirty file.
test('the manifest carrying the P05 archive test dependency is admitted', () => {
  assert.ok(changeScopePaths.includes('crates/vestrace-application/Cargo.toml'));
  assert.ok(!protectedAuthorityPaths.includes('crates/vestrace-application/Cargo.toml'));
});

test('P05-E admits its implementation and evidence paths', () => {
  for (const path of [
    'crates/vestrace-infrastructure/tests/common/mod.rs',
    'crates/vestrace-infrastructure/tests/deployment_qualification.rs',
    'crates/vestrace-infrastructure/tests/embedding_result_finalization.rs',
    'crates/vestrace-infrastructure/tests/embedding_result_preparation.rs',
    'crates/vestrace-infrastructure/tests/embedding_transition_activation.rs',
    'docs/development-evidence/v1-g0-05e-test-migrator.md',
    'tests/p05e_test_migrator.test.mjs',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
  // Every test file whose attribute this package rewrites must be admitted.
  const rewritten = changeScopePaths.filter(
    (path) => /^crates\/vestrace-(infrastructure|cli)\/tests\/.*\.rs$/.test(path),
  );
  assert.ok(rewritten.length >= 60, `only ${rewritten.length} test files admitted`);
});

test('P05-F admits its own plan and evidence paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05f-fresh-closure.md',
    'docs/development-evidence/v1-g0-05-gate/route-authority-test.txt',
    'docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt',
    'docs/development-evidence/v1-g0-05-gate/material-intent-test.txt',
    'docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt',
    'docs/development-evidence/v1-g0-05f-fresh-closure.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});

test('P05-G admits its own plan and evidence paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05g-fresh-closure.md',
    'docs/development-evidence/v1-g0-05-gate/effect-authority-test.txt',
    'docs/development-evidence/v1-g0-05-gate/model-request-evidence-test.txt',
    'docs/development-evidence/v1-g0-05-gate/transport-security-test.txt',
    'docs/development-evidence/v1-g0-05-gate/connection-credential-guards-test.txt',
    'docs/development-evidence/v1-g0-05-gate/admission-leases-test.txt',
    'docs/development-evidence/v1-g0-05-gate/credential-intent-lifecycle-test.txt',
    'docs/development-evidence/v1-g0-05-gate/candidate-abandon-test.txt',
    'docs/development-evidence/v1-g0-05g-fresh-closure.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});

test('P05-H admits its own plan and evidence paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05h-fresh-closure.md',
    'docs/development-evidence/v1-g0-05-gate/embedding-transition-test.txt',
    'docs/development-evidence/v1-g0-05-gate/retrieval-retry-test.txt',
    'docs/development-evidence/v1-g0-05-gate/material-lifecycle-test.txt',
    'docs/development-evidence/v1-g0-05h-fresh-closure.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});

test('P05-I admits its design spec, ahead of its implementation plan', () => {
  const path = 'docs/superpowers/specs/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit-design.md';
  assert.ok(changeScopePaths.includes(path), path);
  assert.ok(!protectedAuthorityPaths.includes(path), path);
});

test('P05-I admits its implementation plan', () => {
  const path = 'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit.md';
  assert.ok(changeScopePaths.includes(path), path);
  assert.ok(!protectedAuthorityPaths.includes(path), path);
});

test('P05-I admits its own plan and implementation paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit.md',
    'migrations/0216_installation_drain_request.sql',
    'crates/vestrace-domain/src/installation_drain.rs',
    'crates/vestrace-application/src/installation_drain.rs',
    'crates/vestrace-infrastructure/src/postgres/installation_drain.rs',
    'crates/vestrace-infrastructure/tests/installation_drain_request.rs',
    'docs/development-evidence/v1-g0-05i-drain-mutation-permit.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});
