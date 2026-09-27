#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: stage8b_p1f_o1_readonly_probe.sh SSH_PRIVATE_KEY" >&2
  exit 64
fi

key="$1"
target="root@45.150.11.252"
[[ -f "$key" ]] || { echo "stage8b-p1f-o1-probe: FAIL missing SSH key" >&2; exit 64; }

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

file_evidence() {
  local index="$1"
  local path="$2"
  local metadata
  [[ -f "$path" && ! -L "$path" ]] || { echo "invalid managed file: $path" >&2; exit 1; }
  metadata="$(stat -c '%u %g %a %h %F' "$path")"
  kv "managed_${index}_path" "$path"
  kv "managed_${index}_sha256" "$(sha256sum "$path" | awk '{print $1}')"
  kv "managed_${index}_uid" "$(awk '{print $1}' <<<"$metadata")"
  kv "managed_${index}_gid" "$(awk '{print $2}' <<<"$metadata")"
  kv "managed_${index}_mode" "$(awk '{print $3}' <<<"$metadata")"
  kv "managed_${index}_nlink" "$(awk '{print $4}' <<<"$metadata")"
  kv "managed_${index}_type" "$(cut -d' ' -f5- <<<"$metadata")"
}

directory_evidence() {
  local index="$1"
  local path="$2"
  local metadata
  [[ -d "$path" && ! -L "$path" ]] || { echo "invalid persistent directory: $path" >&2; exit 1; }
  metadata="$(stat -c '%u %g %a %F' "$path")"
  kv "directory_${index}_path" "$path"
  kv "directory_${index}_uid" "$(awk '{print $1}' <<<"$metadata")"
  kv "directory_${index}_gid" "$(awk '{print $2}' <<<"$metadata")"
  kv "directory_${index}_mode" "$(awk '{print $3}' <<<"$metadata")"
  kv "directory_${index}_type" "$(cut -d' ' -f4- <<<"$metadata")"
}

bundle_dir=/root/stage8b-p1f-o1-8864a2b/bundle
installer="$bundle_dir/scripts/stage8b_p1e_i1_fixed_install.py"
status_json="$(python3 "$installer" status --root /)"

kv schema_version 1
kv probe_kind stage8b-p1f-o1-post-install-read-only-evidence
kv observed_at_utc "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
kv target_id stage8b-p1f-isolated-vps-1
kv hostname "$(hostname -f 2>/dev/null || hostname)"
kv ipv4 45.150.11.252
kv ssh_ed25519_fingerprint "$(ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256 | awk '{print $2}')"
kv staging_directory /root/stage8b-p1f-o1-8864a2b
kv staging_directory_uid "$(stat -c '%u' /root/stage8b-p1f-o1-8864a2b)"
kv staging_directory_gid "$(stat -c '%g' /root/stage8b-p1f-o1-8864a2b)"
kv staging_directory_mode "$(stat -c '%a' /root/stage8b-p1f-o1-8864a2b)"
kv staging_bundle_sha256 "$(sha256sum /root/stage8b-p1f-o1-8864a2b/bundle.zip | awk '{print $1}')"
kv status_json "$status_json"
kv installation_manifest_sha256 "$(sha256sum /usr/local/share/moex/stage8b-p1e/installation-v1.json | awk '{print $1}')"
kv installation_manifest_json "$(cat /usr/local/share/moex/stage8b-p1e/installation-v1.json)"

managed=(
  /etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service
  /etc/systemd/system/moex-finam-p1-paper-bootstrap.service
  /etc/systemd/system/moex-finam-p1-paper.service
  /usr/lib/sysusers.d/moex-finam-p1-paper.conf
  /usr/lib/tmpfiles.d/moex-finam-p1-paper.conf
  /usr/local/libexec/moex/stage8b-p1-paper-supervisor
  /usr/local/share/moex/stage8b-p1e/installation-v1.json
)
for index in "${!managed[@]}"; do
  file_evidence "$index" "${managed[$index]}"
done

passwd_row="$(getent passwd moex-p1-paper)"
group_row="$(getent group moex-p1-paper)"
IFS=: read -r service_user _ service_uid service_primary_gid _ service_home service_shell <<<"$passwd_row"
IFS=: read -r service_group _ service_gid service_group_members <<<"$group_row"
kv service_user "$service_user"
kv service_group "$service_group"
kv service_uid "$service_uid"
kv service_gid "$service_gid"
kv service_primary_gid "$service_primary_gid"
kv service_home "$service_home"
kv service_shell "$service_shell"
kv service_group_members "$service_group_members"

directories=(
  /etc/moex-finam-p1-paper
  /etc/moex-finam-p1-paper/bootstrap
  /etc/moex-finam-p1-paper/credentials
  /var/lib/moex-finam-p1-paper
  /var/lib/moex-finam-p1-paper/state
  /var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine
)
for index in "${!directories[@]}"; do
  directory_evidence "$index" "${directories[$index]}"
done

operator_files=(
  /etc/moex-finam-p1-paper/supervisor.json
  /etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json
  /etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key
)
for index in "${!operator_files[@]}"; do
  kv "operator_${index}_path" "${operator_files[$index]}"
  if [[ -e "${operator_files[$index]}" || -L "${operator_files[$index]}" ]]; then
    kv "operator_${index}_present" true
  else
    kv "operator_${index}_present" false
  fi
done

state_extra_count="$(find /var/lib/moex-finam-p1-paper/state -mindepth 1 -maxdepth 1 ! -name .stage8b-p1-first-boot-quarantine -print | wc -l | tr -d ' ')"
quarantine_entry_count="$(find /var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine -mindepth 1 -print | wc -l | tr -d ' ')"
kv state_extra_entry_count "$state_extra_count"
kv quarantine_entry_count "$quarantine_entry_count"
kv p1_process_count "$(pgrep -fc '[s]tage8b-p1-paper-supervisor' || true)"

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
      if (count != 1 || state == "") exit 2
      print state
    }
  ' <<<"$p1_unit_file_inventory"
}

p1_regular_units=(
  moex-finam-p1-paper.service
  moex-finam-p1-paper-bootstrap.service
)
for index in "${!p1_regular_units[@]}"; do
  unit="${p1_regular_units[$index]}"
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
kv p1_unit_2_fragment_path /etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service
recovery_instances="$(systemctl list-units --all --type=service --plain --no-legend 'moex-finam-p1-paper-bootstrap-recover@*.service' | awk 'NF {print $1}' | LC_ALL=C sort -u | paste -sd, -)"
kv p1_recovery_instances "$recovery_instances"
kv p1_recovery_instances_sha256 "$(printf '%s' "$recovery_instances" | sha256sum | awk '{print $1}')"
kv p1_systemd_query_ok true
# P1_SYSTEMD_BEHAVIOR_END

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

kv redis_db0_size "$(redis-cli -h 127.0.0.1 -p 6379 -n 0 --raw DBSIZE)"
kv redis_db0_keyspace_sha256 "$(redis_keyspace_digest 0)"
kv redis_db15_size "$(redis-cli -h 127.0.0.1 -p 6379 -n 15 --raw DBSIZE)"
kv redis_db15_keyspace_sha256 "$(redis_keyspace_digest 15)"
kv remote_probe_mutation_performed false
REMOTE
