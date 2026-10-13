"""Open a diagram: lower it, then build a Sim (one MCU) or a World (several)."""
import re
import shutil
import tempfile
from pathlib import Path

from ._lower import lower

_PACKAGED = Path(__file__).parent / 'configs'


def _elf_for(elf, node_id, many):
    if elf is None:
        raise TypeError('Sim(elf, diagram=...) needs the firmware: a path, or a mapping of part id to path')
    if isinstance(elf, dict):
        if node_id not in elf:
            raise ValueError(f'no firmware for MCU {node_id!r}; the mapping has {sorted(elf)}')
        return Path(elf[node_id])
    if many:
        raise TypeError(f'this diagram has several MCUs; pass elf={{part id: path}} (needs one entry per MCU)')
    return Path(elf)


def _packaged_chip(root, node):
    """A node the lowering left on a bare chip id runs on the wheel's own descriptor."""
    path = _PACKAGED / 'chips' / f"{node['board']}.yaml"
    if not path.is_file():
        raise ValueError(f"no chip descriptor for {node['board']!r} in this labwired build")
    system = root / node['system']
    system.write_text(re.sub(r'(?m)^chip:.*$', lambda _: f'chip: "{path}"', system.read_text(), count=1))


def open_diagram(cls, elf, diagram, chip_yaml, kw):
    from . import Sim
    from .world import World
    if kw.get('chip') is not None or kw.get('system') is not None:
        raise TypeError('diagram= replaces chip= and system=')
    extra = set(kw) - {'chip', 'system', 'uart', 'coverage'}
    if extra:
        raise TypeError(f'unexpected options for a diagram: {sorted(extra)}')
    tmp = tempfile.TemporaryDirectory(prefix='labwired-diagram-')
    root = Path(tmp.name)
    manifest = lower(diagram, root, chip_yaml=chip_yaml)
    nodes = manifest['nodes']
    if manifest['kind'] == 'single':
        node = nodes[0]
        if node['chip'] is None:
            _packaged_chip(root, node)
        sim = object.__new__(cls)
        Sim.__init__(sim, _elf_for(elf, node['id'], False), system=root / node['system'],
                     uart=kw.get('uart'), coverage=kw.get('coverage', False))
        sim._pads = node['pads']
        sim._tmp = tmp  # the lowered files live as long as the Sim
        sim.lowered = manifest
        return sim
    if kw.get('uart') is not None or kw.get('coverage'):
        raise TypeError('uart= and coverage= apply to one machine; a world captures every MCU console')
    if not isinstance(elf, dict):
        raise TypeError('this diagram has several MCUs; pass elf={part id: path} with one entry per MCU')
    extra_ids = set(elf) - {n['id'] for n in nodes}
    if extra_ids:
        raise ValueError(f'firmware given for {sorted(extra_ids)}, which are not MCUs of this diagram ({[n["id"] for n in nodes]})')
    for node in nodes:
        if node['chip'] is None:
            _packaged_chip(root, node)
        shutil.copyfile(_elf_for(elf, node['id'], True), root / node['id'] / 'firmware.bin')
    return World(root / manifest['world'], nodes, tmp, manifest)
