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

    def test_multiline_comments_and_strings_are_never_mutation_sites(self):
        for name, statement in (("omit-erase-commit", ERASE), ("omit-program-commit", PROGRAM)):
            decoys = (
                b"/*\n" + statement + b"\n*/",
                b"/* outer /* nested */\n" + statement + b"\n*/",
                b'let text = "\n' + statement + b'\n";',
                b'let text = b"\n' + statement + b'\n";',
                b'let text = r"\n' + statement + b'\n";',
                b'let text = r###"embedded " and "##\n' + statement + b'\n"###;',
                b'let text = br##"\n' + statement + b'\n"##;',
                b'let text = cr#"\n' + statement + b'\n"#;',
                b'let quote = \'"\'; /*\n' + statement + b'\n*/',
                b'let text = "escaped \\"\n' + statement + b'\n";',
            )
            for decoy in decoys:
                with self.subTest(name=name, decoy=decoy):
                    # With no executable target left, a decoy must not count.
                    raw = ORIGINAL.replace(statement, decoy)
                    with self.assertRaisesRegex(ValueError, "0 matches"):
                        M.mutate_source(raw, name)
                    # Alongside the real target it must remain byte-for-byte
                    # intact and must not make the executable site ambiguous.
                    raw = ORIGINAL + b"\n" + decoy + b"\n"
                    self.assertEqual(
                        M.mutate_source(raw, name),
                        M.mutate_source(ORIGINAL, name) + b"\n" + decoy + b"\n",
                    )

    def test_unterminated_comments_and_strings_fail_closed(self):
        for suffix in (b"/*", b'let s = "', b'let s = r###"'):
            with self.subTest(suffix=suffix):
                with self.assertRaisesRegex(ValueError, "unterminated Rust"):
                    M.mutate_source(ORIGINAL + b"\n" + suffix, "omit-erase-commit")

    def test_mask_keeps_byte_offsets_and_lifetimes(self):
        raw = b"fn borrow<'a>(x: &'a str) { let c = '\\''; }\n/* nested /* a */ b */\n"
        masked = M.rust_code_only(raw)
        self.assertEqual(len(masked), len(raw))
        self.assertEqual([i for i, value in enumerate(masked) if value == 10],
                         [i for i, value in enumerate(raw) if value == 10])
        self.assertTrue(masked.startswith(b"fn borrow<'a>(x: &'a str)"))


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
