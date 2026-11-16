"""Collector regression controls; synthetic bytes are never native proof.

Run: python3 scripts/ci/test_native_test_bundle.py (requires pinned Rust 1.95).
"""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().with_name('native_test_bundle.py')
spec = importlib.util.spec_from_file_location('native_bundle', SOURCE)
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)
IDENTITY = {'checkout': 'synthetic-packaging-control', 'tree': 'synthetic-packaging-control'}
COMPILER = subprocess.check_output(['rustup', 'which', '--toolchain', '1.95.0', 'rustc'], text=True).strip()


class BundleControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='pr299-bundle-control-')
        self.root = Path(self.temp.name)
        self.deps = self.root / 'target/debug/deps'
        self.deps.mkdir(parents=True)
        self.records = self.root / 'target/records'
        self.records.mkdir(parents=True)
        self.generated = self.root / 'target/debug/build/core/out'
        self.generated.mkdir(parents=True)
        (self.root/'Cargo.lock').write_text('synthetic public lock provenance')
        (self.root/'LICENSE').write_text('synthetic public license provenance')
        (self.generated / 'model-input').write_bytes(b'synthetic packaging input')
        (self.deps / 'liblabwired_core-control.rlib').write_bytes(b'!<arch>\nSYNTHETIC FORMAT CONTROL ONLY')
        (self.deps / 'liblabwired_core-control.rmeta').write_bytes(b'synthetic full-library metadata')
        (self.deps / 'libsupport-control.rmeta').write_bytes(b'synthetic dependency metadata')
        (self.deps / 'libsupport-control.rlib').write_bytes(b'!<arch>\nsynthetic dependency')
        for name in ['labwired_core-control', 'avr-control', 'gates-control']:
            (self.deps / name).write_bytes(b'\x7fELF SYNTHETIC FORMAT CONTROL ONLY')
        for role, crate in [('library', 'labwired_core'), ('core-unit', 'labwired_core'), ('avr', 'avr'), ('gates', 'gates')]:
            args = ['--crate-name', crate, '--out-dir', str(self.deps), '-C', 'extra-filename=-control', '--cfg', 'feature="default"', '--extern', 'support='+str(self.deps/'libsupport-control.rmeta')]
            args += ['--emit=dep-info,metadata,link'] if role == 'library' else ['--test']
            if role in {'avr','gates'}:
                args += ['--extern','labwired_core='+str(self.deps/'liblabwired_core-control.rlib')]
            record = {'compiler': COMPILER, 'argv': args, 'cwd': str(self.root),
                      'env': {'OUT_DIR': str(self.generated)}, 'source': IDENTITY, 'source_after': IDENTITY,
                      'compiler_exit': 0, 'complete': True, 'started_ns': time.time_ns()-10**9,
                      'completed_ns': time.time_ns(),
                      'outputs': {str(p): bundle.file_state(p, hashed=True) for p in bundle.products(args)},
                      'before': {str(p): None for p in bundle.products(args)}}
            (self.records / (role+'.json')).write_text(json.dumps(record))
        self.source_patch = patch.object(bundle, 'source_identity', return_value=IDENTITY)
        self.records_patch = patch.object(bundle, 'RECORDS', self.records)
        self.deps_patch = patch.object(bundle, 'dependency_crates', return_value={'labwired_core', 'support'})
        self.root_patch = patch.object(bundle, 'ROOT', self.root)
        self.git_patch = patch.object(bundle, 'git', return_value='synthetic-provenance-blob')
        for item in [self.source_patch, self.records_patch, self.deps_patch, self.root_patch, self.git_patch]:
            item.start()
            self.addCleanup(item.stop)
        self.addCleanup(self.temp.cleanup)


    def extra_library(self):
        record = json.loads((self.records/'library.json').read_text())
        record['argv'] = [item.replace('extra-filename=-control','extra-filename=-second') for item in record['argv']]
        for product in bundle.products(record['argv']):
            product.write_bytes(b'!<arch>\nSYNTHETIC SECOND VARIANT' if product.suffix=='.rlib' else b'synthetic second metadata')
        record['outputs'] = {str(path):bundle.file_state(path,hashed=True) for path in bundle.products(record['argv'])}
        record['before'] = {str(path):None for path in bundle.products(record['argv'])}
        record['completed_ns'] = time.time_ns()
        (self.records/'second-library.json').write_text(json.dumps(record))
        return record

    def packed_manifest(self):
        output = self.root/'multi.tar.gz'
        bundle.pack('feature-off',output)
        return json.loads(output.with_suffix('.gz.manifest.json').read_text())

    def test_multiple_libraries_selects_exact_avr_extern(self):
        self.extra_library()
        manifest = self.packed_manifest()
        self.assertEqual(manifest['records']['library']['argv'],json.loads((self.records/'library.json').read_text())['argv'])
        self.assertEqual(manifest['library_variants'],[])
        self.assertNotIn('deps/liblabwired_core-second.rlib',manifest['files'])

    def test_other_library_referenced_by_gates_is_preserved(self):
        second = self.extra_library()
        path = self.records/'gates.json';record=json.loads(path.read_text())
        record['argv']=[item.replace('labwired_core-control.rlib','labwired_core-second.rlib') for item in record['argv']]
        path.write_text(json.dumps(record))
        manifest = self.packed_manifest()
        self.assertEqual(manifest['library_variants'],[second])
        for suffix in ['rlib','rmeta']:
            self.assertIn('deps/liblabwired_core-second.'+suffix,manifest['files'])

    def test_library_dependency_variant_is_preserved(self):
        second=self.extra_library()
        path=self.records/'library.json';record=json.loads(path.read_text());record['argv']+=['--extern','labwired_core='+str(self.deps/'liblabwired_core-second.rmeta')];path.write_text(json.dumps(record))
        manifest=self.packed_manifest()
        self.assertEqual(manifest['library_variants'],[second])
        self.assertIn('deps/liblabwired_core-second.rlib',manifest['files'])

    def test_referenced_variant_with_changed_compiler_is_refused(self):
        second=self.extra_library();second['compiler']='wrong-compiler'
        (self.records/'second-library.json').write_text(json.dumps(second))
        path=self.records/'gates.json';record=json.loads(path.read_text());record['argv']=[item.replace('labwired_core-control.rlib','labwired_core-second.rlib') for item in record['argv']];path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError,'compiler identity changed'):
            self.packed_manifest()

    def test_duplicate_exact_library_record_is_refused(self):
        (self.records/'duplicate.json').write_bytes((self.records/'library.json').read_bytes())
        with self.assertRaisesRegex(RuntimeError,'ambiguous exact Core library'):
            self.packed_manifest()

    def test_avr_reference_without_matching_record_is_refused(self):
        (self.records/'library.json').unlink()
        with self.assertRaisesRegex(RuntimeError,'missing or ambiguous exact Core library'):
            self.packed_manifest()

    def test_wrong_feature_referenced_library_is_refused(self):
        path=self.records/'library.json';record=json.loads(path.read_text());record['argv']+=['--cfg','feature="jit-framework"'];path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError,'missing or ambiguous exact Core library'):
            self.packed_manifest()

    def test_wrong_source_referenced_library_is_refused(self):
        path=self.records/'library.json';record=json.loads(path.read_text());record['source']={'checkout':'wrong','tree':'wrong'};path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError,'missing or ambiguous exact Core library'):
            self.packed_manifest()

    def test_missing_avr_core_extern_is_refused(self):
        path=self.records/'avr.json';record=json.loads(path.read_text());record['argv']=record['argv'][:-2];path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError,'AVR must reference exactly one'):
            self.packed_manifest()

    def test_wrapper_excludes_credentials_and_preserves_real_argv(self):
        for path in self.records.iterdir():
            path.unlink()
        args = ['--crate-name', 'avr', '--test', '--out-dir', str(self.deps), '-C', 'extra-filename=-control']
        env = {'CARGO_MANIFEST_DIR': str(bundle.ROOT/'crates/core'), 'CARGO_PKG_NAME': 'labwired-core',
               'CARGO_REGISTRY_TOKEN': 'CONTROL_SECRET_NOT_TO_RETAIN', 'GITHUB_TOKEN': 'SECOND_CONTROL_SECRET'}
        def actual_completion(command, *, close_fds):
            self.assertFalse(close_fds)
            (self.deps/'avr-control').write_bytes(b'\x7fELF synthetic fresh output')
            when = time.time_ns()
            os.utime(self.deps/'avr-control', ns=(when, when))
            return SimpleNamespace(returncode=0)
        with patch.dict(os.environ, env, clear=True), patch.object(bundle.subprocess, 'run', side_effect=actual_completion) as execute:
            self.assertEqual(bundle.wrapper(COMPILER, args), 0)
        record_text = next(self.records.iterdir()).read_text()
        self.assertNotIn('CONTROL_SECRET', record_text)
        self.assertNotIn('REGISTRY_TOKEN', record_text)
        self.assertNotIn('GITHUB_TOKEN', record_text)
        self.assertIn('CARGO_PKG_NAME', record_text)
        execute.assert_called_once_with([COMPILER, *args], close_fds=False)
        self.assertTrue(json.loads(record_text)['complete'])

    def test_pack_round_trip_bytes_metadata_and_complete_roles(self):
        output = self.root/'control.tar.gz'
        bundle.pack('feature-off', output)
        with tarfile.open(output, 'r:gz') as archive:
            manifest = json.load(archive.extractfile('manifest.json'))
            self.assertEqual(set(manifest['records']), {'library', 'core-unit', 'avr', 'gates'})
            self.assertTrue(manifest['complete'])
            self.assertIn('provenance/Cargo.lock', manifest['files'])
            self.assertIn('provenance/LICENSE', manifest['files'])
            for name, data in manifest['files'].items():
                content = archive.extractfile(name).read()
                self.assertEqual(len(content), data['bytes'])
                self.assertEqual(bundle.hashlib.sha256(content).hexdigest(), data['sha256'])
                member = archive.getmember(name)
                self.assertEqual(member.mode, data['mode'])
                self.assertEqual(member.uid, data['uid'])
                self.assertEqual(member.gid, data['gid'])
                self.assertEqual(member.pax_headers['mtime'], str(data['mtime_ns']//10**9)+'.'+f"{data['mtime_ns']%10**9:09d}")
        self.assertTrue(output.with_suffix('.gz.sha256').read_text().startswith(bundle.digest(output)))

    def test_metadata_only_core_cannot_become_a_full_library(self):
        (self.deps/'liblabwired_core-control.rlib').unlink()
        with self.assertRaises((FileNotFoundError, RuntimeError)):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_missing_exact_dependency_is_refused(self):
        (self.deps/'libsupport-control.rmeta').unlink()
        with self.assertRaisesRegex(RuntimeError, 'missing exact compiler dependency'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_wrong_feature_graph_is_refused(self):
        with self.assertRaisesRegex(RuntimeError, 'missing genuine native compiler records'):
            bundle.pack('workspace', self.root/'refused.tar.gz')

    def test_stale_source_identity_is_refused(self):
        path = self.records/'avr.json'
        record = json.loads(path.read_text())
        record['source'] = {'checkout': 'stale', 'tree': 'stale'}
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'missing genuine native compiler records'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_metadata_only_compiler_command_is_refused(self):
        path = self.records/'library.json'
        record = json.loads(path.read_text())
        record['argv'].remove('--emit=dep-info,metadata,link')
        record['argv'].append('--emit=metadata')
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'not compiled in full'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_nonzero_compiler_record_is_refused(self):
        path = self.records/'avr.json'
        record = json.loads(path.read_text())
        record.update(compiler_exit=7, complete=False)
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'unsuccessful real compiler'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_stale_output_is_refused(self):
        path = self.records/'avr.json'
        record = json.loads(path.read_text())
        record['before'] = {name: {k:v for k,v in data.items() if k != 'sha256'} for name,data in record['outputs'].items()}
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'freshly produce'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_output_changed_after_success_is_refused(self):
        (self.deps/'avr-control').write_bytes(b'altered after compiler completion')
        with self.assertRaisesRegex(RuntimeError, 'altered native compiler output'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_generated_root_outside_target_is_refused(self):
        outside = self.root/'outside'
        outside.mkdir()
        path = self.records/'avr.json'
        record = json.loads(path.read_text())
        record['env']['OUT_DIR'] = str(outside)
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'escapes the Cargo target'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_escaping_generated_symlink_is_refused(self):
        outside = self.root/'outside'
        outside.write_text('synthetic secret not to capture')
        (self.generated/'escape').symlink_to(outside)
        with self.assertRaisesRegex(RuntimeError, 'symlink in generated'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_available_failure_bundle_is_explicitly_partial(self):
        for name in ['library', 'avr', 'gates']:
            (self.records/(name+'.json')).unlink()
        output = self.root/'partial.tar.gz'
        bundle.pack('feature-off', output, available=True)
        with tarfile.open(output, 'r:gz') as archive:
            manifest = json.load(archive.extractfile('manifest.json'))
            self.assertFalse(manifest['complete'])
            self.assertEqual(set(manifest['records']), {'core-unit'})
            self.assertEqual(set(manifest['missing_roles']), {'library', 'avr', 'gates'})
            self.assertFalse(any(name.startswith('deps/liblabwired_core-') for name in manifest['files']))

    def test_unfinished_compiler_record_is_refused(self):
        path = self.records/'avr.json'
        record = json.loads(path.read_text())
        record.update(compiler_exit=None, complete=False)
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'unsuccessful real compiler'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_missing_freshness_record_is_refused(self):
        path = self.records/'avr.json'
        record = json.loads(path.read_text())
        del record['before']
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'incomplete compiler record'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_symlinked_provenance_is_refused(self):
        (self.root/'LICENSE').unlink()
        (self.root/'LICENSE').symlink_to(self.generated/'model-input')
        with self.assertRaisesRegex(RuntimeError, 'nonregular native bundle payload'):
            bundle.pack('feature-off', self.root/'refused.tar.gz')

    def test_wrapper_forwards_nonzero_and_never_marks_it_complete(self):
        for path in self.records.iterdir():
            path.unlink()
        args = ['--crate-name', 'avr', '--test', '--out-dir', str(self.deps), '-C', 'extra-filename=-control']
        with patch.dict(os.environ, {'CARGO_MANIFEST_DIR': str(bundle.ROOT/'crates/core')}, clear=True), patch.object(bundle.subprocess, 'run', return_value=SimpleNamespace(returncode=7)):
            self.assertEqual(bundle.wrapper(COMPILER, args), 7)
        record = json.loads(next(self.records.iterdir()).read_text())
        self.assertEqual(record['compiler_exit'], 7)
        self.assertFalse(record['complete'])

    def test_wrapper_zero_exit_does_not_validate_stale_outputs(self):
        for path in self.records.iterdir():
            path.unlink()
        args = ['--crate-name', 'avr', '--test', '--out-dir', str(self.deps), '-C', 'extra-filename=-control']
        with patch.dict(os.environ, {'CARGO_MANIFEST_DIR': str(bundle.ROOT/'crates/core')}, clear=True), patch.object(bundle.subprocess, 'run', return_value=SimpleNamespace(returncode=0)):
            self.assertEqual(bundle.wrapper(COMPILER, args), 0)
        record = json.loads(next(self.records.iterdir()).read_text())
        self.assertEqual(record['compiler_exit'], 0)
        self.assertFalse(record['complete'])

    def test_actual_child_signals_are_forwarded_exactly(self):
        # Real signal/exit control for the wrapper, not a Rust qualification.
        control = self.root/'signal-compiler-control'
        args = ['--crate-name', 'avr', '--test', '--out-dir', str(self.deps), '-C', 'extra-filename=-control']
        program = '''
import importlib.util,json,os,pathlib,sys
spec=importlib.util.spec_from_file_location('subject',sys.argv[1]);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
m.ROOT=pathlib.Path(sys.argv[2]);m.RECORDS=m.ROOT/'target/records';m.source_identity=lambda: {'checkout':'synthetic-packaging-control','tree':'synthetic-packaging-control'}
os.environ['CARGO_MANIFEST_DIR']=str(m.ROOT/'crates/core')
raise SystemExit(m.wrapper(sys.argv[3],json.loads(sys.argv[4])))
'''
        for sig in [signal.SIGTERM, signal.SIGKILL]:
            control.write_text('#!/bin/sh\nkill -'+str(int(sig))+' $$\n')
            control.chmod(0o700)
            result = subprocess.run([sys.executable, '-c', program, str(SOURCE), str(self.root), str(control), json.dumps(args)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            self.assertEqual(result.returncode, -sig, result.stderr.decode())

    def test_locked_host_metadata_retains_aliases_without_unrelated_payload(self):
        # A Core-only build need not have downloaded CLI dev-dependencies;
        # metadata may fetch locked manifests, but only Core's closure is kept.
        metadata = {
            'packages': [
                {'id':'core','name':'labwired-core','targets':[{'name':'labwired_core','kind':['lib']}]},
                {'id':'digest','name':'md-5','targets':[{'name':'md5','kind':['lib']}]},
                {'id':'cli-test','name':'assert_cmd','targets':[{'name':'assert_cmd','kind':['lib']}]},
            ],
            'resolve': {'nodes': [
                {'id':'core','deps':[{'pkg':'digest'}]}, {'id':'digest','deps':[]}, {'id':'cli-test','deps':[]},
            ]},
        }
        self.deps_patch.stop()
        with patch.object(bundle.subprocess, 'check_output', return_value=json.dumps(metadata).encode()) as query:
            self.assertEqual(bundle.dependency_crates('x86_64-unknown-linux-gnu'), {'labwired_core','md5'})
        args = query.call_args.args[0]
        self.assertIn('--locked', args)
        self.assertNotIn('--offline', args)
        self.assertEqual(args[-2:], ['--filter-platform','x86_64-unknown-linux-gnu'])

    def test_actual_inherited_jobserver_descriptor_survives_wrapper(self):
        # Real process/FD control only; this does not compile Rust or qualify a model.
        control = self.root/'jobserver-compiler-control'
        control.write_text('''#!/usr/bin/python3
import os,re,time
from pathlib import Path
read_fd,write_fd=map(int,re.search(r'--jobserver-auth=(\\d+),(\\d+)',os.environ['CARGO_MAKEFLAGS']).groups())
assert os.read(read_fd,1)==b'+'
os.write(write_fd,b'+')
path=Path(os.environ['PR299_PUBLIC_CONTROL_OUTPUT']);path.write_bytes(b'\\x7fELF synthetic fresh FD control')
when=time.time_ns();os.utime(path,ns=(when,when))
''')
        control.chmod(0o700)
        args = ['--crate-name', 'avr', '--test', '--out-dir', str(self.deps), '-C', 'extra-filename=-control']
        program = '''
import importlib.util,json,os,pathlib,sys
spec=importlib.util.spec_from_file_location('subject',sys.argv[1]);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
m.ROOT=pathlib.Path(sys.argv[2]);m.RECORDS=m.ROOT/'target/records';m.source_identity=lambda: {'checkout':'synthetic-packaging-control','tree':'synthetic-packaging-control'}
os.environ['CARGO_MANIFEST_DIR']=str(m.ROOT/'crates/core')
if len(sys.argv)>5 and sys.argv[5]=='old-close-fds':
    real_run=m.subprocess.run
    def old_run(command,**kwargs):
        kwargs['close_fds']=True
        return real_run(command,**kwargs)
    m.subprocess.run=old_run
raise SystemExit(m.wrapper(sys.argv[3],json.loads(sys.argv[4])))
'''
        read_fd, write_fd = os.pipe()
        try:
            os.set_inheritable(read_fd, True)
            os.set_inheritable(write_fd, True)
            os.write(write_fd, b'+')
            env = dict(os.environ, CARGO_MAKEFLAGS=f'-j --jobserver-auth={read_fd},{write_fd}',
                       PR299_PUBLIC_CONTROL_OUTPUT=str(self.deps/'avr-control'))
            old = subprocess.run([sys.executable, '-c', program, str(SOURCE), str(self.root), str(control), json.dumps(args), 'old-close-fds'],
                                 env=env,close_fds=False,capture_output=True)
            self.assertNotEqual(old.returncode, 0)
            self.assertIn('Bad file descriptor', old.stderr.decode())
            result = subprocess.run([sys.executable, '-c', program, str(SOURCE), str(self.root), str(control), json.dumps(args)],
                                    env=env,close_fds=False,capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertEqual(os.read(read_fd,1), b'+')
            print(f'INHERITED_FD_CONTROL old_exit={old.returncode} old_EBADF=True new_exit={result.returncode}')
        finally:
            os.close(read_fd)
            os.close(write_fd)

    def test_uncompressed_size_bound_precedes_archive_creation(self):
        output = self.root/'refused.tar.gz'
        with patch.object(bundle, 'MAX_INPUT_BYTES', 1), self.assertRaisesRegex(RuntimeError, 'input bound'):
            bundle.pack('feature-off', output)
        self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
