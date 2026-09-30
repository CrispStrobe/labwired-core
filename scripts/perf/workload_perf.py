#!/usr/bin/env python3
# LabWired - Firmware Simulation Platform
# Copyright (C) 2026 Andrii Shylenko
# SPDX-License-Identifier: MIT
"""Measure representative firmware workloads, separate from synthetic spin.

The deterministic spin gate answers whether the engine got more expensive.
This harness answers what a user experiences: process/setup latency, simulated
time throughput, host CPU/RSS, first-frame or event latency, and observable
identity between cycle-tick and production-batched execution.

Test executables are built first and then run directly, so the host resource
receipt excludes Cargo and compiler work. Wall-clock values are evidence, not
merge-blocking budgets; deterministic identity and the fleet gate remain the
hard contracts.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
RECEIPT_PREFIX = "WORKLOAD_PERF_JSON "


@dataclass(frozen=True)
class Case:
    name: str
    target: str
    test: str
    ignored: bool = False
    minimum_receipts: int = 1


CASES = (
    Case(
        "esp32c3-oled",
        "esp32c3_oled_profile",
        "esp32c3_oled_native_baseline",
        ignored=True,
    ),
    Case(
        "nrf54l15-embassy-events",
        "nrf54l15_embassy_realtime",
        "embassy_sleep_acceleration_preserves_gpio_edges_and_cpu_state",
        minimum_receipts=2,
    ),
    Case(
        "esp32c3-oled-identity",
        "esp32c3_walk_differential",
        "oled_lab_framebuffer_is_byte_identical_at_recommended_interval",
        ignored=True,
    ),
)


def build_test(case: Case) -> Path:
    cmd = [
        "cargo",
        "test",
        "--release",
        "-p",
        "labwired-core",
        "--features",
        "event-scheduler",
        "--test",
        case.target,
        "--no-run",
        "--message-format=json",
    ]
    proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
    if proc.returncode:
        raise RuntimeError(f"build {case.target} failed:\n{proc.stderr[-6000:]}")
    executables = []
    for line in proc.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (
            message.get("reason") == "compiler-artifact"
            and message.get("target", {}).get("name") == case.target
            and message.get("executable")
        ):
            executables.append(Path(message["executable"]))
    if len(executables) != 1:
        raise RuntimeError(
            f"expected one test executable for {case.target}, got {executables}"
        )
    return executables[0]


def parse_time_receipt(path: Path) -> dict[str, float | int]:
    values: dict[str, float | int] = {}
    for line in path.read_text().splitlines():
        key, value = line.split("=", 1)
        values[key] = int(value) if key == "max_rss_kib" else float(value)
    return values


def run_case(case: Case, executable: Path) -> dict:
    args = [str(executable), case.test, "--exact", "--nocapture"]
    if case.ignored:
        args.append("--ignored")

    time_bin = Path("/usr/bin/time")
    if not time_bin.exists():
        raise RuntimeError("/usr/bin/time is required for per-workload CPU/RSS receipts")
    with tempfile.NamedTemporaryFile(prefix="labwired-workload-time-", delete=False) as f:
        time_path = Path(f.name)
    try:
        timed = [
            str(time_bin),
            "-f",
            "wall_seconds=%e\\nuser_seconds=%U\\nsystem_seconds=%S\\nmax_rss_kib=%M",
            "-o",
            str(time_path),
            *args,
        ]
        started = time.perf_counter()
        proc = subprocess.run(
            timed,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            env=dict(os.environ),
        )
        elapsed = time.perf_counter() - started
        combined = proc.stdout + "\n" + proc.stderr
        receipts = []
        for line in combined.splitlines():
            if RECEIPT_PREFIX in line:
                payload = line.split(RECEIPT_PREFIX, 1)[1]
                receipts.append(json.loads(payload))
        host = parse_time_receipt(time_path)
        host["orchestrator_wall_seconds"] = elapsed
        result = {
            "ok": proc.returncode == 0 and len(receipts) >= case.minimum_receipts,
            "exit_code": proc.returncode,
            "host": host,
            "receipts": receipts,
        }
        if proc.returncode:
            result["error"] = combined[-6000:]
        elif len(receipts) < case.minimum_receipts:
            result["error"] = (
                f"expected at least {case.minimum_receipts} structured receipt(s), "
                f"found {len(receipts)}"
            )
        return result
    finally:
        time_path.unlink(missing_ok=True)


def selected_cases(value: str | None) -> list[Case]:
    if not value:
        return list(CASES)
    wanted = set(value.split(","))
    known = {case.name for case in CASES}
    unknown = wanted - known
    if unknown:
        raise ValueError(f"unknown workload(s): {', '.join(sorted(unknown))}")
    return [case for case in CASES if case.name in wanted]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workloads", help="comma-separated subset")
    parser.add_argument("--status-json", default="workload-perf-status.json")
    args = parser.parse_args()
    try:
        cases = selected_cases(args.workloads)
    except ValueError as exc:
        parser.error(str(exc))

    results = {}
    print(
        "workload                       wall s   CPU s   RSS MiB  receipts  verdict",
        flush=True,
    )
    print("--------------------------------------------------------------------------", flush=True)
    for case in cases:
        try:
            executable = build_test(case)
            result = run_case(case, executable)
        except (OSError, RuntimeError, subprocess.SubprocessError) as exc:
            result = {"ok": False, "error": str(exc), "receipts": []}
        results[case.name] = result
        host = result.get("host", {})
        cpu = host.get("user_seconds", 0) + host.get("system_seconds", 0)
        rss = host.get("max_rss_kib", 0) / 1024
        print(
            f"{case.name:<30} {host.get('wall_seconds', 0):>7.2f} "
            f"{cpu:>7.2f} {rss:>9.1f} {len(result['receipts']):>9}  "
            f"{'PASS' if result['ok'] else 'FAIL'}",
            flush=True,
        )
        if not result["ok"]:
            print(f"  {result.get('error', 'unknown failure')}", file=sys.stderr)

    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    dirty = bool(
        subprocess.run(
            ["git", "status", "--porcelain"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )
    document = {
        "schema_version": 1,
        "commit": revision,
        "dirty": dirty,
        "ok": all(result["ok"] for result in results.values()),
        "workloads": results,
    }
    Path(args.status_json).write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    print(f"\nwrote {args.status_json}")
    return 0 if document["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
