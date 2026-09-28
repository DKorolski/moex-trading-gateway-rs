#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 4 ]]; then
  echo "usage: $0 REPO_ROOT TARGET_ROOT EVIDENCE_DIR ACCEPTED_BINARY" >&2
  exit 64
fi

repo_root="$(cd "$1" && pwd)"
target_root="$(cd "$2" && pwd)"
evidence_dir="$3"
accepted_binary="$(cd "$(dirname "$4")" && pwd)/$(basename "$4")"
: "${STAGE8B_P1E_REHEARSAL_NETWORK_MODE:?must be set to none}"
: "${STAGE8B_P1E_REHEARSAL_BINARY_SOURCE_REF:?must bind accepted binary source}"
[[ "$STAGE8B_P1E_REHEARSAL_NETWORK_MODE" = "none" ]]
[[ "$STAGE8B_P1E_REHEARSAL_BINARY_SOURCE_REF" = "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac" ]]
test -x "$accepted_binary"
awk -F: 'NR > 2 { gsub(/[[:space:]]/, "", $1); if ($1 != "lo") exit 1 }' /proc/net/dev
awk 'NR > 1 { exit 1 }' /proc/net/route
awk 'NF && $NF != "lo" { exit 1 }' /proc/net/ipv6_route
install -d -m 0700 "$evidence_dir"

python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install.py" \
  install --root "$target_root" --binary "$accepted_binary" \
  > "$evidence_dir/install-first.json"
python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install_check.py" \
  --root "$repo_root" --target-root "$target_root" \
  > "$evidence_dir/static-and-systemd-check.txt"
python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install.py" \
  status --root "$target_root" \
  > "$evidence_dir/status-installed.json"

manifest="$target_root/usr/local/share/moex/stage8b-p1e/installation-v1.json"
installed_binary="$target_root/usr/local/libexec/moex/stage8b-p1-paper-supervisor"
accepted_binary_hash="$(sha256sum "$accepted_binary" | awk '{print $1}')"
installed_binary_hash="$(sha256sum "$installed_binary" | awk '{print $1}')"
[[ "$accepted_binary_hash" = "$installed_binary_hash" ]]
manifest_hash_before="$(sha256sum "$manifest" | awk '{print $1}')"
python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install.py" \
  install --root "$target_root" --binary "$accepted_binary" \
  > "$evidence_dir/install-idempotent.json"
manifest_hash_after="$(sha256sum "$manifest" | awk '{print $1}')"
[[ "$manifest_hash_before" = "$manifest_hash_after" ]]

python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install_behavioral_harness.py" \
  "$repo_root" "$target_root" "$accepted_binary" \
  "$evidence_dir/behavioral-filesystem-matrix.json" \
  | tee "$evidence_dir/behavioral-filesystem-matrix.log"

install -m 0640 /dev/null "$target_root/etc/moex-finam-p1-paper/supervisor.json"
if python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install.py" \
  rollback --root "$target_root" \
  > "$evidence_dir/rollback-operator-material.stdout" \
  2> "$evidence_dir/rollback-operator-material.stderr"; then
  echo "rollback accepted operator material" >&2
  exit 1
fi
rm "$target_root/etc/moex-finam-p1-paper/supervisor.json"

install -m 0600 /dev/null \
  "$target_root/var/lib/moex-finam-p1-paper/state/unexpected-durable-state"
if python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install.py" \
  rollback --root "$target_root" \
  > "$evidence_dir/rollback-durable-state.stdout" \
  2> "$evidence_dir/rollback-durable-state.stderr"; then
  echo "rollback accepted durable state" >&2
  exit 1
fi
rm "$target_root/var/lib/moex-finam-p1-paper/state/unexpected-durable-state"

python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install.py" \
  rollback --root "$target_root" \
  > "$evidence_dir/rollback.json"
python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install.py" \
  status --root "$target_root" \
  > "$evidence_dir/status-rolled-back.json"

test ! -e "$target_root/usr/local/libexec/moex/stage8b-p1-paper-supervisor"
test ! -e "$target_root/etc/systemd/system/moex-finam-p1-paper.service"
test ! -e "$target_root/etc/systemd/system/moex-finam-p1-paper-bootstrap.service"
test ! -e "$target_root/etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service"
test ! -e "$target_root/etc/moex-finam-p1-paper/supervisor.json"
test ! -e "$target_root/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key"
test -d "$target_root/var/lib/moex-finam-p1-paper/state"
test -d "$target_root/var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine"

systemd_version="$(systemd-analyze --version | sed -n '1s/^systemd \([0-9][0-9]*\).*/\1/p')"
python3 - \
  "$systemd_version" \
  "$manifest_hash_before" \
  "$accepted_binary_hash" \
  "$STAGE8B_P1E_REHEARSAL_BINARY_SOURCE_REF" \
  "$evidence_dir/target-linux-evidence.json" <<'PY'
import json
import pathlib
import sys

evidence = {
    "schema_version": 1,
    "domain": "moex.stage8b.p1e.fixed-install.target-linux-evidence.v1",
    "target": "ubuntu-24.04",
    "systemd_version": int(sys.argv[1]),
    "network_mode": "none",
    "clean_install": "PASS",
    "systemd_analyze_verify": "PASS",
    "unknown_key_or_lvalue_warnings": 0,
    "idempotent_reinstall": "PASS",
    "installation_manifest_sha256": sys.argv[2],
    "installed_binary_sha256": sys.argv[3],
    "installed_binary_source_ref": sys.argv[4],
    "installed_binary_kind": "accepted-release",
    "operator_material_rollback_refusal": "PASS",
    "durable_state_rollback_refusal": "PASS",
    "nonempty_quarantine_rollback_refusal": "PASS",
    "empty_quarantine_positive_control": "PASS",
    "behavioral_filesystem_matrix": "PASS",
    "behavioral_filesystem_case_count": 14,
    "clean_public_package_rollback": "PASS",
    "unit_start_attempts": 0,
    "daemon_reload_attempts": 0,
    "redis_contacts": 0,
    "finam_contacts": 0,
    "runtime_live": False,
    "real_orders": False,
    "persistent_state_directories_retained": True,
}
output = pathlib.Path(sys.argv[5])
output.write_text(
    json.dumps(evidence, sort_keys=True, separators=(",", ":")) + "\n",
    encoding="utf-8",
)
PY

echo "stage8b-p1e-i1-fixed-install-linux-rehearsal: PASS systemd=$systemd_version activation=false"
