import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../src/sdk/client.ts', import.meta.url), 'utf8');

test('provider execution SDK mirrors governed connection and model mutations', () => {
  for (const method of [
    'createConnection',
    'createConnectionRevision',
    'requestConnectionQualification',
    'createConnectionCredential',
    'abandonConnectionCredential',
    'activateConnectionCredential',
    'revokeConnectionCredential',
    'createModelRevision',
    'requestModelQualification',
  ]) {
    assert.match(source, new RegExp(`\\b${method}\\b`), `SDK lacks ${method}`);
  }
  for (const path of [
    '/connections',
    '/revisions',
    '/qualifications',
    '/credentials',
    '/abandon',
    '/activate',
    '/revoke',
    '/models',
  ]) {
    assert.ok(source.includes(path), `SDK lacks provider contract path fragment ${path}`);
  }
});

test('SDK does not retain the legacy provider-create transport method', () => {
  assert.doesNotMatch(source, /createProvider\s*:/);
});

test('SDK list surfaces expose only governed projection interfaces', () => {
  for (const [method, projection] of [
    ['listConnections', 'GovernedConnectionItem'],
    ['listModels', 'GovernedModelItem'],
    ['listProviders', 'GovernedProviderItem'],
  ]) {
    assert.match(
      source,
      new RegExp(`${method}:\\s*\\(\\): Promise<${projection}\\[\\]>`),
      `${method} must return ${projection}[]`,
    );
  }

  const fields = (name) => {
    const match = source.match(new RegExp(`export interface ${name}\\s*\\{([\\s\\S]*?)\\n\\}`));
    assert.ok(match, `SDK lacks ${name}`);
    return [...match[1].matchAll(/^\s*([a-z_]+):/gm)].map((field) => field[1]).sort();
  };

  assert.deepEqual(fields('GovernedConnectionItem'), [
    'blockers',
    'id',
    'qualification_state',
    'revision_id',
    'state',
  ]);
  assert.deepEqual(fields('GovernedModelItem'), [
    'blockers',
    'id',
    'qualification_state',
    'revision_id',
    'state',
  ]);
  assert.deepEqual(fields('GovernedProviderItem'), ['blockers', 'id', 'state']);
  assert.match(source, /createModel:\s*\(payload: CreateModelPayload\): Promise<ModelItem>/);
});
