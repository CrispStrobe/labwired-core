#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Small host controls; never launch an engine or compiler."""
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import patch


class CaptureControls(unittest.TestCase):
    def test_modes_scrub_environment_and_preserve_timeout(self):
        script = Path(__file__).with_name('capture.py').resolve()
        original = Path.cwd()
        calls = []

        def run(command, **kwargs):
            env = kwargs['env']
            self.assertNotIn('LABWIRED_UNEXPECTED', env)
            self.assertEqual(env['LABWIRED_RUN_STATS'], '1')
            self.assertEqual(kwargs['timeout'], 120)
            self.assertEqual(command[-1], '8000000')
            calls.append(env)
            if env.get('LABWIRED_ARM_SINGLE_STEP') == '1':
                raise subprocess.TimeoutExpired(command, 120, output=b'partial', stderr=b'bound')
            return subprocess.CompletedProcess(command, 0, b'TIER1 observed', b'stats')

        with tempfile.TemporaryDirectory(dir=original) as temporary:
            try:
                os.chdir(temporary)
                root = Path('engine-source')
                for name in ['target/release/labwired',
                             *[f'{prefix}/{chip}.{suffix}'
                               for chip in ['nrf52832', 'stm32f103']
                               for prefix, suffix in [('configs/chips', 'yaml'),
                                                      ('tests/fixtures/tier1', 'elf')]]]:
                    p = root / name
                    p.parent.mkdir(parents=True, exist_ok=True)
                    p.write_bytes(b'mock-only')
                with patch.dict(os.environ, {'EXPECTED_SOURCE': 'f' * 40,
                                             'DIAGNOSTIC_LABEL': 'mock',
                                             'GITHUB_RUN_ID': '1', 'GITHUB_RUN_ATTEMPT': '1',
                                             'LABWIRED_UNEXPECTED': 'must-not-leak'}), \
                        patch('subprocess.check_output', return_value='f' * 40), \
                        patch('subprocess.run', side_effect=run):
                    runpy.run_path(str(script), run_name='__main__')
                output = Path('tier1-observations')
                record = json.loads((output / 'observations.json').read_text())
                self.assertEqual(record['schema'], 2)
                self.assertEqual(len(record['cases']), 10)
                self.assertEqual(len(calls), 10)
                self.assertEqual(len(list(output.iterdir())), 21)
                for case in record['cases']:
                    timeout = case['mode'] == 'single-step'
                    self.assertEqual(case['timed_out'], timeout)
                    self.assertEqual(case['returncode'], None if timeout else 0)
                    for kind, stream in case['streams'].items():
                        data = (output / stream['file']).read_bytes()
                        self.assertEqual(stream['bytes'], len(data))
                        self.assertEqual(stream['sha256'], hashlib.sha256(data).hexdigest())
                        if timeout:
                            self.assertEqual(data, b'partial' if kind == 'stdout' else b'bound')
            finally:
                os.chdir(original)


if __name__ == '__main__':
    unittest.main()
