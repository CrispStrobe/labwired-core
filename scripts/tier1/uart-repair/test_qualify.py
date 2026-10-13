#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Mock-only controls for guest admission and failure evidence retention."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import patch


class Controls(unittest.TestCase):
    def exercise(self, mutation):
        script = Path(__file__).with_name('qualify.py').resolve()
        original = Path.cwd()

        def run(command, **kwargs):
            self.assertNotIn('LABWIRED_UNEXPECTED', kwargs['env'])
            self.assertEqual(kwargs['timeout'], 120)
            chip = Path(command[command.index('--chip') + 1]).stem
            classes = ('gpio clock timer rtc i2c spi adc wdt pwm' if chip == 'nrf52832'
                       else 'clock gpio timer i2c spi adc dma wdt rtc').split()
            lines = [f'TIER1 {name} PASS' for name in classes] + ['TIER1 done']
            if mutation == 'missing':
                lines.pop(0)
            if mutation == 'timeout':
                raise subprocess.TimeoutExpired(command, 120, output=b'partial', stderr=b'bound')
            return subprocess.CompletedProcess(command, 0, ('\n'.join(lines)+'\n').encode(), b'')

        with tempfile.TemporaryDirectory(dir=original) as temporary:
            try:
                os.chdir(temporary)
                for name in ['target/release/labwired',
                             *[f'{prefix}/{chip}.{suffix}'
                               for chip in ['nrf52832', 'stm32f103']
                               for prefix, suffix in [('configs/chips', 'yaml'),
                                                      ('tests/fixtures/tier1', 'elf')]]]:
                    p = Path(name)
                    p.parent.mkdir(parents=True, exist_ok=True)
                    p.write_bytes(b'mock-only')
                with patch.dict(os.environ, {'GITHUB_RUN_ID': '1', 'GITHUB_RUN_ATTEMPT': '1',
                                             'LABWIRED_UNEXPECTED': 'must-not-leak'}), \
                        patch('subprocess.check_output', return_value='f' * 40), \
                        patch('subprocess.run', side_effect=run):
                    with self.assertRaises(SystemExit) as error:
                        runpy.run_path(str(script), run_name='__main__')
                self.assertEqual(error.exception.code, 0 if mutation is None else 1)
                output = Path('uart-repair-results')
                record = json.loads((output / 'qualification.json').read_text())
                self.assertEqual(len(record['cases']), 4)
                self.assertTrue(all(case['pass'] == (mutation is None) for case in record['cases']))
                self.assertEqual(len(list(output.iterdir())), 9)
                if mutation == 'timeout':
                    for case in record['cases']:
                        self.assertTrue(case['timed_out'])
                        self.assertIsNone(case['returncode'])
                        self.assertEqual((output / case['streams']['stdout']['file']).read_bytes(), b'partial')
            finally:
                os.chdir(original)

    def test_all_classes_required(self):
        self.exercise(None)

    def test_exit_zero_cannot_admit_missing_class(self):
        self.exercise('missing')

    def test_timeouts_fail_and_retain_all_four_cases(self):
        self.exercise('timeout')


if __name__ == '__main__':
    unittest.main()
