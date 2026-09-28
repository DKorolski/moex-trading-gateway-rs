#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

image="messense/rust-musl-cross:x86_64-musl@sha256:020ec7f60e63ace4338f8cb492bb2521071d089133732d0fc6a0ecea722b87c5"
target_triple="x86_64-unknown-linux-musl"
build_platform="linux/arm64"
cargo_home="$repo_root/tmp/stage8b-g2-composition-cargo-home"
artifact_root="$repo_root/reports/stage8b-p-r2b-generation2-composition-r0/linux-amd64"
evidence="$repo_root/docs/stage-8/stage8b-p-r2b-generation2-composition-r0-linux-build-evidence.json"
helper_pin="$repo_root/docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-sha256.txt"
build_a="$(mktemp -d "$repo_root/tmp/stage8b-g2-composition-build-a.XXXXXX")"
build_b="$(mktemp -d "$repo_root/tmp/stage8b-g2-composition-build-b.XXXXXX")"

production_binaries=(
  stage8b-r2a5-authority-producer
  stage8b-r2a5-authority-issuer
  stage8b-r2b-run-package-draft-builder
  stage8b-r2a5-package-issuer
  stage8b-r2a5-helper-acceptance-issuer
  stage8b-readonly-preflight
  stage8b-r2b-launcher
)
operation_binaries=(stage8b-r2b-generation2-helper-acceptance-issuer)
binaries=("${production_binaries[@]}" "${operation_binaries[@]}")

command=(
  cargo build --locked --release --no-default-features
  --target "$target_triple"
  --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
)
for binary in "${binaries[@]}"; do
  command+=(--bin "$binary")
done

if ! command -v docker >/dev/null 2>&1; then
  echo "stage8b-generation2-composition-build: FAIL docker unavailable" >&2
  exit 1
fi
if [[ -e "$artifact_root" || -e "$evidence" ]]; then
  echo "stage8b-generation2-composition-build: FAIL output already exists" >&2
  exit 1
fi
if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
  echo "stage8b-generation2-composition-build: FAIL dirty source tree" >&2
  exit 1
fi

mkdir -p "$cargo_home" "$artifact_root/build-a" "$artifact_root/build-b"
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
    -e 'RUSTFLAGS=-C strip=symbols --remap-path-prefix=/src=/usr/src/moex-trading-project' \
    "$image" "${command[@]}"
  for binary in "${binaries[@]}"; do
    cp "$target/$target_triple/release/$binary" "$artifact_root/build-$name/$binary"
  done
done

python3 - "$artifact_root" "$evidence" "$image" "${command[*]}" \
  "$(git rev-parse HEAD)" "$(git rev-parse 'HEAD^{tree}')" \
  "${production_binaries[*]}" -- "${operation_binaries[*]}" <<'PY'
import hashlib
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
evidence = pathlib.Path(sys.argv[2])
image = sys.argv[3]
command = sys.argv[4]
source_ref = sys.argv[5]
source_tree = sys.argv[6]
separator = sys.argv.index("--")
production = tuple(sys.argv[7:separator][0].split())
operation = tuple(sys.argv[separator + 1:][0].split())
binaries = production + operation
records = {}
for binary in binaries:
    left = root / "build-a" / binary
    right = root / "build-b" / binary
    left_bytes = left.read_bytes()
    right_bytes = right.read_bytes()
    left_hash = hashlib.sha256(left_bytes).hexdigest()
    right_hash = hashlib.sha256(right_bytes).hexdigest()
    if left_bytes != right_bytes or left_hash != right_hash:
        raise SystemExit(f"stage8b-generation2-composition-build: FAIL non-reproducible {binary}")
    identity = subprocess.check_output(["file", "-b", str(left)], text=True).strip()
    if "ELF 64-bit LSB" not in identity or "x86-64" not in identity:
        raise SystemExit(f"stage8b-generation2-composition-build: FAIL ELF identity {binary}")
    records[binary] = {
        "build_a_sha256": left_hash,
        "build_b_sha256": right_hash,
        "executable_size": len(left_bytes),
        "file_identity": identity,
        "reproducible": True,
        "classification": "PRODUCTION" if binary in production else "OFFLINE_PUBLIC_AUTHORITY_TOOL",
    }

helper_pin = pathlib.Path(
    "docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-sha256.txt"
).read_text().strip()
if records["stage8b-readonly-preflight"]["build_a_sha256"] != helper_pin:
    raise SystemExit("stage8b-generation2-composition-build: FAIL helper pin mismatch")
launcher = root / "build-a/stage8b-r2b-launcher"
strings = subprocess.check_output(["strings", str(launcher)], text=True, errors="replace")
if helper_pin not in strings:
    raise SystemExit("stage8b-generation2-composition-build: FAIL launcher helper pin absent")

payload = {
    "schema_version": 1,
    "stage": "Stage 8B-P R2B Generation-2 Composition Rebuild R0",
    "source_ref": source_ref,
    "source_tree": source_tree,
    "target": "x86_64-unknown-linux-musl",
    "builder_platform": "linux/arm64",
    "cross_compiled": True,
    "build_profile": "release",
    "default_features": False,
    "controlled_custody_feature": False,
    "clean_target_directories": 2,
    "container_image": image,
    "cargo_command": command,
    "source_mount": "/src:ro",
    "path_remap": "/src=/usr/src/moex-trading-project",
    "production_binary_count": len(production),
    "offline_tool_binary_count": len(operation),
    "binaries": records,
    "helper_sha256": helper_pin,
    "launcher_embeds_exact_helper_sha256": True,
    "all_hashes_identical": True,
    "generation": 2,
    "authorization": "NOT_ISSUED",
    "result": "PASS",
}
evidence.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(
    "stage8b-generation2-composition-build: PASS "
    f"binaries={len(binaries)} builds=2 target=linux/amd64 generation=2 "
    "authorization=NOT_ISSUED"
)
PY
