import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { PINNED_PROTOCOL_PROVENANCE } from './protocol-provenance.mjs';

const ARTIFACTS = ['schemas/a2a/vestrace-v1-profile.json', 'schemas/ag-ui/0.0.58/runtime-schemas.json', 'schemas/ag-ui/vestrace-v1-profile.json', 'schemas/openai-compatible/openai-chat-completions-v1-q1.json', 'tests/fixtures/openai-q1/marker.png'];
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const canon = (value) => Array.isArray(value) ? value.map(canon) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canon(value[key])])) : value;
export const serializeProtocolLock = (lock) => {
  const text = JSON.stringify(canon(lock), null, 2);
  if (typeof text !== 'string') throw new Error('protocol lock cannot be canonically serialized');
  return text + '\n';
};
const file = (root, path) => readFileSync(resolve(root, path));
const digestFile = (root, path) => sha(file(root, path));

export function parseCargoLockPackages(text) {
  const chunks = text.split(/^\[\[package\]\]\r?\n/m).slice(1);
  if (!chunks.length) throw new Error('Cargo.lock has no package blocks');
  return chunks.map((chunk) => {
    const fields = {};
    for (const line of chunk.split(/\r?\n/)) {
      const match = /^(name|version|source|checksum) = "([^"]+)"$/.exec(line);
      if (!match) continue;
      if (fields[match[1]] !== undefined) throw new Error('Cargo.lock duplicate package field: ' + match[1]);
      fields[match[1]] = match[2];
    }
    return fields;
  });
}

function exactCargoPackage(packages, name, expected) {
  const matches = packages.filter((entry) => entry.name === name);
  if (matches.length !== 1) throw new Error('Cargo.lock requires exactly one package tuple: ' + name);
  const actual = matches[0];
  for (const key of ['name', 'version', 'source', 'checksum']) {
    const required = key === 'name' ? name : expected[key];
    if (actual[key] !== required) throw new Error('Cargo.lock package tuple mismatch: ' + name + ' ' + key);
  }
}

function exactJsonDependency(packageJson, section, name, version) {
  if (packageJson[section]?.[name] !== version) throw new Error('package.json direct pin mismatch: ' + name);
}

function exactWorkspaceAlias(cargoToml, alias, expected) {
  const lines = cargoToml.split(/\r?\n/).filter((line) => line === alias + ' = { package = "' + expected.package + '", version = "' + expected.version + '" }');
  if (lines.length !== 1) throw new Error('Cargo.toml workspace alias mismatch: ' + alias);
}

function hasDependency(cargoToml, dependency) {
  return cargoToml.split(/\r?\n/).includes(dependency + '.workspace = true');
}

export function validateDirectPins(repoRoot) {
  const packageJson = JSON.parse(file(repoRoot, 'apps/console/package.json'));
  for (const [name, expected] of Object.entries(PINNED_PROTOCOL_PROVENANCE.npm)) exactJsonDependency(packageJson, 'dependencies', name, expected.version);
  for (const [name, expected] of Object.entries(PINNED_PROTOCOL_PROVENANCE.tooling)) exactJsonDependency(packageJson, 'devDependencies', name, expected.version);
  const cargoToml = file(repoRoot, 'Cargo.toml').toString('utf8');
  for (const [alias, expected] of Object.entries(PINNED_PROTOCOL_PROVENANCE.workspace_aliases)) exactWorkspaceAlias(cargoToml, alias, expected);
  const http = file(repoRoot, 'crates/vestrace-http/Cargo.toml').toString('utf8');
  const infrastructure = file(repoRoot, 'crates/vestrace-infrastructure/Cargo.toml').toString('utf8');
  if (!hasDependency(http, 'a2a') || !hasDependency(http, 'a2a-server') || hasDependency(http, 'a2a-client')) throw new Error('vestrace-http A2A boundary ownership mismatch');
  if (!hasDependency(infrastructure, 'a2a') || !hasDependency(infrastructure, 'a2a-client') || hasDependency(infrastructure, 'a2a-server')) throw new Error('vestrace-infrastructure A2A boundary ownership mismatch');
  return true;
}

function runOfflineCheck(repoRoot, script) {
  const result = spawnSync(process.execPath, [resolve(repoRoot, script), '--check', repoRoot], { cwd: repoRoot, encoding: 'utf8' });
  if (result.status !== 0) throw new Error('offline generated artifact stale: ' + script + (result.stderr ? ': ' + result.stderr.trim() : ''));
}

function validateInputs(repoRoot, verifyGenerated) {
  const provenance = PINNED_PROTOCOL_PROVENANCE;
  if (digestFile(repoRoot, provenance.frozen_spec.path) !== provenance.frozen_spec.sha256) throw new Error('frozen design spec digest changed');
  const external = JSON.parse(file(repoRoot, provenance.external_corpus.manifest_path));
  if (external.aggregate_sha256 !== provenance.external_corpus.aggregate_sha256 || digestFile(repoRoot, provenance.external_corpus.manifest_path) !== provenance.external_corpus.manifest_sha256) throw new Error('external corpus manifest or aggregate mismatch');
  validateDirectPins(repoRoot);
  for (const path of ARTIFACTS) if (!existsSync(resolve(repoRoot, path))) throw new Error('protocol artifact missing: ' + path);
  if (verifyGenerated) {
    runOfflineCheck(repoRoot, 'scripts/extract-ag-ui-runtime-schemas.mjs');
    runOfflineCheck(repoRoot, 'scripts/generate-openai-q1-marker.mjs');
  }
}

export function buildProtocolLock(repoRoot, { verifyGenerated = true } = {}) {
  validateInputs(repoRoot, verifyGenerated);
  const provenance = PINNED_PROTOCOL_PROVENANCE;
  const packageLock = JSON.parse(file(repoRoot, 'apps/console/package-lock.json'));
  const npm = {};
  for (const [name, expected] of Object.entries(provenance.npm)) {
    const entry = packageLock.packages['node_modules/' + name];
    if (!entry || entry.version !== expected.version || entry.integrity !== expected.integrity) throw new Error('npm lock pin mismatch: ' + name);
    npm[name] = expected;
  }
  const cargo = parseCargoLockPackages(file(repoRoot, 'Cargo.lock').toString('utf8'));
  const crates = {};
  for (const [name, expected] of Object.entries(provenance.crates)) {
    exactCargoPackage(cargo, name, expected);
    crates[name] = expected;
  }
  const a2aProfile = JSON.parse(file(repoRoot, 'schemas/a2a/vestrace-v1-profile.json'));
  if (a2aProfile.specification?.commit !== provenance.a2a.commit || a2aProfile.specification?.release !== provenance.a2a.release || a2aProfile.specification?.release_url !== provenance.a2a.release_url || a2aProfile.specification?.repository !== provenance.a2a.repository || a2aProfile.wire?.header !== provenance.a2a.wire_header || a2aProfile.wire?.version !== provenance.a2a.wire_version) throw new Error('A2A profile provenance mismatch');
  return {
    a2a: { crates, specification: provenance.a2a },
    artifacts: Object.fromEntries(ARTIFACTS.slice().sort().map((path) => [path, digestFile(repoRoot, path)])),
    external_corpus: provenance.external_corpus,
    frozen_spec_sha256: provenance.frozen_spec.sha256,
    npm,
    schema_version: 1,
    verification_transport: { crate_user_agent: provenance.online_crate_user_agent },
  };
}

export function verifyProtocolLock(repoRoot, lockPath = 'schemas/protocol-lock.json', options = {}) {
  const path = resolve(repoRoot, lockPath);
  if (!existsSync(path)) return ['protocol lock is missing'];
  try {
    const text = readFileSync(path, 'utf8');
    if (text.includes('\r') || !text.endsWith('\n')) return ['protocol lock text is noncanonical'];
    return text === serializeProtocolLock(buildProtocolLock(repoRoot, options)) ? [] : ['protocol lock differs from reconstruction'];
  }
  catch (error) { return [error.message]; }
}

function main(argv) {
  if (argv.length !== 2 || !['--write', '--check'].includes(argv[0])) throw new Error('usage: node scripts/protocol-lock.mjs --write|--check <repo-root>');
  const root = resolve(argv[1]);
  if (argv[0] === '--write') { const path = resolve(root, 'schemas/protocol-lock.json'); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, serializeProtocolLock(buildProtocolLock(root)), 'utf8'); }
  else { const findings = verifyProtocolLock(root); if (findings.length) throw new Error(findings.join('\n')); }
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) { try { main(process.argv.slice(2)); } catch (error) { console.error(error.message); process.exitCode = 1; } }
