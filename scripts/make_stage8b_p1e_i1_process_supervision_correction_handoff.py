#!/usr/bin/env python3
"""Create the immutable I1 process-supervision correction handoff."""

from __future__ import annotations

import make_stage8b_p1e_i1_process_supervision_handoff as base
import stage8b_p1e_i1_process_supervision_correction_handoff_safety_check as safety


def main() -> None:
    base.safety = safety
    base.main()


if __name__ == "__main__":
    main()
