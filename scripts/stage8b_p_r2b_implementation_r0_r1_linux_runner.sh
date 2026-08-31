#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

image="rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0"
native_arch="$(docker info --format '{{.Architecture}}')"
systemd_image="stage8b-r2b-r0-r1-systemd:ubuntu24.04-$native_arch"
production_target="$repo_root/tmp/stage8b-r2b-r0-r1-native-production"
production_dir="$production_target/release"
controlled_target="$repo_root/tmp/stage8b-r2b-r0-r1-native-controlled"
evidence_dir="$repo_root/reports/stage8b-p-r2b-r0-r1"
container="stage8b-r2b-r0-r1-systemd-$$"

mkdir -p "$production_target" "$controlled_target" "$evidence_dir"

docker run --rm \
  -v "$repo_root:/src:ro" \
  -v "$repo_root/tmp/stage8b-r2b-r0-r1-cargo-home:/cargo-home" \
  -v "$production_target:/target" \
  -w /src \
  -e CARGO_HOME=/cargo-home \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_INCREMENTAL=0 \
  -e SOURCE_DATE_EPOCH=0 \
  -e RUSTFLAGS="-C strip=symbols --remap-path-prefix=/src=/usr/src/moex-trading-project" \
  "$image" \
  cargo build --locked --release --no-default-features \
    --manifest-path tools/stage8b-readonly-preflight/Cargo.toml \
    --bin stage8b-r2b-run-package-draft-builder \
    --bin stage8b-r2a5-package-issuer

docker run --rm \
  -v "$repo_root:/src:ro" \
  -v "$repo_root/tmp/stage8b-r2b-r0-r1-cargo-home:/cargo-home" \
  -v "$controlled_target:/target" \
  -w /src \
  -e CARGO_HOME=/cargo-home \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_INCREMENTAL=0 \
  -e SOURCE_DATE_EPOCH=0 \
  -e RUSTFLAGS="-C strip=symbols --remap-path-prefix=/src=/usr/src/moex-trading-project" \
  "$image" \
  cargo build --locked --release \
    --manifest-path tools/stage8b-readonly-preflight/Cargo.toml \
    --features stage8b-r2b-controlled-custody \
    --bin stage8b-readonly-preflight \
    --bin stage8b-r2a5-authority-producer \
    --bin stage8b-r2a5-authority-issuer \
    --bin stage8b-r2a5-controlled-layout

cleanup() {
  if [[ "${STAGE8B_R2B_KEEP_REHEARSAL_CONTAINER:-0}" = "1" ]]; then
    echo "stage8b-p-r2b-r0-r1-linux-runner: retained container=$container" >&2
    return
  fi
  docker rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT

if ! docker image inspect "$systemd_image" >/dev/null 2>&1; then
  docker build -t "$systemd_image" - <<'DOCKERFILE'
FROM ubuntu@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517
RUN export DEBIAN_FRONTEND=noninteractive \
 && apt-get update \
 && apt-get install -y --no-install-recommends systemd util-linux iproute2 python3 \
 && rm -rf /var/lib/apt/lists/*
STOPSIGNAL SIGRTMIN+3
CMD ["/lib/systemd/systemd"]
DOCKERFILE
fi

docker run -d --privileged --cgroupns=host \
  --name "$container" \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
  -v "$repo_root:/work:ro" \
  -v "$production_dir:/artifacts/build-a:ro" \
  -v "$controlled_target/release:/controlled/release:ro" \
  -v "$evidence_dir:/evidence" \
  --network none \
  "$systemd_image" \
  >/dev/null

ready=false
for _ in $(seq 1 180); do
  systemd_state="$(docker exec "$container" systemctl is-system-running 2>/dev/null || true)"
  if [[ "$systemd_state" = running ]] || [[ "$systemd_state" = degraded ]]; then
    ready=true
    break
  fi
  state="$(docker inspect -f '{{.State.Status}}' "$container" 2>/dev/null || true)"
  if [[ "$state" = exited ]] || [[ "$state" = dead ]]; then
    docker logs "$container" >&2
    exit 1
  fi
  sleep 1
done
if [[ "$ready" != true ]]; then
  docker logs "$container" >&2
  echo "stage8b-p-r2b-r0-r1-linux-runner: FAIL systemd readiness timeout" >&2
  exit 1
fi

docker exec "$container" bash -lc '! ip route show default | grep -q .'
if ! docker exec "$container" bash /work/scripts/stage8b_p_r2b_implementation_r0_r1_linux_rehearsal.sh \
  /work /artifacts/build-a /controlled/release \
  /evidence/stage8b-p-r2b-implementation-r0-r1-linux-rehearsal.json; then
  docker exec "$container" systemctl --no-pager --full status \
    r0r1-builder-trigger.service \
    moex-stage8b-r2b-run-package-draft-builder.service \
    r0r1-signer-trigger.service \
    moex-stage8b-r2b-package-issuer.service \
    r0r1-supervisor-trigger.service \
    moex-stage8b-r2b-readonly-supervisor.service >&2 || true
  docker exec "$container" systemctl show \
    -p LoadState -p ActiveState -p Result -p RefuseManualStart -p Requires -p After \
    moex-stage8b-r2b-run-package-draft-builder.service >&2 || true
  docker exec "$container" systemd-analyze verify --man=no \
    /etc/systemd/system/moex-stage8b-r2b-run-package-draft-builder.service \
    /etc/systemd/system/r0r1-builder-trigger.service >&2 || true
  docker exec "$container" cat /run/r0r1-probe-debug >&2 || true
  docker exec "$container" journalctl --no-pager -n 240 >&2 || true
  exit 1
fi

echo "stage8b-p-r2b-r0-r1-linux-runner: PASS systemd=true network=none"
