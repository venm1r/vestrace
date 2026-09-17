import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { G0_CRITERIA, MalformedEvidence, collect } from '../scripts/p05-g0-gate.mjs';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const gateScript = join(repoRoot, 'scripts', 'p05-g0-gate.mjs');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function workspace() {
  const root = mkdtempSync(join(tmpdir(), 'p05d-gate-'));
  return {
    root,
    /** Writes a proof artifact and returns a digest-bound source entry for it. */
    source(path, body, overrides = {}) {
      const absolute = join(root, path);
      mkdirSync(dirname(absolute), { recursive: true });
      writeFileSync(absolute, body);
      return {
        path,
        sha256: sha256(body),
        command: 'cargo test -p vestrace-cli',
        exit_code: 0,
        ...overrides,
      };
    },
    dispose() {
      rmSync(root, { recursive: true, force: true });
    },
  };
}

/** A manifest in which every declared criterion passes. */
function passingEvidence(space) {
  return {
    schema_version: 1,
    gate: 'G0',
    criteria: G0_CRITERIA.map((criterion, index) => ({
      id: criterion.id,
      claim: 'pass',
      reason: `observed for ${criterion.id}`,
      sources: [space.source(`proof-${index}.txt`, `exit 0 for ${criterion.id}\n`)],
    })),
  };
}

test('the declared criteria are the frozen G0 list, in one canonical order', () => {
  assert.equal(G0_CRITERIA.length, 19);
  assert.deepEqual(
    G0_CRITERIA.map((criterion) => criterion.id),
    G0_CRITERIA.map((_, index) => `g0-${String(index + 1).padStart(2, '0')}`),
  );
  for (const criterion of G0_CRITERIA) {
    assert.ok(criterion.summary.length > 0, `${criterion.id} has no summary`);
  }
});

test('a fully sourced manifest passes, and its entries keep the declared order', () => {
  const space = workspace();
  try {
    const shuffled = passingEvidence(space);
    shuffled.criteria.reverse();
    const result = collect(shuffled, { root: space.root });

    assert.equal(result.aggregate, 'pass');
    assert.deepEqual(
      result.criteria.map((entry) => entry.criterion),
      G0_CRITERIA.map((criterion) => criterion.id),
    );
    for (const entry of result.criteria) {
      assert.equal(entry.status, 'pass');
      assert.ok(entry.sources.length > 0);
    }
  } finally {
    space.dispose();
  }
});

test('a criterion whose source file is gone cannot pass', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    evidence.criteria[3].sources.push({
      path: 'never-written.txt',
      sha256: sha256('never written'),
      command: 'cargo test',
      exit_code: 0,
    });
    const result = collect(evidence, { root: space.root });

    const entry = result.criteria.find((candidate) => candidate.criterion === 'g0-04');
    assert.notEqual(entry.status, 'pass');
    assert.match(entry.reason, /never-written\.txt/);
    assert.notEqual(result.aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('a source whose bytes no longer match its digest cannot pass', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    const source = evidence.criteria[5].sources[0];
    writeFileSync(join(space.root, source.path), 'a different run entirely\n');
    const result = collect(evidence, { root: space.root });

    const entry = result.criteria.find((candidate) => candidate.criterion === 'g0-06');
    assert.notEqual(entry.status, 'pass');
    assert.match(entry.reason, /digest/);
    assert.notEqual(result.aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('a nonzero recorded command exit cannot be claimed as a pass', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    evidence.criteria[7].sources[0].exit_code = 1;
    const result = collect(evidence, { root: space.root });

    const entry = result.criteria.find((candidate) => candidate.criterion === 'g0-08');
    assert.notEqual(entry.status, 'pass');
    assert.match(entry.reason, /exit/);
    assert.notEqual(result.aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('blocked evidence cannot be relabelled as a pass', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    // A run that never produced an exit is blocked, whatever the claim says.
    evidence.criteria[9].sources[0].exit_code = null;
    evidence.criteria[9].sources[0].blocked_reason = 'Docker Compose unavailable';
    const result = collect(evidence, { root: space.root });

    const entry = result.criteria.find((candidate) => candidate.criterion === 'g0-10');
    assert.equal(entry.status, 'blocked');
    assert.match(entry.reason, /Docker Compose unavailable/);
    assert.notEqual(result.aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('prose is never proof: a claim with no source is unknown', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    evidence.criteria[11].sources = [];
    evidence.criteria[11].reason =
      'This was carefully reviewed and is believed to hold in every case.';
    const result = collect(evidence, { root: space.root });

    const entry = result.criteria.find((candidate) => candidate.criterion === 'g0-12');
    assert.equal(entry.status, 'unknown');
    assert.notEqual(result.aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('an omitted criterion is reported unknown rather than silently dropped', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    evidence.criteria.splice(2, 1);
    const result = collect(evidence, { root: space.root });

    assert.equal(result.criteria.length, G0_CRITERIA.length);
    const entry = result.criteria.find((candidate) => candidate.criterion === 'g0-03');
    assert.equal(entry.status, 'unknown');
    assert.match(entry.reason, /not claimed/i);
    assert.notEqual(result.aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('a criterion the gate does not declare is refused outright', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    evidence.criteria.push({
      id: 'g0-20',
      claim: 'pass',
      reason: 'a criterion this gate never declared',
      sources: [space.source('extra.txt', 'exit 0\n')],
    });
    assert.throws(() => collect(evidence, { root: space.root }), MalformedEvidence);
  } finally {
    space.dispose();
  }
});

test('a duplicated criterion is refused rather than resolved by last writer', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    evidence.criteria.push({
      id: 'g0-01',
      claim: 'pass',
      reason: 'a second, more convenient account of the same criterion',
      sources: [space.source('duplicate.txt', 'exit 0\n')],
    });
    assert.throws(() => collect(evidence, { root: space.root }), MalformedEvidence);
  } finally {
    space.dispose();
  }
});

test('an unsupported schema version or gate is refused', () => {
  const space = workspace();
  try {
    assert.throws(
      () => collect({ ...passingEvidence(space), schema_version: 2 }, { root: space.root }),
      MalformedEvidence,
    );
    assert.throws(
      () => collect({ ...passingEvidence(space), gate: 'G1' }, { root: space.root }),
      MalformedEvidence,
    );
    assert.throws(() => collect(null, { root: space.root }), MalformedEvidence);
    assert.throws(() => collect({}, { root: space.root }), MalformedEvidence);
  } finally {
    space.dispose();
  }
});

test('an unrecognised claim, or a malformed source, is refused', () => {
  const space = workspace();
  try {
    const unrecognised = passingEvidence(space);
    unrecognised.criteria[0].claim = 'probably';
    assert.throws(() => collect(unrecognised, { root: space.root }), MalformedEvidence);

    const absolutePath = passingEvidence(space);
    absolutePath.criteria[0].sources[0].path = '/etc/passwd';
    assert.throws(() => collect(absolutePath, { root: space.root }), MalformedEvidence);

    const escaping = passingEvidence(space);
    escaping.criteria[0].sources[0].path = '../outside.txt';
    assert.throws(() => collect(escaping, { root: space.root }), MalformedEvidence);

    const noCommand = passingEvidence(space);
    delete noCommand.criteria[0].sources[0].command;
    assert.throws(() => collect(noCommand, { root: space.root }), MalformedEvidence);

    const badDigest = passingEvidence(space);
    badDigest.criteria[0].sources[0].sha256 = 'not-a-digest';
    assert.throws(() => collect(badDigest, { root: space.root }), MalformedEvidence);
  } finally {
    space.dispose();
  }
});

test('a claim of blocked or unknown is never upgraded by its sources', () => {
  const space = workspace();
  try {
    const evidence = passingEvidence(space);
    evidence.criteria[0].claim = 'blocked';
    evidence.criteria[1].claim = 'unknown';
    const result = collect(evidence, { root: space.root });

    assert.equal(result.criteria[0].status, 'blocked');
    assert.equal(result.criteria[1].status, 'unknown');
    assert.notEqual(result.aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('the aggregate is blocked when anything is blocked, and otherwise unknown', () => {
  const space = workspace();
  try {
    const blocked = passingEvidence(space);
    blocked.criteria[0].claim = 'blocked';
    blocked.criteria[1].claim = 'unknown';
    assert.equal(collect(blocked, { root: space.root }).aggregate, 'blocked');

    const unknown = passingEvidence(space);
    unknown.criteria[0].claim = 'unknown';
    assert.equal(collect(unknown, { root: space.root }).aggregate, 'unknown');
  } finally {
    space.dispose();
  }
});

function runGate(evidencePath) {
  try {
    const stdout = execFileSync(process.execPath, [gateScript, '--evidence', evidencePath], {
      cwd: repoRoot,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return { code: 0, stdout };
  } catch (error) {
    return { code: error.status, stdout: error.stdout ?? '', stderr: error.stderr ?? '' };
  }
}

test('the command exits nonzero for malformed evidence and for a non-passing aggregate', () => {
  const space = workspace();
  try {
    const manifest = join(space.root, 'evidence.json');

    writeFileSync(manifest, 'this is not JSON at all');
    assert.equal(runGate(manifest).code, 2);

    const unknownCriterion = passingEvidence(space);
    unknownCriterion.criteria.push({ id: 'g0-99', claim: 'pass', reason: 'x', sources: [] });
    writeFileSync(manifest, JSON.stringify(unknownCriterion));
    assert.equal(runGate(manifest).code, 2);

    const notPassing = passingEvidence(space);
    notPassing.criteria[0].claim = 'unknown';
    writeFileSync(manifest, JSON.stringify(notPassing));
    const run = runGate(manifest);
    assert.equal(run.code, 1);
    assert.equal(JSON.parse(run.stdout).aggregate, 'unknown');

    writeFileSync(manifest, JSON.stringify(passingEvidence(space)));
    const passing = runGate(manifest);
    assert.equal(passing.code, 0);
    assert.equal(JSON.parse(passing.stdout).aggregate, 'pass');
  } finally {
    space.dispose();
  }
});

test('the recorded G0 evidence manifest is well formed and claims exactly the criteria closed so far', () => {
  const run = runGate('docs/development-evidence/v1-g0-05-gate.json');

  // Exit 2 is malformed evidence, which is a defect in the manifest. Exit 1 is
  // a correctly formed manifest that does not add up to a pass, which is what
  // the manifest actually has through P05-G: it closes twelve of nineteen
  // criteria and cannot speak to the rest.
  assert.equal(run.code, 1, run.stderr);
  const report = JSON.parse(run.stdout);
  assert.notEqual(report.aggregate, 'pass');
  assert.equal(report.criteria.length, G0_CRITERIA.length);

  const byId = new Map(report.criteria.map((entry) => [entry.criterion, entry]));

  // The change boundary is the one criterion P05-D closes outright.
  assert.equal(byId.get('g0-19').status, 'pass');

  // P05-F closed four with fresh re-run evidence: default-deny route
  // authority, mutation-plus-audit atomicity, MaterialKeyCreationIntent
  // enforcement, and the pinned protocol lock/fixtures.
  for (const id of ['g0-01', 'g0-02', 'g0-08', 'g0-18']) {
    assert.equal(byId.get(id).status, 'pass', id);
    assert.ok(byId.get(id).sources.length > 0, `${id} names what it ran`);
  }

  // P05-G closed seven more, all P03-scoped: typed external-effect authority,
  // ModelRequestEvidence, provider transport, connection/credential guards,
  // admission/leases, CredentialKeyCreationIntent lifecycle, and pre-live
  // credential abort plus bound-Candidate abandon closure.
  for (const id of ['g0-03', 'g0-04', 'g0-05', 'g0-06', 'g0-07', 'g0-09', 'g0-11']) {
    assert.equal(byId.get(id).status, 'pass', id);
    assert.ok(byId.get(id).sources.length > 0, `${id} names what it ran`);
  }

  // The installer/Compose criterion is a conjunction. P05-D proves the
  // topology, the safety stores, and the readiness grants, and does not
  // re-prove the archiver or fingerprint-key conjuncts that belong to earlier
  // packages -- so it is blocked, not passed.
  assert.equal(byId.get('g0-17').status, 'blocked');
  assert.ok(byId.get('g0-17').sources.length > 0, 'a blocked criterion still names what it ran');

  // P05-H moved three more from unknown to blocked -- honest partial-conjunction
  // evidence, not a pass. Each still names what it ran.
  for (const id of ['g0-10', 'g0-14', 'g0-15']) {
    assert.equal(byId.get(id).status, 'blocked', id);
    assert.ok(byId.get(id).sources.length > 0, `${id} names what it ran`);
  }

  const passed = report.criteria.filter((entry) => entry.status === 'pass');
  assert.equal(passed.length, 12, 'no package may claim a criterion it did not close');

  const blocked = report.criteria.filter((entry) => entry.status === 'blocked');
  assert.equal(blocked.length, 4, 'g0-17 plus the three P05-H partial conjunctions');

  // Nothing is inherited from P05-A through P05-C.
  assert.equal(byId.get('g0-16').status, 'unknown');
});
