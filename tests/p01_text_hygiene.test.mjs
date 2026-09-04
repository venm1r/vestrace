import assert from 'node:assert/strict';
import test from 'node:test';
import { CREATED_TEXT_PATHS } from '../scripts/p01-scope.mjs';
import { validateText } from '../scripts/verify-p01-text-hygiene.mjs';

const EXPECTED_CREATED_TEXT_PATHS = [
  'apps/console/tests/agUiProtocolProfile.test.mjs',
  'apps/console/tests/protocolLock.test.mjs',
  'docs/development-evidence/v1-g0-01-preflight.json',
  'docs/development-evidence/v1-g0-01-protocol-lock.md',
  'docs/external-corpus/vestrace-docss-2026-08-19.manifest.json',
  'schemas/a2a/vestrace-v1-profile.json',
  'schemas/ag-ui/0.0.58/runtime-schemas.json',
  'schemas/ag-ui/vestrace-v1-profile.json',
  'schemas/openai-compatible/openai-chat-completions-v1-q1.json',
  'schemas/protocol-lock.json',
  'scripts/external-corpus-manifest.mjs',
  'scripts/extract-ag-ui-runtime-schemas.mjs',
  'scripts/generate-openai-q1-marker.mjs',
  'scripts/p01-scope.mjs',
  'scripts/protocol-lock.mjs',
  'scripts/protocol-provenance.mjs',
  'scripts/verify-dirty-baseline.mjs',
  'scripts/verify-p01-text-hygiene.mjs',
  'scripts/verify-protocol-provenance.mjs',
  'tests/external_corpus_manifest.rs',
  'tests/p01_text_hygiene.test.mjs',
  'tests/protocol_a2a_lock.rs',
  'tests/protocol_q1_manifest.rs',
  'tests/protocol_q1_marker.test.mjs',
];

test('P01 text validator rejects noncanonical text and tracks only declared text paths', () => {
  assert.deepEqual(CREATED_TEXT_PATHS, EXPECTED_CREATED_TEXT_PATHS);
  assert.equal(CREATED_TEXT_PATHS.includes('tests/fixtures/openai-q1/marker.png'), false);
  assert.equal(new Set(CREATED_TEXT_PATHS).size, CREATED_TEXT_PATHS.length);
  for (const path of EXPECTED_CREATED_TEXT_PATHS) assert.equal(CREATED_TEXT_PATHS.filter((entry) => entry === path).length, 1);
  assert.throws(() => validateText(Buffer.from('\ufeff{}\n'), 'fixture'), /BOM/);
  assert.throws(() => validateText(Buffer.from([0xc3, 0x28]), 'fixture'), /invalid UTF-8/);
  assert.throws(() => validateText(Buffer.from('\uFFFD\n'), 'fixture'), /noncanonical text/);
  assert.throws(() => validateText(Buffer.from('{\r\n}\r\n'), 'fixture'), /noncanonical text/);
  assert.throws(() => validateText(Buffer.from('{\r}\n'), 'fixture'), /noncanonical text/);
  assert.throws(() => validateText(Buffer.from('{}\t\n'), 'fixture'), /noncanonical text/);
  assert.throws(() => validateText(Buffer.from('{} \n'), 'fixture'), /noncanonical text/);
  assert.throws(() => validateText(Buffer.from('{}'), 'fixture'), /noncanonical text/);
  assert.throws(() => validateText(Buffer.from('{\n  "z": {\n    "b": 1,\n    "a": 2\n  },\n  "a": 1\n}\n'), 'fixture.json'));
  assert.doesNotThrow(() => validateText(Buffer.from('canonical text\n'), 'fixture'));
  assert.doesNotThrow(() => validateText(Buffer.from('{\n  "a": 1\n}\n'), 'fixture.json'));
});
