# SPDX-License-Identifier: MIT
import json
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from microbit_motion_ab import IDENTICAL_FILES, SCHEDULE, invocation, select_executable, summarize, validate_commit, verify_identical_workload


class MotionAbTests(unittest.TestCase):
    def entries(self):
        return [{'role': role, 'exitCode': 0, 'measurementValid': True,
                 'realtimeTargetMet': role == 'candidate',
                 'medianRtx': 1.5 if role == 'candidate' else .8}
                for role in SCHEDULE]

    def test_only_exact_commit_inputs_are_accepted(self):
        self.assertEqual(validate_commit('a' * 40), 'a' * 40)
        for value in ('main', 'a' * 39, 'A' * 40, 'a' * 40 + '; echo bad', '-HEAD', ''):
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_commit(value)

    def test_slow_baseline_does_not_weaken_candidate_gate(self):
        value = summarize(self.entries())
        self.assertTrue(value['passed'])
        self.assertAlmostEqual(value['candidateToBaselineMedianRatio'], 1.875)
        for field, replacement in (('exitCode', 101), ('realtimeTargetMet', False),
                                   ('measurementValid', False)):
            entries = self.entries()
            entries[1][field] = replacement
            self.assertFalse(summarize(entries)['passed'])

    def test_one_failed_candidate_cannot_hide_under_aggregate_median(self):
        entries = self.entries()
        entries[2].update(medianRtx=.9, realtimeTargetMet=False, exitCode=101)
        self.assertFalse(summarize(entries)['candidateRealtimeTargetMet'])

    def test_functional_baseline_error_fails_even_with_candidate_success(self):
        entries = self.entries()
        entries[0]['exitCode'] = 101
        self.assertFalse(summarize(entries)['passed'])

    def test_schedule_and_all_four_invocations_are_required(self):
        for entries in (self.entries()[:3], list(reversed(self.entries()[1:]))):
            with self.assertRaises(ValueError):
                summarize(entries)

    def test_unchanged_guest_and_test_source_are_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            baseline, candidate = Path(temporary) / 'b', Path(temporary) / 'c'
            for root in (baseline, candidate):
                for name in IDENTICAL_FILES:
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(name.encode())
            self.assertEqual(len(verify_identical_workload(baseline, candidate)), 4)
            for name in IDENTICAL_FILES:
                path = candidate / name
                path.write_bytes(b'changed')
                with self.subTest(name=name), self.assertRaises(ValueError):
                    verify_identical_workload(baseline, candidate)
                path.write_bytes(name.encode())

    def test_build_artifact_must_resolve_inside_its_own_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / 'target'
            target.mkdir()
            binary = target / 'motion-test'
            binary.write_bytes(b'fixture')
            artifact = {'reason': 'compiler-artifact', 'target': {'name': 'microbit_v2_motion_io_guest'},
                        'profile': {'test': True}, 'executable': str(binary)}
            self.assertEqual(select_executable(json.dumps(artifact), target), binary)
            outside = root / 'outside'
            outside.write_bytes(b'fixture')
            artifact['executable'] = str(outside)
            with self.assertRaises(ValueError):
                select_executable(json.dumps(artifact), target)
            with self.assertRaises(ValueError):
                select_executable('no artifact', target)

    def test_failed_candidate_keeps_log_receipt_and_strict_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for role, expected in (('baseline', '0'), ('candidate', '1')):
                with patch('microbit_motion_ab.subprocess.run', return_value=SimpleNamespace(
                        returncode=101, stdout='complete benchmark output')) as execute, \
                        patch('microbit_motion_ab.source_bundle', return_value=b'source'), \
                        patch('microbit_motion_ab.report', return_value={
                            'medianRtx': .9, 'realtimeTargetMet': False}):
                    entry = invocation(root, root / 'executable', 'a' * 40, root, 1, role)
                self.assertEqual(execute.call_args.kwargs['env']['LABWIRED_REQUIRE_REALTIME'], expected)
                self.assertEqual(entry['exitCode'], 101)
                self.assertTrue(entry['measurementValid'])
                self.assertEqual((root / entry['log']).read_text(), 'complete benchmark output')
                self.assertFalse(json.loads((root / entry['receipt']).read_text())['realtimeTargetMet'])


if __name__ == '__main__':
    unittest.main()
