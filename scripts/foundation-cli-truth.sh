#!/usr/bin/env bash
set -euo pipefail

binary="${1:-target/debug/vestrace}"
unavailable_url="postgres://p0-user:p0-secret-password@127.0.0.1:9/vestrace_p0_unavailable"

if [[ ! -x "$binary" ]]; then
  echo "vestrace binary is unavailable: $binary" >&2
  exit 1
fi

run_expect_failure() {
  local command="$1"
  local expected="$2"
  local output
  local status

  set +e
  output=$(VESTRACE_DATABASE__URL="$unavailable_url" "$binary" "$command" 2>&1)
  status=$?
  set -e

  if [[ "$status" -eq 0 ]]; then
    echo "$command unexpectedly succeeded" >&2
    exit 1
  fi

  if ! grep -Fq "$expected" <<<"$output"; then
    echo "$command returned an unexpected error: $output" >&2
    exit 1
  fi

  if grep -Fq "p0-secret-password" <<<"$output" || grep -Fq "$unavailable_url" <<<"$output"; then
    echo "$command leaked database credentials" >&2
    exit 1
  fi
}

run_expect_failure worker "worker command is not implemented"
run_expect_failure mcp "mcp command is not implemented"
run_expect_failure doctor "doctor command is not implemented"
run_expect_failure rebuild "rebuild command is not implemented"
run_expect_failure migrate "database is unavailable"

echo "CLI truthfulness checks passed"
