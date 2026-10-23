#!/usr/bin/env python3
"""Offline S2 reference: execute pinned ALOR Python, never the FINAM model.

Print deterministic JSON to stdout. --check compares it to a checked-in fixture.
Only selected pure definitions are compiled; fetch/main/import side effects are
not executed. The row loop gets observation-only before/finally hooks, including
continue paths. Trading statements and conditions are unmodified.
"""
import argparse
import ast
from dataclasses import dataclass
import hashlib
import json
import math
from pathlib import Path
import tarfile
from types import SimpleNamespace

import pandas as pd

AUDIT = "research/imoexf_no_overnight_audit_2026_09_27/run_audit.py"
FORMULAS = "research/imoexf_session_recut_2026_07_22/run_imoexf_session_recut_ab.py"
PINS = {
    AUDIT: "b0fa991a17aae6559ef26380bf9279eb7f770c139556b5094afecb2067d34a6b",
    FORMULAS: "ac49b2eff409a35875916ab3f51a3fa89c0208c81e317b83af37621b5d52f77e",
    "fixtures/imoexf_raw_10m_msk_utc.csv": "e4a30fb81a0abf5711a3702d19b918cad681820380f24422f6ba476b74f92cec",
    "fixtures/baseline07_python_reference_trades.csv": "2accaf0eec6c81f3446832059c231471415094c6cf43dc91e6ddbba85ccfe384",
}
SEED = [f"2026-09-{d}" for d in (15, 16, 17, 18)]
REPLAY = [f"2026-09-{d}" for d in range(21, 26)]
RUST_PINS = {
    "intraday_breakout.rs": "eeee8457c94d2ec7f9c46b8324f8f3a6d7b7e1429b4a0bc95593f43719a9267b",
    "mean_reversion.rs": "6360f76526980258aff76132c469b4e512f4697ac24a4a874dac9c4d8fad927c",
    "orchestrator.rs": "8b051119991c62c5f8e8a6dee2645177b788240b6662710e99ad525395f382e2",
    "types.rs": "8b515e252bc493890483793887248a6a12bedcf072ab87c574d4d3efd3b7eedc",
}
ARCHIVE_SHA256 = "9aa9bde789824b52aebdbde078fe197ab514b0a4e9dfa0b79dcbac8af661c681"


def verify_rust(oracle, freeze=None):
    for name, pin in RUST_PINS.items():
        assert hashlib.sha256((oracle / "rust" / name).read_bytes()).hexdigest() == pin, name
    if freeze:
        archive = freeze / "source/alor_reference_HEAD.tar.gz"
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == ARCHIVE_SHA256
        with tarfile.open(archive) as tar:
            for name in RUST_PINS:
                member = "alor-rs-main/strategy-runtime/src/strategies/hybrid_intraday/" + name
                # Only repository-dependent unit-test modules were omitted.
                prefix = tar.extractfile(member).read().decode().split("#[cfg(test)]")[0]
                assert (prefix.rstrip() + "\n").encode() == (oracle / "rust" / name).read_bytes()


def definitions(raw, names):
    tree = ast.parse(raw)
    nodes = [n for n in tree.body if isinstance(n, (ast.FunctionDef, ast.ClassDef))
             and n.name in names]
    assert {n.name for n in nodes} == set(names)
    return ast.Module(body=[ast.ImportFrom(module="__future__", names=[
        ast.alias(name="annotations")], level=0)] + nodes, type_ignores=[])


class ObserveRows(ast.NodeTransformer):
    count = 0
    continues = 0

    def visit_If(self, node):
        self.generic_visit(node)
        if ast.unparse(node.test) == "pos is not None or closed or hhmm >= '23:30'":
            assert len(node.body) == 1 and isinstance(node.body[0], ast.Continue)
            self.continues += 1
            node.body = ast.parse("observe_continue(locals())").body + node.body
        return node

    def visit_For(self, node):
        self.generic_visit(node)
        if isinstance(node.target, ast.Name) and node.target.id == "row":
            self.count += 1
            node.body = ast.parse("observe_before(locals())").body + [ast.Try(
                body=node.body, handlers=[], orelse=[],
                finalbody=ast.parse("observe_after(locals())").body)]
        return node


def traced_bo_tree(raw):
    tree = definitions(raw, ["build_daily_context", "bo_signal"])
    bo = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "bo_signal")
    # Compile diagnostic predicates FROM the pinned original AST, not FINAM
    # formulas. Hooks run at the exact original return site. No branch changes.
    gate = bo.body[0].test
    assert isinstance(gate, ast.BoolOp) and isinstance(gate.op, ast.Or)
    entries = [n for n in bo.body if isinstance(n, ast.If) and isinstance(n.test, ast.BoolOp)
               and isinstance(n.test.op, ast.And)]
    assert len(entries) == 2 and len(gate.values) == 2
    expressions = [gate.values[0], gate.values[1], entries[0].test.values[2], entries[1].test.values[2]]
    predicates = [compile(ast.Expression(body=e), FORMULAS, "eval") for e in expressions]

    class Returns(ast.NodeTransformer):
        count = 0

        def visit_Return(self, node):
            if isinstance(node.value, ast.Constant) and node.value.value is None:
                tag = "gate" if self.count == 0 else "veto"
                self.count += 1
                node.value = ast.Call(func=ast.Name(id="observe_bo_none", ctx=ast.Load()),
                                     args=[ast.Constant(value=tag), ast.Call(
                                         func=ast.Name(id="locals", ctx=ast.Load()), args=[], keywords=[])],
                                     keywords=[])
            return node

    returns = Returns()
    returns.visit(bo)
    assert returns.count == 2
    return tree, predicates


def generate(root, data_root=None):
    data_root = data_root or root / "fixtures"
    sources = {}
    for name, pin in PINS.items():
        path = data_root / Path(name).name if name.startswith("fixtures/") else root / name
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != pin:
            raise ValueError("reference hash mismatch: " + name)
        sources[name] = raw
    env = dict(pd=pd, math=math, dataclass=dataclass,
               TRANSITION=pd.Timestamp("2026-07-14"), TICK=0.5,
               BROKER_ROUNDTRIP_RUB=1.0, POINT_VALUE_RUB=10.0,
               TAKER_FEE_RATE=0.000066, __name__=__name__)
    branch_trace = {}
    bo_tree, predicates = traced_bo_tree(sources[FORMULAS])

    def observe_bo_none(tag, local):
        if tag == "gate":
            wait = eval(predicates[0], env, local)
            assert wait or eval(predicates[1], env, local)
            branch_trace.update(reason="before_wait" if wait else "minimum_range", veto=None)
        else:
            branch_trace.update(reason="breakout_veto", veto=dict(
                long_allowed=local["can_long"], short_allowed=local["can_short"],
                long_unused=not local["was_long"], short_unused=not local["was_short"],
                above_long=bool(eval(predicates[2], env, local)),
                below_short=bool(eval(predicates[3], env, local))))
        return None

    env["observe_bo_none"] = observe_bo_none
    exec(compile(ast.fix_missing_locations(bo_tree), FORMULAS, "exec"), env)
    env["model"] = SimpleNamespace(**env)
    steps, previous, seed_state = [], {}, {}

    def before(local):
        # Copy, do not retain mutable engine locals.
        previous.clear()
        branch_trace.clear()
        previous.update(position=None if local["pos"] is None else dict(local["pos"]),
                        was_long=local["was_long"], was_short=local["was_short"])

    def observe_continue(local):
        # This hook is inside the pinned replay's actual compound continue.
        if local["pos"] is not None:
            branch_trace.update(reason="position_hold", veto=None)
        elif local["closed"]:
            branch_trace.update(reason="exit_just_closed", veto=None)
        else:
            assert local["hhmm"] >= "23:30"
            branch_trace.update(reason="eod_entry_cutoff", veto=None)

    def after(local):
        ts = local["ts"]
        pre, post = previous["position"], local["pos"]
        action = "none"
        reason = None
        if pre is None and post is not None:
            action = "enter_" + post["side"]
            reason = "breakout_" + post["side"]
        elif pre is not None and post is None:
            action = "exit"
            reason = ("breakout_eod_exit" if local["reason"] == "eod_2330" else
                      "breakout_" + local["reason"].removeprefix("bo_") + "_" + pre["side"])
        pc, pr = local["pc"], local["pr"]
        # These are independently computed Python facts. No Rust observation
        # feeds this generator. day_before_close in Rust is a rollover register
        # (previous completed close), not Python's shifted(2) daily column.
        state = dict(prev_day_close=pc, prev_day_range=pr,
                     prev_day_return=local["ret"], day_before_close=pc,
                     current_day_high=float(local["high"]),
                     current_day_low=float(local["low"]),
                     current_day_close=float(local["close"]),
                     last_bar_close=float(local["close"]),
                     last_day_local=str(ts.date()),
                     today_start_local=local["clock"].isoformat(),
                     was_long_today=bool(local["was_long"]),
                     was_short_today=bool(local["was_short"]),
                     current_owner="intraday_breakout" if post else None,
                     current_side=post["side"] if post else None,
                     last_position_qty=(1 if post["side"] == "long" else -1) if post else 0,
                     bo_levels=dict(long_entry=pc+.53*pr, short_entry=pc-.53*pr,
                                    long_stop1=pc+.51*pr, short_stop1=pc-.51*pr,
                                    long_stop2=pc-.35*pr, short_stop2=pc+.35*pr))
        if str(ts.date()) not in REPLAY:
            # Warmup uses price history only, not seed-session trade effects.
            seed_state.clear()
            seed_state.update(state, was_long_today=False, was_short_today=False,
                              current_owner=None, current_side=None, last_position_qty=0)
            return
        no_signal = None
        if action == "none":
            assert branch_trace, "untraced Python no-intent branch"
            no_signal = dict(branch_trace)
        steps.append(dict(label=ts.isoformat(), action=action, reason=reason,
                          no_intent_trace=no_signal, after_feedback=state,
                          position_before=(1 if pre["side"] == "long" else -1) if pre else 0,
                          used_before=[bool(previous["was_long"]), bool(previous["was_short"])]))

    env.update(observe_before=before, observe_after=after, observe_continue=observe_continue)
    tree = definitions(sources[AUDIT], ["Profile", "prepare", "tick", "replay"])
    observer = ObserveRows()
    tree = observer.visit(tree)
    assert observer.count == 1 and observer.continues == 1
    exec(compile(ast.fix_missing_locations(tree), AUDIT, "exec"), env)
    raw = pd.read_csv(data_root / "imoexf_raw_10m_msk_utc.csv")
    raw.index = pd.to_datetime(raw.pop("bar_start_msk"))
    raw = raw.loc[[str(t.date()) in SEED + REPLAY for t in raw.index],
                  ["open", "high", "low", "close", "volume"]]
    profile = env["Profile"]("bo_only_weekday07_control", mr="off", weekend_state=False)
    # Seed days build daily anchors. Python has no persistent inter-day book;
    # assert every seed session closes flat (replay itself rejects otherwise).
    trades, _ = env["replay"](raw, profile)
    untouched = dict(env)
    exec(compile(ast.fix_missing_locations(definitions(sources[FORMULAS],
        ["build_daily_context", "bo_signal"])), FORMULAS, "exec"), untouched)
    untouched["model"] = SimpleNamespace(**untouched)
    exec(compile(ast.fix_missing_locations(definitions(sources[AUDIT], ["replay"])),
                 AUDIT, "exec"), untouched)
    original_trades, _ = untouched["replay"](raw, profile)
    pd.testing.assert_frame_equal(trades, original_trades)
    selected = trades[trades.entry_bar.dt.strftime("%Y-%m-%d").isin(REPLAY)]
    frozen = pd.read_csv(data_root / "baseline07_python_reference_trades.csv")
    frozen = frozen[frozen.entry_bar.str[:10].isin(REPLAY)]
    cols = ["side", "entry_bar", "exit_bar", "entry_price", "exit_price", "exit_reason"]
    actual = selected[cols].astype(str).to_dict("records")
    assert actual == frozen[cols].astype(str).to_dict("records"), "frozen rounds changed"
    probes = []
    for hour, pr, ret, used_long, used_short, close in [
        (9, 20., 0., False, False, 120.),  # wait first
        (9, .5, 0., False, False, 120.),  # wait precedes minimum range
        (10, .5, 0., False, False, 120.),
        (10, 1.01, 0., False, False, 120.),
        (10, 20., 0., False, False, 110.6),  # strict threshold equality
        (10, 20., 0., False, False, 89.4),
        (10, 20., -.03, False, False, 120.),
        (10, 20., .03, False, False, 80.),
        (10, 20., 0., True, False, 120.),
        (10, 20., 0., False, True, 80.),
        (10, 20., 0., False, False, 120.),
        (10, 20., 0., False, False, 80.),
    ]:
        ts = pd.Timestamp("2026-09-21") + pd.Timedelta(hours=hour)
        clock = pd.Timestamp("2026-09-21T07:00:00")
        args = (ts, SimpleNamespace(close=close), clock, 100., pr, ret, used_long, used_short, 3.)
        branch_trace.clear()
        signal = env["bo_signal"](*args)
        assert signal == untouched["bo_signal"](*args)
        probes.append(dict(label=ts.isoformat(), prev_close=100., prev_range=pr, prev_return=ret,
                           used_long=used_long, used_short=used_short, close=close,
                           signal=signal, no_intent_trace=dict(branch_trace) if signal is None else None))
    return dict(schema_version=1, source_sha256=PINS, seed_dates=SEED,
                replay_dates=REPLAY, feedback="synthetic close-tick full-fill; event=result+1ms; apply=result+3ms; 1 lot; zero fee/slippage in virtual book",
                limitations=["Historical M10, not observed quotes or current ALOR live.",
                             "Python has atomic fills, no pending/owner callback lifecycle.",
                             "No-intent diagnostics are captured at pinned Python return/continue sites and compared to actual Rust branch traces.",
                             "Python stop-before-EOD versus Rust EOD-before-stop precedence is not qualified by non-colliding history."],
                seed_after=seed_state, rounds=actual, steps=steps, entry_probes=probes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--freeze", type=Path, help="optional original freeze cross-check")
    parser.add_argument("--check", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    verify_rust(repo / "fixtures/signal-shadow-state-parity/oracle", args.freeze)
    result = generate(args.freeze) if args.freeze else generate(
        repo / "fixtures/signal-shadow-state-parity/oracle",
        repo / "fixtures/stage8b-p1f-parity")
    if args.check:
        assert result == json.loads(args.check.read_text()), "fixture differs from independent rerun"
        print(f"Independent rerun matches: {len(result['steps'])} steps, {len(result['rounds'])} rounds")
    else:
        print(json.dumps(result, indent=2, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
