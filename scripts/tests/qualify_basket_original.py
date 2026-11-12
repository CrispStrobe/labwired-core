"""Explicit offline original-input check; not preparation or guest qualification."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]


def qualify(directory):
    pins = json.loads((ROOT / 'examples/luckfox-pico-mini-a-linux/basket/pins.json').read_text())
    # Preserve the old positive original-receipt assertion, now requiring an
    # explicitly supplied private input instead of a published original copy.
    for name, pin in {'module-manifest.json': pins['module_receipt'], **pins['modules']}.items():
        path = directory / name
        if path.is_symlink() or any(parent.is_symlink() for parent in path.parents) or not path.is_file():
            raise ValueError('missing or nonregular original input')
        with path.open('rb') as stream:
            data = stream.read(pin['bytes'] + 1)
        if len(data) != pin['bytes'] or hashlib.sha256(data).hexdigest() != pin['sha256']:
            raise ValueError('original input identity mismatch')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--module-artifacts', type=Path, required=True)
    args = parser.parse_args()
    try:
        qualify(args.module_artifacts)
    except (OSError, ValueError):
        print('Original module input identity check FAILED; no preparation or guest ran.', file=sys.stderr)
        return 1
    print('Original module input identities PASS; preparation and guest qualification remain separate.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
