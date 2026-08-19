#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
tree="$(cargo tree -p runtime-durable-service --edges normal)"

for forbidden in finam-gateway broker-finam reqwest; do
  if grep -Eq "(^|[[:space:]])${forbidden}[[:space:]]+v" <<<"$tree"; then
    echo "stage8a4-i3-dependency-graph: FAIL runtime inherits $forbidden" >&2
    exit 1
  fi
done

# broker-core's pre-existing broker-neutral order-path store owns rusqlite.
# R3 removes the new FINAM dependency edge; moving that baseline persistence
# into a separate feature/crate is outside this authority-topology correction.
if grep -Eq 'rusqlite[[:space:]]+v' <<<"$tree"; then
  inverse="$(cargo tree -p runtime-durable-service --edges normal -i rusqlite)"
  grep -Eq 'broker-core v' <<<"$inverse"
  if grep -Eq 'finam-gateway|broker-finam|reqwest' <<<"$inverse"; then
    echo "stage8a4-i3-dependency-graph: FAIL rusqlite inherited through FINAM transport" >&2
    exit 1
  fi
fi

grep -Eq '^runtime-durable-service v' <<<"$tree"
grep -Eq 'strategy-runtime-core v' <<<"$tree"
echo "stage8a4-i3-dependency-graph: PASS broker_neutral=true broker_core_sqlite_baseline=true"
