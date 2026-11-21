"""Disposable LOCAL production-process witness; never operational O3 authority.

Imported only by the explicit --linked-runtime path. No SSH, host socket,
clock override, callback injection, accepted O2 root or network broker access.
The helper generates inputs, but bootstrap and runtime are production ELFs.
"""
import datetime
import hashlib
import json
import pathlib
import struct
import subprocess
import time

BOOTSTRAP = "/target/debug/stage8b-p1-paper-supervisor"
PARENT = "/target/debug/stage8b-p1f-local-supervisor"
FIXED_CHILD = "/usr/local/libexec/moex/stage8b-p1f-o3-composition"
CREDENTIALS = "/run/o3-local-lifecycle"
SOURCE = "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json"
PREFIX = "finam_imoexf_paper:{finam-imoexf-p1}:"
PRODUCER = "/var/lib/moex-finam-p1-paper-o3-producer"


def build_runtime(run, build_prefix, output, result):
    command = list(build_prefix)
    command[command.index("finam-gateway")] = "runtime-durable-service"
    raw = run("build-production-runtime", command + ["--bin", pathlib.Path(BOOTSTRAP).name,
                                                    "--bin", pathlib.Path(PARENT).name])
    artifacts = []
    for line in raw.splitlines():
        try:
            entry = json.loads(line)
        except ValueError:
            continue
        if entry.get("reason") == "compiler-artifact":
            if entry["target"]["name"] in ("runtime_durable_service", "strategy_runtime_core") or entry.get("executable") in (BOOTSTRAP, PARENT):
                artifacts.append(entry)
    libraries = {a["target"]["name"]: a["features"] for a in artifacts if "lib" in a["target"]["kind"]}
    if set(libraries) != {"runtime_durable_service", "strategy_runtime_core"} or any(set(f) - {"default"} for f in libraries.values()):
        raise RuntimeError("production runtime feature contamination")
    for binary in (BOOTSTRAP, PARENT):
        found = [a for a in artifacts if a.get("executable") == binary]
        if len(found) != 1 or found[0]["features"] or found[0]["profile"]["test"]:
            raise RuntimeError("runtime production artifact missing/contaminated")
    (output / "production-runtime-artifacts.json").write_text(json.dumps(artifacts, indent=2) + "\n")
    result["production_runtime_library_features"] = libraries


def wait_for_real_window():
    utc = datetime.timezone.utc
    now = datetime.datetime.now(utc)
    earliest = now.replace(hour=7, minute=0, second=5, microsecond=0)
    if now.weekday() >= 5 or now.hour >= 20:
        raise RuntimeError("no automatic next-day/session fallback")
    while datetime.datetime.now(utc) < earliest:
        remaining = (earliest - datetime.datetime.now(utc)).total_seconds()
        print(f"WAIT real model window, {int(remaining)}s; no phase/container/key staged yet", flush=True)
        time.sleep(min(60, max(0, remaining)))
    # A first fresh semantic bar must fit the ordinary bootstrap truth TTL
    # (300 seconds). Wait BEFORE generating observations/phase/credentials,
    # rather than backdating the clock or extending the accepted TTL.
    while True:
        position = datetime.datetime.now(utc).timestamp() % 600
        if 360 <= position < 550:
            break
        remaining = (360 - position) % 600
        print(f"WAIT local fresh-bootstrap slot, {int(remaining)}s; no observation generated", flush=True)
        time.sleep(min(60, remaining))


def prepare_directories(inside):
    inside("bootstrap-input-directory", "install", "-d", "-m", "0750", "-g", "987",
           "/etc/moex-finam-p1-paper/bootstrap")
    inside("local-state-parent-owner", "chown", "987:987", "/var/lib/moex-finam-p1-paper/state")
    inside("local-state-parent-mode", "chmod", "0700", "/var/lib/moex-finam-p1-paper/state")
    inside("local-hmac-directory", "install", "-d", "-m", "0700", "-o", "987", "-g", "987", CREDENTIALS)
    inside("fixed-child-directory", "install", "-d", "-m", "0755", "/usr/local/libexec/moex")
    inside("install-fixed-production-child", "install", "-m", "0755", "-o", "0", "-g", "0",
           "/target/debug/stage8b-p1f-o3-composition", FIXED_CHILD)


def bootstrap(inside, output, result):
    inside("bootstrap-source-read-group", "chown", "0:987", SOURCE)
    inside("local-hmac-owner", "chown", "987:987", CREDENTIALS + "/stage8b-p1-lifecycle.key")
    provenance = inside("local-input-provenance", "cat", "/run/o3-public/local-fixture-provenance.json")
    (output / "local-fixture-provenance.json").write_bytes(provenance)
    hashes = inside("production-runtime-elf-hashes", "sha256sum", BOOTSTRAP, PARENT, FIXED_CHILD).decode().splitlines()
    result["production_runtime_elf_sha256"] = {line.split()[1]: line.split()[0] for line in hashes}
    inside("ordinary-production-bootstrap", "runuser", "-u", "moex-p1-paper", "--", "env",
           "CREDENTIALS_DIRECTORY=" + CREDENTIALS, BOOTSTRAP, "bootstrap",
           "/etc/moex-finam-p1-paper/supervisor.json", "CREATE_NEW_STAGE8B_P1_DURABLE_ROOT")
    result["ordinary_production_bootstrap"] = True


def journal_records(data):
    if data[:10] != b"S6JNLV1\0\0\1":
        raise RuntimeError("journal header mismatch")
    previous = hashlib.sha256(b"stage6-journal-frame-genesis-v1").digest()
    offset, records = 10, []
    while offset < len(data):
        prefix = data[offset:offset + 42]
        if len(prefix) != 42 or prefix[:6] != b"S6F1\0\1" or prefix[10:] != previous:
            raise RuntimeError("journal frame chain mismatch")
        length = struct.unpack(">I", prefix[6:10])[0]
        end = offset + 42 + length
        body, retained = data[offset + 42:end], data[end:end + 32]
        expected = hashlib.sha256(b"stage6-journal-frame-v1" + prefix[4:] + body).digest()
        if len(body) != length or retained != expected:
            raise RuntimeError("journal frame digest mismatch")
        records.append(json.loads(body))
        previous, offset = retained, end + 32
    return records


def witness(docker, name, redis_name, phase, output, result, inside, redis, signer):
    """Observe real processes and public evidence; never mutate a runtime root."""
    def raw(*command):
        done = subprocess.run([docker, "exec", name, *command], capture_output=True, timeout=20)
        if done.returncode != 0:
            raise RuntimeError("local read-only process probe failed")
        return done.stdout

    def probe_redis(*command):
        done = subprocess.run([docker, "exec", redis_name, "redis-cli", "--json", "-n", "15", *command],
                              capture_output=True, timeout=20)
        if done.returncode:
            raise RuntimeError("local Redis observation failed")
        return json.loads(done.stdout)

    roots = raw("find", "/var/lib/moex-finam-p1-paper/state", "-mindepth", "1", "-maxdepth", "1", "-type", "d").decode().splitlines()
    if len(roots) != 1 or not pathlib.PurePosixPath(roots[0]).name.startswith("stage7b-"):
        raise RuntimeError("ordinary bootstrap did not produce exactly one local root")
    root = roots[0]

    def capture_root(label):
        target = output / label
        target.mkdir()
        files = raw("find", root, "-maxdepth", "1", "-type", "f").decode().splitlines()
        retained = {}
        for path in sorted(files):
            # This fresh production root holds authenticated public state, not
            # the HMAC credential (which remains outside it in /run tmpfs).
            filename = pathlib.PurePosixPath(path).name
            if filename.endswith(".key") or "credential" in filename:
                raise RuntimeError("unexpected credential in public runtime root")
            data = raw("cat", path)
            (target / filename).write_bytes(data)
            retained[filename] = hashlib.sha256(data).hexdigest()
        (target / "files-sha256.json").write_text(json.dumps(retained, indent=2) + "\n")
        # Bootstrap transaction/receipt live BESIDE the root, not inside it.
        # Explicit filenames prevent a vacuous "first-boot unchanged" check.
        parent_state = target / "parent-state"
        parent_state.mkdir()
        for filename in (".stage8b-p1-first-boot-receipt-v2.json",
                         ".stage8b-p1-first-boot-transaction-v4.json"):
            data = raw("cat", str(pathlib.PurePosixPath(root).parent / filename))
            (parent_state / filename).write_bytes(data)
            retained["parent-state/" + filename] = hashlib.sha256(data).hexdigest()
        (target / "files-sha256.json").write_text(json.dumps(retained, indent=2) + "\n")
        return target, retained

    initial, initial_hashes = capture_root("root-after-bootstrap")
    if journal_records((initial / "stage6.journal").read_bytes()):
        raise RuntimeError("bootstrap injected lifecycle journal rows")
    logfile = output / "local-supervisor.log"
    handle = logfile.open("wb")
    parent = subprocess.Popen([docker, "exec", "-e", "CREDENTIALS_DIRECTORY=" + CREDENTIALS, name,
        "/bin/sh", "-c", 'printf "O3_PARENT_PID=%s\\n" "$$"\nexec "$1" run "$2"',
        "o3-local-parent", PARENT, phase], stdout=handle, stderr=subprocess.STDOUT)
    parent_pid = None
    try:
        for _ in range(100):
            lines = logfile.read_text().splitlines()
            if lines and lines[0].startswith("O3_PARENT_PID="):
                parent_pid = int(lines[0].split("=", 1)[1])
                if parent_pid <= 1:
                    raise RuntimeError("invalid parent PID")
                break
            if parent.poll() is not None:
                raise RuntimeError("production parent failed before PID record")
            time.sleep(.05)
        else:
            raise RuntimeError("production parent PID timeout")

        def child_pid():
            children = raw("cat", f"/proc/{parent_pid}/task/{parent_pid}/children").split()
            if len(children) != 1 or not children[0].isdigit():
                return None
            return int(children[0])

        def require_alive():
            if parent.poll() is not None:
                raise RuntimeError(f"production supervisor exited {parent.returncode}; see local-supervisor.log")
            if signer.poll() is not None:
                raise RuntimeError("production signer exited during live local witness")

        def settled():
            for suffix, group in (("market-data:m10", "finam-imoexf-p1-m10-lifecycle-v1"),):
                groups = probe_redis("XINFO", "GROUPS", PREFIX + suffix)
                if len(groups) != 1 or groups[0]["name"] != group or groups[0]["pending"] != 0 or groups[0].get("lag") != 0:
                    return False
            if (probe_redis("XLEN", PREFIX + "market-data:m10") != 2
                    or probe_redis("XLEN", PREFIX + "stage7b:commands") != 1):
                return False
            # Command publication is durable evidence for the typed paper
            # owner, not authorization to start a separate command consumer.
            groups = probe_redis("XINFO", "GROUPS", PREFIX + "stage7b:commands")
            if (len(groups) != 1 or groups[0]["name"] != "stage7b-paper-command-consumer-v1"
                    or groups[0]["pending"] != 0 or groups[0]["last-delivered-id"] != "0-0"
                    or groups[0].get("lag") != 1):
                raise RuntimeError("unexpected independent command-stream consumption")
            # Paper execution commits its ACK/trade/position to the authenticated
            # replacement package and Stage6 journal, not producer truth streams.
            # Requiring synthetic rows in those streams would test the wrong path.
            seal = json.loads(raw("cat", root + "/stage7b-recovery.seal"))
            authenticated = json.loads(bytes(seal["stage6d_authenticated_restart_package"]))
            package = json.loads(bytes(authenticated["stage5g_restart_package"]))
            extension = json.loads(package["stage5g_extension_json"])
            envelope = json.loads(package["envelope_json"])
            state = envelope["strategy_state"]["strategy_state_json"]
            semantic = extension.get("p1_semantic_commit") or {}
            last_id = probe_redis("XINFO", "STREAM", PREFIX + "market-data:m10")["last-generated-id"]
            if semantic.get("m10_redis_id") != last_id:
                return False
            slots = extension["order_position_state"]["slots"]
            if len(slots) != 1:
                raise RuntimeError("expected exactly one durable paper lifecycle")
            slot = slots[0]
            if (slot["ack"]["state"] != "resolved" or slot["terminal"] is not True
                    or len(slot["order_events"]) != 1 or len(slot["trades"]) != 1
                    or slot["order_events"][0]["order"]["status"] != "Filled"
                    or slot["position"][0] != slot["ack"]["canonical_total_sequence"] + 1
                    or float(slot["position"][1]["qty"]) != 1
                    or state["HybridIntradayRuntime"]["last_position_qty"] != 1):
                raise RuntimeError("post-Market continuation lost canonical paper truth/runtime position")
            return True

        # Real producer sleeps until each M10 close; no test clock is passed to
        # bootstrap/production runtime. The signed phase is independently bound.
        end = time.monotonic() + 1700
        next_progress = 0
        while time.monotonic() < end:
            require_alive()
            if settled():
                break
            if time.monotonic() >= next_progress:
                print("WAIT production O3: real closed M10 -> paper settlement -> XACK", flush=True)
                next_progress = time.monotonic() + 60
            time.sleep(2)
        else:
            raise RuntimeError("no actual paper outcome within bounded local phase")
        time.sleep(2)
        require_alive()
        before, hashes_before = capture_root("root-before-child-kill")
        records = journal_records((before / "stage6.journal").read_bytes())
        kinds = [row.get("event_kind") for row in records]
        required = {"request_accepted", "dispatch_attempt_recorded", "broker_order_observed", "broker_trade_observed", "request_finalized"}
        if (not required.issubset(kinds) or any(kinds.count(kind) != 1 for kind in required)
                or not any(row.get("payload") == {"payload_kind":"request_finalized", "disposition":"completed"} for row in records)):
            raise RuntimeError("no completed durable paper request evidence")
        (output / "journal-records-before-restart.json").write_text(json.dumps(records, indent=2) + "\n")
        outcome_rows = {}
        for suffix in ("market-data:m10", "stage7b:commands", "stage7b:acks", "broker:orders", "broker:trades", "broker:positions", "stage7b:dlq"):
            outcome_rows[suffix] = redis("before-restart-" + suffix.replace(":", "-"), "XRANGE", PREFIX + suffix, "-", "+")
        high_water = {filename: inside("before-restart-" + filename, "cat", PRODUCER + "/" + filename)
                      for filename in ("m10-high-water.json", "m10-predecessor.json")}
        old_pid = child_pid()
        if old_pid is None:
            raise RuntimeError("expected exactly one supervised production child")
        inside("kill-exact-production-child", "kill", "-KILL", str(old_pid))
        for _ in range(60):
            require_alive()
            new_pid = child_pid()
            if new_pid is not None and new_pid != old_pid:
                break
            time.sleep(.5)
        else:
            raise RuntimeError("ordinary production restart did not occur")
        # Allow multiple schedule/health cycles and ordinary replay admission.
        for _ in range(15):
            time.sleep(1)
            require_alive()
        if child_pid() != new_pid or not settled():
            raise RuntimeError("restarted production child is not stable/settled")
        _, hashes_after = capture_root("root-after-child-restart")
        if hashes_before.get("stage6.journal") != hashes_after.get("stage6.journal"):
            raise RuntimeError("restart duplicated/changed completed durable requests")
        if hashes_before.get("stage7b-recovery.seal") != hashes_after.get("stage7b-recovery.seal"):
            raise RuntimeError("restart repeated semantic/truth callbacks or replaced their committed state")
        for filename, data in high_water.items():
            if inside("after-restart-" + filename, "cat", PRODUCER + "/" + filename) != data:
                raise RuntimeError("observed M10 high-water changed on restart")
        for suffix, rows in outcome_rows.items():
            if redis("after-restart-" + suffix.replace(":", "-"), "XRANGE", PREFIX + suffix, "-", "+") != rows:
                raise RuntimeError("restart duplicated a paper outcome/source row")
        for filename, digest in initial_hashes.items():
            if "first-boot" in filename and hashes_after.get(filename) != digest:
                raise RuntimeError("restart changed first-boot receipt/marker")
        telemetry = {}
        for suffix in ("health", "readiness"):
            rows = json.loads(redis("after-restart-" + suffix, "XRANGE", PREFIX + suffix, "-", "+"))
            telemetry[suffix] = [json.loads(dict(zip(fields[::2], fields[1::2]))["payload"])
                                 for row_id, fields in rows if row_id != "1-0"]
            if not telemetry[suffix]:
                raise RuntimeError("missing ordinary runtime telemetry")
            for envelope in telemetry[suffix]:
                payload = envelope["payload"]
                if payload.get("paper_only") is not True or any(payload.get(k) is not False for k in
                        ("finam_transport_attached", "broker_network_dispatch_attached", "runtime_live", "real_orders")):
                    raise RuntimeError("closed surface telemetry mismatch")
        if telemetry["readiness"][-1]["payload"]["phase"] != "paper_ready" or telemetry["health"][-1]["payload"]["status"] != "healthy":
            raise RuntimeError("restarted runtime is not paper-ready/healthy")
        result.update(paper_outcome=True, production_executable_linked_proof=True,
                      separate_command_consumer_activated=False,
                      child_restart=True, child_pids=[old_pid, new_pid], journal_frame_count=len(records),
                      journal_event_kinds=kinds, exact_high_water_preserved=True,
                      source_xack_after_paper_settlement=True)
        start_stop = time.monotonic()
        inside("stop-production-parent", "kill", "-TERM", str(parent_pid))
        result["parent_exit_code"] = parent.wait(timeout=35)
        result["bounded_stop_seconds"] = time.monotonic() - start_stop
        if result["parent_exit_code"] != 0 or result["bounded_stop_seconds"] > 31:
            raise RuntimeError("production bounded stop failed")
        # Stop success must be the production supervisor's own terminal
        # result, not just a successfully terminated docker exec client.
        terminal = [json.loads(line) for line in logfile.read_text().splitlines()
                    if line.startswith('{')]
        (output / "parent-terminal-results.json").write_text(json.dumps(terminal, indent=2) + "\n")
        if (len(terminal) != 1 or terminal[0].get("manifest_sha256") != phase
                or terminal[0].get("child_starts") != 2
                or terminal[0].get("last_exit_code") != 0
                or terminal[0].get("force_killed") is not False
                or terminal[0].get("disposition") != "GRACEFUL_STOP"):
            raise RuntimeError("production supervisor terminal receipt mismatch")
        result["bounded_stop"] = True
        for suffix in ("health", "readiness", "runtime:state"):
            redis("final-" + suffix.replace(":", "-"), "XRANGE", PREFIX + suffix, "-", "+")
        inside("authority-head-after-local-stop", "cat", "/var/lib/moex-finam-p1-paper-control/authority/history-head.json")
        capture_root("root-after-bounded-stop")
    except Exception:
        # Retain public failure evidence BEFORE disposal; no rerun/reset/repair.
        try:
            capture_root("root-at-failure")
            for suffix in ("market-data:m10", "stage7b:commands", "stage7b:acks", "broker:trades", "health", "readiness", "runtime:state"):
                redis("failure-" + suffix.replace(":", "-"), "XRANGE", PREFIX + suffix, "-", "+")
            inside("authority-head-at-failure", "cat", "/var/lib/moex-finam-p1-paper-control/authority/history-head.json")
        except Exception as observation_error:
            result["failure_capture_error"] = str(observation_error)
        raise
    finally:
        if parent.poll() is None and parent_pid is not None:
            subprocess.run([docker, "exec", name, "kill", "-TERM", str(parent_pid)], capture_output=True, timeout=5)
            try:
                parent.wait(timeout=35)
            except subprocess.TimeoutExpired:
                subprocess.run([docker, "exec", name, "kill", "-KILL", str(parent_pid)], capture_output=True, timeout=5)
                parent.wait(timeout=5)
        handle.close()
