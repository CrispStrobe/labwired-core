"""Test the capacitive touch drawing from Python.

`diagram.json` is the drawing the playground opens for the capacitive touch lab:
a Nano, a 1 MOhm resistor and a touch pad on D2/D4, an LED on D13. Nothing here
is a hand-written system file. `Sim(diagram=...)` lowers the drawing with the
same compiler the playground uses, so the circuit that runs is the one you see.

    pip install labwired pytest
    npm install -g @labwired/board-config@0.1.0   # or let labwired fetch it with npx
    pytest examples/capacitive-touch-lab/test_drawing.py
"""
import re
from pathlib import Path

import pytest

import labwired

HERE = Path(__file__).resolve().parent
FIRMWARE = HERE.parents[1] / 'tests/fixtures/avr/capacitive-touch-lab.elf'
DRAWING = HERE / 'diagram.json'


def counts(text):
    return [int(n) for n in re.findall(r'total=(\d+)', text)]


@pytest.fixture
def board():
    with labwired.Sim(FIRMWARE, diagram=DRAWING) as sim:
        yield sim


def test_a_finger_on_the_pad_changes_what_the_firmware_reports(board):
    board.watch('D13')  # the LED the sketch lights when the count passes its threshold

    board.run_for('0.4s')
    released = counts(board.read_uart())
    assert released and max(released) < 20
    assert board.edges() == []  # the LED has not come on

    board.set_signal('ui.touch.pressed', True)  # put a finger on the pad
    board.run_for('0.4s')
    pressed = counts(board.read_uart())
    assert pressed and min(pressed[-3:]) > 100
    assert [edge.level for edge in board.edges()] == [True]  # the LED came on, once

    board.set_signal('ui.touch.pressed', False)  # and take it off again
    board.run_for('0.4s')
    assert max(counts(board.read_uart())[-3:]) < 20
    assert [edge.level for edge in board.edges()] == [False]


def test_the_pad_node_is_in_the_analog_trace(board):
    board.run_for('50ms')
    header = board.analog_trace().splitlines()[0].split(',')
    assert 'circuit.v_net_mcu_d2' in header  # the pad node, where a scope probe would sit
