import unittest
from unittest.mock import patch
import subprocess

from link_map import diagnostic_command, invoke_link


class Admission(unittest.TestCase):
    def test_timeout_and_spawn_failure_preserve_failure_state(self):
        with patch("link_map.subprocess.run", side_effect=subprocess.TimeoutExpired(
                "owned-link", 120, output=b"partial", stderr=b"diagnostic")):
            self.assertEqual(invoke_link(["owned-link"], "."),
                             (None, True, None, b"partial", b"diagnostic"))
        with patch("link_map.subprocess.run", side_effect=OSError("cannot start")):
            self.assertEqual(invoke_link(["owned-link"], "."),
                             (None, False, "cannot start", b"", b""))

    def test_only_output_and_map_reporting_change(self):
        source = '/usr/bin/arm-none-eabi-g++ "object with spaces.o" -Wl,--gc-sections -o original.elf lib.a\n'
        original, command = diagnostic_command(source, "/usr/bin/arm-none-eabi-g++", "/owned/diag.elf", "/owned/map")
        self.assertEqual(original, "original.elf")
        self.assertEqual(command, ["/usr/bin/arm-none-eabi-g++", "object with spaces.o",
                                  "-Wl,--gc-sections", "-o", "/owned/diag.elf", "lib.a",
                                  "-Wl,-Map=/owned/map"])

    def test_ambiguous_shell_response_and_existing_map_recipes_rejected(self):
        base = "/usr/bin/arm-none-eabi-g++ a.o -o original.elf"
        for mutant in (base + "\n" + base, base + " && echo done", base + " @inputs",
                       base + " -Wl,-Map=old", base + " -o second", base + " $(FLAGS)",
                       base.replace("arm-none-eabi-g++", "g++"), base.replace("original.elf", "/owned/diag.elf")):
            with self.assertRaises(ValueError):
                diagnostic_command(mutant, "/usr/bin/arm-none-eabi-g++", "/owned/diag.elf", "/owned/map")


if __name__ == "__main__":
    unittest.main()
