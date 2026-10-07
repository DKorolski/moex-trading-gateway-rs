#!/usr/bin/env python3
"""Inherited sparse tamper controls plus timestamp-specific retained evidence."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import zipfile

import stage8b_p1f_o2_v4_timestamp_artifact as binding
import test_stage8b_p1f_o2_sparse_artifact as inherited

a = binding.base


def main(path):
    inherited.main(path)
    with zipfile.ZipFile(path) as z:
        pristine = {i.filename: (z.read(i), i.external_attr) for i in z.infolist()}
    q = a.P + "qualification/"
    cases = {
        "missing-linux-evidence": "evidence inventory",
        "missing-guardian": "release gate inventory",
        "empty-test-selection": "release test selection",
        "fractional-probe-disabled": "fixed probe output: probe-result.json",
        "ELF-after-drift": "release ELF continuity",
        "linux-network-enabled": "release isolation",
    }
    for label, expected_failure in cases.items():
        data = copy.deepcopy(pristine)
        tests = json.loads(data[q + "timestamp-tests.json"][0])
        if label == "missing-linux-evidence":
            data.pop(q + "timestamp-tests.json")
        else:
            if label == "missing-guardian":
                tests["records"].pop()
            elif label == "empty-test-selection":
                tests["records"][0]["command"][-3] = "does_not_exist"
            elif label == "fractional-probe-disabled":
                probe = json.loads(data[q + "probe-result.json"][0])
                probe["staged_exact_bytes_preserved"] = False
                data[q + "probe-result.json"] = (a.encoded(probe), data[q + "probe-result.json"][1])
            elif label == "ELF-after-drift":
                tests["binaries_after"][a.source.builder.BINS[0]] = "a" * 64
            else:
                tests["records"][0]["command"][6] = "host"
            data[q + "timestamp-tests.json"] = (a.encoded(tests), data[q + "timestamp-tests.json"][1])
        descriptor = a.P + "descriptor.json"
        meta = json.loads(data[descriptor][0])
        meta["generated_sha256"] = {n: a.sha(data[n][0]) for n in meta["generated_sha256"] if n in data}
        data[descriptor] = (a.encoded(meta), data[descriptor][1])
        with tempfile.TemporaryDirectory(prefix="v4-artifact-negative-") as temp:
            target = Path(temp) / path.name
            with zipfile.ZipFile(target, "w", zipfile.ZIP_STORED) as z:
                for name, (raw, mode) in data.items():
                    info = zipfile.ZipInfo(name)
                    info.external_attr = mode
                    z.writestr(info, raw)
            try:
                binding.check(target)
            except (ValueError, RuntimeError, KeyError, UnicodeError) as error:
                if str(error) != expected_failure:
                    raise SystemExit(f"FAIL wrong rejection for {label}: {error}") from error
                print("PASS " + label, flush=True)
            else:
                raise SystemExit("FAIL escaped negative: " + label)
    print("PASS v4-timestamp-artifact-negative 22/22", flush=True)


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve())
