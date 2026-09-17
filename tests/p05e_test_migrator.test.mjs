import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

// The filesystem, not `git ls-files`. This package is developed against a
// preserved dirty baseline and pushed through a temporary index, so a file this
// package creates stays untracked in the working tree indefinitely -- and an
// untracked file is exactly the new suite this guard exists to catch.
function testFiles() {
  const roots = [join(repoRoot, 'tests')];
  for (const crate of readdirSync(join(repoRoot, 'crates'), { withFileTypes: true })) {
    if (crate.isDirectory()) roots.push(join(repoRoot, 'crates', crate.name, 'tests'));
  }
  const found = [];
  for (const root of roots) {
    let entries;
    try {
      entries = readdirSync(root, { withFileTypes: true, recursive: true });
    } catch {
      continue; // a crate with no tests directory
    }
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.rs') || entry.name === 'mod.rs') continue;
      const dir = entry.parentPath ?? entry.path;
      found.push(relative(repoRoot, join(dir, entry.name)).split('\\').join('/'));
    }
  }
  return found.sort();
}

// Migrations 0209 and later assert a provisioning step that #[sqlx::test]
// cannot perform, because it builds each database fresh from template1. A
// suite pointed at the raw directory fails at 0209 before its first assertion
// -- which is how the whole database-backed net stayed down for three packages
// without anyone noticing.
test('no test applies the raw migrations directory', () => {
  const offenders = testFiles().filter((file) =>
    readFileSync(join(repoRoot, file), 'utf8').includes('migrations = "../../migrations"'),
  );
  assert.deepEqual(offenders, [], `these suites would fail at migration 0209: ${offenders}`);
});

test('the bounded migrator is named by its exported path', () => {
  const users = testFiles().filter((file) =>
    readFileSync(join(repoRoot, file), 'utf8').includes('migrator = '),
  );
  assert.ok(users.length >= 60, `only ${users.length} suites use the bounded migrator`);
  for (const file of users) {
    assert.match(
      readFileSync(join(repoRoot, file), 'utf8'),
      // P05-I added DRAIN_HISTORICAL_MIGRATOR, bounded at 216 excluding the
      // P05 assertion migrations, alongside the original HISTORICAL_MIGRATOR
      // (bounded at 208) -- both are legitimately exported by
      // vestrace_infrastructure. A file may use either, or (like
      // intent_crash_boundaries.rs) both across different tests.
      /migrator = "vestrace_infrastructure::(HISTORICAL_MIGRATOR|DRAIN_HISTORICAL_MIGRATOR)"/,
      `${file} names a migrator this package does not export`,
    );
  }
});

