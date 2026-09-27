#!/usr/bin/env python3
"""Negative controls for the Stage 8B-P1F parity comparator."""

from __future__ import annotations

import argparse
import csv
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "stage8b_p1f_alor_finam_compare_rounds.py"
FIXTURES = ROOT / "fixtures" / "stage8b-p1f-parity"
SPEC = importlib.util.spec_from_file_location("stage8b_compare", SCRIPT)
assert SPEC and SPEC.loader
COMPARE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = COMPARE
SPEC.loader.exec_module(COMPARE)


def args(actual: Path, source: Path | None = None) -> argparse.Namespace:
    return argparse.Namespace(
        profile="baseline07",
        source_m10=str(source or FIXTURES / "imoexf_raw_10m_msk_utc.csv"),
        expected=str(FIXTURES / "baseline07_python_reference_trades.csv"),
        actual=str(actual),
        price_tolerance=1e-8,
        output="unused.json",
    )


class ComparatorContractTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="stage8b-parity-")
        self.directory = Path(self.temp.name)
        self.expected = FIXTURES / "baseline07_python_reference_trades.csv"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def copy_rows(self) -> tuple[list[str], list[dict[str, str]]]:
        with self.expected.open(newline="", encoding="utf-8") as stream:
            reader = csv.DictReader(stream)
            return list(reader.fieldnames or []), list(reader)

    def write_rows(self, name: str, fields: list[str], rows: list[dict[str, str]]) -> Path:
        path = self.directory / name
        with path.open("w", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, fieldnames=fields)
            writer.writeheader()
            writer.writerows(rows)
        return path

    def test_frozen_fixture_self_check_passes(self) -> None:
        result = COMPARE.compare(args(self.expected))
        self.assertTrue(result["pass"])
        self.assertEqual(result["expected"], 38)

    def test_nan_is_rejected(self) -> None:
        fields, rows = self.copy_rows()
        rows[0]["entry_price"] = "NaN"
        actual = self.write_rows("nan.csv", fields, rows)
        with self.assertRaises(COMPARE.ContractError):
            COMPARE.compare(args(actual))

    def test_empty_replay_is_rejected(self) -> None:
        fields, _ = self.copy_rows()
        actual = self.write_rows("empty.csv", fields, [])
        with self.assertRaises(COMPARE.ContractError):
            COMPARE.compare(args(actual))

    def test_wrong_profile_is_rejected(self) -> None:
        fields, rows = self.copy_rows()
        rows[0]["profile"] = "bo_only_weekend09"
        actual = self.write_rows("wrong-profile.csv", fields, rows)
        with self.assertRaises(COMPARE.ContractError):
            COMPARE.compare(args(actual))

    def test_changed_price_fails_comparison(self) -> None:
        fields, rows = self.copy_rows()
        rows[0]["entry_price"] = str(float(rows[0]["entry_price"]) + 0.5)
        actual = self.write_rows("price-drift.csv", fields, rows)
        result = COMPARE.compare(args(actual))
        self.assertFalse(result["pass"])
        self.assertEqual(len(result["execution_price_drift"]), 1)

    def test_unbound_source_dataset_is_rejected(self) -> None:
        source = self.directory / "m10.csv"
        source.write_bytes((FIXTURES / "imoexf_raw_10m_msk_utc.csv").read_bytes() + b"\n")
        with self.assertRaises(COMPARE.ContractError):
            COMPARE.compare(args(self.expected, source=source))


if __name__ == "__main__":
    unittest.main()
