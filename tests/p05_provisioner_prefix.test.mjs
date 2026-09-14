import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const provisionerPath = 'docker/postgres/init-runtime-role.sh';
const preflightPath = 'docs/development-evidence/v1-g0-05-preflight.json';
const marker = '# P05 safety supervisor role \u2014 do not move';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

test('P05 provisioner suffix preserves the captured P04 predecessor byte-for-byte', () => {
  const preflight = JSON.parse(readFileSync(preflightPath, 'utf8'));
  const captured = preflight.dirty_files.find((entry) => entry.path === provisionerPath);
  assert.ok(captured, 'P05 preflight must capture the P04 provisioner baseline');

  const current = readFileSync(provisionerPath);
  const markerOffset = current.indexOf(Buffer.from(marker, 'utf8'));
  assert.notEqual(markerOffset, -1, 'P05 suffix marker is required');
  const predecessor = current.subarray(0, markerOffset);
  assert.equal(predecessor.length, captured.bytes);
  assert.equal(sha256(predecessor), captured.sha256);
  assert.equal(current.indexOf(Buffer.from(marker, 'utf8'), markerOffset + 1), -1, 'P05 marker must be unique');
});

test('P05 guarded journal digest hashes only the canonical journal payload', () => {
  const provisioner = readFileSync(provisionerPath, 'utf8');
  const expected = "expected_journal := digest(journal_payload, 'sha256');";
  assert.equal(
    provisioner.split(expected).length - 1,
    2,
    'initializer and generation registration must use the domain digest model',
  );
  assert.equal(
    provisioner.includes("journal_payload || public.vestrace_p05_field(p_journal_signature)"),
    false,
    'detached journal signatures must not be included in SafetyJournalDigest',
  );
});

test('P05 guarded journal calls resolve the declared SMALLINT event-kind parameter', () => {
  const provisioner = readFileSync(provisionerPath, 'utf8');

  assert.match(
    provisioner,
    /vestrace_p05_journal_payload\([\s\S]*?previous, 0::SMALLINT, p_generation/,
    'initializer must not pass an untyped integer literal to the SMALLINT canonical payload helper',
  );
  assert.match(
    provisioner,
    /vestrace_p05_journal_payload\([\s\S]*?current_state\.journal_digest, 1::SMALLINT, p_generation/,
    'registration must not pass an untyped integer literal to the SMALLINT canonical payload helper',
  );
});

test('P05-B archive installer admits every declared safety event kind', () => {
  const provisioner = readFileSync(provisionerPath, 'utf8');
  const installerStart = provisioner.indexOf(
    'CREATE OR REPLACE FUNCTION public.vestrace_install_p05_backup_archive_schema()',
  );
  assert.notEqual(installerStart, -1, 'P05-B archive installer is required');
  const archiveInstaller = provisioner.slice(installerStart);

  assert.match(
    archiveInstaller,
    /CHECK \(event_kind IN \(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10\)\)/,
    'the archive event family must be writable only through the complete guarded journal range',
  );
});

test('P05-B base capture keeps a guarded first-base boundary', () => {
  const provisioner = readFileSync(provisionerPath, 'utf8');
  const migration = readFileSync('migrations/0211_managed_backup_base_capture.sql', 'utf8');
  for (const trigger of [
    'managed_backup_base_checkpoint_required',
    'managed_backup_hold_requires_base_checkpoint',
    'managed_backup_lifecycle_requires_base_checkpoint',
  ]) assert.match(provisioner, new RegExp(`CREATE TRIGGER ${trigger} `));

  assert.match(
    migration,
    /SELECT public\.vestrace_install_p05_base_capture_guards\(\);/,
    'the restricted migration role must invoke the guarded trigger installer',
  );

  assert.match(
    provisioner,
    /NEW\.ordinal = 1 AND NEW\.object_kind <> 0[\s\S]*?NEW\.ordinal > 1 AND NEW\.object_kind = 0/,
    'the guarded archive member path must require one base checkpoint at ordinal one',
  );
  assert.match(
    provisioner,
    /NEW\.object_kind = 1[\s\S]*?timeline <> NEW\.timeline OR NEW\.start_lsn < start_lsn/,
    'a WAL descriptor must retain the first base checkpoint timeline and start LSN ancestry',
  );
});

test('P05 verifier extension resolves MODULE_PATHNAME to its installed library', () => {
  const control = readFileSync('docker/postgres/vestrace_safety_verify/vestrace_safety_verify.control', 'utf8');
  assert.match(control, /^module_pathname = '\$libdir\/vestrace_safety_verify'$/m);
});

test('P05 verifier links libsodium into the extension shared object', () => {
  const makefile = readFileSync('docker/postgres/vestrace_safety_verify/Makefile', 'utf8');
  assert.match(makefile, /^MODULE_big = vestrace_safety_verify$/m);
  assert.match(makefile, /^OBJS = vestrace_safety_verify\.o$/m);
  assert.match(makefile, /^SHLIB_LINK = -lsodium$/m);
});

test('P05 role provisioner waits for an authenticated PostgreSQL query before role changes', () => {
  const compose = readFileSync('docker-compose.yml', 'utf8');
  const start = compose.indexOf('  vestrace-role-provision:\n');
  const end = compose.indexOf('\n  vestrace-migrate-history:\n', start);
  assert.notEqual(start, -1, 'role provisioner service is required');
  assert.notEqual(end, -1, 'role provisioner service boundary is required');
  const provisioner = compose.slice(start, end);

  assert.match(provisioner, /attempts=60/);
  assert.match(provisioner, /psql --username "\$\$POSTGRES_USER" --dbname "\$\$POSTGRES_DB"[^\n]*-c 'SELECT 1'/);
  assert.match(provisioner, /exec \/opt\/vestrace\/init-runtime-role\.sh/);
  assert.match(provisioner, /exit 1/);
});

test('P05-B initialization admits authenticated physical-base-backup replication only on the database instance', () => {
  const provisioner = readFileSync(provisionerPath, 'utf8');
  assert.match(
    provisioner,
    /if \[ -f "\$PGDATA\/PG_VERSION" \] && ! grep -qxF 'host replication all all scram-sha-256' "\$PGDATA\/pg_hba\.conf"; then/,
    'the standalone role-provisioner must not mutate an unmounted PGDATA directory',
  );
  assert.match(provisioner, /printf '%s\\n' 'host replication all all scram-sha-256' >> "\$PGDATA\/pg_hba\.conf"/);
  assert.match(provisioner, /SELECT pg_reload_conf\(\)/);
});

test('P05-C refusal installer remains callable for the next forward-only migration', () => {
  const provisioner = readFileSync(provisionerPath, 'utf8');
  const migration = readFileSync('migrations/0214_managed_restore_safety_events.sql', 'utf8');
  const start = provisioner.indexOf(
    'CREATE OR REPLACE FUNCTION public.vestrace_install_p05_restore_refusal_guards()',
  );
  const end = provisioner.indexOf('ALTER FUNCTION public.vestrace_install_p05_restore_refusal_guards()', start);
  assert.notEqual(start, -1, 'P05-C refusal installer is required');
  assert.notEqual(end, -1, 'P05-C refusal installer ownership boundary is required');
  const body = provisioner.slice(start, end);

  assert.doesNotMatch(
    body,
    /REVOKE ALL ON FUNCTION public\.vestrace_install_p05_restore_refusal_guards\(\) FROM PUBLIC, vestrace;/,
    'one invocation must not revoke the grant required by the next P05-C migration',
  );
  assert.match(migration, /SELECT public\.vestrace_install_p05_restore_refusal_guards\(\);/);
});
