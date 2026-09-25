#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "FAIL stage8b-p1f-multi-uid reason=linux-required" >&2
  exit 2
fi
if [[ "$(id -u)" != "0" ]]; then
  echo "FAIL stage8b-p1f-multi-uid reason=root-required" >&2
  exit 2
fi

service_uid=65534
service_gid=65534
service_user="$(getent passwd "$service_uid" | cut -d: -f1)"
if [[ -z "$service_user" ]]; then
  echo "FAIL stage8b-p1f-multi-uid reason=service-uid-missing" >&2
  exit 2
fi

scratch="$(mktemp -d /tmp/stage8b-p1f-multi-uid.XXXXXX)"
control_root="$scratch/moex-finam-p1-paper-control"
moved_root="$scratch/moex-finam-p1-paper-control.moved"
cleanup() {
  rm -rf -- "$control_root" "$moved_root" "$scratch"
}
trap cleanup EXIT

chmod 0755 "$scratch"
install -d -o root -g "$service_gid" -m 0750 "$control_root"
install -d -o root -g "$service_gid" -m 0750 "$control_root/authority"
install -o root -g "$service_gid" -m 0440 /dev/null "$control_root/authority/history-head.json"
install -o root -g root -m 0600 /dev/null "$control_root/.guardian.lock"
printf '%s\n' '{"state":"GENESIS_ACTIVATED"}' > "$control_root/authority/.history-head.next"
chown root:"$service_gid" "$control_root/authority/.history-head.next"
chmod 0440 "$control_root/authority/.history-head.next"
mv "$control_root/authority/.history-head.next" "$control_root/authority/history-head.json"

run_as_service() {
  runuser -u "$service_user" -- "$@"
}

expect_denied() {
  local case_name="$1"
  shift
  if run_as_service "$@" >/dev/null 2>&1; then
    echo "FAIL stage8b-p1f-multi-uid case=$case_name reason=unexpected-success" >&2
    exit 1
  fi
  echo "PASS stage8b-p1f-multi-uid case=$case_name"
}

run_as_service test -r "$control_root/authority/history-head.json"
expect_denied unlink-authority rm -f "$control_root/authority/history-head.json"
expect_denied rename-authority mv "$control_root/authority/history-head.json" "$control_root/authority/head.moved"
expect_denied mutate-authority chmod 0640 "$control_root/authority/history-head.json"
expect_denied create-authority mkdir "$control_root/authority/injected"
expect_denied parent-substitution mv "$control_root" "$moved_root"
expect_denied guardian-lock-read test -r "$control_root/.guardian.lock"

test -f "$control_root/authority/history-head.json"
test "$(stat -c '%u:%g:%a' "$control_root")" = "0:${service_gid}:750"
test "$(stat -c '%u:%g:%a' "$control_root/authority/history-head.json")" = "0:${service_gid}:440"
echo "PASS stage8b-p1f-multi-uid root-transition-and-custody"
