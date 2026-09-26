from pathlib import Path
from types import SimpleNamespace

import pytest

import board_rtx as br


def test_cpu_hz_requires_positive_integer():
    assert br.cpu_hz({"cpu_hz": 48_000_000}) == 48_000_000
    for value in (None, 0, -1, 1.5, "48000000"):
        with pytest.raises(ValueError):
            br.cpu_hz({"cpu_hz": value})


def test_run_once_requires_exact_batched_proof(monkeypatch):
    times = iter((10.0, 10.25))
    monkeypatch.setattr(br.time, "perf_counter", lambda: next(times))
    monkeypatch.setattr(
        br.subprocess,
        "run",
        lambda *args, **kwargs: SimpleNamespace(
            returncode=0,
            stdout="",
            stderr=(
                "[batched] instructions=48000000 batches=93750 "
                "steps_per_batch=512.00 tick_interval=512\n"
            ),
        ),
    )
    wall, width = br.run_once(Path("labwired"), "atsamd21", Path("spin.elf"), 48_000_000)
    assert wall == pytest.approx(0.25)
    assert width == pytest.approx(512.0)


def test_run_once_rejects_partial_work(monkeypatch):
    times = iter((1.0, 2.0))
    monkeypatch.setattr(br.time, "perf_counter", lambda: next(times))
    monkeypatch.setattr(
        br.subprocess,
        "run",
        lambda *args, **kwargs: SimpleNamespace(
            returncode=0,
            stdout="",
            stderr=(
                "[batched] instructions=47 batches=1 "
                "steps_per_batch=47.00 tick_interval=512\n"
            ),
        ),
    )
    with pytest.raises(br.bp.ModeNotTakenError):
        br.run_once(Path("labwired"), "atsamd21", Path("spin.elf"), 48)
