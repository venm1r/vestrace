#!/usr/bin/env bash
set -euo pipefail

fail_if_present() {
  local needle="$1"
  shift

  if grep -Fqn "$needle" "$@"; then
    echo "stale or false documentation claim found: $needle" >&2
    grep -Fn "$needle" "$@" >&2 || true
    exit 1
  fi
}

public_docs=(README.md docs/architecture.md docs/getting-started.md docs/database-schema.md)

fail_if_present "five distinct member crates" "${public_docs[@]}"
fail_if_present "migrations/                 # Forward-only SQL schema migrations (0001 - 0016)" "${public_docs[@]}"
fail_if_present "Run background extraction worker" "${public_docs[@]}"
fail_if_present "Run database migration check" "${public_docs[@]}"
fail_if_present "All database migrations (0001 - 0016) are up to date." crates/vestrace-cli/src/commands/migrate.rs
fail_if_present "Worker loop initialized successfully." crates/vestrace-cli/src/commands/worker.rs

echo "documentation truthfulness checks passed"
