#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"

failures=0

report_failure() {
  echo "forbidden-surface-scan: $*" >&2
  failures=$((failures + 1))
}

if rg -n --glob 'crates/**/*.rs' '\.delete\(' crates >/tmp/moex_forbidden_delete.$$; then
  cat /tmp/moex_forbidden_delete.$$ >&2
  report_failure "real HTTP DELETE surface is forbidden"
fi
rm -f /tmp/moex_forbidden_delete.$$

if rg -n --glob 'crates/**/*.rs' 'Method::DELETE' crates >/tmp/moex_forbidden_method_delete.$$; then
  cat /tmp/moex_forbidden_method_delete.$$ >&2
  report_failure "Method::DELETE surface is forbidden"
fi
rm -f /tmp/moex_forbidden_method_delete.$$

if rg -n --glob 'crates/**/*.rs' 'Method::POST' crates >/tmp/moex_forbidden_method_post.$$; then
  cat /tmp/moex_forbidden_method_post.$$ >&2
  report_failure "Method::POST is not allowed in gateway/order surfaces"
fi
rm -f /tmp/moex_forbidden_method_post.$$

if rg -n --glob 'crates/**/*.rs' '\.post\(' crates >/tmp/moex_post_matches.$$; then
  while IFS= read -r match; do
    case "$match" in
      crates/broker-finam/src/lib.rs:*)
        ;;
      *)
        echo "$match" >&2
        report_failure ".post( is allowed only in broker-finam auth/session/token-details code"
        ;;
    esac
  done </tmp/moex_post_matches.$$
fi
rm -f /tmp/moex_post_matches.$$

python3 - <<'PY'
from pathlib import Path
import sys

source = Path("crates/finam-gateway/src/lib.rs").read_text()

scopes = {
    "real-readonly transport": (
        "pub struct ReqwestFinamRealReadonlyBrokerTruthTransport",
        "#[derive(Clone)]\npub struct LocalMockFinamRealReadonlyBrokerTruthTransport",
    ),
    "real-readonly operator probe": (
        "pub struct FinamRealReadonlyContractProbeOperatorRunConfig",
        "#[derive(Clone)]\npub struct LocalMockCancelBrokerTruthReadonlyHttpClient",
    ),
}

forbidden = [
    ".post(",
    ".delete(",
    "Method::POST",
    "Method::DELETE",
    "FinamPlaceOrderRequestSpec",
    "FinamCancelOrderRequestSpec",
    "FinamRealOrderEndpointTransport",
    "place_order_endpoint",
    "cancel_order_endpoint",
]

failures = 0
for scope_name, (start, end) in scopes.items():
    try:
        scoped = source.split(start, 1)[1].split(end, 1)[0]
    except IndexError:
        print(f"forbidden-surface-scan: cannot locate {scope_name} scope", file=sys.stderr)
        failures += 1
        continue
    for pattern in forbidden:
        if pattern in scoped:
            print(
                f"forbidden-surface-scan: {pattern!r} found in {scope_name}",
                file=sys.stderr,
            )
            failures += 1

sys.exit(1 if failures else 0)
PY

if (( failures > 0 )); then
  exit 1
fi

echo "forbidden-surface-scan: ok"
