"""Sim(diagram=...): the plumbing around labwired-lower, against recorded output.

The compiler lives in TypeScript (@labwired/board-config), so this suite does
not need Node: `tools/labwired-lower` below is a stand-in that replays what the
real tool wrote for two diagrams (tests/lowered/), and the expected UART, edges,
net counts and analog trace are what the native `labwired` CLI produced from the
same files (tests/cli/, and the numbers in examples/gpio-net-two-boards/README).
Everything the Python layer does after the lowering is real: the engine, the
world, the edges, the analog island and the multimeter function.

tests/e2e/ runs the same checks with the real tool and a live CLI.
"""
import json
import os
import shutil
import stat
import sys
from pathlib import Path

import pytest

import labwired

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
LOWERED = HERE / 'lowered'
CLI = HERE / 'cli'
FIRMWARE = ROOT / 'examples/gpio-net-two-boards/firmware'
STM, AVR = FIRMWARE / 'stm.elf', FIRMWARE / 'avr.elf'

STUB = '''#!{python}
import pathlib, shutil, sys
LOWERED = pathlib.Path({lowered!r})
CLI = pathlib.Path({cli!r})
args = sys.argv[1:]
mode = (LOWERED / 'MODE').read_text().strip() if (LOWERED / 'MODE').exists() else 'ok'
if args == ['--version']:
    print('0.1.0'); sys.exit(0)
if args[0] == '--meters':
    csv = pathlib.Path(args[1]).read_text()
    if csv != (CLI / 'bench-divider.analog.csv').read_text():
        print('stub: only the recorded trace has recorded readings', file=sys.stderr); sys.exit(3)
    print((CLI / 'bench-divider.meters.json').read_text()); sys.exit(0)
if mode == 'bad':
    print("labwired-lower: the diagram does not lower:\\n  mcu: part 'x' has unknown type 'mystery'", file=sys.stderr); sys.exit(2)
if mode == 'crash':
    print('boom', file=sys.stderr); sys.exit(1)
source = pathlib.Path(args[0])
out = pathlib.Path(args[args.index('--out') + 1])
shutil.copytree(LOWERED / source.stem, out, dirs_exist_ok=True)
print('{{}}')
'''


@pytest.fixture
def tool(tmp_path, monkeypatch):
    """A labwired-lower on PATH that replays the recorded outputs."""
    bindir = tmp_path / 'tools'
    bindir.mkdir()
    exe = bindir / 'labwired-lower'
    work = tmp_path / 'lowered'
    shutil.copytree(LOWERED, work)
    exe.write_text(STUB.format(python=sys.executable, lowered=str(work), cli=str(CLI)))
    exe.chmod(exe.stat().st_mode | stat.S_IEXEC)
    monkeypatch.delenv('LABWIRED_LOWER', raising=False)
    monkeypatch.setenv('PATH', str(bindir))
    return work


def diagram_copy(tmp_path, source, name):
    target = tmp_path / f'{name}.json'
    shutil.copyfile(source, target)
    return target


@pytest.fixture
def two_boards(tmp_path):
    return diagram_copy(tmp_path, ROOT / 'examples/gpio-net-two-boards/diagram.json', 'gpio-net-two-boards')


@pytest.fixture
def bench(tmp_path):
    return diagram_copy(tmp_path, HERE / 'diagrams/bench-divider.json', 'bench-divider')


def test_several_mcus_open_as_a_world_with_the_cli_numbers(tool, two_boards):
    with labwired.Sim({'mcu': STM, 'avr': AVR}, diagram=two_boards) as world:
        assert isinstance(world, labwired.World)
        assert world.nodes == ['mcu', 'avr']
        world.run_for('30ms')
        # Console lines and net edge counts: examples/gpio-net-two-boards/README.md,
        # from `labwired test --script examples/gpio-net-two-boards/test.yaml`.
        assert world.machine('mcu').uart_transcript() == 'STM irq r=10 f=10 alert r=3 f=3\n'
        assert world.machine('avr').uart_transcript() == 'AVR ready=7 alert f=5 r=5\n'
        assert {n['name']: n['edges'] for n in world.nets()} == {'mcu.PB0': 20, 'mcu.PB1': 14, 'mcu.PB4': 16}
        assert world.net('mcu.PB4')['pull'] == 'up'
        assert all(not n['diagnostics'] for n in world.nets())
        assert world.time == pytest.approx(0.030, abs=1e-6)
    assert world.closed
    with pytest.raises(RuntimeError, match='closed'):
        world.run_for('1ms')


def test_a_world_can_be_checked_part_way(tool, two_boards):
    with labwired.Sim({'mcu': STM, 'avr': AVR}, diagram=two_boards) as world:
        world.run_for('1.2ms')  # the AVR is part way through its ten pulses
        early = world.net('mcu.PB0')['edges']
        assert 0 < early < 20
        assert world.machine('mcu').read_uart() == ''
        world.run_for('28.8ms')
        assert world.net('mcu.PB0')['edges'] == 20
        assert world.machine('mcu').read_uart() == 'STM irq r=10 f=10 alert r=3 f=3\n'
        assert world.machine('mcu').read_uart() == ''


def test_world_expect_waits_in_simulated_time(tool, two_boards):
    with labwired.Sim({'mcu': STM, 'avr': AVR}, diagram=two_boards) as world:
        match = world.machine('avr').expect(r'ready=(\d+)', timeout='30ms')
        assert match.captures == ['7']
        with pytest.raises(labwired.ExpectTimeout):
            world.machine('avr').expect('never printed', timeout='1ms')


def test_world_pads_are_the_diagram_pins(tool, two_boards):
    with labwired.Sim({'mcu': STM, 'avr': AVR}, diagram=two_boards) as world:
        avr = world.machine('avr')
        assert avr.watch('D2') == [False]
        world.run_for('30ms')
        edges = avr.edges()
        assert len(edges) == 20 and edges[0].pad == 'D2' and edges[0].level is True
        assert edges[0].time == pytest.approx(edges[0].cycle / 16_000_000)
        assert avr.edges() == []
        with pytest.raises(ValueError, match='unknown pad'):
            avr.watch('PB9')


def test_a_world_needs_firmware_for_every_mcu(tool, two_boards):
    with pytest.raises(TypeError, match='several MCUs'):
        labwired.Sim(AVR, diagram=two_boards)
    with pytest.raises(ValueError, match="no firmware for MCU 'avr'"):
        labwired.Sim({'mcu': STM}, diagram=two_boards)
    with pytest.raises(ValueError, match='not MCUs of this diagram'):
        labwired.Sim({'mcu': STM, 'avr': AVR, 'extra': AVR}, diagram=two_boards)


def test_one_mcu_opens_as_a_sim_and_matches_the_cli_recording(tool, bench):
    recorded = json.loads((CLI / 'bench-divider.edges.json').read_text())
    with labwired.Sim(AVR, diagram=bench) as sim:
        assert isinstance(sim, labwired.Sim)
        assert sim.watch('D2') == [bool(recorded['initial'])]
        sim.run_for(recorded['cycles'] / sim.cpu_hz)
        assert sim.cycles == recorded['cycles']
        # Edges: the cycles and levels `labwired test --watch-gpio portd:2` recorded.
        assert [(e.cycle, int(e.level)) for e in sim.edges()] == [(t['cycle'], t['value']) for t in recorded['transitions']]
        # Analog island: byte for byte the file `--analog-trace` wrote.
        assert sim.analog_trace() == (CLI / 'bench-divider.analog.csv').read_text()
        # Meter: the playground's own function over that trace.
        assert sim.meters() == json.loads((CLI / 'bench-divider.meters.json').read_text())
        reading = sim.meter('dmm')
        assert reading['mode'] == 'vdc' and reading['value'] == pytest.approx(2.4987507, abs=1e-6)
        with pytest.raises(KeyError):
            sim.meter('nope')


def test_diagram_replaces_chip_and_system(tool, bench):
    with pytest.raises(TypeError, match='replaces chip= and system='):
        labwired.Sim(AVR, diagram=bench, chip='stm32f103')
    with pytest.raises(TypeError, match='needs the firmware'):
        labwired.Sim(None, diagram=bench)


def test_a_diagram_that_does_not_lower_names_the_problem(tool, bench):
    (tool / 'MODE').write_text('bad')
    with pytest.raises(labwired.DiagramError, match="unknown type 'mystery'"):
        labwired.Sim(AVR, diagram=bench)
    (tool / 'MODE').write_text('crash')
    with pytest.raises(RuntimeError, match='boom'):
        labwired.Sim(AVR, diagram=bench)


def test_without_node_there_is_one_clear_error_and_no_network_fallback(tmp_path, monkeypatch, bench):
    monkeypatch.delenv('LABWIRED_LOWER', raising=False)
    monkeypatch.setenv('PATH', str(tmp_path))  # no labwired-lower, no npx
    with pytest.raises(labwired.LowerUnavailable) as err:
        labwired.Sim(AVR, diagram=bench)
    text = str(err.value)
    assert 'Node.js' in text and 'npm install -g @labwired/board-config@0.1.0' in text
    assert 'http' not in text.replace('https://nodejs.org', '')


def test_without_labwired_lower_on_path_the_pinned_package_runs_through_npx(tool, tmp_path, monkeypatch, bench):
    stub = tool.parent / 'tools' / 'labwired-lower'
    record = tmp_path / 'npx-argv.json'
    npx = tmp_path / 'npxbin' / 'npx'
    npx.parent.mkdir()
    npx.write_text(
        f'#!{sys.executable}\n'
        'import json, subprocess, sys\n'
        'argv = sys.argv[1:]\n'
        f'open({str(record)!r}, "w").write(json.dumps(argv))\n'
        f'sys.exit(subprocess.call([{str(stub)!r}] + argv[argv.index("labwired-lower") + 1:]))\n')
    npx.chmod(npx.stat().st_mode | stat.S_IEXEC)
    monkeypatch.setenv('PATH', str(npx.parent))
    with labwired.Sim(AVR, diagram=bench) as sim:
        assert sim.analog_trace().startswith('time_ns,circuit.dmm_vdc_p_dmm')
    # `--package=` selects the package, then the bin: a bare `npx pkg labwired-lower`
    # would hand "labwired-lower" to the tool as its first argument.
    assert json.loads(record.read_text())[:3] == ['-y', '--package=@labwired/board-config@0.1.0', 'labwired-lower']


def test_a_lowering_tool_of_another_major_minor_is_refused(tmp_path, monkeypatch, bench):
    exe = tmp_path / 'labwired-lower'
    exe.write_text(f'#!{sys.executable}\nprint("0.2.3")\n')
    exe.chmod(exe.stat().st_mode | stat.S_IEXEC)
    monkeypatch.setenv('LABWIRED_LOWER', str(exe))
    with pytest.raises(labwired.LowerUnavailable, match=r'0\.2\.3.*tested with 0\.1\.0'):
        labwired.Sim(AVR, diagram=bench)


def test_the_wheel_records_the_board_config_it_was_tested_with():
    from labwired._board_config import BOARD_CONFIG_VERSION
    major, minor, _ = BOARD_CONFIG_VERSION.split('.')
    assert (int(major), int(minor)) == (0, 1)
