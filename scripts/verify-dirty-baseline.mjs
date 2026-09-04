import { createHash } from 'node:crypto';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { CHANGE_SCOPE_PATHS, PROTECTED_AUTHORITY_PATHS } from './p01-scope.mjs';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const equalArrays = (left, right) => left.length === right.length && left.every((value, index) => value === right[index]);
const scopeNamePattern = /^p(\d{2})-scope\.mjs$/;
const scopeUsage = 'usage: node scripts/verify-dirty-baseline.mjs --check <repo-root> <preflight-path> [--scope pNN-scope.mjs]';

function gitStatusBytes(repoRoot) {
  const result = spawnSync('git', ['-c', 'safe.directory=E:/Soft/vestrace', '-c', 'core.excludesFile=', 'status', '--porcelain=v1', '-z', '--untracked-files=all'], { cwd: repoRoot, encoding: null });
  if (result.status !== 0) throw new Error(`git status failed: ${result.stderr.toString('utf8')}`);
  return result.stdout;
}

function gitHead(repoRoot) {
  const result = spawnSync('git', ['-c', 'safe.directory=E:/Soft/vestrace', 'rev-parse', 'HEAD'], { cwd: repoRoot, encoding: null });
  if (result.status !== 0) throw new Error(`git rev-parse HEAD failed: ${result.stderr.toString('utf8')}`);
  return result.stdout.toString('ascii').trim();
}

export function parsePorcelainV1Z(raw) {
  if (!Buffer.isBuffer(raw)) throw new Error('porcelain bytes must be a Buffer');
  const records = [];
  let offset = 0;
  const decoder = new TextDecoder('utf-8', { fatal: true });
  const validStatusByte = new Set([0x20, 0x21, 0x3f, 0x41, 0x43, 0x44, 0x4d, 0x52, 0x54, 0x55]);
  const validStatuses = new Set([
    ' M', ' A', ' T', ' D', ' R', ' C', 'M ', 'MM', 'MT', 'MD', 'T ', 'TM', 'TT', 'TD', 'A ', 'AM', 'AT', 'AD', 'D ',
    'R ', 'RM', 'RT', 'RD', 'C ', 'CM', 'CT', 'CD',
    'DD', 'AU', 'UD', 'UA', 'DU', 'AA', 'UU', '??',
  ]);
  while (offset < raw.length) {
    const end = raw.indexOf(0, offset);
    if (end < 0) throw new Error('unterminated porcelain-v1 record');
    const record = raw.subarray(offset, end);
    offset = end + 1;
    if (!record.length) throw new Error('empty porcelain-v1 record');
    if (record.length < 4) throw new Error('porcelain-v1 record is too short');
    if (record[2] !== 0x20) throw new Error('porcelain-v1 record separator is not ASCII space');
    if (!validStatusByte.has(record[0]) || !validStatusByte.has(record[1])) throw new Error('invalid porcelain-v1 status byte');
    const status = record.subarray(0, 2).toString('ascii');
    if (!validStatuses.has(status)) throw new Error('invalid porcelain-v1 status combination');
    let path;
    try { path = decoder.decode(record.subarray(3)); } catch { throw new Error('invalid UTF-8 porcelain path'); }
    if (!path) throw new Error('empty porcelain-v1 path');
    if (status[0] === 'R' || status[0] === 'C' || status[1] === 'R' || status[1] === 'C') {
      const sourceEnd = raw.indexOf(0, offset);
      if (sourceEnd < 0) throw new Error('unterminated rename/copy source');
      const source = raw.subarray(offset, sourceEnd);
      offset = sourceEnd + 1;
      if (!source.length) throw new Error('empty rename/copy source');
      try { decoder.decode(source); } catch { throw new Error('invalid UTF-8 rename/copy source'); }
    }
    records.push({ path, status });
  }
  return records;
}

function gitStatus(repoRoot) { return parsePorcelainV1Z(gitStatusBytes(repoRoot)); }

function decodeCapturedPorcelain(preflight, findings) {
  const base64 = preflight.status_porcelain_v1_z_base64;
  if (typeof base64 !== 'string' || (base64.length !== 0 && (!/^[A-Za-z0-9+/]+={0,2}$/.test(base64) || base64.length % 4 !== 0))) {
    findings.push('preflight raw porcelain Base64 is missing or malformed');
    return [];
  }
  const raw = Buffer.from(base64, 'base64');
  if (raw.toString('base64') !== base64) {
    findings.push('preflight raw porcelain Base64 is noncanonical');
    return [];
  }
  if (typeof preflight.status_porcelain_v1_z_sha256 !== 'string' || sha256(raw) !== preflight.status_porcelain_v1_z_sha256) {
    findings.push('preflight raw porcelain SHA-256 mismatch');
    return [];
  }
  try { return parsePorcelainV1Z(raw); } catch (error) { findings.push('preflight raw porcelain cannot be parsed: ' + error.message); return []; }
}

function workingTreeRecord(repoRoot, path, status) {
  const fullPath = resolve(repoRoot, path);
  if (!existsSync(fullPath)) return { bytes: 0, path, sha256: 'absent', status };
  const bytes = readFileSync(fullPath);
  return { bytes: bytes.length, path, sha256: sha256(bytes), status };
}

function baselineAmendmentsByPath(preflight, dirtyByPath, findings) {
  if (preflight.baseline_amendments === undefined) return new Map();
  if (!Array.isArray(preflight.baseline_amendments)) {
    findings.push('preflight baseline_amendments is missing or malformed');
    return new Map();
  }
  const amendmentsByPath = new Map();
  for (const amendment of preflight.baseline_amendments) {
    if (!amendment || typeof amendment.path !== 'string' || !amendment.path || typeof amendment.authorized_at_utc !== 'string' || !amendment.authorized_at_utc || typeof amendment.authorized_by !== 'string' || !amendment.authorized_by || typeof amendment.reason !== 'string' || !amendment.reason || !Number.isInteger(amendment.bytes) || typeof amendment.sha256 !== 'string' || !amendment.sha256) {
      findings.push('malformed preflight baseline_amendments entry');
      continue;
    }
    if (amendmentsByPath.has(amendment.path)) {
      findings.push('duplicate preflight baseline_amendments path: ' + amendment.path);
      continue;
    }
    const baseline = dirtyByPath.get(amendment.path);
    if (!baseline || baseline.bytes !== amendment.bytes || baseline.sha256 !== amendment.sha256) {
      findings.push('baseline amendment does not match baseline dirty file: ' + amendment.path);
      continue;
    }
    amendmentsByPath.set(amendment.path, amendment);
  }
  return amendmentsByPath;
}

/// A protected authority or baseline dirty file may be corrected after its
/// preflight was captured, by an authorization this package cannot itself
/// grant. Recording that correction is not the same as excusing it: an entry
/// pins the exact bytes it moved from and the exact bytes it moved to, so it
/// authorizes one outcome and nothing else. Any further drift trips the guard
/// again.
function protectedAuthorityRevisionsByPath(preflight, protectedByPath, capturedDirtyByPath, findings) {
  if (preflight.protected_authority_revisions === undefined) return new Map();
  if (!Array.isArray(preflight.protected_authority_revisions)) {
    findings.push('preflight protected_authority_revisions is missing or malformed');
    return new Map();
  }
  const revisionsByPath = new Map();
  for (const revision of preflight.protected_authority_revisions) {
    if (!revision || typeof revision.path !== 'string' || !revision.path || typeof revision.authorized_at_utc !== 'string' || !revision.authorized_at_utc || typeof revision.authorized_by !== 'string' || !revision.authorized_by || typeof revision.reason !== 'string' || !revision.reason || !Number.isInteger(revision.bytes) || typeof revision.sha256 !== 'string' || !revision.sha256 || !Number.isInteger(revision.captured_bytes) || typeof revision.captured_sha256 !== 'string' || !revision.captured_sha256) {
      findings.push('malformed preflight protected_authority_revisions entry');
      continue;
    }
    if (revisionsByPath.has(revision.path)) {
      findings.push('duplicate preflight protected_authority_revisions path: ' + revision.path);
      continue;
    }
    const captured = protectedByPath.get(revision.path) ?? capturedDirtyByPath.get(revision.path);
    if (!captured) {
      findings.push('unused preflight protected_authority_revisions path: ' + revision.path);
      continue;
    }
    if (captured.bytes !== revision.captured_bytes || captured.sha256 !== revision.captured_sha256) {
      findings.push('preflight protected_authority_revisions does not match the captured baseline: ' + revision.path);
      continue;
    }
    revisionsByPath.set(revision.path, revision);
  }
  return revisionsByPath;
}

/// True only when this revision records exactly this transition.
function revisionAuthorizes(revision, captured, current) {
  if (!revision) return false;
  return revision.captured_bytes === captured.bytes && revision.captured_sha256 === captured.sha256 && revision.bytes === current.bytes && revision.sha256 === current.sha256;
}

export function verifyDirtyBaseline(repoRoot, preflight) {
  return verifyDirtyBaselineInScope(repoRoot, preflight, { changeScopePaths: CHANGE_SCOPE_PATHS, protectedAuthorityPaths: PROTECTED_AUTHORITY_PATHS }, 'p01-scope.mjs');
}

export function verifyDirtyBaselineInScope(repoRoot, preflight, { changeScopePaths, protectedAuthorityPaths }, scopeName = 'selected scope module') {
  const findings = [];
  if (typeof preflight.head !== 'string' || preflight.head !== gitHead(repoRoot)) findings.push('preflight HEAD differs from current HEAD');
  if (!equalArrays(preflight.change_scope_paths, changeScopePaths)) findings.push(`preflight change_scope_paths differ from ${scopeName}`);
  if (!equalArrays(preflight.protected_authority_paths, protectedAuthorityPaths)) findings.push(`preflight protected_authority_paths differ from ${scopeName}`);
  const protectedByPath = new Map(preflight.protected_authority_digests.map((entry) => [entry.path, entry]));
  const capturedDirtyByPath = new Map();
  for (const entry of preflight.dirty_files ?? []) {
    if (entry && typeof entry.path === 'string' && Number.isInteger(entry.bytes) && typeof entry.sha256 === 'string' && !capturedDirtyByPath.has(entry.path)) capturedDirtyByPath.set(entry.path, entry);
  }
  const revisionsByPath = protectedAuthorityRevisionsByPath(preflight, protectedByPath, capturedDirtyByPath, findings);
  for (const path of protectedAuthorityPaths) {
    const captured = protectedByPath.get(path);
    if (!captured) { findings.push(`missing protected authority digest: ${path}`); continue; }
    const actual = workingTreeRecord(repoRoot, path, '');
    if ((actual.bytes !== captured.bytes || actual.sha256 !== captured.sha256) && !revisionAuthorizes(revisionsByPath.get(path), captured, actual)) findings.push(`protected authority changed: ${path}`);
  }
  const capturedRecords = decodeCapturedPorcelain(preflight, findings);
  const capturedByPath = new Map();
  for (const record of capturedRecords) {
    if (capturedByPath.has(record.path)) findings.push('duplicate preflight raw porcelain path: ' + record.path);
    capturedByPath.set(record.path, record);
  }
  const dirtyByPath = new Map();
  for (const entry of preflight.dirty_files ?? []) {
    if (!entry || typeof entry.path !== 'string' || typeof entry.status !== 'string' || !Number.isInteger(entry.bytes) || typeof entry.sha256 !== 'string') { findings.push('malformed preflight dirty_files entry'); continue; }
    if (dirtyByPath.has(entry.path)) findings.push('duplicate preflight dirty_files path: ' + entry.path);
    dirtyByPath.set(entry.path, entry);
    const raw = capturedByPath.get(entry.path);
    if (!raw || raw.status !== entry.status) findings.push('preflight raw porcelain and dirty_files mismatch: ' + entry.path);
    if (entry.status.includes('D') && (entry.bytes !== 0 || entry.sha256 !== 'absent')) findings.push('preflight deleted dirty_files entry is not absent: ' + entry.path);
  }
  for (const path of capturedByPath.keys()) if (!dirtyByPath.has(path)) findings.push('preflight raw porcelain path missing from dirty_files: ' + path);
  const amendmentsByPath = baselineAmendmentsByPath(preflight, dirtyByPath, findings);
  const scope = new Set(changeScopePaths);
  const expected = new Map([...dirtyByPath.values()].filter((entry) => !scope.has(entry.path)).map((entry) => [entry.path, entry]));
  const actual = new Map(gitStatus(repoRoot).map(({ path, status }) => [path, workingTreeRecord(repoRoot, path, status)]));
  for (const [path, baseline] of expected) {
    const current = actual.get(path);
    if (!current) {
      if (amendmentsByPath.has(path) && !existsSync(resolve(repoRoot, path))) continue;
      findings.push(`baseline dirty path is no longer dirty: ${path}`);
      continue;
    }
    if ((current.status !== baseline.status || current.bytes !== baseline.bytes || current.sha256 !== baseline.sha256) && !revisionAuthorizes(revisionsByPath.get(path), baseline, current)) findings.push(`baseline dirty path changed: ${path}`);
  }
  const scopeLabel = `P${scopeNamePattern.exec(scopeName)?.[1] ?? scopeName}`;
  for (const [path] of actual) if (!scope.has(path) && !expected.has(path)) findings.push(`new dirty path outside ${scopeLabel} scope: ${path}`);
  return findings;
}

async function main(argv) {
  if ((argv.length !== 3 && argv.length !== 5) || argv[0] !== '--check' || (argv.length === 5 && argv[3] !== '--scope')) {
    throw new Error(scopeUsage);
  }
  const [repoRoot, preflightPath] = argv.slice(1).map((value) => resolve(value));
  const scopeName = argv[4] ?? 'p01-scope.mjs';
  if (!scopeNamePattern.test(scopeName)) throw new Error(`invalid scope module name: ${scopeName}; expected pNN-scope.mjs`);
  const scopeUrl = new URL(scopeName, import.meta.url);
  const scopePath = fileURLToPath(scopeUrl);
  if (!existsSync(scopePath) || !statSync(scopePath).isFile()) throw new Error(`scope module does not exist: ${scopeName}`);
  const scope = scopeName === 'p01-scope.mjs'
    ? { changeScopePaths: CHANGE_SCOPE_PATHS, protectedAuthorityPaths: PROTECTED_AUTHORITY_PATHS }
    : await import(scopeUrl.href);
  const findings = verifyDirtyBaselineInScope(repoRoot, JSON.parse(readFileSync(preflightPath, 'utf8')), scope, scopeName);
  if (findings.length) throw new Error(findings.join('\n'));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
