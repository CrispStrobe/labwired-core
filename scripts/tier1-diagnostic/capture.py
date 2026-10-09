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
record = {'schema': 1, 'source': source, 'label': os.environ['DIAGNOSTIC_LABEL'],
          'run': int(os.environ['GITHUB_RUN_ID']), 'attempt': int(os.environ['GITHUB_RUN_ATTEMPT']),
          'cases': []}
binary = root / 'target/release/labwired'
record['cli_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
environment = {k: v for k, v in os.environ.items() if not k.startswith('LABWIRED_')}
for chip in ['nrf52832', 'stm32f103']:
    config = f'configs/chips/{chip}.yaml'
    fixture = f'tests/fixtures/tier1/{chip}.elf'
    inputs = {p: hashlib.sha256((root / p).read_bytes()).hexdigest() for p in [config, fixture]}
    command = ['./target/release/labwired', 'run', '--chip', config,
               '--firmware', fixture, '--max-steps', '8000000']
    result = subprocess.run(command, cwd=root, env=environment, capture_output=True, timeout=120)
    streams = {}
    for kind, data in [('stdout', result.stdout), ('stderr', result.stderr)]:
        assert len(data) <= 2 * 1024 * 1024, 'refuse oversized diagnostic output'
        name = f'{chip}.{kind}'
        (output / name).write_bytes(data)
        streams[kind] = {'file': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    record['cases'].append({'chip': chip, 'inputs': inputs, 'command': command,
                            'returncode': result.returncode, 'streams': streams})
    print(f'{chip}: exit={result.returncode}, stdout={len(result.stdout)}B stderr={len(result.stderr)}B; observations only')
    (output / 'observations.json').write_text(json.dumps(record, indent=2) + '\n')
