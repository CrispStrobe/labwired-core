"""Sim(diagram=...) against the native CLI, with the real labwired-lower.

For the same lowered files, a Python run must give the same console, the same
edges, the same net counts and the same meter readings as `labwired test`.
tests/test_diagram.py checks the Python layer against recordings so it needs no
Node; this runs it all live. It is not part of `pytest crates/python/tests`
because it needs two things the wheel gate does not have:

    LABWIRED_LOWER   path to labwired-lower (from @labwired/board-config)
    LABWIRED_CLI     path to the native `labwired` binary

They are required, not optional: without them every test here fails with a
message that says so, instead of passing by not running.

    LABWIRED_LOWER=.../dist/labwired-lower.mjs LABWIRED_CLI=target/release/labwired \
        pytest crates/python/e2e
"""
import json
import os
import re
import shutil
import subprocess
from pathlib import Path

import pytest

import labwired

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
FIRMWARE = ROOT / 'examples/gpio-net-two-boards/firmware'
STM, AVR = FIRMWARE / 'stm.elf', FIRMWARE / 'avr.elf'


def need(name):
    value = os.environ.get(name)
    if not value or not Path(value).exists():
        pytest.fail(f'{name} must point at an existing file; see the module docstring', pytrace=False)
    return value


@pytest.fixture(scope='module')
def cli():
    return need('LABWIRED_CLI')


@pytest.fixture(autouse=True)
def lower_tool():
    need('LABWIRED_LOWER')


def run_cli(cli, cwd, script, *flags):
    done = subprocess.run([cli, 'test', '--script', str(script), '--output-dir', 'out', '--no-uart-stdout', *flags],
                          cwd=cwd, capture_output=True, text=True)
    assert done.returncode == 0, done.stderr[-2000:]
    return json.loads((Path(cwd) / 'out/result.json').read_text())


def test_one_mcu_edges_analog_trace_and_meter_match_the_cli(cli, tmp_path):
    diagram = ROOT / 'crates/python/tests/diagrams/bench-divider.json'
    low = tmp_path / 'low'
    labwired.lower_diagram(diagram, low)
    (tmp_path / 't.yaml').write_text(
        'schema_version: "1.0"\n'
        f'inputs:\n  firmware: "{AVR}"\n  system: "{low}/mcu/system.yaml"\n'
        'limits:\n  max_steps: 3000000\nassertions: []\n')
    result = run_cli(cli, tmp_path, 't.yaml', '--watch-gpio', 'portd:2', '--analog-trace', str(tmp_path / 'cli.csv'))
    cli_edges = [(t['cycle'], t['value']) for t in result['logic_edges']['channels'][0]['transitions']]
    uart = (tmp_path / 'out/uart.log').read_text()

    with labwired.Sim(AVR, diagram=diagram) as sim:
        sim.watch('D2')
        sim.run_for(result['cycles'] / sim.cpu_hz)
        assert sim.cycles == result['cycles']
        assert [(e.cycle, int(e.level)) for e in sim.edges()] == cli_edges
        assert sim.uart_transcript() == uart
        assert sim.analog_trace() == (tmp_path / 'cli.csv').read_text()
        from_cli = labwired.meter_readings((tmp_path / 'cli.csv').read_text())
        assert sim.meters() == from_cli
        assert from_cli[0]['value'] == pytest.approx(2.4987507, abs=1e-6)


def test_two_boards_console_and_nets_match_the_cli(cli, tmp_path):
    diagram = ROOT / 'examples/gpio-net-two-boards/diagram.json'
    chip = {'mcu': ROOT / 'configs/chips/stm32g0b1re.yaml'}
    low = tmp_path / 'low'
    manifest = labwired.lower_diagram(diagram, low, chip_yaml=chip)
    for node, elf in (('mcu', STM), ('avr', AVR)):
        shutil.copyfile(elf, low / node / 'firmware.bin')
    (tmp_path / 't.yaml').write_text(
        'schema_version: "1.0"\n'
        f'inputs:\n  env: "{low}/world.yaml"\n'
        'limits:\n  max_steps: 400000\nassertions: []\n')
    result = run_cli(cli, tmp_path, 't.yaml')
    cli_nets = {n['name']: n['edges'] for n in result['gpio_nets']}
    log = (tmp_path / 'out/uart.log').read_text()
    cli_console = {m.group(1): m.group(2) for m in re.finditer(r'\[node:(\w+)\]\n(.*?)(?=\[node:|\Z)', log, re.S)}
    assert set(cli_console) == {'mcu', 'avr'} and manifest['kind'] == 'world'

    with labwired.Sim({'mcu': STM, 'avr': AVR}, diagram=diagram, chip_yaml=chip) as world:
        world.run_for('30ms')
        assert {n['name']: n['edges'] for n in world.nets()} == cli_nets == {'mcu.PB0': 20, 'mcu.PB1': 14, 'mcu.PB4': 16}
        for node, text in cli_console.items():
            assert world.machine(node).uart_transcript() == text


def test_a_signal_set_mid_run_gives_the_cli_console_edges_and_trace(cli, tmp_path):
    """The capacitive touch lab: a finger goes on the pad half way through."""
    example = ROOT / 'examples/capacitive-touch-lab'
    firmware = ROOT / 'tests/fixtures/avr/capacitive-touch-lab.elf'
    low = tmp_path / 'low'
    labwired.lower_diagram(example / 'diagram.json', low)
    (tmp_path / 't.yaml').write_text(
        'schema_version: "1.2"\n'
        f'inputs:\n  firmware: "{firmware}"\n  system: "{low}/mcu/system.yaml"\n'
        'stimuli:\n  - cosim_signal: { path: ui.touch.pressed, value: 1 }\n'
        '    trigger: !after_cycles { cycles: 8000000 }\n'
        'limits:\n  max_steps: 50000000\n  max_cycles: 24000000\nassertions: []\n')
    result = run_cli(cli, tmp_path, 't.yaml', '--watch-gpio', 'portb:5', '--analog-trace', str(tmp_path / 'cli.csv'))
    cli_edges = [(t['cycle'], t['value']) for t in result['logic_edges']['channels'][0]['transitions']]

    with labwired.Sim(firmware, diagram=example / 'diagram.json') as sim:
        sim.watch('D13')
        sim.run_for(8_000_000 / sim.cpu_hz)
        sim.set_signal('ui.touch.pressed', True)
        # The CLI stops at max_cycles of simulated time (its result.json `cycles` is
        # its own accounting and is not compared); run the session to the same time.
        sim.run_for(24_000_000 / sim.cpu_hz - sim.time)
        # The CLI's uart.log folds the sketch's CRLF line ends to LF; the transcript keeps the bytes.
        assert sim.uart_transcript().replace('\r\n', '\n') == (tmp_path / 'out/uart.log').read_text()
        assert [(e.cycle, int(e.level)) for e in sim.edges()] == cli_edges and cli_edges
        assert sim.analog_trace() == (tmp_path / 'cli.csv').read_text()
