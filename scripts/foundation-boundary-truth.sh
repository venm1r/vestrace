#!/usr/bin/env bash
set -euo pipefail

if grep -RInE 'sqlx::|vestrace_infrastructure' crates/vestrace-http/src; then
  echo "HTTP source must depend on application ports, not SQLx or infrastructure adapters" >&2
  exit 1
fi

if ! grep -RIlq 'vestrace_application' crates/vestrace-http/src; then
  echo "HTTP source does not expose an application-layer boundary" >&2
  exit 1
fi

echo "HTTP boundary checks passed"
