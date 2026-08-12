# Q29 Diagnostics Lease Schema Correction

## Goal

Restore the read-only doctor path against the canonical `run_leases` schema
discovered during the final runtime audit.

## Design

1. Add a regression contract requiring `lease_until` in the diagnostics query.
2. Replace the stale `expires_at` reference without changing persistence or lease behavior.
3. Re-run focused, workspace, compile, and safe Docker read-only checks.

## Verification gate

- RED source-contract test observed before the correction.
- Focused schema-contract test passes after the correction.
- Workspace tests, no-run compilation, and scoped diff check pass.
- No existing PostgreSQL volume is stopped or deleted by the verification.
