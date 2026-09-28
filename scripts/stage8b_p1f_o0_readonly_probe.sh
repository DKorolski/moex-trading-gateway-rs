#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: stage8b_p1f_o0_readonly_probe.sh SSH_PRIVATE_KEY" >&2
  exit 64
fi

key="$1"
target="root@45.150.11.252"
[[ -f "$key" ]] || { echo "stage8b-p1f-o0-probe: FAIL missing SSH key" >&2; exit 64; }

ssh -i "$key" \
  -o BatchMode=yes \
  -o StrictHostKeyChecking=yes \
  -o ConnectTimeout=10 \
  "$target" 'bash -s' <<'REMOTE'
set -euo pipefail

kv() {
  printf '%s=%s\n' "$1" "$2"
}

unit_value() {
  systemctl show "$1" -p "$2" --value
}

unit_hash() {
  local fragment
  fragment="$(unit_value "$1" FragmentPath)"
  if [[ -n "$fragment" && -f "$fragment" && ! -L "$fragment" ]]; then
    sha256sum "$fragment" | awk '{print $1}'
  else
    printf 'ABSENT'
  fi
}

redis_keyspace_digest() {
  local db="$1"
  redis-cli -h 127.0.0.1 -p 6379 -n "$db" --scan --raw |
    LC_ALL=C sort |
    while IFS= read -r key; do
      printf '%s\0' "$key"
      redis-cli -h 127.0.0.1 -p 6379 -n "$db" --raw DUMP "$key"
    done |
    sha256sum |
    awk '{print $1}'
}

kv schema_version 1
kv probe_kind stage8b-p1f-o0-immutable-read-only-target-preflight
kv observed_at_utc "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
kv target_id stage8b-p1f-isolated-vps-1
kv hostname "$(hostname -f 2>/dev/null || hostname)"
kv ipv4 45.150.11.252
kv ssh_ed25519_fingerprint "$(ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256 | awk '{print $2}')"
kv os_id "$(. /etc/os-release; printf '%s' "$ID")"
kv os_version_id "$(. /etc/os-release; printf '%s' "$VERSION_ID")"
kv architecture "$(uname -m)"
kv systemd_major "$(systemd --version | awk 'NR==1{print $2}')"
kv cpu_count "$(getconf _NPROCESSORS_ONLN)"
kv memory_kib "$(awk '/MemTotal/{print $2}' /proc/meminfo)"
kv root_free_kib "$(df -Pk / | awk 'NR==2{print $4}')"
kv ntp_synchronized "$(timedatectl show -p NTPSynchronized --value 2>/dev/null || printf unknown)"
kv redis_cli "$(command -v redis-cli || true)"
kv redis_cli_version "$(redis-cli --version | awk '{print $2}')"
redis_server_version="$(
  redis-cli -h 127.0.0.1 -p 6379 --raw INFO server |
    awk -F: '$1 == "redis_version" {gsub(/\r/, "", $2); count += 1; value = $2} END {if (count != 1 || value == "") exit 1; print value}'
)"
[[ "$redis_server_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "invalid Redis server version evidence" >&2
  exit 1
}
kv redis_server_version "$redis_server_version"
kv redis_ping "$(redis-cli -h 127.0.0.1 -p 6379 -n 15 --raw PING)"
kv redis_bind "$(redis-cli -h 127.0.0.1 -p 6379 --raw CONFIG GET bind | tail -1)"
kv redis_protected_mode "$(redis-cli -h 127.0.0.1 -p 6379 --raw CONFIG GET protected-mode | tail -1)"
kv redis_databases "$(redis-cli -h 127.0.0.1 -p 6379 --raw CONFIG GET databases | tail -1)"
kv redis_appendonly "$(redis-cli -h 127.0.0.1 -p 6379 --raw CONFIG GET appendonly | tail -1)"
kv redis_listeners "$(ss -H -lnt | awk '$4 ~ /:6379$/ {print $4}' | LC_ALL=C sort | paste -sd, -)"
kv redis_listener_sha256 "$(ss -H -lnt | awk '$4 ~ /:6379$/ {print $4}' | LC_ALL=C sort | sha256sum | awk '{print $1}')"
kv redis_db0_size "$(redis-cli -h 127.0.0.1 -p 6379 -n 0 --raw DBSIZE)"
kv redis_db0_keyspace_sha256 "$(redis_keyspace_digest 0)"
kv redis_db15_size "$(redis-cli -h 127.0.0.1 -p 6379 -n 15 --raw DBSIZE)"
kv redis_db15_keyspace_sha256 "$(redis_keyspace_digest 15)"

for unit in moex-finam-paper-runtime.service moex-finam-paper-ws.service; do
  prefix="${unit%.service}"
  prefix="${prefix//-/_}"
  kv "${prefix}_load_state" "$(unit_value "$unit" LoadState)"
  kv "${prefix}_active_state" "$(unit_value "$unit" ActiveState)"
  kv "${prefix}_sub_state" "$(unit_value "$unit" SubState)"
  kv "${prefix}_unit_file_state" "$(unit_value "$unit" UnitFileState)"
  kv "${prefix}_fragment_sha256" "$(unit_hash "$unit")"
  kv "${prefix}_execstart_sha256" "$(systemctl show "$unit" -p ExecStart --value | sha256sum | awk '{print $1}')"
done

if getent passwd moex-p1-paper >/dev/null; then
  kv p1_service_user_present true
else
  kv p1_service_user_present false
fi
if getent group moex-p1-paper >/dev/null; then
  kv p1_service_group_present true
else
  kv p1_service_group_present false
fi

paths=(
  /usr/local/libexec/moex/stage8b-p1-paper-supervisor
  /usr/local/share/moex/stage8b-p1e/installation-v1.json
  /etc/systemd/system/moex-finam-p1-paper.service
  /etc/systemd/system/moex-finam-p1-paper-bootstrap.service
  /etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service
  /usr/lib/sysusers.d/moex-finam-p1-paper.conf
  /usr/lib/tmpfiles.d/moex-finam-p1-paper.conf
  /etc/moex-finam-p1-paper
  /etc/moex-finam-p1-paper/bootstrap
  /etc/moex-finam-p1-paper/credentials
  /etc/moex-finam-p1-paper/supervisor.json
  /etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json
  /etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key
  /var/lib/moex-finam-p1-paper
  /var/lib/moex-finam-p1-paper/state
  /var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine
  /var/lib/moex-finam-p1-paper-control
)
for index in "${!paths[@]}"; do
  path="${paths[$index]}"
  kv "p1_path_${index}" "$path"
  if [[ -e "$path" || -L "$path" ]]; then
    kv "p1_path_${index}_present" true
  else
    kv "p1_path_${index}_present" false
  fi
done

# P1_SYSTEMD_BEHAVIOR_BEGIN
if ! p1_unit_file_inventory="$(systemctl list-unit-files --no-legend --no-pager --type=service)"; then
  echo "P1 unit-file inventory query failed" >&2
  exit 1
fi
p1_unit_file_state() {
  local unit="$1"
  awk -v expected="$unit" '
    $1 == expected {count += 1; state = $2}
    END {
      if (count > 1) exit 2
      if (count == 1) print state
      else print "not-found"
    }
  ' <<<"$p1_unit_file_inventory"
}

p1_units=(
  moex-finam-p1-paper.service
  moex-finam-p1-paper-bootstrap.service
)
for index in "${!p1_units[@]}"; do
  unit="${p1_units[$index]}"
  if ! load_state="$(unit_value "$unit" LoadState)"; then
    echo "P1 LoadState query failed: $unit" >&2
    exit 1
  fi
  if ! active_state="$(unit_value "$unit" ActiveState)"; then
    echo "P1 ActiveState query failed: $unit" >&2
    exit 1
  fi
  if ! unit_file_state="$(p1_unit_file_state "$unit")"; then
    echo "P1 UnitFileState query failed: $unit" >&2
    exit 1
  fi
  if ! fragment_path="$(unit_value "$unit" FragmentPath)"; then
    echo "P1 FragmentPath query failed: $unit" >&2
    exit 1
  fi
  kv "p1_unit_${index}_name" "$unit"
  kv "p1_unit_${index}_kind" regular
  kv "p1_unit_${index}_load_state" "$load_state"
  kv "p1_unit_${index}_active_state" "$active_state"
  kv "p1_unit_${index}_unit_file_state" "$unit_file_state"
  kv "p1_unit_${index}_fragment_path" "$fragment_path"
done

template=moex-finam-p1-paper-bootstrap-recover@.service
if ! template_unit_file_state="$(p1_unit_file_state "$template")"; then
  echo "P1 template UnitFileState query failed: $template" >&2
  exit 1
fi
kv p1_unit_2_name "$template"
kv p1_unit_2_kind template
kv p1_unit_2_load_state not-applicable
kv p1_unit_2_active_state not-applicable
kv p1_unit_2_unit_file_state "$template_unit_file_state"
kv p1_unit_2_fragment_path ""

if ! recovery_raw="$(
  systemctl list-units --all --type=service --plain --no-legend \
    'moex-finam-p1-paper-bootstrap-recover@*.service'
)"; then
  echo "P1 recovery-instance query failed" >&2
  exit 1
fi
if ! recovery_instances="$(
  printf '%s\n' "$recovery_raw" |
    awk 'NF {print $1}' |
    LC_ALL=C sort -u |
    paste -sd, -
)"; then
  echo "P1 recovery-instance normalization failed" >&2
  exit 1
fi
if [[ -n "$recovery_instances" ]]; then
  if ! recovery_count="$(awk -F, '{print NF}' <<<"$recovery_instances")"; then
    echo "P1 recovery-instance count failed" >&2
    exit 1
  fi
else
  recovery_count=0
fi
kv p1_recovery_instances_count "$recovery_count"
kv p1_recovery_instances "$recovery_instances"
kv p1_recovery_instances_sha256 "$(printf '%s' "$recovery_instances" | sha256sum | awk '{print $1}')"
kv p1_systemd_query_ok true
# P1_SYSTEMD_BEHAVIOR_END

kv remote_mutation_performed false
REMOTE
