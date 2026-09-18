#!/usr/bin/env bash
set -euo pipefail

base_url="${1:-http://127.0.0.1:8080}"
attempts=10
umask 077
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/vestrace-foundation-smoke.XXXXXXXXXX")"
trap 'rm -rf -- "$temp_dir"' EXIT
body_file="$temp_dir/body"
expected_file="$temp_dir/expected"
printf '%s' '{"status":"ok"}' >"$expected_file"

check_endpoint() {
  local endpoint="$1"
  local attempt
  local status

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
    if [[ "$status" == "200" ]] && cmp --silent "$body_file" "$expected_file"; then
      return 0
    fi

    if ((attempt < attempts)); then
      sleep 1
    fi
  done

  printf 'health check failed for %s (HTTP %s; expected HTTP 200 with exact safe JSON body)\n' \
    "$endpoint" "${status:-unavailable}" >&2
  return 1
}

check_endpoint /health/live
check_endpoint /health/ready
printf 'foundation smoke check passed\n'
