#!/usr/bin/env python3
"""Create the immutable committed owner-loop correction handoff."""

from __future__ import annotations

import make_stage8b_p1e_i1_owner_loop_handoff as base
import stage8b_p1e_i1_owner_loop_correction_handoff_safety_check as safety


def main() -> None:
    base.safety = safety
    base.main()


if __name__ == "__main__":
    main()
