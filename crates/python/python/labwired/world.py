"""A diagram with several MCUs, run as one world."""
import json

from ._native import NativeWorld
from . import Match, _Edges, _nanoseconds, _pad_ref

__all__ = ['World', 'WorldMachine']


class WorldMachine(_Edges):
    """One MCU of a World. Time only moves when the World runs."""

    def __init__(self, world, node_id, pads):
        self._world = world
        self.id = node_id
        self._pads = pads

    @property
    def _native(self):
        return self._world._open()

    @property
    def cycles(self):
        return self._native.cycles(self.id)

    @property
    def time(self):
        return self._native.node_time_ns(self.id) / 1e9

    def uart_transcript(self):
        return self._native.uart_transcript(self.id)

    def read_uart(self):
        return self.read_uart_bytes().decode('utf-8', errors='replace')

    def read_uart_bytes(self):
        return bytes(self._native.read_uart(self.id))

    def expect(self, pattern, timeout='1s'):
        ns = _nanoseconds(timeout)
        if ns == 0:
            raise ValueError('expect timeout must be positive')
        return Match(*self._native.expect(self.id, pattern, ns))

    def set_input(self, channel, value):
        self._native.set_input(self.id, channel, value)

    def set_inputs(self, values):
        for channel, value in values.items():
            self.set_input(channel, value)

    def list_inputs(self):
        return [dict(device=device, **channel) for device, channel in json.loads(self._native.list_inputs(self.id))]

    def set_pin(self, pad, level):
        """Hold a pad (a diagram pin name, 'gpiob:4' or a pair) at `level`, as an outside contact would."""
        peripheral, pin = _pad_ref(pad, self._pads)
        self._native.set_pin(self.id, peripheral, pin, bool(level))

    def watch(self, *pads):
        return self._arm(pads, lambda refs: self._native.watch_logic(self.id, refs))

    def edges(self):
        native = self._native
        return self._drain(lambda cursor: native.logic_edges(self.id, cursor), native.cpu_hz(self.id))

    def read_memory(self, address, length):
        return bytes(self._native.read_memory(self.id, address, length))

    def read_u32(self, address):
        return int.from_bytes(self.read_memory(address, 4), 'little')


class World:
    """The MCUs of a diagram, the UART links and GPIO nets between them, stepped together.

    ``world.machine("u1")`` is one MCU; ``world.run_for("10ms")`` advances all of them.
    ``world.nets()`` reports every GPIO net between chips: its level, edge count,
    members and any contention or floating diagnostics.
    """

    def __init__(self, environment, nodes, tmp, lowered):
        self._native = NativeWorld(str(environment))
        self._tmp = tmp
        self.lowered = lowered
        self._machines = {n['id']: WorldMachine(self, n['id'], n['pads']) for n in nodes}
        self._closed = False

    def _open(self):
        if self._closed:
            raise RuntimeError('World is closed')
        return self._native

    @property
    def closed(self):
        return self._closed

    def close(self):
        if not self._closed:
            self._native.close()
            self._closed = True

    def __enter__(self):
        self._open()
        return self

    def __exit__(self, *exc):
        self.close()

    @property
    def nodes(self):
        return list(self._machines)

    def machine(self, node_id):
        try:
            return self._machines[node_id]
        except KeyError:
            raise KeyError(f'no MCU {node_id!r}; the diagram has {self.nodes}') from None

    @property
    def time(self):
        """World time in seconds: the slowest node's clock."""
        return self._open().time_ns() / 1e9

    def run_for(self, duration):
        self._open().run_for(_nanoseconds(duration))

    def nets(self):
        """Every GPIO net between chips, as dicts: name, level, edges, contention_events, members, diagnostics."""
        return json.loads(self._open().gpio_nets())

    def net(self, name):
        for net in self.nets():
            if net['name'] == name:
                return net
        raise KeyError(f'no net {name!r}; the world has {[n["name"] for n in self.nets()]}')
