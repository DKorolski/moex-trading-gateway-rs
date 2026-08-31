#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
cross_image='messense/rust-musl-cross:x86_64-musl@sha256:020ec7f60e63ace4338f8cb492bb2521071d089133732d0fc6a0ecea722b87c5'
cargo_home="$repo_root/tmp/stage8b-g2-composition-cargo-home"
controlled_target="$(mktemp -d "$repo_root/tmp/stage8b-g2-composition-controlled.XXXXXX")"
materialized_dir="$(mktemp -d "$repo_root/tmp/stage8b-g2-composition-phase6.XXXXXX")"
materialized="$materialized_dir/rehearsal.sh"
artifact_root="$repo_root/reports/stage8b-p-r2b-generation2-composition-r0/linux-amd64"
evidence_dir="$repo_root/reports/stage8b-p-r2b-generation2-composition-r0"
evidence_name=stage8b-p-r2b-generation2-composition-r0-linux-rehearsal-evidence.json
published_evidence="$repo_root/docs/stage-8/$evidence_name"
build_evidence="$repo_root/docs/stage-8/stage8b-p-r2b-generation2-composition-r0-linux-build-evidence.json"
image=stage8b-r2b-generation2-composition-r0-systemd:ubuntu24.04-amd64
container="stage8b-r2b-generation2-composition-r0-$$"
ceremony_root="${STAGE8B_R2B_PHASE6_CEREMONY_DIR:-}"

if [[ -z "$ceremony_root" || ! -f "$ceremony_root/package-authorization.ed25519" ]]; then
  echo "stage8b-generation2-composition-phase6: FAIL missing offline ceremony environment" >&2
  exit 1
fi
if [[ ! -x "$artifact_root/build-a/stage8b-readonly-preflight" || ! -f "$build_evidence" || -e "$evidence_dir/$evidence_name" || -e "$published_evidence" ]]; then
  echo "stage8b-generation2-composition-phase6: FAIL build artifacts missing or evidence exists" >&2
  exit 1
fi
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "stage8b-generation2-composition-phase6: FAIL tracked source tree is dirty" >&2
  exit 1
fi

python3 - "$artifact_root" "$build_evidence" "$(git rev-parse HEAD)" <<'PY'
import hashlib,json,pathlib,sys
root=pathlib.Path(sys.argv[1])
evidence=json.loads(pathlib.Path(sys.argv[2]).read_text())
if evidence.get('source_ref') != sys.argv[3] or evidence.get('result') != 'PASS':
    raise SystemExit('stage8b-generation2-composition-phase6: FAIL build/source binding')
for name, record in evidence.get('binaries', {}).items():
    path=root/'build-a'/name
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != record.get('build_a_sha256'):
        raise SystemExit(f'stage8b-generation2-composition-phase6: FAIL artifact drift {name}')
PY

python3 scripts/stage8b_p_r2b_generation2_composition_r0_materialize_phase6.py "$materialized"
mkdir -p "$cargo_home" "$evidence_dir"
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
    --bin stage8b-r2a5-authority-producer

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
  if [[ "${STAGE8B_R2B_KEEP_PHASE6_CONTAINER:-0}" != 1 ]]; then
    docker rm -f "$container" >/dev/null 2>&1 || true
  fi
  rm -f "$materialized"
  rmdir "$materialized_dir" 2>/dev/null || true
}
trap cleanup EXIT
docker run -d --privileged --platform linux/amd64 --cgroupns=host --name "$container" \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
  -v "$repo_root:/work:ro" \
  -v "$artifact_root/build-a:/artifacts/build-a:ro" \
  -v "$controlled_target/x86_64-unknown-linux-musl/release:/controlled/release:ro" \
  -v "$materialized:/phase6-rehearsal.sh:ro" \
  -v "$ceremony_root:/ceremony:ro" \
  -v "$evidence_dir:/evidence" --network none "$image" >/dev/null
for _ in $(seq 1 180); do
  state="$(docker exec "$container" systemctl is-system-running 2>/dev/null || true)"
  [[ "$state" = running || "$state" = degraded ]] && break
  sleep 1
done
docker exec "$container" bash /phase6-rehearsal.sh \
  /work /artifacts/build-a /controlled/release "/evidence/$evidence_name" /ceremony
python3 - "$evidence_dir/$evidence_name" "$build_evidence" \
  "$(git rev-parse HEAD)" "$(git rev-parse 'HEAD^{tree}')" "$published_evidence" <<'PY'
import hashlib,json,pathlib,sys
path=pathlib.Path(sys.argv[1])
build_path=pathlib.Path(sys.argv[2])
payload=json.loads(path.read_text())
if payload.get('result') != 'PASS' or payload.get('generation') != 2:
    raise SystemExit('stage8b-generation2-composition-phase6: FAIL rehearsal evidence')
payload.update({
    'source_ref':sys.argv[3],
    'source_tree':sys.argv[4],
    'linux_build_evidence_sha256':hashlib.sha256(build_path.read_bytes()).hexdigest(),
    'container_network_mode':'none',
    'production_authorization':'NOT_ISSUED',
    'production_credentials_installed':False,
    'isolated_rehearsal_package_destroyed_with_container':True,
})
encoded=json.dumps(payload,indent=2,sort_keys=True)+'\n'
path.write_text(encoded)
published=pathlib.Path(sys.argv[5])
if published.exists():
    raise SystemExit('stage8b-generation2-composition-phase6: FAIL published evidence exists')
published.write_text(encoded)
PY
echo "stage8b-generation2-composition-phase6: PASS generation=2 network=none authorization=NOT_ISSUED"
