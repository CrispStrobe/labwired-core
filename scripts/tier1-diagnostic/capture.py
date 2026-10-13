#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Observe unchanged public fixtures; never mutate a ratchet or guest input."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

root = Path('engine-source')
source = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
assert source == os.environ['EXPECTED_SOURCE']
output = Path('tier1-observations')
output.mkdir()
record = {'schema': 2, 'source': source, 'label': os.environ['DIAGNOSTIC_LABEL'],
          'run': int(os.environ['GITHUB_RUN_ID']), 'attempt': int(os.environ['GITHUB_RUN_ATTEMPT']),
          'cases': []}
binary = root / 'target/release/labwired'
record['cli_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
environment = {k: v for k, v in os.environ.items() if not k.startswith('LABWIRED_')}
modes = [
    ('default-stats', {}),
    ('single-step', {'LABWIRED_ARM_SINGLE_STEP': '1'}),
    ('jit-off', {'LABWIRED_CORTEX_M_JIT': '0'}),
    ('idle-off', {'LABWIRED_IDLE_FAST_FORWARD': '0'}),
    ('jit-idle-off', {'LABWIRED_CORTEX_M_JIT': '0', 'LABWIRED_IDLE_FAST_FORWARD': '0'}),
]
for chip, mode, overrides in [(chip, mode, overrides)
                              for chip in ['nrf52832', 'stm32f103']
                              for mode, overrides in modes]:
    config = f'configs/chips/{chip}.yaml'
    fixture = f'tests/fixtures/tier1/{chip}.elf'
    inputs = {p: hashlib.sha256((root / p).read_bytes()).hexdigest() for p in [config, fixture]}
    command = ['./target/release/labwired', 'run', '--chip', config,
               '--firmware', fixture, '--max-steps', '8000000']
    case_environment = {'LABWIRED_RUN_STATS': '1', **overrides}
    timed_out = False
    try:
        result = subprocess.run(command, cwd=root, env={**environment, **case_environment},
                                capture_output=True, timeout=120)
        stdout, stderr, returncode = result.stdout, result.stderr, result.returncode
    except subprocess.TimeoutExpired as error:
        timed_out = True
        stdout, stderr, returncode = error.stdout or b'', error.stderr or b'', None
    streams = {}
    for kind, data in [('stdout', stdout), ('stderr', stderr)]:
        assert len(data) <= 2 * 1024 * 1024, 'refuse oversized diagnostic output'
        name = f'{chip}.{mode}.{kind}'
        (output / name).write_bytes(data)
        streams[kind] = {'file': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    record['cases'].append({'chip': chip, 'mode': mode, 'environment': case_environment,
                            'inputs': inputs, 'command': command, 'timeout_seconds': 120,
                            'timed_out': timed_out, 'returncode': returncode, 'streams': streams})
    print(f'{chip}/{mode}: exit={returncode}, timeout={timed_out}, stdout={len(stdout)}B stderr={len(stderr)}B; observations only')
    (output / 'observations.json').write_text(json.dumps(record, indent=2) + '\n')
