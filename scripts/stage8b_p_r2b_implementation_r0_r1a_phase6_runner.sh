#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
cross_image='messense/rust-musl-cross:x86_64-musl@sha256:020ec7f60e63ace4338f8cb492bb2521071d089133732d0fc6a0ecea722b87c5'
controlled_target="$repo_root/tmp/stage8b-r2b-r0-r1a-controlled-amd64"
cargo_home="$repo_root/tmp/stage8b-r2b-r0-r1a-cargo-home"
artifact_root="$repo_root/reports/stage8b-p-r2b-r0-r1a/linux-amd64"
evidence_dir="$repo_root/reports/stage8b-p-r2b-r0-r1a"
image=stage8b-r2b-r0-r1a-systemd:ubuntu24.04-amd64
container="stage8b-r2b-r0-r1a-systemd-$$"
ceremony_root="${STAGE8B_R2B_PHASE6_CEREMONY_DIR:-}"

if [[ -z "$ceremony_root" || ! -f "$ceremony_root/package-authorization.ed25519" ]]; then
  echo "stage8b-p-r2b-r0-r1a-phase6-runner: FAIL set STAGE8B_R2B_PHASE6_CEREMONY_DIR to the offline ceremony directory" >&2
  exit 1
fi

mkdir -p "$controlled_target" "$cargo_home" "$evidence_dir"
docker run --rm --platform linux/arm64 \
  -v "$repo_root:/src:ro" -v "$cargo_home:/cargo-home" -v "$controlled_target:/target" \
  -w /src -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/target \
  -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-unknown-linux-musl-gcc \
  -e CARGO_INCREMENTAL=0 -e SOURCE_DATE_EPOCH=0 \
  -e 'RUSTFLAGS=-C strip=symbols --remap-path-prefix=/src=/usr/src/moex-trading-project' \
  "$cross_image" cargo build --locked --release \
    --target x86_64-unknown-linux-musl \
    --manifest-path tools/stage8b-readonly-preflight/Cargo.toml \
    --features stage8b-r2b-controlled-custody \
    --bin stage8b-r2a5-controlled-layout \
    --bin stage8b-r2a5-authority-producer \
    --bin stage8b-r2a5-authority-issuer

if ! docker image inspect "$image" >/dev/null 2>&1; then
  docker build --platform linux/amd64 -t "$image" - <<'DOCKERFILE'
FROM ubuntu@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517
RUN export DEBIAN_FRONTEND=noninteractive \
 && apt-get update \
 && apt-get install -y --no-install-recommends systemd util-linux iproute2 python3 findutils \
 && rm -rf /var/lib/apt/lists/*
STOPSIGNAL SIGRTMIN+3
CMD ["/lib/systemd/systemd"]
DOCKERFILE
fi

cleanup() {
  if [[ "${STAGE8B_R2B_KEEP_PHASE6_CONTAINER:-0}" = 1 ]]; then
    echo "stage8b-p-r2b-r0-r1a-phase6-runner: retained container=$container" >&2
    return
  fi
  docker rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT
docker run -d --privileged --platform linux/amd64 --cgroupns=host --name "$container" \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
  -v "$repo_root:/work:ro" \
  -v "$artifact_root/build-a:/artifacts/build-a:ro" \
  -v "$controlled_target/x86_64-unknown-linux-musl/release:/controlled/release:ro" \
  -v "$ceremony_root:/ceremony:ro" \
  -v "$evidence_dir:/evidence" --network none "$image" >/dev/null
for _ in $(seq 1 180); do
  state="$(docker exec "$container" systemctl is-system-running 2>/dev/null || true)"
  [[ "$state" = running || "$state" = degraded ]] && break
  sleep 1
done
docker exec "$container" bash /work/scripts/stage8b_p_r2b_implementation_r0_r1a_phase6_rehearsal.sh \
  /work /artifacts/build-a /controlled/release \
  /evidence/stage8b-p-r2b-implementation-r0-r1-linux-rehearsal.json /ceremony
echo "stage8b-p-r2b-r0-r1a-phase6-runner: PASS"
