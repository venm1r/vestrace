# Q3 CLI qualification persistence delta — 2026-08-12

## Scope

Q3 wires the Q1 `conformance bundle` command to the Q2 durable repository through an explicit `--persist` flag.

Command behavior:

- without `--persist`, bundle generation remains database-free and keeps the Q1 artifact/reporting behavior;
- with `--persist`, the command writes the requested JSON artifact first;
- it then loads the standard CLI `AppConfig`, connects through `PgStore`, runs and verifies migrations, and inserts through `PgQualificationRepository`;
- a database/configuration failure returns an error after the local artifact exists;
- a successfully persisted failed/skipped qualification still returns a non-zero qualification result and cannot be promoted to pass.

The persistence delegation is tested against the application `QualificationRepository` port, while the CLI regression verifies artifact-first behavior when database configuration is unavailable.

## Verification

Passed:

- persistence delegation unit test: 1/1;
- Q3 CLI artifact-first regression: 1/1;
- Q1 CLI regression: 1/1;
- Q1 domain regression: 5/5.

Live PostgreSQL insert/migration verification still requires the standard `VESTRACE__DATABASE__URL` environment configuration. Q3 therefore proves composition and failure ordering in the current environment, but does not claim a successful deployed persistence round-trip.

## Explicit non-claims

Q3 does not establish:

- automatic persistence from worker/server qualification runs;
- signed or attested evidence;
- deployment identity/environment approval;
- a passing CORE/TRUSTED/FEDERATION profile;
- v1.0 readiness.
