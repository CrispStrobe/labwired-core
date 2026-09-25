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
import os
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

# Sentinel: run with NO `--firmware` (a rom-boot image handed over in the env).
NO_FIRMWARE = Path("<none>")


def profile(
    cli: Path,
    board: str,
    mode: str,
    steps: int,
    out_dir: Path,
    firmware: Path | None = None,
    run_args: list[str] | None = None,
    run_env: dict[str, str] | None = None,
) -> Path:
    """Run one measurement under callgrind and KEEP the profile.

    `firmware=None` profiles the board's perf-spin fixture. A spin loop is the
    right instrument for per-instruction overhead and the wrong one for hit
    rates -- how often a fast path is taken is a property of the firmware --
    so a real image can be named instead, with the extra CLI arguments and
    environment its boot path needs (e.g. an ESP32-S3 `--rom-boot` image is
    handed over in `LABWIRED_ESP32S3_FLASH`, with no `--firmware` at all:
    pass `firmware=None` plus `run_args`/`run_env` and `--no-fixture`).
    """
    chip = bp.CHIP_DIR / f"{board}.yaml"
    if not chip.exists():
        raise FileNotFoundError(f"no chip descriptor for board '{board}': {chip}")
    if firmware is None:
        # Same fixture resolution the gate uses -- a board maps to a linked
        # spin loop by (arch, flash base, ram base), not by name.
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
    elif firmware != NO_FIRMWARE and not firmware.exists():
        raise FileNotFoundError(f"firmware image not found: {firmware}")
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
        *([] if firmware == NO_FIRMWARE else ["--firmware", str(firmware)]),
        "--max-steps",
        str(steps),
        *(run_args or []),
    ]
    if mode == bp.MODE_BATCH:
        cmd.append("--batched")
    env = {**os.environ, **(run_env or {})}
    print(f"$ {' '.join(f'{k}={v}' for k, v in (run_env or {}).items())} {' '.join(cmd[cmd.index(str(cli)):])}")
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env)
    # The run's own last words: a real image that faulted at step 2,000 would
    # otherwise profile as a fast machine.
    tail = [
        line
        for line in proc.stdout.strip().splitlines() + proc.stderr.strip().splitlines()
        if not line.startswith("==")  # valgrind's own trailer, not the run's
    ]
    for line in tail[-6:]:
        print(f"  | {line}")
    # Exit 2 is the CLI's EXIT_CONFIG_ERROR, and clap's usage error: the
    # simulator never ran, and what callgrind measured is argument parsing.
    # (Run 36041333522 profiled exactly that -- 1.7M Ir of a usage message --
    # and uploaded it as a result.) Other non-zero exits are legitimate: a
    # real image may stop on the step limit or a tolerated sim error.
    if proc.returncode == 2:
        raise RuntimeError(
            f"{board}: the CLI refused the invocation (exit 2: config/usage error), "
            "so there is no simulation to profile"
        )
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
    ap.add_argument(
        "--top",
        type=int,
        default=TOP_N,
        help=(
            f"functions to print per board (default {TOP_N}). Raise it to ask "
            "where a cost WENT: a fixed depth answers 'what is hot' but not "
            "'did this move or vanish', because a function that fell out of "
            "the top N is indistinguishable from one that was deleted."
        ),
    )
    ap.add_argument(
        "--keep",
        type=Path,
        default=None,
        help=(
            "directory to keep the raw callgrind profiles in. The function table "
            "cannot say WHICH LINE of an inlined-into function is hot; the raw "
            "profile can, via `callgrind_annotate <out> <source-file>` against "
            "the same commit's tree. Default: a temp dir, discarded."
        ),
    )
    ap.add_argument(
        "--firmware",
        type=Path,
        default=None,
        help="profile this image instead of the board's perf-spin fixture",
    )
    ap.add_argument(
        "--no-firmware",
        action="store_true",
        help="pass no --firmware at all (e.g. an ESP32-S3 --rom-boot image in the env)",
    )
    ap.add_argument(
        "--run-arg",
        action="append",
        default=[],
        help="extra argument for `labwired run` (repeatable), e.g. --run-arg=--rom-boot",
    )
    ap.add_argument(
        "--run-env",
        action="append",
        default=[],
        help="KEY=VALUE for the profiled run's environment (repeatable)",
    )
    args = ap.parse_args()

    run_env = {}
    for kv in args.run_env:
        k, sep, v = kv.partition("=")
        if not sep:
            ap.error(f"--run-env wants KEY=VALUE, got {kv!r}")
        run_env[k] = v

    with tempfile.TemporaryDirectory() as tmp:
        out_dir = Path(tmp)
        if args.keep is not None:
            args.keep.mkdir(parents=True, exist_ok=True)
            out_dir = args.keep
        failed = []
        for board in args.boards:
            print(f"\n{'=' * 72}\n{board} [{args.mode}], {args.steps} steps\n{'=' * 72}")
            # One board's failure (a missing Xtensa toolchain, say) must not
            # silently take the rest of the request down with it -- nor pass:
            # it is reported here AND in the exit status.
            try:
                path = profile(
                    args.cli,
                    board,
                    args.mode,
                    args.steps,
                    out_dir,
                    firmware=NO_FIRMWARE if args.no_firmware else args.firmware,
                    run_args=args.run_arg,
                    run_env=run_env,
                )
                text = annotate(path)
            except (RuntimeError, FileNotFoundError) as e:
                print(f"FAILED {board}: {e}")
                failed.append(board)
                continue
            # `callgrind_annotate` leads with a summary then the function table;
            # both are worth keeping, so trim by lines rather than by section.
            # The +25 is the summary block, which is why this is not simply
            # `--top` lines: the header is not part of the ranking.
            shown = text.splitlines()[: args.top + 25]
            for line in shown:
                print(line)
            # Say so when there was more. A truncated profile that does not
            # announce its own truncation is how "X is absent from the profile"
            # gets read as "X costs nothing" -- the two are only the same claim
            # when the whole ranking was shown.
            hidden = len(text.splitlines()) - len(shown)
            if hidden > 0:
                print(
                    f"... {hidden} further line(s) not shown "
                    f"(--top {args.top}); re-run with a larger --top to see them"
                )
    if failed:
        print(f"\nFAILED boards (no profile): {' '.join(failed)}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
