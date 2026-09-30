#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Build both native engines, then measure B/C/C/B on one unchanged runner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys

from microbit_motion_report import report, source_bundle

TEST_TARGET = 'microbit_v2_motion_io_guest'
BENCHMARK = 'active_motion_display_button_workload_throughput'
SCHEDULE = ('baseline', 'candidate', 'candidate', 'baseline')
IDENTICAL_FILES = (
    'examples/microbit-v2/board-io.S',
    'examples/microbit-v2/motion-polled.inc',
    'examples/microbit-v2/board-io.ld',
    'crates/core/tests/microbit_v2_motion_io_guest.rs',
)


def validate_commit(value):
    if not re.fullmatch(r'[0-9a-f]{40}', value):
        raise ValueError('baseline_commit must be a full lowercase 40-hex commit SHA')
    return value


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def git_output(repo, *args):
    return subprocess.check_output(['git', *args], cwd=repo, text=True).strip()


def verify_identical_workload(baseline, candidate):
    hashes = {}
    for name in IDENTICAL_FILES:
        payload = (baseline / name).read_bytes()
        if payload != (candidate / name).read_bytes():
            raise ValueError('baseline/candidate workload differs: ' + name)
        hashes[name] = hashlib.sha256(payload).hexdigest()
    return hashes


def select_executable(build_log, target_dir):
    executables = []
    for line in build_log.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (item.get('reason') == 'compiler-artifact'
                and item.get('target', {}).get('name') == TEST_TARGET
                and item.get('profile', {}).get('test') is True
                and item.get('executable')):
            path = Path(item['executable']).resolve()
            if not path.is_relative_to(target_dir.resolve()) or not path.is_file():
                raise ValueError('compiled test executable is outside its isolated target directory')
            executables.append(path)
    unique = set(executables)
    if len(unique) != 1:
        raise ValueError('build did not identify exactly one motion test executable')
    return unique.pop()


def fingerprint():
    commands = (('kernel', ['uname', '-a']), ('cpu', ['lscpu']),
                ('rustc', ['rustc', '-Vv']), ('cargo', ['cargo', '-V']),
                ('armGcc', ['arm-none-eabi-gcc', '--version']))
    result = {}
    for label, command in commands:
        completed = subprocess.run(command, text=True, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, check=False)
        if completed.returncode:
            raise ValueError('runner fingerprint command failed: ' + label)
        result[label] = completed.stdout
    result['github'] = {name: os.environ.get(name) for name in (
        'GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_REF', 'GITHUB_SHA',
        'RUNNER_OS', 'RUNNER_ARCH', 'RUNNER_NAME')}
    return result


def build(repo, target_dir, evidence, role):
    env = os.environ.copy()
    env['CARGO_TARGET_DIR'] = str(target_dir)
    completed = subprocess.run([
        'cargo', 'test', '--release', '-p', 'labwired-core',
        '--features', 'microbit-board-io-test', '--test', TEST_TARGET,
        '--no-run', '--message-format=json'], cwd=repo, env=env, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    (evidence / ('build-' + role + '.log')).write_text(completed.stdout)
    if completed.returncode:
        raise ValueError(role + ' build failed; complete compiler log retained')
    return select_executable(completed.stdout, target_dir)


def invocation(repo, executable, commit, evidence, index, role):
    name = '%02d-%s' % (index, role)
    env = os.environ.copy()
    env['LABWIRED_REQUIRE_REALTIME'] = '1' if role == 'candidate' else '0'
    env['LABWIRED_GUEST_ARTIFACT_DIR'] = str(evidence / (name + '-guest-artifacts'))
    completed = subprocess.run([
        str(executable), BENCHMARK, '--ignored', '--nocapture', '--test-threads=1', '--format=terse'],
        cwd=repo, env=env, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, check=False)
    (evidence / (name + '.log')).write_text(completed.stdout)
    entry = {'index': index, 'role': role, 'commit': commit,
             'requireRealtime': role == 'candidate', 'exitCode': completed.returncode,
             'log': name + '.log', 'receipt': name + '.json'}
    try:
        receipt = report(completed.stdout, commit, source_bundle(repo))
        entry['medianRtx'] = receipt['medianRtx']
        entry['realtimeTargetMet'] = receipt['realtimeTargetMet']
        entry['measurementValid'] = True
        receipt['abInvocation'] = {'index': index, 'role': role,
                                   'requireRealtime': role == 'candidate',
                                   'exitCode': completed.returncode}
        write_json(evidence / entry['receipt'], receipt)
    except (ValueError, OSError) as exc:
        entry['measurementValid'] = False
        entry['error'] = str(exc)
        write_json(evidence / entry['receipt'], {'measurementValid': False,
                                                'error': str(exc), 'invocation': entry})
    return entry


def summarize(entries):
    if len(entries) != 4 or tuple(item['role'] for item in entries) != SCHEDULE:
        raise ValueError('expected exactly baseline/candidate/candidate/baseline invocations')
    valid = all(item['measurementValid'] for item in entries)
    baseline_ok = all(item['exitCode'] == 0 for item in entries if item['role'] == 'baseline')
    candidate_ok = all(item['exitCode'] == 0 and item.get('realtimeTargetMet') is True
                       for item in entries if item['role'] == 'candidate')
    value = {'schema': 'labwired.microbit.motion-same-runner-ab.v1',
             'schedule': list(SCHEDULE), 'invocations': entries,
             'passed': valid and baseline_ok and candidate_ok,
             'candidateRealtimeTargetMet': valid and candidate_ok,
             'limitations': ['same runner, sequential alternating order; no host contention isolation',
                             'each invocation retains five samples and existing eight-million-step warmup',
                             'baseline may be below 1x; each candidate median must remain >=1x',
                             'native held-input polled sensor workload only; no browser/audio qualification']}
    if valid:
        baseline = statistics.median(item['medianRtx'] for item in entries if item['role'] == 'baseline')
        candidate = statistics.median(item['medianRtx'] for item in entries if item['role'] == 'candidate')
        value.update(baselineMedianOfMediansRtx=baseline, candidateMedianOfMediansRtx=candidate,
                     candidateToBaselineMedianRatio=candidate / baseline)
    return value


def run(candidate, baseline_commit, work_root, evidence):
    validate_commit(baseline_commit)
    candidate = candidate.resolve()
    work_root = work_root.resolve()
    evidence = evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    context = fingerprint()
    write_json(evidence / 'runner-context.json', context)
    context_hash = hashlib.sha256(json.dumps(context, sort_keys=True).encode()).hexdigest()
    if work_root.exists():
        raise ValueError('isolated build/worktree directory must not already exist')
    work_root.mkdir(parents=True)
    candidate_commit = validate_commit(git_output(candidate, 'rev-parse', 'HEAD'))
    if git_output(candidate, 'cat-file', '-t', baseline_commit) != 'commit':
        raise ValueError('baseline object must be a commit in the fetched repository')
    if git_output(candidate, 'status', '--porcelain', '--untracked-files=no'):
        raise ValueError('candidate checkout must have no tracked modifications')
    baseline = work_root / 'baseline'
    subprocess.run(['git', 'worktree', 'add', '--detach', str(baseline), baseline_commit],
                   cwd=candidate, check=True)
    if git_output(baseline, 'rev-parse', 'HEAD') != baseline_commit:
        raise ValueError('baseline checkout provenance mismatch')
    hashes = verify_identical_workload(baseline, candidate)
    provenance = {'baselineCommit': baseline_commit, 'candidateCommit': candidate_commit,
                  'identicalWorkloadSha256': hashes, 'runnerFingerprintSha256': context_hash,
                  'baselineSourceBundleSha256': hashlib.sha256(source_bundle(baseline)).hexdigest(),
                  'candidateSourceBundleSha256': hashlib.sha256(source_bundle(candidate)).hexdigest()}
    write_json(evidence / 'source-provenance.json', provenance)
    # Complete both builds before invoking either timer-bearing benchmark.
    executables = {role: build(repo, work_root / ('target-' + role), evidence, role)
                   for role, repo in (('baseline', baseline), ('candidate', candidate))}
    entries = []
    for index, role in enumerate(SCHEDULE, 1):
        repo = baseline if role == 'baseline' else candidate
        commit = baseline_commit if role == 'baseline' else candidate_commit
        print('Running same-runner invocation %d: %s (%s)' % (index, role, commit), flush=True)
        entries.append(invocation(repo, executables[role], commit, evidence, index, role))
    value = summarize(entries)
    value.update(provenance)
    write_json(evidence / 'summary.json', value)
    print(json.dumps(value, indent=2), flush=True)
    return 0 if value['passed'] else 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline-commit', required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--work-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        return run(args.candidate, args.baseline_commit, args.work_root, args.output)
    except (ValueError, OSError, subprocess.CalledProcessError) as exc:
        print('Same-runner qualification failed: ' + str(exc), file=sys.stderr)
        if args.output.is_dir():
            write_json(args.output / 'failure.json', {'error': str(exc)})
        return 1


if __name__ == '__main__':
    sys.exit(main())
