#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

image="messense/rust-musl-cross:x86_64-musl@sha256:020ec7f60e63ace4338f8cb492bb2521071d089133732d0fc6a0ecea722b87c5"
target_triple="x86_64-unknown-linux-musl"
build_platform="linux/arm64"
cargo_home="$repo_root/tmp/stage8b-r2b-r0-r1a-cargo-home"
build_a="$repo_root/tmp/stage8b-r2b-r0-r1a-linux-a"
build_b="$repo_root/tmp/stage8b-r2b-r0-r1a-linux-b"
artifact_root="$repo_root/reports/stage8b-p-r2b-r0-r1a/linux-amd64"
evidence="$repo_root/docs/stage-8/stage8b-p-r2b-implementation-r0-r1-linux-build-evidence.json"
binaries=(
  stage8b-r2b-run-package-draft-builder
  stage8b-r2a5-package-issuer
  stage8b-readonly-preflight
  stage8b-r2b-launcher
)

command=(
  cargo build --locked --release --no-default-features
  --target "$target_triple"
  --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
  --bin stage8b-r2b-run-package-draft-builder
  --bin stage8b-r2a5-package-issuer
  --bin stage8b-readonly-preflight
  --bin stage8b-r2b-launcher
)

if ! command -v docker >/dev/null 2>&1; then
  echo "stage8b-p-r2b-r0-r1-linux-build: FAIL docker unavailable" >&2
  exit 1
fi

rm -rf "$build_a" "$build_b" "$artifact_root"
mkdir -p "$cargo_home" "$build_a" "$build_b" "$artifact_root/build-a" "$artifact_root/build-b"

for pair in "a:$build_a" "b:$build_b"; do
  name="${pair%%:*}"
  target="${pair#*:}"
  docker run --rm --platform "$build_platform" \
    -v "$repo_root:/src:ro" \
    -v "$cargo_home:/cargo-home" \
    -v "$target:/target" \
    -w /src \
    -e CARGO_HOME=/cargo-home \
    -e CARGO_TARGET_DIR=/target \
    -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-unknown-linux-musl-gcc \
    -e CARGO_INCREMENTAL=0 \
    -e SOURCE_DATE_EPOCH=0 \
    -e RUSTFLAGS="-C strip=symbols --remap-path-prefix=/src=/usr/src/moex-trading-project" \
    "$image" "${command[@]}"
  for binary in "${binaries[@]}"; do
    cp "$target/$target_triple/release/$binary" "$artifact_root/build-$name/$binary"
  done
done

python3 - "$artifact_root" "$evidence" "$image" "${command[*]}" <<'PY'
import hashlib
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
evidence = pathlib.Path(sys.argv[2])
image = sys.argv[3]
command = sys.argv[4]
binaries = (
    "stage8b-r2b-run-package-draft-builder",
    "stage8b-r2a5-package-issuer",
    "stage8b-readonly-preflight",
    "stage8b-r2b-launcher",
)
records = {}
for binary in binaries:
    left = root / "build-a" / binary
    right = root / "build-b" / binary
    left_bytes = left.read_bytes()
    right_bytes = right.read_bytes()
    left_hash = hashlib.sha256(left_bytes).hexdigest()
    right_hash = hashlib.sha256(right_bytes).hexdigest()
    if left_bytes != right_bytes or left_hash != right_hash:
        raise SystemExit(f"stage8b-p-r2b-r0-r1-linux-build: FAIL non-reproducible {binary}")
    file_identity = subprocess.check_output(["file", "-b", str(left)], text=True).strip()
    if "ELF 64-bit LSB" not in file_identity or "x86-64" not in file_identity:
        raise SystemExit(f"stage8b-p-r2b-r0-r1-linux-build: FAIL ELF identity {binary}: {file_identity}")
    records[binary] = {
        "build_a_sha256": left_hash,
        "build_b_sha256": right_hash,
        "executable_size": len(left_bytes),
        "file_identity": file_identity,
        "reproducible": True,
    }
payload = {
    "schema_version": 1,
    "stage": "Stage 8B-P R2B Implementation Package R0-R1A",
    "target": "x86_64-unknown-linux-musl",
    "builder_platform": "linux/arm64",
    "cross_compiled": True,
    "build_profile": "release",
    "default_features": False,
    "controlled_custody_feature": False,
    "production_helper_feature": "no-default-features",
    "production_launcher_pins_helper_sha256": records["stage8b-readonly-preflight"]["build_a_sha256"],
    "clean_target_directories": 2,
    "container_image": image,
    "cargo_command": command,
    "source_mount": "/src:ro",
    "path_remap": "/src=/usr/src/moex-trading-project",
    "binaries": records,
    "all_hashes_identical": True,
    "result": "PASS",
}
evidence.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
accepted = pathlib.Path("docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt").read_text().strip()
if records["stage8b-readonly-preflight"]["build_a_sha256"] != accepted:
    raise SystemExit("stage8b-p-r2b-r0-r1a-linux-build: FAIL accepted helper SHA mismatch")
print("stage8b-p-r2b-r0-r1a-linux-build: PASS binaries=4 builds=2 target=linux/amd64 controlled_feature=false")
PY
