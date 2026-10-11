#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Manufactured result-reader controls only; never actual guest testimony."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('timer_record', Path(__file__).with_name('record.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReaderControls(unittest.TestCase):
    def fixture(self):
        elf, mapping, source = b'manufactured ELF', b'manufactured map', 'a' * 40
        before = {'schema': 1, 'samd_commit': source, 'core_commit': module.CORE_PIN,
                  'files': {name: module.record(b'manufactured source') for name in module.FILES}}
        build = {'schema': 1, 'source': copy.deepcopy(before), 'elf': module.record(elf),
                 'map': module.record(mapping), 'guest_executed': False, 'runtime_admitted': False,
                 'units': {'timer': sorted('core/' + name for name in module.CORE_FILES if name != 'LICENSE'),
                           'profile': ['samd/paired-irq/profile.cpp'], 'probe': ['samd/timer-caller/probe.cpp']}}
        row = (f'SAMD_REAL_TIMER elf_sha256={module.record(elf)["sha256"]} source={source} steps=123 '
               'bss_clear=true data_copy=true vectors=true milestones=6 irq_entries=2 irq_returns=2 '
               'handler_mask=0 handler_exception=16 paired_closed=true fault=0 status=0x600d '
               'final_mask=1 final_exception=0\n').encode()
        return {'default': row, 'scheduler': row}, elf, mapping, build, before, source

    def test_manufactured_complete_profiles_accept(self):
        self.assertEqual(set(module.validate(*self.fixture())), {'default', 'scheduler'})

    def test_missing_duplicate_and_corrupt_witness_refuse(self):
        for mode in ('missing', 'duplicate', 'mask', 'progress', 'timeout', 'zero', 'source', 'elf', 'return'):
            args = list(self.fixture()); logs = args[0]
            if mode == 'missing': del logs['scheduler']
            elif mode == 'duplicate': logs['default'] *= 2
            else:
                changes = {'mask': (b'handler_mask=0', b'handler_mask=1'),
                           'progress': (b'milestones=6', b'milestones=5'),
                           'timeout': (b'steps=123', b'steps=250001'), 'zero': (b'steps=123', b'steps=0'),
                           'source': (b'source=' + b'a'*40, b'source=' + b'b'*40),
                           'elf': (module.record(args[1])['sha256'].encode(), b'f'*64),
                           'return': (b'irq_returns=2', b'irq_returns=1')}
                logs['default'] = logs['default'].replace(*changes[mode])
            with self.subTest(mode=mode), self.assertRaises(ValueError): module.validate(*args)

    def test_source_inventory_drift_and_admission_refuse(self):
        for mode in ('inventory', 'drift', 'admission', 'elf', 'map', 'headers', 'unit', 'bytes'):
            args = list(self.fixture()); build, before = args[3:5]
            if mode == 'inventory':
                before['files']['private/unknown'] = module.record(b'injected'); build['source'] = copy.deepcopy(before)
            elif mode == 'drift': build['source']['samd_commit'] = 'b'*40
            elif mode == 'admission': build['runtime_admitted'] = True
            elif mode == 'elf': args[1] = b'changed ELF'
            elif mode == 'map': args[2] = b'changed map'
            elif mode == 'headers': build['units']['timer'].remove('core/inc/types/Event.h')
            elif mode == 'unit': build['units']['probe'] = ['samd/paired-irq/profile.cpp']
            else:
                name = next(iter(before['files'])); before['files'][name]['bytes'] = True
                build['source'] = copy.deepcopy(before)
            with self.subTest(mode=mode), self.assertRaises(ValueError): module.validate(*args)


if __name__ == '__main__':
    unittest.main()
