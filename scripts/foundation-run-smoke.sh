#!/usr/bin/env bash
set -euo pipefail

base_url="${1:-http://127.0.0.1:8080}"
workspace_id="10000000-0000-0000-0000-000000000001"
principal_id="10000000-0000-0000-0000-000000000002"
other_workspace_id="10000000-0000-0000-0000-000000000003"
other_principal_id="10000000-0000-0000-0000-000000000004"

# Local development identity is seeded explicitly outside the HTTP adapter.
docker compose exec -T postgres \
  psql --username vestrace_bootstrap --dbname vestrace --set ON_ERROR_STOP=1 <<SQL
INSERT INTO workspaces (id, slug)
VALUES
  ('$workspace_id', 'p0-smoke'),
  ('$other_workspace_id', 'p0-smoke-other')
ON CONFLICT (id) DO NOTHING;

INSERT INTO principals (id, workspace_id, identifier)
VALUES
  ('$principal_id', '$workspace_id', 'p0-smoke-operator'),
  ('$other_principal_id', '$other_workspace_id', 'p0-smoke-other-operator')
ON CONFLICT (id) DO NOTHING;
SQL

umask 077
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/vestrace-run-smoke.XXXXXXXXXX")"
trap 'rm -rf -- "$temp_dir"' EXIT
create_body="$temp_dir/create.json"
list_body="$temp_dir/list.json"
get_body="$temp_dir/get.json"
other_body="$temp_dir/other.json"

common_headers=(
  --header "x-workspace-id: $workspace_id"
  --header "x-principal-id: $principal_id"
  --header 'content-type: application/json'
)

status="$(curl --silent --show-error --output "$create_body" --write-out '%{http_code}' \
  "${common_headers[@]}" \
  --request POST \
  --data '{"title":"P0 compose smoke run"}' \
  "${base_url}/v1/runs")"
[[ "$status" == "201" ]] || {
  printf 'run create failed (HTTP %s): %s\n' "$status" "$(cat "$create_body")" >&2
  exit 1
}

run_id="$(python3 - "$create_body" <<'PY'
import json
import sys
with open(sys.argv[1], encoding='utf-8') as handle:
    body = json.load(handle)
assert body['title'] == 'P0 compose smoke run'
assert body['status'] == 'created'
assert body['version'] == 1
print(body['id'])
PY
)"

event_count="$(docker compose exec -T postgres \
  psql --username vestrace_bootstrap --dbname vestrace --tuples-only --no-align \
  --command "SELECT count(*) FROM run_events WHERE workspace_id = '$workspace_id' AND run_id = '$run_id' AND sequence = 1 AND event_type = 'run.created';")"
[[ "$event_count" == "1" ]] || {
  printf 'HTTP run create did not append one canonical run.created event (count=%s)\n' "$event_count" >&2
  exit 1
}

status="$(curl --silent --show-error --output "$list_body" --write-out '%{http_code}' \
  "${common_headers[@]}" \
  "${base_url}/v1/runs")"
[[ "$status" == "200" ]] || {
  printf 'run list failed (HTTP %s): %s\n' "$status" "$(cat "$list_body")" >&2
  exit 1
}

python3 - "$list_body" "$run_id" <<'PY'
import json
import sys
with open(sys.argv[1], encoding='utf-8') as handle:
    body = json.load(handle)
assert any(run['id'] == sys.argv[2] for run in body)
PY

status="$(curl --silent --show-error --output "$get_body" --write-out '%{http_code}' \
  "${common_headers[@]}" \
  "${base_url}/v1/runs/${run_id}")"
[[ "$status" == "200" ]] || {
  printf 'run get failed (HTTP %s): %s\n' "$status" "$(cat "$get_body")" >&2
  exit 1
}

python3 - "$get_body" "$run_id" <<'PY'
import json
import sys
with open(sys.argv[1], encoding='utf-8') as handle:
    body = json.load(handle)
assert body['id'] == sys.argv[2]
assert body['title'] == 'P0 compose smoke run'
PY

status="$(curl --silent --show-error --output "$other_body" --write-out '%{http_code}' \
  --header "x-workspace-id: $other_workspace_id" \
  --header "x-principal-id: $other_principal_id" \
  "${base_url}/v1/runs/${run_id}")"
[[ "$status" == "404" ]] || {
  printf 'cross-workspace run lookup was not isolated (HTTP %s): %s\n' "$status" "$(cat "$other_body")" >&2
  exit 1
}

printf 'foundation run smoke check passed\n'
printf 'console identity: VITE_VESTRACE_WORKSPACE_ID=%s VITE_VESTRACE_PRINCIPAL_ID=%s\n' \
  "$workspace_id" "$principal_id"
