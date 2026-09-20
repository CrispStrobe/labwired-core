#!/usr/bin/env bash
# Build the ESP32-C6 Tier-1 fixture ELF and install it to the fixtures
# directory.
#
# Usage: scripts/tier1/build_esp32c6.sh [--no-copy]
#
# By default the built ELF is copied to tests/fixtures/tier1/esp32c6.elf so
# the committed blob always matches a fresh build (a committed blob that
# silently diverges from source is exactly the drift this script exists to
# prevent). Pass --no-copy (or TIER1_COPY=0) only for a build-only smoke
# check; that mode never touches the committed fixture.
#
# Unlike the ESP32-C3/ESP32-S3 tier-1 targets this one is a fast-boot bare
# ELF: the C6 chip descriptor declares no ROM image region and the fixture
# runs from the ELF entry, so no flash.bin / --rom-boot artifact is produced.
#
# Prerequisites:
#   rustup target add riscv32imc-unknown-none-elf
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE_DIR="${REPO_ROOT}/examples/tier1-fixture/esp32c6"
FIXTURE_ELF="${FIXTURE_DIR}/target/riscv32imc-unknown-none-elf/release/tier1-fixture-esp32c6"
DEST_ELF="${REPO_ROOT}/tests/fixtures/tier1/esp32c6.elf"

# Ensure the riscv32imc target is available.
if ! rustup target list --installed 2>/dev/null | grep -q riscv32imc-unknown-none-elf; then
    echo "Installing riscv32imc-unknown-none-elf target..."
    rustup target add riscv32imc-unknown-none-elf
fi

echo "Building tier1-fixture-esp32c6..."
(
    cd "${FIXTURE_DIR}"
    cargo build --release
)

echo "Built: ${FIXTURE_ELF}"

if [[ "${1:-}" == "--no-copy" ]] || [[ "${TIER1_COPY:-1}" == "0" ]]; then
    echo "NOT installed (--no-copy / TIER1_COPY=0): ${DEST_ELF} left untouched."
else
    mkdir -p "$(dirname "${DEST_ELF}")"
    cp "${FIXTURE_ELF}" "${DEST_ELF}"
    echo "Installed to: ${DEST_ELF}"
fi
