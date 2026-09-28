#!/usr/bin/env bash
# H563 UDS evidence run: one command from a clean checkout.
#
#   scripts/uds_evidence.sh [OUT_DIR]        (default: out/uds-evidence)
#
# 1. Verifies the committed responder ELF against its pinned sha256
#    (examples/h563-uds-ecu/firmware/h563_uds_ecu.elf.sha256). Set
#    LABWIRED_UDS_FIRMWARE=<elf> to run a firmware you built yourself instead;
#    the report then names that file's own sha256.
# 2. Builds the labwired CLI (release).
# 3. Runs examples/h563-uds-ecu/uds-evidence.yaml: the STM32H563 ECU firmware
#    against the scripted UDS tester on FDCAN1.
# 4. Leaves result.json (machine-readable, with a `uds` block), junit.xml and
#    uds-report.md (the human report) in OUT_DIR.
#
# Exit status is the run's verdict: 0 pass, 1 assertion failure, 2 error.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-out/uds-evidence}"
EXAMPLE=examples/h563-uds-ecu
SCRIPT="$EXAMPLE/uds-evidence.yaml"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

if [ -n "${LABWIRED_UDS_FIRMWARE:-}" ]; then
  # A user-built ELF: run it in place of the committed one, via a copy of the
  # test script next to the example (its paths are relative to the script).
  fw="$(cd "$(dirname "$LABWIRED_UDS_FIRMWARE")" && pwd)/$(basename "$LABWIRED_UDS_FIRMWARE")"
  echo "firmware: $fw (user-supplied, sha256 $(sha256_of "$fw"))"
  SCRIPT="$EXAMPLE/.uds-evidence-custom.yaml"
  sed "s#\./firmware/h563_uds_ecu.elf#$fw#" "$EXAMPLE/uds-evidence.yaml" >"$SCRIPT"
  trap 'rm -f "$ROOT/$EXAMPLE/.uds-evidence-custom.yaml"' EXIT
else
  pinned="$(cut -d' ' -f1 "$EXAMPLE/firmware/h563_uds_ecu.elf.sha256")"
  actual="$(sha256_of "$EXAMPLE/firmware/h563_uds_ecu.elf")"
  if [ "$pinned" != "$actual" ]; then
    echo "error: $EXAMPLE/firmware/h563_uds_ecu.elf sha256 $actual does not match the pinned $pinned" >&2
    exit 2
  fi
  echo "firmware: $EXAMPLE/firmware/h563_uds_ecu.elf (sha256 $actual, pinned)"
fi

cargo build --release -q -p labwired-cli --bin labwired
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/labwired"

rm -rf "$OUT_DIR"
status=0
"$BIN" test --script "$SCRIPT" --output-dir "$OUT_DIR" --no-uart-stdout || status=$?

echo
echo "result:  $OUT_DIR/result.json"
echo "report:  $OUT_DIR/uds-report.md"
exit "$status"
