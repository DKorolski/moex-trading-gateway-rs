#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
probe="$root/scripts/stage8b_p1f_o0_readonly_probe.sh"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-p1f-o0-systemd.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

mkdir -p "$scratch/bin"
cat >"$scratch/bin/systemctl" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail

if [[ "$1" == "list-unit-files" ]]; then
  exit 0
fi
if [[ "$1" == "list-units" ]]; then
  exit 0
fi
if [[ "$1" != "show" || "$3" != "-p" || "$5" != "--value" ]]; then
  echo "unexpected systemctl arguments: $*" >&2
  exit 64
fi
unit="$2"
property="$4"
case "$property" in
  LoadState) printf 'not-found\n' ;;
  ActiveState) printf 'inactive\n' ;;
  FragmentPath)
    if [[ "${FRAGMENT_QUERY_MODE:-pass}" == "fail" && "$unit" == "moex-finam-p1-paper.service" ]]; then
      echo "injected FragmentPath failure" >&2
      exit 1
    fi
    printf ''
    ;;
  *)
    echo "unexpected property: $property" >&2
    exit 64
    ;;
esac
STUB
chmod +x "$scratch/bin/systemctl"

cat >"$scratch/prelude.sh" <<'PRELUDE'
set -euo pipefail
kv() { printf '%s=%s\n' "$1" "$2"; }
unit_value() { systemctl show "$1" -p "$2" --value; }
PRELUDE

awk '
  /# P1_SYSTEMD_BEHAVIOR_BEGIN/ {copy = 1; next}
  /# P1_SYSTEMD_BEHAVIOR_END/ {copy = 0}
  copy {print}
' "$probe" >>"$scratch/prelude.sh"

positive="$({ PATH="$scratch/bin:$PATH" FRAGMENT_QUERY_MODE=pass bash "$scratch/prelude.sh"; } 2>&1)"
grep -Fxq 'p1_unit_0_fragment_path=' <<<"$positive"
grep -Fxq 'p1_systemd_query_ok=true' <<<"$positive"
echo "PASS p1-systemd-empty-fragment-success"

set +e
negative="$({ PATH="$scratch/bin:$PATH" FRAGMENT_QUERY_MODE=fail bash "$scratch/prelude.sh"; } 2>&1)"
negative_status=$?
set -e
if [[ "$negative_status" -eq 0 ]]; then
  echo "FAIL FragmentPath nonzero status survived" >&2
  exit 1
fi
if grep -Fxq 'p1_systemd_query_ok=true' <<<"$negative"; then
  echo "FAIL FragmentPath failure produced success marker" >&2
  exit 1
fi
grep -Fq 'P1 FragmentPath query failed: moex-finam-p1-paper.service' <<<"$negative"
echo "PASS p1-systemd-empty-fragment-failure-rejected"
echo "PASS stage8b-p1f-o0-systemd-query-behavioral-test controls=2"
