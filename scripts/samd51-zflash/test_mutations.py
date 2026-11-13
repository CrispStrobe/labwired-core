#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Fast source/runner controls; these do not substitute for hosted guest tests."""
import importlib.util
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("zflash_mutations", Path(__file__).with_name("mutations.py"))
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
MODEL = Path("crates/core/src/bus/samd51_storage.rs")
ORIGINAL = (M.ROOT / MODEL).read_bytes()
ERASE = b"self.flash.data[address..end].fill(0xff);"
PROGRAM = b"*old &= new;"


class MutationSourceTests(unittest.TestCase):
    def test_real_checked_erase_commit_only_is_removed(self):
        mutated = M.mutate_source(ORIGINAL, "omit-erase-commit")
        self.assertNotIn(ERASE, mutated)
        self.assertIn(PROGRAM, mutated)
        replacement = M.MUTATIONS["omit-erase-commit"][1]
        self.assertEqual(mutated.replace(replacement, ERASE), ORIGINAL)
        self.assertIn(b".checked_add(8192)", mutated)
        self.assertIn(b".filter(|end| *end <= self.flash.data.len())", mutated)

    def test_real_program_commit_only_is_removed(self):
        mutated = M.mutate_source(ORIGINAL, "omit-program-commit")
        self.assertNotIn(PROGRAM, mutated)
        self.assertIn(ERASE, mutated)
        replacement = M.MUTATIONS["omit-program-commit"][1]
        self.assertEqual(mutated.replace(replacement, PROGRAM), ORIGINAL)

    def test_rust_whitespace_does_not_change_target(self):
        for name, statement, formatted in (
            ("omit-erase-commit", ERASE, b"self.flash.data[ address .. end ]\n .fill ( 0xff );"),
            ("omit-program-commit", PROGRAM, b"* old &=\n new ;"),
        ):
            with self.subTest(name=name):
                raw = ORIGINAL.replace(statement, formatted)
                mutated = M.mutate_source(raw, name)
                self.assertEqual(mutated.replace(M.MUTATIONS[name][1], formatted), raw)

    def test_missing_or_ambiguous_sites_are_refused(self):
        for name, statement in (("omit-erase-commit", ERASE), ("omit-program-commit", PROGRAM)):
            for raw in (ORIGINAL.replace(statement, b""), ORIGINAL + b"\n" + statement + b"\n"):
                with self.subTest(name=name, raw_length=len(raw)):
                    with self.assertRaisesRegex(ValueError, "unique actual model mutation site"):
                        M.mutate_source(raw, name)

    def test_comments_and_different_ranges_are_not_sites(self):
        for replacement in (b"// " + ERASE, b'let text = "' + ERASE + b'";', ERASE.replace(b"end", b"other_end")):
            with self.subTest(replacement=replacement):
                with self.assertRaises(ValueError):
                    M.mutate_source(ORIGINAL.replace(ERASE, replacement), "omit-erase-commit")


class MutationRunnerTests(unittest.TestCase):
    def run_control(self, outcome, profile="default"):
        # Only subprocess boundaries are simulated. The helper writes and then
        # restores the real source bytes in an isolated temporary model file.
        with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as directory:
            root = Path(directory).resolve()
            model = root / MODEL
            model.parent.mkdir(parents=True)
            model.write_bytes(ORIGINAL)
            calls = []

            def run(command, **kwargs):
                name = kwargs["env"]["ZFLASH_MUTANT"]
                calls.append(name)
                self.assertEqual(model.read_bytes(), M.mutate_source(ORIGINAL, name))
                self.assertEqual("event-scheduler" in command, profile == "event-scheduler")
                if isinstance(outcome, Exception):
                    raise outcome
                marker = b"ZFLASH_MUTANT_WRONG_ERASE " if name == "omit-erase-commit" else b"ZFLASH_MUTANT_WRONG_PROGRAM "
                code, stdout, stderr = outcome
                return subprocess.CompletedProcess(command, code, marker if stdout is None else stdout, stderr)

            with mock.patch.object(M, "ROOT", root), mock.patch.object(M.sys, "argv", ["mutations.py", profile, "out"]), mock.patch.dict(M.os.environ, {"GITHUB_ACTIONS": "true"}), mock.patch.object(M.subprocess, "check_output", return_value=b"diff"), mock.patch.object(M.subprocess, "run", side_effect=run), mock.patch("sys.stdout", new_callable=io.StringIO):
                try:
                    M.main()
                finally:
                    self.assertEqual(model.read_bytes(), ORIGINAL)
            self.assertEqual(calls, list(M.MUTATIONS))
            self.assertTrue((root / "out/mutation-report.json").is_file())

    def test_both_mutants_restore_source_for_both_profiles(self):
        for profile in ("default", "event-scheduler"):
            with self.subTest(profile=profile):
                self.run_control((0, None, b""), profile)

    def test_subprocess_exception_restores_original_bytes(self):
        with self.assertRaises(subprocess.TimeoutExpired):
            self.run_control(subprocess.TimeoutExpired("cargo", 600))

    def test_failed_or_wrong_guest_output_is_still_refused_and_restored(self):
        for outcome in (
            (1, None, b"failed"),
            (0, b"", b"ZFLASH_MUTANT_WRONG_ERASE "),
            (0, b"ZFLASH_MUTANT_WRONG_PROGRAM ", b""),
            (0, b"ZFLASH_MUTANT_WRONG_ERASE ZFLASH_MUTANT_WRONG_ERASE ", b""),
            (0, b"ZFLASH_MUTANT_WRONG_ERASE ACTUAL_ZFLASH_FINITE_PASS", b""),
        ):
            with self.subTest(outcome=outcome):
                with self.assertRaisesRegex(ValueError, "specific real guest wrong memory result not admitted"):
                    self.run_control(outcome)


if __name__ == "__main__":
    unittest.main()
