#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Check assembled transaction return flow before expensive simulated guests."""
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GUEST = ROOT / "examples/microbit-v2"


def assembled_flow(source):
    with tempfile.TemporaryDirectory(prefix="microbit-sensor-flow-") as scratch:
        directory = Path(scratch)
        for name in ["motion-polled.inc", "board-io.ld"]:
            shutil.copyfile(GUEST / name, directory / name)
        (directory / "motion-drdy.S").write_text(source)
        subprocess.run([
            "arm-none-eabi-gcc", "-mcpu=cortex-m4", "-mthumb", "-nostdlib",
            "-Wl,-T,board-io.ld", "-o", "guest.elf", "motion-drdy.S"
        ], cwd=directory, check=True, capture_output=True)
        return subprocess.check_output([
            "arm-none-eabi-objdump", "-d", "--no-show-raw-insn", "guest.elf"
        ], cwd=directory, text=True)


def write_returns_to_command_completion(disassembly):
    block = disassembly.split("<write_byte>:\n", 1)[1].split("\n\n", 1)[0]
    instructions = [line.strip() for line in block.splitlines() if line.strip()]
    return (
        len(instructions) >= 2
        and re.search(r"\bbl\s+[0-9a-f]+\s+<motion_write>", instructions[0]) is not None
        and re.search(r"\bb(?:\.[nw])?\s+[0-9a-f]+\s+<complete>", instructions[1]) is not None
    )


class SensorGuestFlow(unittest.TestCase):
    def test_every_write_reaches_completion_without_sensor_command_fallthrough(self):
        self.assertTrue(write_returns_to_command_completion(
            assembled_flow((GUEST / "motion-drdy.S").read_text())))

    def test_original_fallthrough_mutation_is_detected(self):
        source = (GUEST / "motion-drdy.S").read_text()
        original = source.replace("write_byte:\n    bl motion_write\n    b complete\n",
                                  "write_byte:\n    bl motion_write\n", 1)
        self.assertNotEqual(source, original)
        self.assertFalse(write_returns_to_command_completion(assembled_flow(original)))


if __name__ == "__main__":
    unittest.main()
