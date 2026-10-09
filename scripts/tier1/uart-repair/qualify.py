#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Bounded source-built fixture qualification; keep original failure streams."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

output = Path('uart-repair-results')
output.mkdir()
record = {'schema': 1,
          'source': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
          'tree': subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], text=True).strip(),
          'run': int(os.environ['GITHUB_RUN_ID']),
          'attempt': int(os.environ['GITHUB_RUN_ATTEMPT']), 'cases': []}
environment = {k: v for k, v in os.environ.items() if not k.startswith('LABWIRED_')}
failed = False
for chip, classes in [('nrf52832', 'gpio clock timer rtc i2c spi adc wdt pwm'),
                      ('stm32f103', 'clock gpio timer i2c spi adc dma wdt rtc')]:
    config = f'configs/chips/{chip}.yaml'
    fixture = f'tests/fixtures/tier1/{chip}.elf'
    inputs = {p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
              for p in [config, fixture, 'target/release/labwired']}
    for mode in ['default', 'single-step']:
        overrides = {'LABWIRED_ARM_SINGLE_STEP': '1'} if mode == 'single-step' else {}
        command = ['./target/release/labwired', 'run', '--chip', config,
                   '--firmware', fixture, '--max-steps', '8000000']
        timed_out = False
        try:
            result = subprocess.run(command, env={**environment, **overrides},
                                    capture_output=True, timeout=120)
            stdout, stderr, code = result.stdout, result.stderr, result.returncode
        except subprocess.TimeoutExpired as error:
            stdout, stderr, code = error.stdout or b'', error.stderr or b'', None
            timed_out = True
        streams = {}
        for kind, data in [('stdout', stdout), ('stderr', stderr)]:
            assert len(data) <= 2 * 1024 * 1024
            name = f'{chip}.{mode}.{kind}'
            (output / name).write_bytes(data)
            streams[kind] = {'file': name, 'bytes': len(data),
                             'sha256': hashlib.sha256(data).hexdigest()}
        lines = stdout.decode('utf-8', errors='replace').splitlines()
        expected = [f'TIER1 {name} PASS' for name in classes.split()]
        passed = (code == 0 and not timed_out and lines.count('TIER1 done') == 1
                  and all(lines.count(line) == 1 for line in expected)
                  and not any(re.match(r'^TIER1 .* FAIL', line) for line in lines))
        record['cases'].append({'chip': chip, 'mode': mode, 'inputs': inputs,
                                'command': command, 'environment': overrides,
                                'returncode': code, 'timed_out': timed_out,
                                'streams': streams, 'expected': expected, 'pass': passed})
        (output / 'qualification.json').write_text(json.dumps(record, indent=2) + '\n')
        print(f'{chip}/{mode}: {"PASS" if passed else "FAIL"}, exit={code}, timeout={timed_out}')
        failed |= not passed
raise SystemExit(1 if failed else 0)
