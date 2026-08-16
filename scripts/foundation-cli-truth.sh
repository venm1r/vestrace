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
  shift
  local expected=()
  local output
  local status

  while [[ "$1" != "--" ]]; do
    expected+=("$1")
    shift
  done
  shift

  set +e
  output=$(VESTRACE_DATABASE__URL="$unavailable_url" "$binary" "$command" "$@" 2>&1)
  status=$?
  set -e

  if [[ "$status" -eq 0 ]]; then
    echo "$command unexpectedly succeeded" >&2
    exit 1
  fi

  for expected in "${expected[@]}"; do
    if ! grep -Fq "$expected" <<<"$output"; then
      echo "$command returned an unexpected error: $output" >&2
      exit 1
    fi
  done

  if grep -Fq "p0-secret-password" <<<"$output" || grep -Fq "$unavailable_url" <<<"$output"; then
    echo "$command leaked database credentials" >&2
    exit 1
  fi
}

run_expect_failure worker "starting vestrace worker" "database is unavailable" --
run_expect_failure mcp "database is unavailable" --
run_expect_failure doctor "Connecting to database at postgres://***@127.0.0.1:9/vestrace_p0_unavailable" "database is unavailable" --
run_expect_failure rebuild "Connecting to database at postgres://***@127.0.0.1:9/vestrace_p0_unavailable" "database is unavailable" -- search-documents
run_expect_failure migrate "database is unavailable" --

echo "CLI truthfulness checks passed"
