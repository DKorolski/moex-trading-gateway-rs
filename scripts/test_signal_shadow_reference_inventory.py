import csv
import io
import tempfile
import unittest
from pathlib import Path

import signal_shadow_reference_inventory as inventory

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures/stage8b-p1f-parity"


class ReferenceInventoryTests(unittest.TestCase):
    def raw(self):
        return (FIXTURES / "imoexf_raw_10m_msk_utc.csv").read_bytes()

    def mutate(self, edit):
        reader = csv.DictReader(io.StringIO(self.raw().decode()))
        rows = list(reader)
        edit(rows)
        output = io.StringIO()
        writer = csv.DictWriter(output, fieldnames=reader.fieldnames)
        writer.writeheader()
        writer.writerows(rows)
        return output.getvalue().encode()

    def test_pinned_reference_and_actual_window(self):
        report = inventory.inventory(FIXTURES)
        self.assertEqual(report["raw_bar_count"], 7307)
        self.assertEqual(len(report["selected_python_rounds"]), 5)
        self.assertEqual(len(report["sessions"]), 9)
        self.assertTrue(all(s["observed_count"] == 101
                            for s in report["sessions"].values()))
        self.assertFalse(report["parity_accepted"])
        self.assertFalse(report["calendar_authority"])
        self.assertFalse(report["current_alor_live"])

    def test_hash_drift_refuses_before_data_interpretation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in inventory.PINS:
                (root / name).write_bytes((FIXTURES / name).read_bytes() + b"\n")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                inventory.inventory(root)

    def test_duplicate_bad_clock_and_nonfinite_refuse(self):
        mutations = [
            lambda rows: rows.insert(1, rows[0].copy()),
            lambda rows: rows[0].update(available_at_utc=rows[0]["bar_start_utc"]),
            lambda rows: rows[0].update(high="NaN"),
            lambda rows: rows[0].update(low="999999"),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with self.assertRaises(ValueError):
                    inventory.inspect_bars(self.mutate(mutation))

    def test_gap_is_reported_not_silently_filled(self):
        raw = self.mutate(lambda rows: rows.__delitem__(next(
            i for i, row in enumerate(rows)
            if row["bar_start_msk"] == "2026-09-21 10:00:00")))
        _, sessions = inventory.inspect_bars(raw)
        self.assertEqual(sessions["2026-09-21"]["observed_count"], 100)
        self.assertEqual(sessions["2026-09-21"]["missing_nominal_labels"], ["10:00"])


if __name__ == "__main__":
    unittest.main()
