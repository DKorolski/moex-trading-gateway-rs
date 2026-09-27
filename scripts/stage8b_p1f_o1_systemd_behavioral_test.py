#!/usr/bin/env python3
"""Behavioral controls for fail-closed O1 systemd observation."""

from __future__ import annotations

from copy import deepcopy

import stage8b_p1f_o1_collect as collector


def exact_units() -> list[dict[str, object]]:
    return [
        {
            "name": name,
            "kind": kind,
            "load_state": "loaded" if kind == "regular" else "not-applicable",
            "active_state": "inactive" if kind == "regular" else "not-applicable",
            "unit_file_state": "static",
            "fragment_path": fragment,
        }
        for name, kind, fragment in collector.P1_UNITS
    ]


def main() -> None:
    positive = exact_units()
    if not collector.p1_systemd_inactive(positive, True):
        raise SystemExit("positive inactive/static control rejected")
    mutations = []
    value = deepcopy(positive); value[0]["load_state"] = ""; mutations.append(("empty-load-state", value, True))
    value = deepcopy(positive); value[0]["unit_file_state"] = "enabled-runtime"; mutations.append(("enabled-runtime", value, True))
    value = deepcopy(positive); value[0]["active_state"] = "active"; mutations.append(("active", value, True))
    value = deepcopy(positive); value[2]["active_state"] = "inactive"; mutations.append(("template-active-query", value, True))
    value = deepcopy(positive); value[2]["fragment_path"] = ""; mutations.append(("template-identity", value, True))
    mutations.append(("query-failure", deepcopy(positive), False))
    for name, units, query_ok in mutations:
        if collector.p1_systemd_inactive(units, query_ok):
            raise SystemExit(f"negative systemd control accepted: {name}")
    print("PASS stage8b-p1f-o1-systemd-behavioral-test controls=7 positive=inactive-static negatives=6")


if __name__ == "__main__":
    main()
