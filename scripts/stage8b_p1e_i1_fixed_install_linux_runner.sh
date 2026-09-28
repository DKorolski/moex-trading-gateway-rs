#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

image="stage8b-p1e-i1-install:ubuntu24.04"
rust_image="rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922"
accepted_binary_source_ref="b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac"
root_dir="$repo_root/tmp/stage8b-p1e-i1-install-root"
binary_target="$repo_root/tmp/stage8b-p1e-i1-install-binary"
source_dir="$repo_root/tmp/stage8b-p1e-i1-accepted-source"
source_archive="$repo_root/tmp/moex-trading-project-b6f6d5b.tar.gz"
evidence_dir="$repo_root/reports/stage8b-p1e-i1-fixed-install"

mkdir -p "$root_dir" "$binary_target" "$source_dir" "$evidence_dir"
find "$root_dir" -mindepth 1 -depth -delete
find "$binary_target" -mindepth 1 -depth -delete
find "$source_dir" -mindepth 1 -depth -delete
find "$evidence_dir" -mindepth 1 -depth -delete
rm -f "$source_archive"
exec > >(tee "$evidence_dir/linux-runner.log") 2>&1
printf 'invocation=%q\nworking_directory=%q\nnetwork_mode=none\n' \
  "$0" "$repo_root" > "$evidence_dir/linux-runner-invocation.txt"

git diff --quiet "$accepted_binary_source_ref" -- Cargo.toml Cargo.lock crates
if git status --porcelain --untracked-files=all -- Cargo.toml Cargo.lock crates | grep -q .; then
  echo "accepted binary source boundary has uncommitted Rust/Cargo paths" >&2
  exit 1
fi
accepted_binary_source_tree="$(git rev-parse "$accepted_binary_source_ref^{tree}")"
printf '%s\n%s\n' "$accepted_binary_source_ref" "$accepted_binary_source_tree" \
  > "$evidence_dir/accepted-binary-source.txt"
git archive --format=tar.gz --output="$source_archive" "$accepted_binary_source_ref"
source_archive_sha256="$(sha256sum "$source_archive" | awk '{print $1}')"
tar -xzf "$source_archive" -C "$source_dir"
printf 'source_ref=%s\nsource_tree=%s\narchive_sha256=%s\nverification=PASS\n' \
  "$accepted_binary_source_ref" "$accepted_binary_source_tree" "$source_archive_sha256" \
  > "$evidence_dir/source-archive-check.txt"

set +e
docker run --rm \
  -v "$source_dir:/src:ro" \
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
build_exit_code="${PIPESTATUS[0]}"
set -e
binary_path="$binary_target/release/stage8b-p1-paper-supervisor"
accepted_binary_path="$binary_target/accepted/stage8b-p1-paper-supervisor"
binary_sha256=""
if [[ "$build_exit_code" = "0" && -x "$binary_path" ]]; then
  binary_sha256="$(sha256sum "$binary_path" | awk '{print $1}')"
  install -D -m 0755 "$binary_path" "$accepted_binary_path"
  [[ "$(stat -c %h "$accepted_binary_path")" = "1" ]]
  [[ "$(sha256sum "$accepted_binary_path" | awk '{print $1}')" = "$binary_sha256" ]]
fi
python3 - \
  "$accepted_binary_source_ref" "$accepted_binary_source_tree" \
  "$source_archive_sha256" "$rust_image" "$build_exit_code" "$binary_sha256" \
  "$evidence_dir/accepted-binary-build-result.json" <<'PY'
import json
import pathlib
import sys

document = {
    "schema_version": 1,
    "source_ref": sys.argv[1],
    "source_tree": sys.argv[2],
    "source_archive_sha256": sys.argv[3],
    "rust_image": sys.argv[4],
    "build_exit_code": int(sys.argv[5]),
    "binary_sha256": sys.argv[6],
    "locked": True,
    "profile": "release",
    "package": "runtime-durable-service",
    "binary": "stage8b-p1-paper-supervisor",
}
pathlib.Path(sys.argv[7]).write_text(
    json.dumps(document, sort_keys=True, separators=(",", ":")) + "\n",
    encoding="utf-8",
)
PY
if [[ "$build_exit_code" != "0" || -z "$binary_sha256" ]]; then
  echo "accepted release build failed: exit=$build_exit_code" >&2
  exit 1
fi
sha256sum "$accepted_binary_path" \
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
  -v "$binary_target/accepted:/artifacts:ro" \
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

python3 "$repo_root/scripts/stage8b_p1e_i1_fixed_install_check.py" \
  --root "$repo_root" --evidence-dir "$evidence_dir" \
  | tee "$evidence_dir/source-and-evidence-gate.log"

echo "stage8b-p1e-i1-fixed-install-linux-runner: PASS target=ubuntu24.04 systemd=255 network=none activation=false"
