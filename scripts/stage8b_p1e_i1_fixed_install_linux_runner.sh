#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

image="stage8b-p1e-i1-install:ubuntu24.04"
rust_image="rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922"
accepted_binary_source_ref="b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac"
root_dir="$repo_root/tmp/stage8b-p1e-i1-install-root"
binary_target="$repo_root/tmp/stage8b-p1e-i1-install-binary"
evidence_dir="$repo_root/reports/stage8b-p1e-i1-fixed-install"

mkdir -p "$root_dir" "$binary_target" "$evidence_dir"
find "$root_dir" -mindepth 1 -depth -delete
find "$binary_target" -mindepth 1 -depth -delete
find "$evidence_dir" -mindepth 1 -depth -delete

git diff --quiet "$accepted_binary_source_ref" -- Cargo.toml Cargo.lock crates
if git status --porcelain --untracked-files=all -- Cargo.toml Cargo.lock crates | grep -q .; then
  echo "accepted binary source boundary has uncommitted Rust/Cargo paths" >&2
  exit 1
fi
git rev-parse "$accepted_binary_source_ref^{tree}" \
  > "$evidence_dir/accepted-binary-source-tree.txt"
docker run --rm \
  -v "$repo_root:/src:ro" \
  -v "$repo_root/tmp/stage8b-p1e-i1-cargo-home:/cargo-home" \
  -v "$binary_target:/target" \
  -w /src \
  -e CARGO_HOME=/cargo-home \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_INCREMENTAL=0 \
  -e SOURCE_DATE_EPOCH=0 \
  -e RUSTFLAGS="-C strip=symbols --remap-path-prefix=/src=/usr/src/moex-trading-project" \
  "$rust_image" \
  cargo build --locked --release -p runtime-durable-service \
    --bin stage8b-p1-paper-supervisor \
  2>&1 | tee "$evidence_dir/accepted-binary-build.log"
sha256sum "$binary_target/release/stage8b-p1-paper-supervisor" \
  > "$evidence_dir/accepted-binary.sha256"

if ! docker image inspect "$image" >/dev/null 2>&1; then
  docker build -t "$image" - <<'DOCKERFILE'
FROM ubuntu@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517
RUN export DEBIAN_FRONTEND=noninteractive \
 && apt-get update \
 && apt-get install -y --no-install-recommends python3 systemd \
 && rm -rf /var/lib/apt/lists/*
DOCKERFILE
fi

docker run --rm --network none \
  -v "$repo_root:/work:ro" \
  -v "$root_dir:/target" \
  -v "$binary_target/release:/artifacts:ro" \
  -v "$evidence_dir:/evidence" \
  "$image" \
  bash -euo pipefail -c '
    mkdir -p /target/etc
    cp /etc/os-release /target/etc/os-release
    install -d -m 0755 /target/usr/lib/systemd
    cp -a /usr/lib/systemd/system /target/usr/lib/systemd/
    STAGE8B_P1E_REHEARSAL_NETWORK_MODE=none \
      STAGE8B_P1E_REHEARSAL_BINARY_SOURCE_REF=b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac \
      bash /work/scripts/stage8b_p1e_i1_fixed_install_linux_rehearsal.sh \
      /work /target /evidence /artifacts/stage8b-p1-paper-supervisor
  '

echo "stage8b-p1e-i1-fixed-install-linux-runner: PASS target=ubuntu24.04 systemd=255 network=none activation=false"
