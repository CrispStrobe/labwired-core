#!/usr/bin/env python3
# LabWired - Firmware Simulation Platform
# Copyright (C) 2026 Andrii Shylenko
# SPDX-License-Identifier: MIT
"""Score fidelity cases on LabWired and, when asked, on Renode.

One table: name, firmware, system, success marker, silicon verdict, Renode
platform. A marker in the captured UART is PASS. Its absence is FAIL. A case
that never started is ERROR, which is not a silicon FAIL.

LabWired must match the silicon verdict. Renode's verdict is the one this run
printed. It is stored, not checked against a historical column.

    python3 scripts/perf/compare_renode_cases.py --engines labwired
    python3 scripts/perf/compare_renode_cases.py \\
        --labwired target/release/labwired \\
        --renode /path/to/renode \\
        --engines labwired,renode
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MAX_STEPS = 200_000
RENODE_RUNFOR_S = "0.05"
HOST_TIMEOUT_S = 180

@dataclass(frozen=True)
class Case:
    name: str
    elf: str
    system: str
    marker: str
    expected: str
    renode_repl: str
    renode_uart: str


# Silicon expected is PASS or FAIL. Renode is not listed here.
CASES: tuple[Case, ...] = (
    Case(
        "control",
        "examples/f103-fidelity-bench/firmware/build/control.elf",
        "examples/f103-fidelity-bench/system.yaml",
        "BENCH_UART_OK",
        "PASS",
        "platforms/cpus/stm32f103.repl",
        "usart1",
    ),
    Case(
        "clockbug",
        "examples/f103-fidelity-bench/firmware/build/clockbug.elf",
        "examples/f103-fidelity-bench/system.yaml",
        "BENCH_UART_OK",
        "FAIL",
        "platforms/cpus/stm32f103.repl",
        "usart1",
    ),
    Case(
        "gpiobug",
        "examples/f103-fidelity-bench/firmware/build/gpiobug.elf",
        "examples/f103-fidelity-bench/system.yaml",
        "BENCH_GPIO_OK",
        "FAIL",
        "platforms/cpus/stm32f103.repl",
        "usart1",
    ),
    Case(
        "rambug",
        "examples/f103-fidelity-bench/firmware/build/rambug.elf",
        "examples/f103-fidelity-bench/system.yaml",
        "BENCH_RAM_OK",
        "FAIL",
        "platforms/cpus/stm32f103.repl",
        "usart1",
    ),
    Case(
        "irqtime",
        "examples/f103-fidelity-bench/firmware/build/irqtime.elf",
        "examples/f103-fidelity-bench/system.yaml",
        "BENCH_UIF_OK",
        "FAIL",
        "platforms/cpus/stm32f103.repl",
        "usart1",
    ),
    Case(
        "nvicclear",
        "examples/f103-fidelity-bench/firmware/build/nvicclear.elf",
        "examples/f103-fidelity-bench/system.yaml",
        "BENCH_NVIC_OK",
        "FAIL",
        "platforms/cpus/stm32f103.repl",
        "usart1",
    ),
    Case(
        "usartmux",
        "examples/f103-fidelity-bench/firmware/build/usartmux.elf",
        "examples/f103-fidelity-bench/system.yaml",
        "BENCH_UART_OK",
        "FAIL",
        "platforms/cpus/stm32f103.repl",
        "usart1",
    ),
    Case(
        "nrf-control",
        "examples/nrf52840-fidelity-bench/firmware/build/nrf-control.elf",
        "examples/nrf52840-fidelity-bench/system.yaml",
        "BENCH_NRF_OK",
        "PASS",
        "platforms/cpus/nrf52840.repl",
        "sysbus.uart0",
    ),
    Case(
        "rtcclock",
        "examples/nrf52840-fidelity-bench/firmware/build/rtcclock.elf",
        "examples/nrf52840-fidelity-bench/system.yaml",
        "BENCH_RTC_CPU",
        "FAIL",
        "platforms/cpus/nrf52840.repl",
        "sysbus.uart0",
    ),
    # ERASEPAGE of the first page past the 1 MB map. The marker means
    # that erase blanked the last real page, which silicon does not do.
    Case(
        "flashbound",
        "examples/nrf52840-fidelity-bench/firmware/build/flashbound.elf",
        "examples/nrf52840-fidelity-bench/system.yaml",
        "BENCH_FLASH_BOUND",
        "FAIL",
        "platforms/cpus/nrf52840.repl",
        "sysbus.uart0",
    ),
)


def score_text(text: str | None, marker: str) -> str:
    """PASS if the marker was captured, FAIL if the engine ran and it was not."""
    if text is None:
        return "ERROR"
    return "PASS" if marker in text else "FAIL"


def find_labwired(explicit: Path | None) -> Path | None:
    if explicit is not None:
        return explicit if explicit.is_file() else None
    env = os.environ.get("LABWIRED_BIN")
    if env and Path(env).is_file():
        return Path(env)
    for rel in ("target/debug/labwired", "target/release/labwired"):
        cand = REPO / rel
        if cand.is_file():
            return cand
    return None


def build_firmware() -> None:
    for rel in (
        "examples/f103-fidelity-bench/firmware",
        "examples/nrf52840-fidelity-bench/firmware",
    ):
        subprocess.run(["make", "-C", str(REPO / rel)], check=True)


def run_labwired(cli: Path, case: Case, work: Path) -> str:
    elf = (REPO / case.elf).resolve()
    system = (REPO / case.system).resolve()
    if not elf.is_file() or not system.is_file():
        return "ERROR"
    script = work / f"{case.name}-labwired.yaml"
    out = work / f"{case.name}-lw"
    out.mkdir()
    script.write_text(
        f"""schema_version: "1.0"
inputs:
  firmware: "{elf}"
  system: "{system}"
limits:
  max_steps: {MAX_STEPS}
assertions:
  - expected_stop_reason: max_steps
"""
    )
    env = dict(os.environ)
    env["RUST_LOG"] = "error"
    try:
        subprocess.run(
            [
                str(cli),
                "test",
                "--script",
                str(script),
                "--no-uart-stdout",
                "--output-dir",
                str(out),
            ],
            capture_output=True,
            text=True,
            env=env,
            timeout=HOST_TIMEOUT_S,
            check=False,
        )
    except subprocess.TimeoutExpired:
        pass
    except OSError:
        return "ERROR"
    uart = out / "uart.log"
    if not uart.is_file():
        return "ERROR"
    return score_text(uart.read_text(errors="replace"), case.marker)


def run_renode(renode: Path, case: Case, work: Path) -> str:
    elf = (REPO / case.elf).resolve()
    if not elf.is_file() or not renode.is_file():
        return "ERROR"
    uart = work / f"{case.name}-renode.txt"
    resc = work / f"{case.name}.resc"
    resc.write_text(
        f"""using sysbus
mach create "{case.name}"
machine LoadPlatformDescription @{case.renode_repl}
sysbus LoadELF @{elf}
{case.renode_uart} CreateFileBackend @{uart} true
emulation SetAdvanceImmediately true
emulation RunFor "{RENODE_RUNFOR_S}"
quit
"""
    )
    try:
        subprocess.run(
            [str(renode), "--disable-gui", "--console", str(resc)],
            capture_output=True,
            text=True,
            cwd=str(renode.parent),
            timeout=HOST_TIMEOUT_S,
            check=False,
        )
    except subprocess.TimeoutExpired:
        pass
    except OSError:
        return "ERROR"
    if not uart.is_file():
        return "ERROR"
    return score_text(uart.read_text(errors="replace"), case.marker)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--labwired", type=Path, default=None)
    p.add_argument("--renode", type=Path, default=None)
    p.add_argument(
        "--engines",
        default="labwired",
        help="comma list: labwired, renode",
    )
    p.add_argument("--output-json", type=Path, default=None)
    p.add_argument("--skip-build", action="store_true")
    args = p.parse_args(argv)

    engines = [e.strip() for e in args.engines.split(",") if e.strip()]
    unknown = [e for e in engines if e not in ("labwired", "renode")]
    if unknown or not engines:
        print(f"unknown engines {unknown or engines}", file=sys.stderr)
        return 2
    cli = find_labwired(args.labwired)
    if "labwired" in engines and cli is None:
        print("missing labwired binary (build -p labwired-cli or set --labwired)", file=sys.stderr)
        return 2
    renode = args.renode
    if renode is None and os.environ.get("RENODE_BIN"):
        renode = Path(os.environ["RENODE_BIN"])
    if "renode" in engines and (renode is None or not renode.is_file()):
        print("missing renode binary (--renode)", file=sys.stderr)
        return 2

    if not args.skip_build:
        try:
            build_firmware()
        except subprocess.CalledProcessError as exc:
            print(f"firmware build failed: {exc}", file=sys.stderr)
            return 2

    rows = []
    failed = False
    with tempfile.TemporaryDirectory(prefix="lw-renode-cases-") as tmp:
        work = Path(tmp)
        for case in CASES:
            lw = run_labwired(cli, case, work) if "labwired" in engines and cli else "skipped"
            rn = run_renode(renode, case, work) if "renode" in engines and renode else "skipped"
            print(
                f"case {case.name} silicon={case.expected} labwired={lw} renode={rn}",
                flush=True,
            )
            if "labwired" in engines and lw != case.expected:
                failed = True
            if lw == "ERROR" or rn == "ERROR":
                failed = True
            rows.append(
                {
                    "case": case.name,
                    "silicon": case.expected,
                    "marker": case.marker,
                    "labwired": lw,
                    "renode": rn,
                    "elf": case.elf,
                    "renode_repl": case.renode_repl,
                }
            )

    if args.output_json is not None:
        args.output_json.write_text(json.dumps({"cases": rows}, indent=2) + "\n")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
