#!/usr/bin/env bash
set -Eeuo pipefail

: "${POSTGRES_DB:?POSTGRES_DB is required}"
: "${POSTGRES_USER:?POSTGRES_USER is required}"
: "${VESTRACE_RUNTIME_PASSWORD:?VESTRACE_RUNTIME_PASSWORD is required}"

psql \
  --username "$POSTGRES_USER" \
  --dbname "$POSTGRES_DB" \
  --no-password \
  --no-psqlrc \
  --set=ON_ERROR_STOP=1 \
  --set=database_name="$POSTGRES_DB" \
  --set=runtime_password="$VESTRACE_RUNTIME_PASSWORD" <<'SQL'
SELECT 'CREATE ROLE vestrace LOGIN'
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'vestrace')
\gexec

ALTER ROLE vestrace WITH
  LOGIN
  NOSUPERUSER
  NOCREATEDB
  NOCREATEROLE
  NOINHERIT
  NOREPLICATION
  NOBYPASSRLS
  PASSWORD :'runtime_password';

CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

SELECT format('ALTER DATABASE %I OWNER TO vestrace', :'database_name')
\gexec

ALTER SCHEMA public OWNER TO vestrace;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT USAGE, CREATE ON SCHEMA public TO vestrace;
SQL
