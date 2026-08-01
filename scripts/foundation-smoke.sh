#!/usr/bin/env bash
set -euo pipefail

base_url="${1:-http://127.0.0.1:8080}"
attempts=10
umask 077
body_file="${TMPDIR:-/tmp}/vestrace-foundation-smoke.$$.body"
expected_file="${TMPDIR:-/tmp}/vestrace-foundation-smoke.$$.expected"
: >"$body_file"
printf '%s' '{"status":"ok"}' >"$expected_file"
trap 'rm -f "$body_file" "$expected_file"' EXIT

check_endpoint() {
  local endpoint="$1"
  local attempt
  local status
  local body

  for ((attempt = 1; attempt <= attempts; attempt += 1)); do
    status="$(
      curl \
        --silent \
        --connect-timeout 1 \
        --max-time 2 \
        --output "$body_file" \
        --write-out '%{http_code}' \
        "${base_url}${endpoint}" || true
    )"
    body="$(<"$body_file")"

    if [[ "$status" == "200" ]] && cmp --silent "$body_file" "$expected_file"; then
      return 0
    fi

    if ((attempt < attempts)); then
      sleep 1
    fi
  done

  printf 'health check failed for %s (HTTP %s, body %q)\n' \
    "$endpoint" "${status:-unavailable}" "$body" >&2
  return 1
}

check_endpoint /health/live
check_endpoint /health/ready
printf 'foundation smoke check passed\n'
