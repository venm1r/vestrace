// Deterministic G0 evidence collector.
//
// This reports what the recorded evidence actually supports. It has no way to
// produce a pass that the evidence does not already contain: a pass requires
// every named source to exist on disk, to match the digest recorded for it, and
// to carry a successful command exit. Anything else is `blocked` or `unknown`.
//
// Prose is never proof. The collector reads one declarative manifest and the
// digest-bound artifacts it names; it never parses an evidence document's
// narrative, and a `reason` field carries no weight on its own.

import { createHash } from 'node:crypto';
import { readFileSync, statSync } from 'node:fs';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';

/**
 * The frozen v1 G0 criteria, in the order section G0 of
 * `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md`
 * states them. The order is part of the contract: a report that reordered them
 * could not be diffed against an earlier one.
 */
export const G0_CRITERIA = [
  { id: 'g0-01', summary: 'default-deny route authority covers every current route' },
  { id: 'g0-02', summary: 'mutation plus audit is atomic' },
  { id: 'g0-03', summary: 'model calls, EmbeddingJobs, and probes are typed uses of the shared external-effect authority' },
  { id: 'g0-04', summary: 'every effect has an immutable ModelRequestEvidence graph and fresh reconstruction check' },
  { id: 'g0-05', summary: 'the shared provider transport enforces egress, Web PKI, redirect/proxy refusal, and redaction' },
  { id: 'g0-06', summary: 'connection and credential guards, occupancy, and serialization pass PostgreSQL tests' },
  { id: 'g0-07', summary: 'admission policy, leases, throttle, and no-retry-after-dispatch pass PostgreSQL and fault tests' },
  { id: 'g0-08', summary: 'MaterialKeyCreationIntent state/receipt/attachment enforcement permits only Live references' },
  { id: 'g0-09', summary: 'CredentialKeyCreationIntent lifecycle and guarded pre-live abort converge crash-safely' },
  { id: 'g0-10', summary: 'EmbeddingSpaceTransition spaces, recipes, barriers, and carry mapping pass every oracle' },
  { id: 'g0-11', summary: 'pre-live credential abort and bound-Candidate abandon close in guard order' },
  { id: 'g0-12', summary: 'DrainMutationPermit reconciles only the exact pre-Quiescing set across every crash boundary' },
  { id: 'g0-13', summary: 'unbound pre-Prepared drain work resumes or aborts by its fixed route, never by synthetic bytes' },
  { id: 'g0-14', summary: 'duplicate-charge successors, transition retry pinning, and retrieval fences refuse fallback' },
  { id: 'g0-15', summary: 'append-only evidence is structural-only and dictionary-resistant; material lifecycle is one-way' },
  { id: 'g0-16', summary: 'InstallationMutationPermit, witnessed backup/restore, and crypto-erasure prevent rollback or resurrection' },
  { id: 'g0-17', summary: 'the local installer and Compose provision least-privilege roles, stores, readiness, and all grants' },
  { id: 'g0-18', summary: 'pinned protocol lock and executable contract fixtures exist' },
  { id: 'g0-19', summary: 'current dirty work is preserved and the selected change boundary is documented' },
];

const DECLARED = new Map(G0_CRITERIA.map((criterion) => [criterion.id, criterion]));
const CLAIMS = new Set(['pass', 'blocked', 'unknown']);
const DIGEST = /^[0-9a-f]{64}$/;

/** A defect in the manifest itself, as distinct from evidence that falls short. */
export class MalformedEvidence extends Error {
  constructor(message) {
    super(message);
    this.name = 'MalformedEvidence';
  }
}

function requireObject(value, what) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new MalformedEvidence(`${what} must be an object`);
  }
}

function requireText(value, what) {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new MalformedEvidence(`${what} must be a non-empty string`);
  }
}

/**
 * A source path is repository-relative and may not climb out of the root. A
 * manifest that could name `/etc/passwd` or `../..` would let a criterion cite
 * a file nobody reviewed as its proof.
 */
function resolveSource(root, path, what) {
  requireText(path, `${what} path`);
  if (isAbsolute(path) || /^[A-Za-z]:/.test(path)) {
    throw new MalformedEvidence(`${what} path must be repository-relative, got ${path}`);
  }
  const absolute = resolve(root, path);
  const inside = relative(resolve(root), absolute);
  if (inside.startsWith('..') || isAbsolute(inside) || inside.split(sep).includes('..')) {
    throw new MalformedEvidence(`${what} path escapes the evidence root: ${path}`);
  }
  return absolute;
}

function validateSource(root, source, what) {
  requireObject(source, what);
  const absolute = resolveSource(root, source.path, what);
  requireText(source.command, `${what} command`);
  if (typeof source.sha256 !== 'string' || !DIGEST.test(source.sha256)) {
    throw new MalformedEvidence(`${what} sha256 must be 64 lowercase hex characters`);
  }
  if (!('exit_code' in source)) {
    throw new MalformedEvidence(`${what} must record an exit_code, or null when blocked`);
  }
  if (source.exit_code !== null && !Number.isInteger(source.exit_code)) {
    throw new MalformedEvidence(`${what} exit_code must be an integer or null`);
  }
  if (source.exit_code === null) {
    requireText(source.blocked_reason, `${what} blocked_reason`);
  }
  return absolute;
}

/**
 * Judges one source. Returns `null` when it independently supports a pass, or
 * a `{ status, reason }` downgrade otherwise.
 */
function inspectSource(absolute, source) {
  if (source.exit_code === null) {
    return { status: 'blocked', reason: `${source.path}: ${source.blocked_reason}` };
  }
  let bytes;
  try {
    if (!statSync(absolute).isFile()) {
      return { status: 'blocked', reason: `${source.path} is not a file` };
    }
    bytes = readFileSync(absolute);
  } catch {
    return { status: 'blocked', reason: `${source.path} is missing` };
  }
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (actual !== source.sha256) {
    return {
      status: 'blocked',
      reason: `${source.path} does not match its recorded digest (${actual})`,
    };
  }
  if (source.exit_code !== 0) {
    return {
      status: 'blocked',
      reason: `${source.path} records a nonzero exit ${source.exit_code} for \`${source.command}\``,
    };
  }
  return null;
}

/**
 * Collects one report from a manifest.
 *
 * Throws `MalformedEvidence` for a manifest defect. Otherwise every declared
 * criterion appears exactly once, in declared order, and the aggregate passes
 * only when all of them do.
 */
export function collect(evidence, { root } = {}) {
  requireText(root, 'evidence root');
  requireObject(evidence, 'evidence');
  if (evidence.schema_version !== 1) {
    throw new MalformedEvidence('evidence schema_version must be 1');
  }
  if (evidence.gate !== 'G0') {
    throw new MalformedEvidence('this collector reports only the G0 gate');
  }
  if (!Array.isArray(evidence.criteria)) {
    throw new MalformedEvidence('evidence criteria must be an array');
  }

  const claimed = new Map();
  for (const [index, entry] of evidence.criteria.entries()) {
    requireObject(entry, `criteria[${index}]`);
    requireText(entry.id, `criteria[${index}].id`);
    if (!DECLARED.has(entry.id)) {
      throw new MalformedEvidence(`criteria[${index}] names ${entry.id}, which G0 does not declare`);
    }
    if (claimed.has(entry.id)) {
      throw new MalformedEvidence(`${entry.id} is claimed more than once`);
    }
    if (!CLAIMS.has(entry.claim)) {
      throw new MalformedEvidence(`${entry.id} claim must be pass, blocked, or unknown`);
    }
    requireText(entry.reason, `${entry.id} reason`);
    if (!Array.isArray(entry.sources)) {
      throw new MalformedEvidence(`${entry.id} sources must be an array`);
    }
    const resolved = entry.sources.map((source, position) => ({
      source,
      absolute: validateSource(root, source, `${entry.id} sources[${position}]`),
    }));
    claimed.set(entry.id, { entry, resolved });
  }

  const criteria = G0_CRITERIA.map((criterion) => {
    const held = claimed.get(criterion.id);
    if (!held) {
      return {
        criterion: criterion.id,
        summary: criterion.summary,
        status: 'unknown',
        sources: [],
        reason: 'not claimed by the recorded evidence',
      };
    }
    const { entry, resolved } = held;
    const sources = resolved.map(({ source }) => source.path);

    // A claim of blocked or unknown is reported as made. Only a claim of pass
    // is tested, and testing can lower it but never raise it.
    if (entry.claim !== 'pass') {
      return {
        criterion: criterion.id,
        summary: criterion.summary,
        status: entry.claim,
        sources,
        reason: entry.reason,
      };
    }
    if (resolved.length === 0) {
      return {
        criterion: criterion.id,
        summary: criterion.summary,
        status: 'unknown',
        sources,
        reason: 'claimed as passing with no named source; prose is not proof',
      };
    }
    for (const { source, absolute } of resolved) {
      const downgrade = inspectSource(absolute, source);
      if (downgrade) {
        return {
          criterion: criterion.id,
          summary: criterion.summary,
          status: downgrade.status,
          sources,
          reason: downgrade.reason,
        };
      }
    }
    return {
      criterion: criterion.id,
      summary: criterion.summary,
      status: 'pass',
      sources,
      reason: entry.reason,
    };
  });

  // Blocked outranks unknown: a criterion with named evidence that did not run
  // is a more specific statement than one nobody attempted.
  let aggregate = 'pass';
  if (criteria.some((entry) => entry.status === 'blocked')) {
    aggregate = 'blocked';
  } else if (criteria.some((entry) => entry.status !== 'pass')) {
    aggregate = 'unknown';
  }

  return {
    gate: 'G0',
    aggregate,
    counts: {
      pass: criteria.filter((entry) => entry.status === 'pass').length,
      blocked: criteria.filter((entry) => entry.status === 'blocked').length,
      unknown: criteria.filter((entry) => entry.status === 'unknown').length,
    },
    criteria,
  };
}

const USAGE = 'usage: node scripts/p05-g0-gate.mjs --evidence <path>';

function main(argv) {
  const flag = argv.indexOf('--evidence');
  if (flag < 0 || !argv[flag + 1]) {
    process.stderr.write(`${USAGE}\n`);
    return 2;
  }
  const manifestPath = argv[flag + 1];
  let evidence;
  try {
    evidence = JSON.parse(readFileSync(manifestPath, 'utf8'));
  } catch (error) {
    process.stderr.write(`evidence manifest is unreadable: ${error.message}\n`);
    return 2;
  }

  // Sources are named relative to the manifest's own directory, so a manifest
  // and the artifacts it cites move together.
  const root = join(resolve(manifestPath), '..');
  let report;
  try {
    report = collect(evidence, { root });
  } catch (error) {
    if (error instanceof MalformedEvidence) {
      process.stderr.write(`malformed evidence: ${error.message}\n`);
      return 2;
    }
    throw error;
  }

  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  return report.aggregate === 'pass' ? 0 : 1;
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'))) {
  process.exit(main(process.argv.slice(2)));
}
