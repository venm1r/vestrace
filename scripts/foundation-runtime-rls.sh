#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

umask 077
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/vestrace-foundation-runtime-rls.XXXXXXXXXX")"
trap 'rm -rf -- "$temp_dir"' EXIT

seed_sql="$temp_dir/seed.sql"
check_sql="$temp_dir/check.sql"
cleanup_sql="$temp_dir/cleanup.sql"

cat >"$seed_sql" <<'SQL'
BEGIN;
DELETE FROM workspaces
WHERE id IN (
  '52000000-0000-0000-0000-000000000001',
  '52000000-0000-0000-0000-000000000002',
  '52000000-0000-0000-0000-000000000003'
)
OR slug IN ('runtime-rls-a', 'runtime-rls-b', 'runtime-rls-cross-write');
INSERT INTO workspaces (id, slug) VALUES
  ('52000000-0000-0000-0000-000000000001', 'runtime-rls-a'),
  ('52000000-0000-0000-0000-000000000002', 'runtime-rls-b');
COMMIT;
SQL

cat >"$check_sql" <<'SQL'
DO $check_role$
DECLARE
  is_superuser BOOLEAN;
  bypasses_rls BOOLEAN;
BEGIN
  IF current_user <> 'vestrace' THEN
    RAISE EXCEPTION 'runtime database user mismatch';
  END IF;

  SELECT rolsuper, rolbypassrls
  INTO is_superuser, bypasses_rls
  FROM pg_roles
  WHERE rolname = current_user;

  IF is_superuser OR bypasses_rls THEN
    RAISE EXCEPTION 'runtime database role bypasses RLS';
  END IF;

  IF pg_has_role(current_user, 'vestrace_bootstrap', 'MEMBER') THEN
    RAISE EXCEPTION 'runtime database role inherits bootstrap privileges';
  END IF;
END
$check_role$;

BEGIN;
SET LOCAL vestrace.workspace_id = '52000000-0000-0000-0000-000000000001';
DO $check_scoped_access$
DECLARE
  visible_count BIGINT;
  changed_count BIGINT;
BEGIN
  SELECT count(*) INTO visible_count FROM workspaces;
  IF visible_count <> 1 THEN
    RAISE EXCEPTION 'runtime workspace scope exposed an unexpected row count';
  END IF;

  IF EXISTS (
    SELECT 1 FROM workspaces
    WHERE id = '52000000-0000-0000-0000-000000000002'
  ) THEN
    RAISE EXCEPTION 'runtime workspace scope exposed a cross-workspace row';
  END IF;

  UPDATE workspaces
  SET slug = slug
  WHERE id = '52000000-0000-0000-0000-000000000001';
  GET DIAGNOSTICS changed_count = ROW_COUNT;
  IF changed_count <> 1 THEN
    RAISE EXCEPTION 'runtime same-workspace write was not visible';
  END IF;

  BEGIN
    INSERT INTO workspaces (id, slug) VALUES
      ('52000000-0000-0000-0000-000000000003', 'runtime-rls-cross-write');
    RAISE EXCEPTION 'runtime cross-workspace write unexpectedly succeeded';
  EXCEPTION
    WHEN insufficient_privilege THEN NULL;
  END;
END
$check_scoped_access$;
ROLLBACK;

BEGIN;
DO $check_missing_context$
DECLARE
  visible_count BIGINT;
BEGIN
  SELECT count(*) INTO visible_count FROM workspaces;
  IF visible_count <> 0 THEN
    RAISE EXCEPTION 'missing workspace context exposed runtime rows';
  END IF;
END
$check_missing_context$;
ROLLBACK;
SQL

cat >"$cleanup_sql" <<'SQL'
DELETE FROM workspaces
WHERE id IN (
  '52000000-0000-0000-0000-000000000001',
  '52000000-0000-0000-0000-000000000002',
  '52000000-0000-0000-0000-000000000003'
)
OR slug IN ('runtime-rls-a', 'runtime-rls-b', 'runtime-rls-cross-write');
SQL

admin_psql() {
  timeout 20s docker compose exec -T postgres sh -ec '
    export PGPASSWORD="$POSTGRES_PASSWORD" PGCONNECT_TIMEOUT=5
    exec psql \
      --host=postgres \
      --username="$POSTGRES_USER" \
      --dbname="$POSTGRES_DB" \
      --no-psqlrc \
      --quiet \
      --set=ON_ERROR_STOP=1
  '
}

runtime_psql() {
  timeout 20s docker compose exec -T postgres sh -ec '
    export PGCONNECT_TIMEOUT=5
    exec psql \
      "$VESTRACE_RUNTIME_DATABASE_URL" \
      --no-psqlrc \
      --quiet \
      --set=ON_ERROR_STOP=1
  '
}

cleanup() {
  local exit_code=$?
  local cleanup_code=0
  trap - EXIT
  set +e
  admin_psql <"$cleanup_sql" >/dev/null
  cleanup_code=$?
  rm -rf -- "$temp_dir"
  if [[ "$cleanup_code" != 0 && "$exit_code" == 0 ]]; then
    printf 'runtime RLS cleanup failed\n' >&2
    exit_code=1
  fi
  exit "$exit_code"
}
trap cleanup EXIT

admin_psql <"$seed_sql" >/dev/null
runtime_psql <"$check_sql" >/dev/null

printf 'foundation runtime role and RLS checks passed\n'
