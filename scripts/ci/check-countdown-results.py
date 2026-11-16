#!/usr/bin/env python3
"""Require the exact opted-in countdown controls to have executed and passed."""
import re
import sys
from pathlib import Path

EXPECTED = (
    {
        'aggregate_survives_cap_and_counts_zero_returns',
        'unmatched_entry_and_counter_overflow_are_not_complete',
        'unfinished_overrun_and_overflow_are_not_complete',
        'real_dispatch_counts_retirement_without_changing_cpu_state',
    },
    {'machine_advance_tests::countdown_driver_uses_real_machine_progress_and_detects_outside_steps'},
)


def check(text, names):
    actual = re.findall(r'^test (\S+) \.\.\. ok$', text, re.MULTILINE)
    summaries = re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text, re.MULTILINE)
    if len(actual) != len(names) or set(actual) != names or summaries != [(str(len(names)), '0', '0')]:
        raise ValueError('countdown control names, counts or results do not match')


def main(args):
    if len(args) != 2:
        raise SystemExit('usage: check-countdown-results.py CORE_LOG WASM_LOG')
    for name, expected in zip(args, EXPECTED):
        check(Path(name).read_text(encoding='utf-8', errors='replace'), expected)
    print('countdown controls: exactly four core and one native WASM-driver test passed')


if __name__ == '__main__':
    main(sys.argv[1:])
