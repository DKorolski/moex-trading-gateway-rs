#!/usr/bin/env python3
"""Isolated signer + fresh Redis witness, NOT O3 paper/operational acceptance.

Builds the fixture client and production signer in SEPARATE Cargo invocations.
Cargo artifact messages prove the signer's libraries exclude fixture features;
the example client itself has dev-dependency features and is not production ELF.
Only the selected schedule credential is mounted read-only, then staged in
container tmpfs. No image is committed, no network/host socket is exposed,
and no private data is returned. Caller must separately authorize key use.
"""
import argparse
import hashlib
import json
import pathlib
import stat
import subprocess
import time
import uuid
from stage8b_p1f_o3_local_components import ROOT as SOURCE, snapshot

IMAGE = "sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1"
TARGET = "stage8b-p1f-ib-target"
EXAMPLE = "/target/debug/examples/o3_local_signer_witness"
SIGNER = "/target/debug/stage8b-p1f-o3-signer"
PROVISIONER = "/target/debug/stage8b-p1f-o3-provisioner"
COMPOSITION = "/target/debug/stage8b-p1f-o3-composition"
CHECKPOINT = "/var/lib/moex-finam-p1-paper-o3-producer/schedule-high-water.json"
REDIS_IMAGE = "sha256:ee64a64eaab618d88051c3ade8f6352d11531fcf79d9a4818b9b183d8c1d18ba"
SCHEDULE_STREAM = "finam_imoexf_paper:{finam-imoexf-p1}:market-schedule"
SOCKET = "/run/moex-finam-p1-o3/schedule-signer.sock"
KEY = "/run/credentials/moex-finam-p1f-o3-signer.service/schedule.ed25519"
INPUT_KEY = "/root/o3-input-schedule-key"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("schedule_key", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--authorized-local-schedule-key-use", action="store_true", required=True)
    parser.add_argument("--linked-runtime", action="store_true",
                        help="operator-authorized recent synthetic input and disposable production bootstrap/runtime")
    parser.add_argument("--early-shutdown-regressions", action="store_true",
                        help="real SIGTERM/SIGINT at deterministic Published-window barrier; test ELF only")
    args = parser.parse_args()
    if args.early_shutdown_regressions and not args.linked_runtime:
        parser.error("--early-shutdown-regressions requires --linked-runtime")
    key = args.schedule_key.absolute()
    metadata = key.lstat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1
            or metadata.st_mode & 0o077 or key.resolve() != key or metadata.st_size not in (32, 64, 65)):
        raise SystemExit("credential custody rejected (contents not read)")
    output = args.output.absolute()
    output.mkdir(parents=True, exist_ok=False)
    docker = "/usr/local/bin/docker"
    name = "moex-o3-signer-local-" + uuid.uuid4().hex[:12]
    redis_name = name + "-redis"
    redis_created = False
    result = dict(schema_version=1, scope="ISOLATED_LINUX_SIGNER_AND_FRESH_PROVISIONER",
                  production_executable_linked_proof=False, paper_outcome=False,
                  operational_o3_ready=False, vps_mutated=False,
                  schedule_public_key="432a274889bc2b3a4492b3a1510f7356e1c2878337263e28472335f8600e4d99",
                  image=IMAGE, fixture_signing_trust=False, realtime=True,
                  schedule_key_used=False, container_removed=False, checks=[], verdict="FAIL",
                  redis_fresh_provisioning=False, redis_container_removed=False)
    signer = None
    log = None
    source_before = snapshot()
    (output / "checked-source.json").write_text(json.dumps(source_before, indent=2) + "\n")

    def run(label, command, expected=0):
        print(f"RUN {label}", flush=True)
        timeout = 1100 if label.startswith("o301-process-") else (600 if label.startswith("build-") else 60)
        done = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
        # Commands/credential source paths are intentionally not retained.
        (output / (label + ".log")).write_bytes(done.stdout)
        result["checks"].append(dict(name=label, exit_code=done.returncode,
            expected_exit_code=expected, log_sha256=hashlib.sha256(done.stdout).hexdigest()))
        if done.returncode != expected:
            raise RuntimeError(f"{label}: exit {done.returncode}; see retained log")
        return done.stdout

    def inside(label, *command, expected=0):
        return run(label, [docker, "exec", name, *command], expected)

    def start(logfile, phase):
        handle = logfile.open("wb")
        # exec preserves the PID even under Docker Desktop's amd64 emulation.
        # Do not discover by /proc argv: the signer disables ptrace/dumpability.
        child = subprocess.Popen([docker,"exec",name,"/bin/sh","-c",
            'printf "O3_SIGNER_PID=%s\\n" "$$"\nexec "$1" run "$2"',
            "o3-local-launch",SIGNER,phase], stdout=handle, stderr=subprocess.STDOUT)
        return child, handle

    def stop(label, logfile):
        line = logfile.read_text().splitlines()[0]
        if not line.startswith("O3_SIGNER_PID=") or not line[14:].isdigit() or int(line[14:]) <= 1:
            raise RuntimeError("invalid retained signer PID")
        inside(label, "kill", "-TERM", line[14:])

    try:
        build_prefix = [docker,"run","--rm","--platform","linux/amd64","--network","none",
            "--mount",f"type=bind,src={SOURCE},dst=/src,readonly",
            "--mount","type=volume,src=stage8b-p1f-ib-cargo-registry,dst=/usr/local/cargo/registry",
            "--mount",f"type=volume,src={TARGET},dst=/target",
            "-e","CARGO_TARGET_DIR=/target","-e","CARGO_NET_OFFLINE=true","-w","/src",IMAGE,
            "cargo","build","--offline","--locked","-p","finam-gateway","--no-default-features",
            "--message-format=json"]
        # Examples activate dev-dependency features under resolver=2. Building
        # --example and --bin together would contaminate the signer build.
        run("build-fixture-client", build_prefix + ["--example","o3_local_signer_witness"])
        if args.early_shutdown_regressions:
            from stage8b_p1f_o3_shutdown_witness import build_tests
            test_elf = build_tests(run, build_prefix, output)
        production_build = run("build-production-signer", build_prefix + ["--bin","stage8b-p1f-o3-signer",
            "--bin","stage8b-p1f-o3-provisioner","--bin","stage8b-p1f-o3-composition"])
        expected = {"finam_gateway", "runtime_durable_service", "strategy_runtime_core"}
        artifacts = []
        for line in production_build.splitlines():
            try:
                entry = json.loads(line)
            except (ValueError, UnicodeDecodeError):
                continue
            if entry.get("reason") == "compiler-artifact":
                if entry["target"]["name"] in expected or entry.get("executable") in (SIGNER, PROVISIONER, COMPOSITION):
                    artifacts.append(entry)
        libraries = {entry["target"]["name"]: entry["features"] for entry in artifacts
                     if entry["target"]["name"] in expected}
        if set(libraries) != expected or any(set(features) - {"default"} for features in libraries.values()):
            raise RuntimeError("production signer feature isolation rejected")
        for path in (SIGNER, PROVISIONER, COMPOSITION):
            executable = [entry for entry in artifacts if entry.get("executable") == path]
            if len(executable) != 1 or executable[0]["features"] or executable[0]["profile"]["test"]:
                raise RuntimeError("production executable artifact missing or contaminated")
        result["production_signer_library_features"] = libraries
        result["production_signer_fixture_features"] = False
        result["fixture_client_dev_features"] = True
        (output / "production-signer-artifacts.json").write_text(json.dumps(artifacts, indent=2) + "\n")
        if args.linked_runtime:
            from stage8b_p1f_o3_runtime_witness import build_runtime, wait_for_real_window
            build_runtime(run, build_prefix, output, result)
            wait_for_real_window()
            result["scope"] = "LOCAL_SYNTHETIC_PRODUCTION_EXECUTABLE_PAPER_LIFECYCLE"
        run("create", [docker, "run", "-d", "--name", name, "--platform", "linux/amd64",
            "--network", "none", "--pids-limit", "64", "--memory", "1g", "--ulimit", "core=0",
            "--mount", f"type=volume,src={TARGET},dst=/target,readonly",
            "--mount", f"type=bind,src={key},dst={INPUT_KEY},readonly",
            "--tmpfs", "/run:rw,nosuid,nodev,mode=0755,size=16m",
            IMAGE, "sleep", "2400" if args.linked_runtime else "180"])
        inside("group", "groupadd", "-g", "987", "moex-p1-paper")
        inside("user", "useradd", "-u", "987", "-g", "987", "-M", "-s", "/usr/sbin/nologin", "moex-p1-paper")
        # Docker Desktop bind-mount permission virtualization is not a custody
        # boundary. A native root-owned 0700 ancestor is required as well.
        inside("input-native-parent-custody", "chmod", "0700", "/root")
        inside("directories", "install", "-d", "-m", "0750", "-g", "987",
               "/etc/moex-finam-p1-paper/o2", "/etc/moex-finam-p1-paper/o3",
               "/var/lib/moex-finam-p1-paper/state", "/var/lib/moex-finam-p1-paper-control",
               "/run/moex-finam-p1-o3")
        inside("credential-directory", "install", "-d", "-m", "0700",
               "/run/credentials/moex-finam-p1f-o3-signer.service")
        inside("credential-stage-tmpfs", "install", "-m", "0400", "-o", "0", "-g", "0", INPUT_KEY, KEY)
        inside("public-output-directory", "install", "-d", "-o", "987", "-g", "987", "-m", "0700", "/run/o3-public")
        inside("producer-directory", "install", "-d", "-o", "987", "-g", "987", "-m", "0700",
            "/var/lib/moex-finam-p1-paper-o3-producer")
        if args.linked_runtime:
            from stage8b_p1f_o3_runtime_witness import prepare_directories, bootstrap
            prepare_directories(inside)
        inside("prepare", EXAMPLE, "prepare-linked" if args.linked_runtime else "prepare")
        # The signer runs as root, but the composition must read the same fixed
        # public inputs as the paper UID. Fixture creation inherits root:root;
        # install the explicit production read group before testing that child.
        for number, path in enumerate(("/etc/moex-finam-p1-paper/supervisor.json",
            "/etc/moex-finam-p1-paper/o3/composition.json",
            "/etc/moex-finam-p1-paper/o3/phase-manifest.signed.json",
            "/etc/moex-finam-p1-paper/o2/authority-public-key.hex")):
            inside(f"public-input-group-{number}", "chown", "0:987", path)
            inside(f"paper-public-input-readable-{number}", "runuser", "-u", "moex-p1-paper", "--", "test", "-r", path)
        if args.linked_runtime:
            bootstrap(inside, output, result)
        hashes = inside("elf-hashes", "sha256sum", SIGNER, PROVISIONER, COMPOSITION, EXAMPLE).decode().splitlines()
        result["elf_sha256"] = {line.split()[1]: line.split()[0] for line in hashes}
        phase = inside("phase-hash", "cat", "/run/o3-local-phase.sha256").decode().strip()
        if len(phase) != 64 or any(c not in "0123456789abcdef" for c in phase):
            raise RuntimeError("invalid local phase digest")
        result["local_phase_sha256"] = phase
        head_path = "/var/lib/moex-finam-p1-paper-control/authority/history-head.json"
        before_head = inside("authority-head-before", "cat", head_path)
        run("redis-create", [docker,"run","-d","--name",redis_name,
            "--network","container:"+name,"--pids-limit","32","--memory","128m",
            "--tmpfs","/data:rw,nosuid,nodev,size=64m",
            REDIS_IMAGE,"redis-server","--bind","127.0.0.1","--save","","--appendonly","no"])
        redis_created = True
        def redis(label, *args):
            return run(label, [docker,"exec",redis_name,"redis-cli","--json","-n","15",*args])
        for _ in range(100):
            ping = subprocess.run([docker,"exec",redis_name,"redis-cli","PING"],capture_output=True)
            if ping.returncode == 0 and ping.stdout.strip() == b"PONG":
                break
            time.sleep(0.05)
        else:
            raise RuntimeError("isolated Redis readiness timeout")
        if json.loads(redis("redis-empty-before", "DBSIZE")) != 0:
            raise RuntimeError("isolated Redis not fresh")
        inside("private-credential-not-readable-by-paper", "runuser", "-u", "moex-p1-paper", "--", "test", "-r", KEY, expected=1)
        inside("input-mount-not-readable-by-paper", "runuser", "-u", "moex-p1-paper", "--", "test", "-r", INPUT_KEY, expected=1)
        signer, log = start(output / "signer.log", phase)
        result["schedule_key_used"] = True
        for _ in range(100):
            if signer.poll() is not None:
                raise RuntimeError("signer exited before ready")
            if subprocess.run([docker, "exec", name, "test", "-S", SOCKET], capture_output=True).returncode == 0:
                break
            time.sleep(0.05)
        else:
            raise RuntimeError("signer readiness timeout")
        inside("ordinary-publisher-request", "runuser", "-u", "moex-p1-paper", "--", EXAMPLE, "request")
        inside("wrong-peer-uid", EXAMPLE, "wrong-peer")
        (output / "signed-envelope.json").write_bytes(inside("public-envelope-read", "cat", "/run/o3-public/signed-envelope.json"))
        envelope = json.loads((output / "signed-envelope.json").read_bytes())
        if envelope["key_generation"] != 2 or envelope["source_generation"] != "1":
            raise RuntimeError("generation mismatch")
        inside("initial-schedule-install", "install", "-m", "0440", "-o", "0", "-g", "987",
            "/run/o3-public/signed-envelope.json", "/etc/moex-finam-p1-paper/o3/initial-schedule.json")
        inside("missing-checkpoint-rejects-provision", PROVISIONER, "initialize-fixed", phase,
            "INITIALIZE-FRESH-O3-DB15", expected=70)
        inside("runtime-missing-checkpoint-rejected", "runuser", "-u", "moex-p1-paper", "--",
            COMPOSITION, "run", phase, expected=70)
        if json.loads(redis("redis-empty-after-missing-checkpoint", "DBSIZE")) != 0:
            raise RuntimeError("missing checkpoint wrote to Redis")
        # The production executable prepares and rereads its actual fixed-path
        # checkpoint. The example's separate valid envelope must NOT replace it.
        exact_initial = inside("production-prepare-initial-schedule", "runuser", "-u", "moex-p1-paper",
            "--", COMPOSITION, "prepare-initial-schedule", phase, "PREPARE-FRESH-O3-SCHEDULE")
        if exact_initial == (output/"signed-envelope.json").read_bytes():
            raise RuntimeError("negative control requires distinct signed initial bytes")
        initial_before = inside("prepared-checkpoint-before-repeat", "sha256sum", CHECKPOINT)
        inside("prepare-repeat-rejected", "runuser", "-u", "moex-p1-paper", "--", COMPOSITION,
            "prepare-initial-schedule", phase, "PREPARE-FRESH-O3-SCHEDULE", expected=70)
        if inside("prepared-checkpoint-after-repeat", "sha256sum", CHECKPOINT) != initial_before:
            raise RuntimeError("repeat preparation changed checkpoint")
        inside("different-signed-initial-rejected", PROVISIONER, "initialize-fixed", phase,
            "INITIALIZE-FRESH-O3-DB15", expected=70)
        if json.loads(redis("redis-empty-after-conflicting-envelope", "DBSIZE")) != 0:
            raise RuntimeError("conflicting signed envelope wrote to Redis")
        (output/"production-initial-envelope.json").write_bytes(exact_initial)
        run("copy-public-initial-envelope", [docker,"cp",str(output/"production-initial-envelope.json"),
            name+":/etc/moex-finam-p1-paper/o3/initial-schedule.json"])
        inside("initial-envelope-owner", "chown", "0:987", "/etc/moex-finam-p1-paper/o3/initial-schedule.json")
        inside("initial-envelope-mode", "chmod", "0440", "/etc/moex-finam-p1-paper/o3/initial-schedule.json")
        inside("provision-exact-fresh-db15", PROVISIONER, "initialize-fixed", phase, "INITIALIZE-FRESH-O3-DB15")
        if json.loads(redis("redis-count-after", "DBSIZE")) != 12:
            raise RuntimeError("unexpected DB15 inventory")
        rows_before = redis("schedule-exact-after-provision", "XRANGE", SCHEDULE_STREAM, "-", "+")
        if json.loads(rows_before) != [["1-0", ["payload", exact_initial.decode()]]]:
            raise RuntimeError("initial schedule is not exact authenticated bytes")
        for suffix, group in [("market-data:m10", "finam-imoexf-p1-m10-lifecycle-v1"),
                              ("stage7b:commands", "stage7b-paper-command-consumer-v1")]:
            stream = "finam_imoexf_paper:{finam-imoexf-p1}:" + suffix
            info = json.loads(redis("groups-"+suffix.replace(":", "-"), "XINFO", "GROUPS", stream))
            if len(info) != 1 or info[0]["name"] != group or info[0]["pending"] != 0:
                raise RuntimeError("accepted group inventory mismatch")
            if json.loads(redis("empty-"+suffix.replace(":", "-"), "XLEN", stream)) != 0:
                raise RuntimeError("initializer injected consumed input")
        inside("provision-repeat-rejected", PROVISIONER, "initialize-fixed", phase, "INITIALIZE-FRESH-O3-DB15", expected=70)
        if redis("schedule-after-rejected-repeat", "XRANGE", SCHEDULE_STREAM, "-", "+") != rows_before:
            raise RuntimeError("repeat changed schedule")
        if json.loads(redis("redis-count-after-rejected-repeat", "DBSIZE")) != 12:
            raise RuntimeError("repeat changed inventory")
        result["redis_fresh_provisioning"] = True
        result["production_initial_prepared_checkpoint"] = True
        result["missing_or_conflicting_checkpoint_rejected_before_redis"] = True
        result["runtime_missing_checkpoint_does_not_start_first_publication"] = True
        if args.linked_runtime:
            if args.early_shutdown_regressions:
                from stage8b_p1f_o3_shutdown_witness import early_shutdown
                early_shutdown(run, docker, name, phase, test_elf, output, result)
            from stage8b_p1f_o3_runtime_witness import witness
            witness(docker, name, redis_name, phase, output, result, inside, redis, signer)
            # The terminal guardian head may already have stopped the signer.
            if signer.poll() is None:
                stop("stop-signer-after-terminal", output / "signer.log")
            result["signer_exit_code"] = signer.wait(timeout=5)
            log.close()
            log = None
            result["verdict"] = "PASS_LOCAL_PRODUCTION_PAPER_LIFECYCLE"
            return
        inside("prepared-replay-publish-and-read", "runuser", "-u", "moex-p1-paper", "--", EXAMPLE, "replay-publish-and-read")
        rows = json.loads(redis("schedule-after-exact-replay", "XRANGE", SCHEDULE_STREAM, "-", "+"))
        if len(rows) != 3 or rows[0][1] != rows[1][1] or json.loads(rows[2][1][1])["publication_sequence"] != "2":
            raise RuntimeError("Prepared replay or normal sequence continuation drift")
        (output/"schedule-reader-high-water.json").write_bytes(inside("public-reader-high-water", "cat", "/run/o3-public/schedule-reader-high-water.json"))
        result["prepared_replay_and_reader_high_water"] = True
        stop("stop-signer-exact", output / "signer.log")
        result["signer_exit_code"] = signer.wait(timeout=5)
        log.close()
        if result["signer_exit_code"] != 0:
            raise RuntimeError("unclean signer exit")
        inside("socket-removed", "test", "!", "-e", SOCKET)
        # Restart only the signer against the SAME signed phase and history.
        # This is not publisher high-water or runtime restart acceptance.
        signer, log = start(output / "signer-restarted.log", phase)
        for _ in range(100):
            if signer.poll() is not None:
                raise RuntimeError("restarted signer exited before ready")
            if subprocess.run([docker,"exec",name,"test","-S",SOCKET],capture_output=True).returncode == 0:
                break
            time.sleep(0.05)
        else:
            raise RuntimeError("signer restart timeout")
        inside("ordinary-request-after-signer-restart", "runuser","-u","moex-p1-paper","--",EXAMPLE,"request-after-restart")
        (output / "signed-envelope-after-restart.json").write_bytes(inside("public-restart-envelope-read", "cat", "/run/o3-public/signed-envelope-after-restart.json"))
        stop("stop-restarted-signer", output / "signer-restarted.log")
        result["restarted_signer_exit_code"] = signer.wait(timeout=5)
        log.close()
        if result["restarted_signer_exit_code"] != 0:
            raise RuntimeError("unclean restarted signer exit")
        inside("restarted-socket-removed", "test","!","-e",SOCKET)
        after_head = inside("authority-head-after", "cat", head_path)
        if before_head != after_head:
            raise RuntimeError("signer changed authority history")
        result["authority_head_unchanged"] = True
        result["verdict"] = "PASS_SIGNER_AND_FRESH_REDIS_ONLY"
    except Exception as error:
        result["failure_reason"] = str(error)
        raise
    finally:
        if redis_created:
            result["redis_container_removed"] = subprocess.run([docker,"rm","-f",redis_name],capture_output=True,timeout=30).returncode == 0
        removed = subprocess.run([docker, "rm", "-f", name], capture_output=True, timeout=30)
        result["container_removed"] = removed.returncode == 0
        if signer is not None and signer.poll() is None:
            signer.wait(timeout=5)
        if log is not None:
            log.close()
        result["source_unchanged_during_checks"] = source_before == snapshot()
        if not result["source_unchanged_during_checks"] or not result["container_removed"] or (redis_created and not result["redis_container_removed"]):
            result["verdict"] = "FAIL"
        result["evidence_files_sha256"] = {str(p.relative_to(output)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(output.rglob("*")) if p.is_file() and p.name != "result.json"}
        (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        if args.linked_runtime:
            print(json.dumps({k: result[k] for k in ["verdict", "container_removed", "paper_outcome", "operational_o3_ready"]}), flush=True)
            if result["verdict"] != "PASS_LOCAL_PRODUCTION_PAPER_LIFECYCLE":
                raise RuntimeError(result.get("failure_reason", "local linked witness failed"))
    print(json.dumps({k: result[k] for k in ["verdict", "container_removed", "paper_outcome", "operational_o3_ready"]}))
    if result["verdict"] != "PASS_SIGNER_AND_FRESH_REDIS_ONLY":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
