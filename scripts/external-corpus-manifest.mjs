import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const MANIFEST_PATH = 'docs/external-corpus/vestrace-docss-2026-08-19.manifest.json';
const PROVENANCE = 'operator-supplied snapshot at E:\\Junk\\AI\\vestrace-docss, registered from the approved spec on 2026-08-26';
const AGGREGATE_SHA256 = '762c25f3781f1ba30783e218db64aa4dc9536add490f61255056bfe8635d8594';
const BORROWED = ['canonical truth remains distinct from projections', 'single-writer/CAS/lease/fencing may preserve declared ownership'];
const REJECTED = ['second durable runtime', 'message-bus or graph truth', 'cloud/microservice requirement', 'owner assertion as release evidence'];

const SPECS = [
  ['AMENDMENT-2026-08-19.md', 2987, '798dea48d277c545050e9637adc568a7a6c47aeff836749dfcdbc7c86b298df6', 'proposed post-v1 contract', 'defer_post_v1', 'post-v1 architecture-contract reconciliation is required'],
  ['RFC-Model-Request-Reconstruction-and-Context-Surface-2026-08-19.md', 23397, 'f90163409eae865c818e572936f936215f955537e56f5d4a5b4b440bdd4ef423', 'proposed post-v1 contract', 'defer_post_v1', 'post-v1 architecture-contract reconciliation is required'],
  ['Vestrace-post-v1-gate-roadmap-2026-08-12.md', 62896, 'b7a5b959ad1f055dea52b00a1dd9326a69d401199ced3c2a4d919188e018fdeb', 'strategic post-v1 roadmap', 'defer_post_v1', 'its v1/TRUSTED-complete premise is unverified and supplies no evidence'],
  ['Vestrace-post-v1-gate-roadmap-artifact-2026-08-12.json', 87553, 'e430fa9c20519d43398f2504561d19a8b6fccfb3b10b595543b1658b86f12b0b', 'derived roadmap data companion', 'defer_post_v1'],
  ['Vestrace-post-v1-gate-roadmap-report-2026-08-12.html', 517604, 'd48231290e0c6ffbb7c81e6e2af283af616e5ee4d68ab2e96bfc9de7876cdf99', 'derived roadmap render companion', 'defer_post_v1'],
  ['Vestrace-post-v1-gate-roadmap-source-notes-2026-08-12.md', 2533, '6e6622c914c9ecd20fbea3cd7d8d9b066934e74a1b10faac7c59c140928c5c3c', 'roadmap source notes with missing-reference warning', 'defer_post_v1'],
  ['Vestrace-prospective-technologies-artifact-2026-08-12.json', 90605, '62550dfc4e41a371b626166803a6d427a5d2a4330f645dfc2e69c261c25e070d', 'derived research data companion', 'defer_post_v1'],
  ['Vestrace-prospective-technologies-report-2026-08-12.html', 624448, 'a9f2f93c9846654f8e0029c03880b26e248ce7e5eaa7943c063521e7a837f835', 'derived research render companion', 'defer_post_v1'],
  ['Vestrace-prospective-technologies-research-2026-08-12.md', 66180, 'ca4c9e0f8dea834d4b5d430cac7fb77d1b17c22003c58254aeaabaf94bd103ff', 'non-normative donor research', 'compatibility_seam'],
  ['Vestrace-reference-systems-artifact-2026-08-11.json', 72429, '0aa249d87edb176e8fba43a86933f438995259139c6a9017084fd2670db18de2', 'derived research data companion', 'defer_post_v1'],
  ['Vestrace-reference-systems-report-2026-08-11.html', 485361, 'f1a58da57c079703f9c478e448682b73cd0cc53de93265e80170dc3d84a01ed9', 'derived research render companion', 'defer_post_v1'],
  ['Vestrace-reference-systems-research-2026-08-11.md', 55105, 'db2b404618a08720b4a8a7b89bc07998e72ddea4ea2119d23795949c86481c9f', 'non-normative donor research', 'compatibility_seam'],
];
const DERIVED_FROM = {
  'Vestrace-post-v1-gate-roadmap-artifact-2026-08-12.json': 'Vestrace-post-v1-gate-roadmap-2026-08-12.md',
  'Vestrace-post-v1-gate-roadmap-report-2026-08-12.html': 'Vestrace-post-v1-gate-roadmap-2026-08-12.md',
  'Vestrace-prospective-technologies-artifact-2026-08-12.json': 'Vestrace-prospective-technologies-research-2026-08-12.md',
  'Vestrace-prospective-technologies-report-2026-08-12.html': 'Vestrace-prospective-technologies-research-2026-08-12.md',
  'Vestrace-reference-systems-artifact-2026-08-11.json': 'Vestrace-reference-systems-research-2026-08-11.md',
  'Vestrace-reference-systems-report-2026-08-11.html': 'Vestrace-reference-systems-research-2026-08-11.md',
};

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const canonicalize = (value) => Array.isArray(value) ? value.map(canonicalize) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonicalize(value[key])])) : value;
export const serializeCanonicalJson = (value) => `${JSON.stringify(canonicalize(value), null, 2)}\n`;

function entryFromSpec(sourceRoot, [name, expectedBytes, expectedSha, role, decision, decisionNote]) {
  const bytes = readFileSync(resolve(sourceRoot, name));
  const actualSha = sha256(bytes);
  if (bytes.length !== expectedBytes || actualSha !== expectedSha) throw new Error(`source digest mismatch: ${name}`);
  const entry = { bytes: bytes.length, decision, name, provenance: PROVENANCE, role, sha256: actualSha };
  if (decisionNote) entry.decision_note = decisionNote;
  if (DERIVED_FROM[name]) Object.assign(entry, { derived_from: DERIVED_FROM[name], independent_authority: false });
  if (decision === 'compatibility_seam') Object.assign(entry, { borrowed_invariants: BORROWED, rejected_patterns: REJECTED });
  return entry;
}

export function buildExternalCorpusManifest(sourceRoot) {
  const names = readdirSync(sourceRoot, { withFileTypes: true }).filter((item) => item.isFile()).map((item) => item.name).sort();
  const expectedNames = SPECS.map(([name]) => name);
  if (JSON.stringify(names) !== JSON.stringify(expectedNames)) throw new Error('external corpus must contain exactly the approved twelve files');
  const entries = SPECS.map((spec) => entryFromSpec(sourceRoot, spec));
  const aggregate = sha256(Buffer.from(entries.map((entry) => `${entry.name}\t${entry.bytes}\t${entry.sha256}\n`).join(''), 'utf8'));
  if (aggregate !== AGGREGATE_SHA256) throw new Error('external corpus aggregate digest mismatch');
  return {
    aggregate_sha256: aggregate,
    entries,
    missing_references: [
      { contents_inferred: false, name: 'vestrace-brain-face-organ-system-model.md', present: false },
      { contents_inferred: false, name: 'ADR-0011 — Brain–Face–Organ System Decomposition', present: false },
    ],
    schema_version: 1,
    source_path_is_runtime_dependency: false,
  };
}

export function validateExternalCorpusManifest(manifest) {
  if (manifest.schema_version !== 1 || manifest.source_path_is_runtime_dependency !== false || !Array.isArray(manifest.entries)) throw new Error('invalid manifest envelope');
  const expectedNames = SPECS.map(([name]) => name);
  if (JSON.stringify(manifest.entries.map((entry) => entry.name)) !== JSON.stringify(expectedNames)) throw new Error('manifest entry names differ');
  const expectedAggregate = sha256(Buffer.from(manifest.entries.map((entry) => `${entry.name}\t${entry.bytes}\t${entry.sha256}\n`).join(''), 'utf8'));
  if (manifest.aggregate_sha256 !== AGGREGATE_SHA256 || expectedAggregate !== AGGREGATE_SHA256) throw new Error('manifest aggregate digest mismatch');
  for (const [name, bytes, digest, role, decision] of SPECS) {
    const entry = manifest.entries.find((candidate) => candidate.name === name);
    if (!entry || entry.bytes !== bytes || entry.sha256 !== digest || entry.role !== role || entry.decision !== decision || entry.provenance !== PROVENANCE) throw new Error(`manifest entry mismatch: ${name}`);
  }
  return true;
}

function main(argv) {
  const [mode, repoArg, sourceArg] = argv;
  if (!['--write', '--check', '--verify-source'].includes(mode) || !repoArg || (mode !== '--check' && !sourceArg) || (mode === '--check' && sourceArg) || argv.length !== (mode === '--check' ? 2 : 3)) throw new Error('usage: --write <repo-root> <source-root> | --check <repo-root> | --verify-source <repo-root> <source-root>');
  const repoRoot = resolve(repoArg);
  const manifestFile = resolve(repoRoot, MANIFEST_PATH);
  if (mode === '--write') {
    const manifest = buildExternalCorpusManifest(resolve(sourceArg));
    mkdirSync(dirname(manifestFile), { recursive: true });
    writeFileSync(manifestFile, serializeCanonicalJson(manifest), { encoding: 'utf8', flag: 'w' });
    return;
  }
  if (!existsSync(manifestFile)) throw new Error('repo-local external corpus manifest is missing');
  const manifest = JSON.parse(readFileSync(manifestFile, 'utf8'));
  validateExternalCorpusManifest(manifest);
  if (mode === '--verify-source') {
    const source = buildExternalCorpusManifest(resolve(sourceArg));
    if (serializeCanonicalJson(source) !== serializeCanonicalJson(manifest)) throw new Error('mutable source does not match registered manifest');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { main(process.argv.slice(2)); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
