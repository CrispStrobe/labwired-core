"""Run `labwired-lower` (from @labwired/board-config) on a diagram.

The compiler that turns a drawing into engine config lives in TypeScript and is
the same one the playground and the hosted API run. There is no second copy
here and no hosted fallback: a design is never sent over the network to be
lowered. Without Node the call fails with a message that says how to install it.
"""
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from ._board_config import BOARD_CONFIG_VERSION

__all__ = ['LowerUnavailable', 'DiagramError', 'lower', 'meter_readings']

PACKAGE = '@labwired/board-config'
_INSTALL = (
    'Install Node.js 20 or newer (https://nodejs.org), then either let labwired '
    f'fetch the tool on demand (it runs `npx -y --package={PACKAGE}@{BOARD_CONFIG_VERSION} labwired-lower`) '
    f'or install it once with `npm install -g {PACKAGE}@{BOARD_CONFIG_VERSION}`. '
    'Set LABWIRED_LOWER to the path of a labwired-lower you already have to skip both.'
)


class LowerUnavailable(RuntimeError):
    """The lowering tool cannot be run on this machine."""


class DiagramError(ValueError):
    """The diagram does not lower. The message names each problem."""


def _major_minor(version):
    match = re.match(r'(\d+)\.(\d+)', version.strip())
    return (int(match.group(1)), int(match.group(2))) if match else None


def _run(cmd, **kw):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, **kw)
    except FileNotFoundError as e:
        raise LowerUnavailable(f'cannot run {cmd[0]!r}: {e}. {_INSTALL}') from None


def _command():
    """The argv prefix that runs labwired-lower: LABWIRED_LOWER, PATH, then npx."""
    pinned = _major_minor(BOARD_CONFIG_VERSION)
    explicit = os.environ.get('LABWIRED_LOWER')
    found = explicit or shutil.which('labwired-lower')
    if found:
        cmd = [found]
        got = _run(cmd + ['--version'])
        version = got.stdout.strip()
        if got.returncode != 0 or _major_minor(version) is None:
            raise LowerUnavailable(
                f'{found} --version failed ({got.stderr.strip() or got.stdout.strip() or "no output"}). {_INSTALL}')
        if _major_minor(version) != pinned:
            raise LowerUnavailable(
                f'{found} is {PACKAGE} {version}, but this labwired wheel was tested with '
                f'{BOARD_CONFIG_VERSION} and needs the same major.minor. '
                f'Run `npm install -g {PACKAGE}@{BOARD_CONFIG_VERSION}`, or remove that '
                'labwired-lower from PATH and labwired will fetch the right one with npx.')
        return cmd
    npx = shutil.which('npx')
    if not npx:
        raise LowerUnavailable(f'Sim(diagram=...) needs Node.js, and neither `labwired-lower` nor `npx` is on PATH. {_INSTALL}')
    return [npx, '-y', f'--package={PACKAGE}@{BOARD_CONFIG_VERSION}', 'labwired-lower']


def lower(diagram, out_dir=None, *, name='diagram', chip_yaml=None):
    """Lower `diagram` (a path, or a dict) into `out_dir`; return the manifest dict.

    `chip_yaml` maps an MCU part id to a chip descriptor file to use instead of
    the built-in one. The manifest is `lowered.json`: kind (`single` or
    `world`), the nodes with their system and chip files and pads, and the
    `world.yaml` path when there is one.
    """
    cmd = _command()
    out = Path(out_dir) if out_dir is not None else Path(tempfile.mkdtemp(prefix='labwired-lower-'))
    out.mkdir(parents=True, exist_ok=True)
    cleanup = None
    if isinstance(diagram, dict):
        handle, path = tempfile.mkstemp(suffix='.json')
        with os.fdopen(handle, 'w') as f:
            json.dump(diagram, f)
        cleanup = path
        source = path
    else:
        source = str(diagram)
        if not Path(source).is_file():
            raise FileNotFoundError(f'diagram not found: {source}')
    try:
        args = cmd + [source, '--out', str(out), '--name', name]
        for part, file in (chip_yaml or {}).items():
            args += ['--chip-yaml', f'{part}={file}']
        done = _run(args)
    finally:
        if cleanup:
            os.unlink(cleanup)
    if done.returncode == 2:
        raise DiagramError(done.stderr.strip())
    if done.returncode != 0:
        raise RuntimeError(f'labwired-lower failed ({done.returncode}): {done.stderr.strip() or done.stdout.strip()}')
    manifest = json.loads((out / 'lowered.json').read_text())
    manifest['dir'] = str(out)
    return manifest


def meter_readings(csv_text):
    """Multimeter readings from an analog trace, computed by the playground's own function."""
    cmd = _command()
    handle, path = tempfile.mkstemp(suffix='.csv')
    try:
        with os.fdopen(handle, 'w') as f:
            f.write(csv_text)
        done = _run(cmd + ['--meters', path])
    finally:
        os.unlink(path)
    if done.returncode != 0:
        raise RuntimeError(f'labwired-lower --meters failed: {done.stderr.strip()}')
    return json.loads(done.stdout)
