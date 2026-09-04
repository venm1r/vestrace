import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { generateMarkerPng } from '../scripts/generate-openai-q1-marker.mjs';

const script = resolve('scripts/generate-openai-q1-marker.mjs');
test('q1 marker generation is byte-identical and check is read-only', () => {
  const first = generateMarkerPng();
  assert.deepEqual(generateMarkerPng(), first);
  const root = mkdtempSync(join(tmpdir(), 'vestrace-q1-marker-'));
  try {
    execFileSync('node', [script, '--write', root]);
    const marker = join(root, 'tests/fixtures/openai-q1/marker.png');
    const before = statSync(marker);
    assert.deepEqual(readFileSync(marker), first);
    execFileSync('node', [script, '--check', root]);
    const after = statSync(marker);
    assert.equal(after.mtimeMs, before.mtimeMs);
    assert.throws(() => execFileSync('node', [script, '--bogus', root]));
    assert.throws(() => execFileSync('node', [script, '--check']));
  } finally { rmSync(root, { recursive: true, force: true }); }
});
