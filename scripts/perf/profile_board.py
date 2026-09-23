#!/usr/bin/env python3
"""Where a board's host instructions actually go, per function.

`board_perf.py` already runs the whole simulation under callgrind on every
perf run -- and then deletes the profile. `measure_once` writes
`--callgrind-out-file` into a `TemporaryDirectory`, reads one total out of
stderr, and throws the per-function attribution away. That attribution is the
expensive part and it is already paid for.

This keeps it, for the question the perf table raises but cannot answer:

    ARM / nRF / STM32 / RP   ~54 Ir/step   width  511.9
    esp32c3  (RISC-V)        201.8         width  511.9
    atmega328p (AVR)         241.3         width   25.0
    esp32s3  (Xtensa)        364.8         width 1023.9   <-- WIDEST, still 6.7x
    esp32    (Xtensa)        442.4         width  511.9

`esp32s3` has the widest CPU batch of any board in the fleet and still costs
6.7x ARM. A wide batch rules per-batch overhead OUT, so the cost is inside the
instruction loop -- but "inside the instruction loop" is a region, not a
function. This prints the functions.

Usage:

    scripts/perf/profile_board.py --cli target/release/labwired esp32 nrf52840

Runs each board the way the perf gate does, keeps the callgrind file and pipes
it through `callgrind_annotate`, so two boards can be read side by side. Names
a cause or refuses to: if `callgrind_annotate` is missing, that is an error
rather than a silent empty report.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import board_perf as bp  # noqa: E402

#: How many functions to show. Enough to see a shape, few enough to read.
TOP_N = 25

#: Simulated steps per profile.
#:
#: NOT `board_perf.STEPS_LOW`. That number exists to be one end of a slope, and
#: at 200_000 steps a fast board has not done enough simulating to outweigh its
#: own startup: the first nrf52840 profile taken this way was ~51% unsafe-libyaml
#: and malloc -- the chip descriptor being PARSED -- with 3.13% in
#: `run_t16_fast_block`, the actual CPU work. That profile says nothing about
#: the simulator and everything about serde_yaml.
#:
#: A profile has the opposite requirement from a slope: it wants the run long
#: enough that fixed startup is noise. Ten million keeps the fast boards honest
#: and still finishes under callgrind in a couple of minutes.
PROFILE_STEPS = 10_000_000


def profile(cli: Path, board: str, mode: str, steps: int, out_dir: Path) -> Path:
    """Run one measurement under callgrind and KEEP the profile."""
    chip = bp.CHIP_DIR / f"{board}.yaml"
    if not chip.exists():
        raise FileNotFoundError(f"no chip descriptor for board '{board}': {chip}")
    # Same fixture resolution the gate uses -- a board maps to a linked spin
    # loop by (arch, flash base, ram base), not by name.
    chips = bp.discover_chips()
    fixture = bp.fixture_for(chips[board])
    if fixture is None:
        raise RuntimeError(
            f"{board} has no perf fixture (its memory map matches no linked "
            "spin loop), so there is nothing to profile"
        )
    built, skipped = bp.build_fixtures({fixture})
    if fixture in skipped:
        raise RuntimeError(f"fixture '{fixture}' was skipped: {skipped[fixture]}")
    firmware = built[fixture]
    out = out_dir / f"cg-{board}-{mode}.out"
    cmd = [
        "valgrind",
        "--tool=callgrind",
        f"--callgrind-out-file={out}",
        "--cache-sim=no",
        "--branch-sim=no",
        str(cli),
        "run",
        "--chip",
        str(chip),
        "--firmware",
        str(firmware),
        "--max-steps",
        str(steps),
    ]
    if mode == bp.MODE_BATCH:
        cmd.append("--batched")
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if not out.exists():
        raise RuntimeError(
            f"callgrind wrote no profile for {board} [{mode}]:\n{proc.stderr[-2000:]}"
        )
    return out


def annotate(profile_path: Path) -> str:
    tool = shutil.which("callgrind_annotate")
    if tool is None:
        # Refuse rather than print nothing. An empty report reads as "no hot
        # functions", which is the opposite of what a missing tool means.
        raise RuntimeError(
            "callgrind_annotate is not on PATH. It ships with valgrind; "
            "without it this script can produce a profile but cannot read it."
        )
    proc = subprocess.run(
        [tool, "--auto=no", str(profile_path)], capture_output=True, text=True
    )
    if proc.returncode != 0:
        raise RuntimeError(f"callgrind_annotate failed:\n{proc.stderr[-2000:]}")
    return proc.stdout


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("boards", nargs="+", help="board ids, e.g. esp32 nrf52840")
    ap.add_argument("--cli", required=True, type=Path)
    ap.add_argument("--mode", default=bp.MODE_BATCH, help="batch (default) or step")
    ap.add_argument(
        "--steps",
        type=int,
        default=PROFILE_STEPS,
        help=f"simulated steps (default {PROFILE_STEPS}); see PROFILE_STEPS",
    )
    args = ap.parse_args()

    # One board that cannot be profiled must not veto the others. The first
    # real use of this script asked for esp32, nrf52840 and esp32c3; the
    # Xtensa toolchain install had hit a GitHub 403, esp32 raised, and the two
    # boards that WOULD have profiled were never reached -- so a transient
    # rate limit cost the whole comparison. A profile is a comparison, and a
    # partial comparison still answers something; nothing answers nothing.
    failed: list[tuple[str, str]] = []
    with tempfile.TemporaryDirectory() as tmp:
        for board in args.boards:
            print(f"\n{'=' * 72}\n{board} [{args.mode}], {args.steps} steps\n{'=' * 72}")
            try:
                path = profile(args.cli, board, args.mode, args.steps, Path(tmp))
                text = annotate(path)
            except (RuntimeError, FileNotFoundError, KeyError) as exc:
                # Loud, and still non-zero at the end. Skipping quietly is how
                # a missing board becomes "that share looked normal".
                print(f"  SKIPPED: {exc}")
                failed.append((board, str(exc)))
                continue
            # `callgrind_annotate` leads with a summary then the function table;
            # both are worth keeping, so trim by lines rather than by section.
            for line in text.splitlines()[: TOP_N + 25]:
                print(line)

    if failed:
        print(f"\n{len(failed)} of {len(args.boards)} board(s) could not be profiled:")
        for board, why in failed:
            print(f"  {board}: {why}")
        # Non-zero even though the other boards printed: a comparison missing
        # the board under investigation is not the comparison that was asked
        # for, and a green exit would say it was.
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
