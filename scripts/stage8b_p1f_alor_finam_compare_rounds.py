#!/usr/bin/env python3
"""Fail-closed comparison of frozen ALOR and normalized FINAM model rounds."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import sys
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path


RAW_M10_SHA256 = "e4a30fb81a0abf5711a3702d19b918cad681820380f24422f6ba476b74f92cec"
RAW_M10_ROWS = 7_307
BAR_FORMAT = "%Y-%m-%d %H:%M:%S"
UTC_FORMAT = "%Y-%m-%d %H:%M:%S%z"
ROUND_REQUIRED_FIELDS = {
    "profile",
    "component",
    "side",
    "entry_bar",
    "exit_bar",
    "entry_price",
    "exit_price",
    "exit_reason",
}
RAW_REQUIRED_FIELDS = {
    "bar_start_msk",
    "bar_start_utc",
    "available_at_utc",
    "open",
    "high",
    "low",
    "close",
    "volume",
}


@dataclass(frozen=True)
class FrozenProfile:
    csv_profile: str
    expected_rows: int
    expected_sha256: str


PROFILES = {
    "baseline07": FrozenProfile(
        csv_profile="bo_only_weekday07_control",
        expected_rows=38,
        expected_sha256="2accaf0eec6c81f3446832059c231471415094c6cf43dc91e6ddbba85ccfe384",
    ),
    "candidate09": FrozenProfile(
        csv_profile="bo_only_weekend09",
        expected_rows=41,
        expected_sha256="4c813979ac5609b5b7abbdb8ed3876eee1374ed2923863180cb418a22ce8d3a7",
    ),
}


class ContractError(ValueError):
    pass


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require_finite(row: dict[str, str], field: str, path: Path, line: int) -> float:
    raw = row.get(field, "")
    try:
        value = float(raw)
    except ValueError as error:
        raise ContractError(f"{path}:{line}: {field} is not numeric") from error
    if not math.isfinite(value):
        raise ContractError(f"{path}:{line}: {field} is not finite")
    return value


def read_dict_rows(path: Path, required: set[str]) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream)
        fields = set(reader.fieldnames or [])
        missing = sorted(required - fields)
        if missing:
            raise ContractError(f"{path}: missing columns: {', '.join(missing)}")
        return list(reader)


def validate_raw_m10(path: Path) -> dict[str, object]:
    digest = sha256(path)
    if digest != RAW_M10_SHA256:
        raise ContractError(f"{path}: frozen M10 SHA-256 mismatch")
    rows = read_dict_rows(path, RAW_REQUIRED_FIELDS)
    if len(rows) != RAW_M10_ROWS:
        raise ContractError(f"{path}: expected {RAW_M10_ROWS} M10 rows, got {len(rows)}")
    seen: set[str] = set()
    for line, row in enumerate(rows, start=2):
        label = row["bar_start_msk"]
        if not label or label in seen:
            raise ContractError(f"{path}:{line}: empty or duplicate model label")
        seen.add(label)
        try:
            datetime.strptime(label, BAR_FORMAT)
            start = datetime.strptime(row["bar_start_utc"], UTC_FORMAT)
            available = datetime.strptime(row["available_at_utc"], UTC_FORMAT)
        except ValueError as error:
            raise ContractError(f"{path}:{line}: invalid timestamp") from error
        if start.tzinfo != timezone.utc or available.tzinfo != timezone.utc:
            raise ContractError(f"{path}:{line}: timestamps must be UTC")
        if available - start != timedelta(minutes=10):
            raise ContractError(f"{path}:{line}: bar availability is not start + 10m")
        values = {field: require_finite(row, field, path, line) for field in ("open", "high", "low", "close", "volume")}
        if values["volume"] < 0 or values["low"] > values["high"]:
            raise ContractError(f"{path}:{line}: invalid OHLCV")
        if values["high"] < max(values["open"], values["close"]) or values["low"] > min(values["open"], values["close"]):
            raise ContractError(f"{path}:{line}: inconsistent OHLC")
    return {
        "sha256": digest,
        "rows": len(rows),
        "first_model_label_msk": rows[0]["bar_start_msk"],
        "last_model_label_msk": rows[-1]["bar_start_msk"],
    }


def load_rounds(path: Path, profile: FrozenProfile, expected: bool) -> dict[tuple[str, str, str], dict[str, str]]:
    rows = read_dict_rows(path, ROUND_REQUIRED_FIELDS)
    if not rows:
        raise ContractError(f"{path}: empty replay is not acceptance evidence")
    if expected and len(rows) != profile.expected_rows:
        raise ContractError(f"{path}: expected fixture must contain {profile.expected_rows} rounds")
    keyed: dict[tuple[str, str, str], dict[str, str]] = {}
    for line, row in enumerate(rows, start=2):
        if row["profile"] != profile.csv_profile:
            raise ContractError(f"{path}:{line}: profile binding mismatch")
        if row["component"] != "BO" or row["side"] not in {"long", "short"}:
            raise ContractError(f"{path}:{line}: unsupported component or side")
        if not row["exit_reason"]:
            raise ContractError(f"{path}:{line}: empty exit_reason")
        try:
            datetime.strptime(row["entry_bar"], BAR_FORMAT)
            datetime.strptime(row["exit_bar"], BAR_FORMAT)
        except ValueError as error:
            raise ContractError(f"{path}:{line}: invalid model-bar label") from error
        require_finite(row, "entry_price", path, line)
        require_finite(row, "exit_price", path, line)
        key = (row["component"], row["side"], row["entry_bar"])
        if key in keyed:
            raise ContractError(f"{path}:{line}: duplicate round key {key}")
        keyed[key] = row
    return keyed


def compare(args: argparse.Namespace) -> dict[str, object]:
    profile = PROFILES[args.profile]
    expected_path = Path(args.expected)
    actual_path = Path(args.actual)
    source_path = Path(args.source_m10)
    expected_sha = sha256(expected_path)
    if expected_sha != profile.expected_sha256:
        raise ContractError(f"{expected_path}: frozen expected SHA-256 mismatch")
    source = validate_raw_m10(source_path)
    expected = load_rounds(expected_path, profile, expected=True)
    actual = load_rounds(actual_path, profile, expected=False)

    signal: list[dict[str, object]] = []
    price: list[dict[str, object]] = []
    for key in sorted(expected.keys() & actual.keys()):
        for field in ("exit_bar", "exit_reason"):
            if expected[key][field] != actual[key][field]:
                signal.append({"key": key, "field": field, "expected": expected[key][field], "actual": actual[key][field]})
        for field in ("entry_price", "exit_price"):
            if abs(float(expected[key][field]) - float(actual[key][field])) > args.price_tolerance:
                price.append({"key": key, "field": field, "expected": expected[key][field], "actual": actual[key][field]})

    result: dict[str, object] = {
        "schema_version": 1,
        "domain": "moex.stage8b.p1f.alor-finam-round-parity.v1",
        "profile": args.profile,
        "csv_profile": profile.csv_profile,
        "source_m10": source,
        "expected_fixture_sha256": expected_sha,
        "actual_sha256": sha256(actual_path),
        "price_tolerance": args.price_tolerance,
        "expected": len(expected),
        "actual": len(actual),
        "common": len(expected.keys() & actual.keys()),
        "missing": sorted(expected.keys() - actual.keys()),
        "extra": sorted(actual.keys() - expected.keys()),
        "signal_or_contract_drift": signal,
        "execution_price_drift": price,
    }
    result["pass"] = not any(result[key] for key in ("missing", "extra", "signal_or_contract_drift", "execution_price_drift"))
    return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", required=True, choices=sorted(PROFILES))
    parser.add_argument("--source-m10", required=True)
    parser.add_argument("--expected", required=True)
    parser.add_argument("--actual", required=True)
    parser.add_argument("--price-tolerance", type=float, default=1e-8)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    if not math.isfinite(args.price_tolerance) or args.price_tolerance < 0:
        parser.error("--price-tolerance must be finite and non-negative")
    return args


def main() -> int:
    args = parse_args()
    try:
        result = compare(args)
    except (ContractError, OSError) as error:
        print(f"ERROR {error}", file=sys.stderr)
        return 2
    Path(args.output).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in ("profile", "expected", "actual", "common", "pass")}, sort_keys=True))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
