"""Unit tests for the product-matrix failure classifier.

`classify_failure` is what turns a red cell into a *finding*. Getting the label
wrong is worse than getting no label: `unmodeled` (🟣) reads as "the model is
missing a peripheral, this is a known gap", and a reviewer skips it. A wedged
boot filed that way is invisible.

That is not hypothetical. Before the fix these tests pin, the classifier folded
`json.dumps(result)` into the text blob it word-searched, and the result carries
`memory.main_stack_method: "unsupported"` on every Xtensa run because
stack-usage measurement is not implemented for that arch. The bare
`"unsupported" in blob` check therefore matched unconditionally: EVERY failing
ESP32/ESP32-S3 cell reported `unmodeled`, whatever had actually gone wrong. All
18 cells of a total Xtensa boot regression were filed as missing peripherals.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from matrix_lib.invoke import classify_failure  # noqa: E402


def _proc(returncode: int = 1, stdout: str = "", stderr: str = ""):
    return subprocess.CompletedProcess(args=[], returncode=returncode, stdout=stdout, stderr=stderr)


# The `memory` block every Xtensa run emits, regardless of outcome. Its VALUES
# contain "unsupported"; its presence says nothing about the run.
XTENSA_MEMORY_BLOCK = {
    "main_stack_method": "unsupported",
    "main_stack_unsupported_reason": "arch_not_implemented",
}


def test_exit_zero_is_a_pass():
    assert classify_failure(_proc(returncode=0), {"status": "ok"}, "LW_L0_OK") == "pass"


def test_wedged_xtensa_boot_is_a_boot_fail_not_unmodeled():
    """The regression this file exists for.

    A run that burned its whole step budget with an empty console is a WEDGE.
    There is no coverage gap (`fidelity` is empty), no fault stop reason, and
    nothing in stdout/stderr — only the always-present `unsupported` inside the
    result's `memory` block. It must not be labelled `unmodeled`.
    """
    result = {
        "status": "fail",
        "stop_reason": "max_steps",
        "steps_executed": 50_000_000,
        "memory": XTENSA_MEMORY_BLOCK,
    }
    assert classify_failure(_proc(), result, "") == "boot_fail"


def test_wedge_that_printed_something_is_an_oracle_fail():
    result = {
        "status": "fail",
        "stop_reason": "max_steps",
        "memory": XTENSA_MEMORY_BLOCK,
    }
    assert classify_failure(_proc(), result, "ESP-ROM:esp32s3\n") == "oracle_fail"


def test_result_json_words_never_decide_the_label():
    """No field VALUE in result.json may steer the classifier.

    Mutation guard: put every gap phrase the text fallback looks for into the
    result as data. The verdict must be unchanged — only stdout/stderr and the
    structured fields may speak.
    """
    poisoned = {
        "status": "fail",
        "stop_reason": "max_steps",
        "memory": XTENSA_MEMORY_BLOCK,
        "notes": (
            "unmodeled unimplemented unknown instruction bus read fault "
            "bus write fault outside of memory map memory access violation "
            "not modeled unsupported"
        ),
    }
    assert classify_failure(_proc(), poisoned, "") == "boot_fail"


def test_derived_device_time_is_not_a_coverage_gap():
    """An approximation note must not turn an oracle miss into `unmodeled`."""
    result = {
        "status": "fail",
        "stop_reason": "max_steps",
        "fidelity": [
            {
                "kind": "derived_device_time",
                "address": "0x0",
                "detail": "device time derived from cpu_hz",
            }
        ],
    }
    assert classify_failure(_proc(), result, "LW_L4_BOOT\n") == "oracle_fail"


def test_structured_fidelity_gap_is_authoritative():
    """A real coverage gap is reported structurally and must win."""
    result = {
        "status": "fail",
        "stop_reason": "max_steps",
        "memory": XTENSA_MEMORY_BLOCK,
        "fidelity": [{"kind": "unmapped_mmio", "address": "0x60031204"}],
    }
    assert classify_failure(_proc(), result, "") == "unmodeled"


@pytest.mark.parametrize("stop", ["memory_violation", "exception", "fault"])
def test_fault_stop_reasons_are_unmodeled(stop):
    result = {"status": "error", "stop_reason": stop, "memory": XTENSA_MEMORY_BLOCK}
    assert classify_failure(_proc(), result, "") == "unmodeled"


@pytest.mark.parametrize(
    "phrase",
    [
        "unmodeled peripheral at 0x6003_1204",
        "unimplemented opcode",
        "unknown instruction 0xdeadbeef",
        "bus read fault at 0x40000000",
        "address outside of memory map",
        "unsupported instruction",
    ],
)
def test_engine_prose_gaps_still_classify_as_unmodeled(phrase):
    """The text fallback keeps working — from stdout/stderr, where it belongs."""
    result = {"status": "error", "stop_reason": "", "memory": XTENSA_MEMORY_BLOCK}
    assert classify_failure(_proc(stderr=phrase), result, "") == "unmodeled"


def test_bare_unsupported_in_prose_does_not_alone_mean_unmodeled():
    """`unsupported` is too common a word to be evidence on its own.

    It must be qualified (instruction/opcode/peripheral) to count.
    """
    result = {"status": "fail", "stop_reason": "max_steps", "memory": XTENSA_MEMORY_BLOCK}
    proc = _proc(stderr="warning: unsupported board revision, continuing")
    assert classify_failure(proc, result, "") == "boot_fail"
