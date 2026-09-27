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
  systemctl show "$1" -p "$2" --value 2>/dev/null || true
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
kv redis_version "$(redis-cli --version | awk '{print $2}')"
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

paths=(
  /usr/local/libexec/moex/stage8b-p1-paper-supervisor
  /etc/moex-finam-p1-paper
  /var/lib/moex-finam-p1-paper
  /var/lib/moex-finam-p1-paper-control
  /etc/systemd/system/moex-finam-p1-paper.service
  /etc/systemd/system/moex-finam-p1-paper-bootstrap.service
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

kv remote_mutation_performed false
REMOTE
