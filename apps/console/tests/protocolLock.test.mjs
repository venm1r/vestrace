import assert from 'node:assert/strict';
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { spawnSync } from 'node:child_process';
import { buildProtocolLock, parseCargoLockPackages, serializeProtocolLock, validateDirectPins, verifyProtocolLock } from '../../../scripts/protocol-lock.mjs';
import { parseCargoArchiveMetadata, parseNpmAttestation, parseNpmMetadata, readTarGzEntries, verifyOnlineProvenance } from '../../../scripts/verify-protocol-provenance.mjs';
import { PINNED_PROTOCOL_PROVENANCE } from '../../../scripts/protocol-provenance.mjs';
import { parsePorcelainV1Z, verifyDirtyBaseline } from '../../../scripts/verify-dirty-baseline.mjs';
import { CHANGE_SCOPE_PATHS, PROTECTED_AUTHORITY_PATHS } from '../../../scripts/p01-scope.mjs';

test('protocol lock is deterministic and detects referenced artifact drift', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  const lock = buildProtocolLock(root);
  assert.equal(lock.schema_version, 1);
  assert.equal(lock.frozen_spec_sha256, 'b31b5be62504e1a65f411cd31b446cd41b3d032b7282f1aa42907706ef9c1473');
  assert.equal(lock.npm['@ag-ui/core'].version, '0.0.58');
  assert.equal(lock.npm['@ag-ui/core'].metadata_url, PINNED_PROTOCOL_PROVENANCE.npm['@ag-ui/core'].metadata_url);
  assert.equal(lock.npm['@ag-ui/core'].resolved_dependency_uri, PINNED_PROTOCOL_PROVENANCE.npm['@ag-ui/core'].resolved_dependency_uri);
  assert.equal(lock.npm['@ag-ui/core'].resolved_git_commit, PINNED_PROTOCOL_PROVENANCE.npm['@ag-ui/core'].resolved_git_commit);
  assert.equal(lock.a2a.specification.release, 'v1.0.1');
  assert.equal(lock.a2a.specification.release_url, PINNED_PROTOCOL_PROVENANCE.a2a.release_url);
  assert.equal(lock.a2a.crates['a2a-server-lf'].archive_url, PINNED_PROTOCOL_PROVENANCE.crates['a2a-server-lf'].archive_url);
  assert.equal(lock.a2a.crates['a2a-server-lf'].adapter_owner, 'vestrace-http');
  assert.equal(serializeProtocolLock(lock), readFileSync(join(root, 'schemas/protocol-lock.json'), 'utf8'));
  assert.deepEqual(verifyProtocolLock(root), []);
});

test('local provenance parsers reject mutated identity data', () => {
  const npm = { repository: { url: 'git+https://github.com/ag-ui-protocol/ag-ui.git' }, version: '0.0.58' };
  const npmExpected = { repository: npm.repository.url, version: npm.version };
  assert.doesNotThrow(() => parseNpmMetadata(npm, npmExpected));
  assert.throws(() => parseNpmMetadata({ ...npm, version: '0.0.57' }, { repository: npm.repository.url, version: npm.version }));
  assert.throws(() => parseNpmMetadata({ ...npm, repository: { url: 'https://wrong.example' } }, npmExpected));
  const tarEntry = (name, text) => { const header = Buffer.alloc(512); header.write(name); header.write(text.length.toString(8).padStart(11, '0') + '\0', 124); header[156] = 48; const body = Buffer.from(text); return Buffer.concat([header, body, Buffer.alloc((512 - body.length % 512) % 512)]); };
  const archive = gzipSync(Buffer.concat([tarEntry('a2a-lf-0.3.0/Cargo.toml', 'repository = "https://github.com/a2aproject/a2a-rs"\n'), tarEntry('a2a-lf-0.3.0/.cargo_vcs_info.json', '{"git":{"sha1":"abc"}}'), Buffer.alloc(1024)]));
  const expected = { checksum: createHash('sha256').update(archive).digest('hex'), repository: 'https://github.com/a2aproject/a2a-rs', vcs_commit: 'abc' };
  assert.doesNotThrow(() => parseCargoArchiveMetadata(archive, expected));
  assert.equal(readTarGzEntries(archive).size, 2);
  assert.throws(() => parseCargoArchiveMetadata(Buffer.from('wrong'), expected));
  const wrongRepositoryArchive = gzipSync(Buffer.concat([tarEntry('a2a-lf-0.3.0/Cargo.toml', 'repository = "https://wrong.example"\n'), tarEntry('a2a-lf-0.3.0/.cargo_vcs_info.json', '{"git":{"sha1":"abc"}}'), Buffer.alloc(1024)]));
  assert.throws(() => parseCargoArchiveMetadata(wrongRepositoryArchive, { ...expected, checksum: createHash('sha256').update(wrongRepositoryArchive).digest('hex') }));
  const statement = { predicate: { digest: { gitCommit: '0c0b88a3fe087a631decd6225efaf40e068e4449' }, resolvedDependencies: [{ uri: 'git+https://github.com/ag-ui-protocol/ag-ui@refs/heads/main' }] }, subject: [{ digest: { sha512: 'digest' } }] };
  assert.rejects(() => verifyOnlineProvenance(async () => ({ ok: false })));
});

test('SLSA attestation parser selects provenance v1 and validates decoded SRI identity', () => {
  const integrity = 'sha512-' + Buffer.from('subject-bytes').toString('base64');
  const digest = Buffer.from('subject-bytes').toString('hex');
  const statement = { predicate: { buildDefinition: { resolvedDependencies: [{ digest: { gitCommit: '0c0b88a3fe087a631decd6225efaf40e068e4449' }, uri: 'git+https://github.com/ag-ui-protocol/ag-ui@refs/heads/main' }] } }, subject: [{ digest: { sha512: digest } }] };
  const slsa = { bundle: { dsseEnvelope: { payload: Buffer.from(JSON.stringify(statement)).toString('base64') } }, predicateType: 'https://slsa.dev/provenance/v1' };
  const publish = { bundle: { dsseEnvelope: { payload: Buffer.from('{}').toString('base64') } }, predicateType: 'https://npm.dev/attestation/publish/v0.1' };
  const expected = { resolved_dependency_uri: 'git+https://github.com/ag-ui-protocol/ag-ui@refs/heads/main', resolved_git_commit: '0c0b88a3fe087a631decd6225efaf40e068e4449' };
  assert.doesNotThrow(() => parseNpmAttestation({ attestations: [publish, slsa] }, expected, integrity));
  assert.throws(() => parseNpmAttestation({ attestations: [publish] }, expected, integrity));
  assert.throws(() => parseNpmAttestation({ attestations: [{ ...slsa, predicateType: 'wrong' }] }, expected, integrity));
  assert.throws(() => parseNpmAttestation({ attestations: [{ ...slsa, bundle: { dsseEnvelope: { payload: 'not base64 json' } } }] }, expected, integrity));
  const wrongUri = structuredClone(slsa);
  wrongUri.bundle.dsseEnvelope.payload = Buffer.from(JSON.stringify({ ...statement, predicate: { buildDefinition: { resolvedDependencies: [{ digest: { gitCommit: '0c0b88a3fe087a631decd6225efaf40e068e4449' }, uri: 'wrong' }] } } })).toString('base64');
  assert.throws(() => parseNpmAttestation({ attestations: [wrongUri] }, expected, integrity));
  const duplicateSubjects = structuredClone(slsa);
  duplicateSubjects.bundle.dsseEnvelope.payload = Buffer.from(JSON.stringify({ ...statement, subject: [statement.subject[0], statement.subject[0]] })).toString('base64');
  assert.throws(() => parseNpmAttestation({ attestations: [duplicateSubjects] }, expected, integrity));
  const missingSubject = structuredClone(slsa);
  missingSubject.bundle.dsseEnvelope.payload = Buffer.from(JSON.stringify({ ...statement, subject: [] })).toString('base64');
  assert.throws(() => parseNpmAttestation({ attestations: [missingSubject] }, expected, integrity));
  const duplicateDependency = structuredClone(slsa);
  duplicateDependency.bundle.dsseEnvelope.payload = Buffer.from(JSON.stringify({ ...statement, predicate: { buildDefinition: { resolvedDependencies: [statement.predicate.buildDefinition.resolvedDependencies[0], statement.predicate.buildDefinition.resolvedDependencies[0]] } } })).toString('base64');
  assert.throws(() => parseNpmAttestation({ attestations: [duplicateDependency] }, expected, integrity));
  const wrongCommit = structuredClone(slsa);
  wrongCommit.bundle.dsseEnvelope.payload = Buffer.from(JSON.stringify({ ...statement, predicate: { buildDefinition: { resolvedDependencies: [{ digest: { gitCommit: 'wrong' }, uri: expected.resolved_dependency_uri }] } } })).toString('base64');
  assert.throws(() => parseNpmAttestation({ attestations: [wrongCommit] }, expected, integrity));
});

test('strict local lock parser and canonical serialization reject tuple and text mutations', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  const cargo = readFileSync(join(root, 'Cargo.lock'), 'utf8');
  assert.equal(parseCargoLockPackages(cargo).filter((entry) => entry.name === 'a2a-lf').length, 1);
  assert.throws(() => parseCargoLockPackages('[[package]]\nname = "a2a-lf"\nname = "a2a-lf"\n'));
  assert.doesNotThrow(() => validateDirectPins(root));
  const serialized = serializeProtocolLock(buildProtocolLock(root));
  assert.ok(serialized.endsWith('\n'));
  assert.equal(serialized.includes('\r'), false);
  assert.notEqual(serialized.replace(/\n/g, '\r\n'), serialized);
  assert.notEqual(serialized.slice(0, -1), serialized);
  const temporary = mkdtempSync(join(tmpdir(), 'vestrace-p01-lock-text-'));
  try {
    const path = join(temporary, 'protocol-lock.json');
    writeFileSync(path, serialized.replace(/\n/g, '\r\n'));
    assert.deepEqual(verifyProtocolLock(root, path), ['protocol lock text is noncanonical']);
    writeFileSync(path, serialized.slice(0, -1));
    assert.deepEqual(verifyProtocolLock(root, path), ['protocol lock text is noncanonical']);
  } finally { rmSync(temporary, { force: true, recursive: true }); }
});

test('direct pin and crate-boundary validator rejects independent range and ownership mutations', () => {
  const root = mkdtempSync(join(tmpdir(), 'vestrace-p01-pins-'));
  const write = (relative, text) => { const path = join(root, relative); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, text); };
  const packageJson = () => JSON.stringify({
    dependencies: Object.fromEntries(Object.entries(PINNED_PROTOCOL_PROVENANCE.npm).map(([name, value]) => [name, value.version])),
    devDependencies: Object.fromEntries(Object.entries(PINNED_PROTOCOL_PROVENANCE.tooling).map(([name, value]) => [name, value.version])),
  });
  const cargo = () => '[workspace.dependencies]\n' + Object.entries(PINNED_PROTOCOL_PROVENANCE.workspace_aliases).map(([alias, value]) => alias + ' = { package = "' + value.package + '", version = "' + value.version + '" }').join('\n') + '\n';
  const http = '[dependencies]\na2a.workspace = true\na2a-server.workspace = true\n';
  const infrastructure = '[dependencies]\na2a.workspace = true\na2a-client.workspace = true\n';
  try {
    write('apps/console/package.json', packageJson()); write('Cargo.toml', cargo()); write('crates/vestrace-http/Cargo.toml', http); write('crates/vestrace-infrastructure/Cargo.toml', infrastructure);
    assert.doesNotThrow(() => validateDirectPins(root));
    write('apps/console/package.json', packageJson().replace('"0.0.58"', '"^0.0.58"'));
    assert.throws(() => validateDirectPins(root), /direct pin mismatch/);
    write('apps/console/package.json', packageJson()); write('Cargo.toml', cargo().replace('"=0.3.0"', '"0.3.0"'));
    assert.throws(() => validateDirectPins(root), /workspace alias mismatch/);
    write('Cargo.toml', cargo()); write('crates/vestrace-http/Cargo.toml', http + 'a2a-client.workspace = true\n');
    assert.throws(() => validateDirectPins(root), /boundary ownership mismatch/);
  } finally { rmSync(root, { force: true, recursive: true }); }
});

test('aggregate verifier independently rejects q1, PNG, npm, Cargo, A2A, and stale-lock fixture mutations', () => {
  const source = fileURLToPath(new URL('../../../', import.meta.url));
  const root = mkdtempSync(join(tmpdir(), 'vestrace-p01-lock-fixture-'));
  const paths = [
    'Cargo.lock', 'Cargo.toml', 'apps/console/package-lock.json', 'apps/console/package.json',
    'crates/vestrace-http/Cargo.toml', 'crates/vestrace-infrastructure/Cargo.toml',
    'docs/external-corpus/vestrace-docss-2026-08-19.manifest.json',
    'docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md',
    'schemas/a2a/vestrace-v1-profile.json', 'schemas/ag-ui/0.0.58/runtime-schemas.json',
    'schemas/ag-ui/vestrace-v1-profile.json', 'schemas/openai-compatible/openai-chat-completions-v1-q1.json',
    'tests/fixtures/openai-q1/marker.png',
  ];
  const copy = (relative) => { const target = join(root, relative); mkdirSync(dirname(target), { recursive: true }); copyFileSync(join(source, relative), target); };
  const restore = (relative) => copyFileSync(join(source, relative), join(root, relative));
  const mutateByte = (relative) => { const target = join(root, relative); const bytes = readFileSync(target); bytes[bytes.length - 1] ^= 1; writeFileSync(target, bytes); };
  try {
    for (const path of paths) copy(path);
    const lockPath = join(root, 'schemas/protocol-lock.json');
    writeFileSync(lockPath, serializeProtocolLock(buildProtocolLock(root, { verifyGenerated: false })));
    assert.deepEqual(verifyProtocolLock(root, lockPath, { verifyGenerated: false }), []);
    mutateByte('schemas/openai-compatible/openai-chat-completions-v1-q1.json');
    assert.deepEqual(verifyProtocolLock(root, lockPath, { verifyGenerated: false }), ['protocol lock differs from reconstruction']);
    restore('schemas/openai-compatible/openai-chat-completions-v1-q1.json');
    mutateByte('tests/fixtures/openai-q1/marker.png');
    assert.deepEqual(verifyProtocolLock(root, lockPath, { verifyGenerated: false }), ['protocol lock differs from reconstruction']);
    restore('tests/fixtures/openai-q1/marker.png');
    mutateByte('schemas/ag-ui/0.0.58/runtime-schemas.json');
    assert.deepEqual(verifyProtocolLock(root, lockPath, { verifyGenerated: false }), ['protocol lock differs from reconstruction']);
    restore('schemas/ag-ui/0.0.58/runtime-schemas.json');
    mutateByte('schemas/ag-ui/vestrace-v1-profile.json');
    assert.deepEqual(verifyProtocolLock(root, lockPath, { verifyGenerated: false }), ['protocol lock differs from reconstruction']);
    restore('schemas/ag-ui/vestrace-v1-profile.json');
    writeFileSync(join(root, 'apps/console/package-lock.json'), readFileSync(join(source, 'apps/console/package-lock.json'), 'utf8').replace('sha512-XgGb7YmhV+yMBaEmlrpsd5S+nUxq0JgSegss2t4gIFR1j7w3w0ibtKfRgcQHWeMvwZxcT5S28VEEarqtgxYYHw==', 'sha512-mutated'));
    assert.match(verifyProtocolLock(root, lockPath, { verifyGenerated: false })[0], /npm lock pin mismatch/);
    restore('apps/console/package-lock.json');
    writeFileSync(join(root, 'Cargo.lock'), readFileSync(join(source, 'Cargo.lock'), 'utf8').replace('name = "a2a-client-lf"\nversion = "0.2.1"\nsource = "registry+https://github.com/rust-lang/crates.io-index"', 'name = "a2a-client-lf"\nversion = "0.2.1"\nsource = "registry+https://wrong.example"'));
    assert.match(verifyProtocolLock(root, lockPath, { verifyGenerated: false })[0], /Cargo.lock package tuple mismatch/);
    restore('Cargo.lock');
    writeFileSync(join(root, 'Cargo.lock'), readFileSync(join(source, 'Cargo.lock'), 'utf8').replace('f68a06a40df172bb5ae0f25e3d49e922f0d33f8af49d0b1276307857dbd28ad2', 'checksum-mutated'));
    assert.match(verifyProtocolLock(root, lockPath, { verifyGenerated: false })[0], /Cargo.lock package tuple mismatch/);
    restore('Cargo.lock');
    writeFileSync(join(root, 'schemas/a2a/vestrace-v1-profile.json'), readFileSync(join(source, 'schemas/a2a/vestrace-v1-profile.json'), 'utf8').replace('https://github.com/a2aproject/A2A.git', 'https://wrong.example/A2A.git'));
    assert.match(verifyProtocolLock(root, lockPath, { verifyGenerated: false })[0], /A2A profile provenance mismatch/);
    for (const [pinned, mutated] of [
      ['3303592588e388e62e0f69f701af531d2f4e3991', '3303592588e388e62e0f69f701af531d2f4e3990'],
      ['https://github.com/a2aproject/A2A/releases/tag/v1.0.1', 'https://wrong.example/A2A/releases/tag/v1.0.1'],
    ]) {
      restore('schemas/a2a/vestrace-v1-profile.json');
      writeFileSync(join(root, 'schemas/a2a/vestrace-v1-profile.json'), readFileSync(join(source, 'schemas/a2a/vestrace-v1-profile.json'), 'utf8').replace(pinned, mutated));
      assert.match(verifyProtocolLock(root, lockPath, { verifyGenerated: false })[0], /A2A profile provenance mismatch/);
    }
    restore('schemas/a2a/vestrace-v1-profile.json');
    writeFileSync(join(root, 'schemas/a2a/vestrace-v1-profile.json'), readFileSync(join(source, 'schemas/a2a/vestrace-v1-profile.json'), 'utf8').replace('vestrace-v1-a2a/1.0', 'vestrace-v1-a2a/mutated'));
    assert.deepEqual(verifyProtocolLock(root, lockPath, { verifyGenerated: false }), ['protocol lock differs from reconstruction']);
    restore('schemas/a2a/vestrace-v1-profile.json');
    const originalLock = readFileSync(lockPath, 'utf8');
    const lockedValues = [
      ...Object.values(PINNED_PROTOCOL_PROVENANCE.npm).flatMap((entry) => [entry.attestation_url, entry.metadata_url, entry.repository, entry.resolved_dependency_uri, entry.resolved_git_commit]),
      ...Object.values(PINNED_PROTOCOL_PROVENANCE.crates).flatMap((entry) => [entry.archive_url, entry.repository, entry.vcs_commit]),
    ];
    for (const value of lockedValues) {
      writeFileSync(lockPath, originalLock.replace(value, value + '-mutated'));
      assert.deepEqual(verifyProtocolLock(root, lockPath, { verifyGenerated: false }), ['protocol lock differs from reconstruction']);
    }
    writeFileSync(lockPath, originalLock);
  } finally { rmSync(root, { force: true, recursive: true }); }
});

test('online provenance uses the pinned crate user agent without changing npm request headers', async () => {
  const calls = [];
  const npmAttestation = (expected) => {
    const subject = Buffer.from(expected.integrity.slice('sha512-'.length), 'base64').toString('hex');
    const statement = {
      predicate: { buildDefinition: { resolvedDependencies: [{ digest: { gitCommit: expected.resolved_git_commit }, uri: expected.resolved_dependency_uri }] } },
      subject: [{ digest: { sha512: subject } }],
    };
    return { attestations: [{ bundle: { dsseEnvelope: { payload: Buffer.from(JSON.stringify(statement)).toString('base64') } }, predicateType: 'https://slsa.dev/provenance/v1' }] };
  };
  const fetchFixture = async (url, init) => {
    calls.push({ init, url });
    for (const expected of Object.values(PINNED_PROTOCOL_PROVENANCE.npm)) {
      if (url === expected.metadata_url) return { json: async () => ({ repository: { url: expected.repository }, version: expected.version }), ok: true };
      if (url === expected.attestation_url) return { json: async () => npmAttestation(expected), ok: true };
    }
    const bytes = Buffer.from('checksum failure fixture');
    return { arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), ok: true };
  };
  await assert.rejects(() => verifyOnlineProvenance(fetchFixture), /crate archive checksum mismatch/);
  const crateCall = calls.find((call) => call.url === PINNED_PROTOCOL_PROVENANCE.crates['a2a-client-lf'].archive_url);
  assert.equal(crateCall.init.headers['User-Agent'], 'vestrace-p01-provenance/1.0');
  assert.equal(crateCall.init.redirect, 'follow');
  for (const call of calls.filter((entry) => entry.url.includes('registry.npmjs.org'))) assert.equal(call.init, undefined);
});

test('dirty baseline binds HEAD and exact raw porcelain bytes to captured dirty records', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  const preflight = JSON.parse(readFileSync(join(root, 'docs/development-evidence/v1-g0-01-preflight.json'), 'utf8'));
  assert.deepEqual(verifyDirtyBaseline(root, preflight), []);
  const mutations = [
    (value) => { value.head = '0000000000000000000000000000000000000000'; },
    (value) => { delete value.status_porcelain_v1_z_base64; },
    (value) => { value.status_porcelain_v1_z_base64 = 'not-valid-base64'; },
    (value) => { value.status_porcelain_v1_z_sha256 = '0'.repeat(64); },
    (value) => {
      const raw = Buffer.from(value.status_porcelain_v1_z_base64, 'base64');
      raw[0] ^= 1;
      value.status_porcelain_v1_z_base64 = raw.toString('base64');
      value.status_porcelain_v1_z_sha256 = createHash('sha256').update(raw).digest('hex');
    },
    (value) => {
      const raw = Buffer.concat([Buffer.from(value.status_porcelain_v1_z_base64, 'base64'), Buffer.from([0])]);
      value.status_porcelain_v1_z_base64 = raw.toString('base64');
      value.status_porcelain_v1_z_sha256 = createHash('sha256').update(raw).digest('hex');
    },
    (value) => {
      const raw = Buffer.from(value.status_porcelain_v1_z_base64, 'base64');
      raw[2] = 'X'.charCodeAt(0);
      value.status_porcelain_v1_z_base64 = raw.toString('base64');
      value.status_porcelain_v1_z_sha256 = createHash('sha256').update(raw).digest('hex');
    },
    (value) => {
      const raw = Buffer.concat([Buffer.from(value.status_porcelain_v1_z_base64, 'base64'), Buffer.from('MA Cargo.lock\0')]);
      const cargo = readFileSync(join(root, 'Cargo.lock'));
      value.status_porcelain_v1_z_base64 = raw.toString('base64');
      value.status_porcelain_v1_z_sha256 = createHash('sha256').update(raw).digest('hex');
      value.dirty_files.push({ bytes: cargo.length, path: 'Cargo.lock', sha256: createHash('sha256').update(cargo).digest('hex'), status: 'MA' });
    },
    (value) => { value.dirty_files[0].status = '??'; },
  ];
  for (const mutate of mutations) {
    const candidate = structuredClone(preflight);
    mutate(candidate);
    assert.notDeepEqual(verifyDirtyBaseline(root, candidate), []);
  }
});

test('porcelain-v1-z parser accepts only complete strict records', () => {
  assert.deepEqual(parsePorcelainV1Z(Buffer.alloc(0)), []);
  assert.deepEqual(parsePorcelainV1Z(Buffer.from(' M tracked.txt\0')), [{ path: 'tracked.txt', status: ' M' }]);
  assert.deepEqual(parsePorcelainV1Z(Buffer.from('RM renamed.txt\0original.txt\0')), [{ path: 'renamed.txt', status: 'RM' }]);
  assert.deepEqual(parsePorcelainV1Z(Buffer.from(' R renamed.txt\0original.txt\0')), [{ path: 'renamed.txt', status: ' R' }]);
  assert.deepEqual(parsePorcelainV1Z(Buffer.from(' C copied.txt\0original.txt\0')), [{ path: 'copied.txt', status: ' C' }]);
  for (const raw of [
    Buffer.from('\0'),
    Buffer.from(' M\0'),
    Buffer.from(' MXpath\0'),
    Buffer.from(' Q path\0'),
    Buffer.from('?M path\0'),
    Buffer.from('!! ignored.txt\0'),
    Buffer.from('R  path\0'),
    Buffer.from(' R renamed.txt\0'),
    Buffer.from(' C copied.txt\0'),
    Buffer.from('R  renamed.txt\0\0'),
    Buffer.concat([Buffer.from(' M tracked.txt\0'), Buffer.from([0])]),
    Buffer.concat([Buffer.from('R  renamed.txt\0'), Buffer.from([0xc3, 0x28, 0])]),
  ]) assert.throws(() => parsePorcelainV1Z(raw));
});

test('porcelain-v1-z parser admits only the closed XY status set', () => {
  const allowed = new Set([
    ' M', ' A', ' T', ' D', ' R', ' C', 'M ', 'MM', 'MT', 'MD', 'T ', 'TM', 'TT', 'TD', 'A ', 'AM', 'AT', 'AD', 'D ',
    'R ', 'RM', 'RT', 'RD', 'C ', 'CM', 'CT', 'CD',
    'DD', 'AU', 'UD', 'UA', 'DU', 'AA', 'UU', '??',
  ]);
  const alphabet = [' ', 'M', 'T', 'D', 'A', 'R', 'C', 'U', '?', '!'];
  for (const first of alphabet) for (const second of alphabet) {
    const status = first + second;
    const raw = Buffer.from(status + ' path.txt\0' + ((first === 'R' || first === 'C' || second === 'R' || second === 'C') ? 'source.txt\0' : ''));
    if (allowed.has(status)) assert.doesNotThrow(() => parsePorcelainV1Z(raw), status);
    else assert.throws(() => parsePorcelainV1Z(raw), status);
  }
});

test('dirty baseline permits P01 scope while preserving a real temporary Git baseline', () => {
  const root = mkdtempSync(join(tmpdir(), 'vestrace-p01-dirty-git-'));
  const git = (...args) => {
    const result = spawnSync('git', ['-c', 'core.excludesFile=', ...args], { cwd: root, encoding: null });
    assert.equal(result.status, 0, result.stderr.toString('utf8'));
    return result.stdout;
  };
  const write = (relative, text) => {
    const path = join(root, relative);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, text);
  };
  const dirtyFiles = (raw) => parsePorcelainV1Z(raw).map(({ path, status }) => {
    const fullPath = join(root, path);
    if (!existsSync(fullPath)) return { bytes: 0, path, sha256: 'absent', status };
    const bytes = readFileSync(fullPath);
    return { bytes: bytes.length, path, sha256: createHash('sha256').update(bytes).digest('hex'), status };
  });
  try {
    git('init');
    git('config', 'user.email', 'p01@example.test');
    git('config', 'user.name', 'P01 fixture');
    for (const path of PROTECTED_AUTHORITY_PATHS) write(path, `protected:${path}\n`);
    write('Cargo.lock', 'scope baseline\n');
    write('scripts/p01-scope.mjs', 'scope baseline\n');
    write('outside-clean.txt', 'clean\n');
    write('outside-preserved.txt', 'clean\n');
    git('add', '.');
    git('commit', '-m', 'fixture baseline');

    write('outside-preserved.txt', 'captured baseline dirty\n');
    write('outside-intent-to-add.txt', 'intent to add baseline dirty\n');
    git('add', '-N', 'outside-intent-to-add.txt');
    const raw = git('status', '--porcelain=v1', '-z', '--untracked-files=all');
    assert.equal(raw.includes(Buffer.from(' A outside-intent-to-add.txt\0')), true);
    const preflight = {
      change_scope_paths: CHANGE_SCOPE_PATHS,
      dirty_files: dirtyFiles(raw),
      head: git('rev-parse', 'HEAD').toString('ascii').trim(),
      protected_authority_digests: PROTECTED_AUTHORITY_PATHS.map((path) => {
        const bytes = readFileSync(join(root, path));
        return { bytes: bytes.length, path, sha256: createHash('sha256').update(bytes).digest('hex') };
      }),
      protected_authority_paths: PROTECTED_AUTHORITY_PATHS,
      status_porcelain_v1_z_base64: raw.toString('base64'),
      status_porcelain_v1_z_sha256: createHash('sha256').update(raw).digest('hex'),
    };
    assert.equal(preflight.dirty_files.find((entry) => entry.path === 'outside-intent-to-add.txt')?.status, ' A');
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    write('Cargo.lock', 'scope changed\n');
    write('schemas/protocol-lock.json', '{}\n');
    rmSync(join(root, 'scripts/p01-scope.mjs'));
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    write('outside-clean.txt', 'newly dirty changed\n');
    assert.notDeepEqual(verifyDirtyBaseline(root, preflight), []);
    write('outside-clean.txt', 'clean\n');
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    rmSync(join(root, 'outside-clean.txt'));
    assert.notDeepEqual(verifyDirtyBaseline(root, preflight), []);
    write('outside-clean.txt', 'clean\n');
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    write(PROTECTED_AUTHORITY_PATHS[0], 'tampered\n');
    assert.notDeepEqual(verifyDirtyBaseline(root, preflight), []);
    write(PROTECTED_AUTHORITY_PATHS[0], `protected:${PROTECTED_AUTHORITY_PATHS[0]}\n`);
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    write('outside-new.txt', 'new dirty\n');
    assert.notDeepEqual(verifyDirtyBaseline(root, preflight), []);
    rmSync(join(root, 'outside-new.txt'));
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    write('outside-preserved.txt', 'changed after capture\n');
    assert.notDeepEqual(verifyDirtyBaseline(root, preflight), []);
    write('outside-preserved.txt', 'captured baseline dirty\n');
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    git('add', 'outside-preserved.txt');
    assert.notDeepEqual(verifyDirtyBaseline(root, preflight), []);
    git('reset', '--', 'outside-preserved.txt');
    assert.deepEqual(verifyDirtyBaseline(root, preflight), []);

    rmSync(join(root, 'outside-preserved.txt'));
    assert.notDeepEqual(verifyDirtyBaseline(root, preflight), []);
  } finally { rmSync(root, { force: true, recursive: true }); }
});
