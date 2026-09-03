#!/usr/bin/env python3
"""In-memory contract mutations that must be rejected by the P1-c checker."""

from __future__ import annotations

import importlib.util
import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts/stage8b_p1c_check.py"
spec = importlib.util.spec_from_file_location("stage8b_p1c_check", CHECKER_PATH)
assert spec and spec.loader
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


redis_source = checker.REDIS.read_text(encoding="utf-8")
semantic = checker.SEMANTIC.read_text(encoding="utf-8")
recovery = checker.RECOVERY.read_text(encoding="utf-8")
live_core = checker.LIVE_CORE.read_text(encoding="utf-8")


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) != 1:
        raise SystemExit(f"fixture drift: expected one occurrence of {old!r}")
    return source.replace(old, new, 1)


cases = [
    ("remove-xreadgroup", replace_once(redis_source, 'redis::cmd("XREADGROUP")', 'redis::cmd("XREAD")'), semantic, recovery, live_core),
    ("remove-xautoclaim", replace_once(redis_source, 'redis::cmd("XAUTOCLAIM")', 'redis::cmd("PING")'), semantic, recovery, live_core),
    ("remove-xpending", redis_source.replace('redis::cmd("XPENDING")', 'redis::cmd("PING")'), semantic, recovery, live_core),
    ("open-command-xack", redis_source.replace("return {'published', output_id}", "redis.call('XACK', source, group, source_id)\nreturn {'published', output_id}", 1), semantic, recovery, live_core),
    ("open-m10-trim", redis_source.replace("let count: usize = redis::cmd(\"XLEN\")", "let count: usize = redis::cmd(\"XTRIM\")", 1), semantic, recovery, live_core),
    ("remove-command-group-check", redis_source.replace("if not has_group(command_stream, command_group) then", "if false then", 1), semantic, recovery, live_core),
    ("remove-m10-group-check", redis_source.replace("if not has_group(stream, group) then", "if false then", 1), semantic, recovery, live_core),
    ("public-owner-extraction", redis_source, semantic, recovery.replace("pub(crate) fn into_p1c_parts", "pub fn into_p1c_parts", 1), live_core),
    ("remove-restart-command", redis_source, semantic, recovery, live_core.replace("projection.canonical_command.as_ref()?.clone()", "return None", 1)),
    ("open-paper-provider", redis_source.replace("const COMMAND_ENVELOPE_SOURCE", "use runtime_command_bridge::Stage7aPaperOutcomeProvider;\nconst COMMAND_ENVELOPE_SOURCE", 1), semantic, recovery, live_core),
]


failures: list[str] = []
for name, mutated_redis, mutated_semantic, mutated_recovery, mutated_live_core in cases:
    try:
        checker.validate(mutated_redis, mutated_semantic, mutated_recovery, mutated_live_core)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        failures.append(name)
        print(f"FAIL {name}")

if failures:
    print("mutations escaped: " + ", ".join(failures), file=sys.stderr)
    sys.exit(1)
print(f"PASS stage8b-p1c-negative-harness {len(cases)}/{len(cases)}")
