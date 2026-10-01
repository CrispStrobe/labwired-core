# LabWired - Firmware Simulation Platform
# Copyright (C) 2026 Andrii Shylenko
# SPDX-License-Identifier: MIT
"""Drive compare_renode_cases from its real start and check LabWired.

Requires a built `labwired` (LABWIRED_BIN, --labwired via env, or target/).
Does not assert a Renode column. core-ci does not collect this file: the
Renode workflow and a local run do, because the runner executes the CLI.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import compare_renode_cases as cases  # noqa: E402

LINE = "case {name} silicon={silicon} labwired={labwired} renode={renode}"


def test_usartmux_poison_is_a_fail() -> None:
    usartmux = next(c for c in cases.CASES if c.name == "usartmux")
    assert usartmux.marker == "BENCH_UART_OK"
    assert usartmux.absent == "BENCH_POISON"
    assert usartmux.expected == "PASS"
    assert cases.score_text("BENCH_BANNER\nBENCH_UART_OK\n", usartmux.marker, usartmux.absent) == "PASS"
    assert (
        cases.score_text(
            "BENCH_BANNER\nBENCH_POISON\nBENCH_UART_OK\n",
            usartmux.marker,
            usartmux.absent,
        )
        == "FAIL"
    )
    assert cases.score_text("BENCH_BANNER\n", usartmux.marker, usartmux.absent) == "FAIL"
    assert cases.score_text(None, usartmux.marker, usartmux.absent) == "ERROR"


def test_runner_labwired_matches_silicon() -> None:
    script = Path(cases.__file__).resolve()
    cli = cases.find_labwired(None)
    assert cli is not None, "build labwired-cli or set LABWIRED_BIN"
    proc = subprocess.run(
        [
            sys.executable,
            str(script),
            "--labwired",
            str(cli),
            "--engines",
            "labwired",
        ],
        cwd=str(cases.REPO),
        capture_output=True,
        text=True,
        check=False,
    )
    sys.stdout.write(proc.stdout)
    sys.stderr.write(proc.stderr)
    assert proc.returncode == 0, proc.stderr
    parsed: dict[str, tuple[str, str, str]] = {}
    for line in proc.stdout.splitlines():
        parts = line.split()
        if len(parts) != 5 or parts[0] != "case":
            continue
        name = parts[1]
        fields = {}
        for item in parts[2:]:
            key, value = item.split("=", 1)
            fields[key] = value
        parsed[name] = (fields["silicon"], fields["labwired"], fields["renode"])
    assert set(parsed) == {c.name for c in cases.CASES}
    for case in cases.CASES:
        silicon, labwired, renode = parsed[case.name]
        assert silicon == case.expected
        assert labwired == case.expected
        assert labwired in ("PASS", "FAIL")
        assert renode == "skipped"
        # The line the log is graded on.
        print(
            LINE.format(
                name=case.name,
                silicon=silicon,
                labwired=labwired,
                renode=renode,
            )
        )
