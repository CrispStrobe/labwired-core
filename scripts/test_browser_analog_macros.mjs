#!/usr/bin/env node
// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT
//
// Browser gate for the analog engine's op-amp, comparator, zener/TVS and
// dependent-source support, and for the regulators and the supply-aware MCU
// (`crates/wasm/tests/fixtures/analog-regulator/`: an F401 powered through an
// AMS1117-3.3 that sags into brown-out and power-down, with a buck and an
// LM317 on the same input — the supervisor's held/running pattern, its reset
// counts and the firmware's own boot log are compared too).
//
// The native tests (`crates/core/tests/analog_*.rs`) prove the physics against
// ngspice. None of them runs the .wasm the playground loads. This script does:
// it boots the wasm-bindgen `--target nodejs` package of `crates/wasm` — the
// same module the browser instantiates — with
// `crates/wasm/tests/fixtures/analog-macros/system.yaml`, steps it exactly as
// `analog_macros_run_in_the_browser_engine_as_committed` in
// `crates/wasm/src/cosim_tests.rs` does natively, reads the oscilloscope ring
// through `analog_trace_snapshot` (the call the scope panel makes), and
// requires every sample to equal `native.json` to the bit.
//
// Both halves compare against that one committed file, so the pair says: the
// browser computes these circuits exactly as the native engine does, and the
// native engine still computes what was committed. A missing package, a
// channel that is not there, or fewer samples than the native run is a FAIL,
// not a skip.
//
// Usage (after `wasm-pack build --target nodejs --dev --out-dir pkg` from
// `crates/wasm/`):
//
//   node scripts/test_browser_analog_macros.mjs

import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..');

const pkgPath = resolve(repoRoot, 'crates/wasm/pkg/labwired_wasm.js');
if (!existsSync(pkgPath)) {
  console.error(
    `[analog-macros] ${pkgPath} missing — run \`wasm-pack build --target nodejs --dev --out-dir pkg\` from crates/wasm/ first.`,
  );
  process.exit(1);
}
const { WasmSimulator } = await import(pkgPath);

const fixtures = resolve(repoRoot, 'crates/wasm/tests/fixtures/analog-macros');
const systemYaml = readFileSync(resolve(fixtures, 'system.yaml'), 'utf8');
const expected = JSON.parse(readFileSync(resolve(fixtures, 'native.json'), 'utf8'));
const chipYaml = readFileSync(resolve(repoRoot, 'configs/chips/stm32f401.yaml'), 'utf8');
const elfBytes = readFileSync(resolve(repoRoot, 'tests/fixtures/stm32f401-blinky.elf'));

// Must equal MACROS_BATCH_CYCLES / MACROS_BATCHES in cosim_tests.rs.
const BATCH_CYCLES = 4_200;
const BATCHES = 40;

const sim = WasmSimulator.new_from_config(systemYaml, chipYaml, elfBytes, null);
for (let i = 0; i < BATCHES; i++) {
  sim.step_batch(BATCH_CYCLES);
}
const batch = sim.analog_trace_snapshot(0);

const f32 = new Float32Array(1);
const u32 = new Uint32Array(f32.buffer);
const bits = (value) => {
  f32[0] = value;
  return u32[0].toString(16).padStart(8, '0');
};

const failures = [];
const check = (ok, message) => {
  if (!ok) failures.push(message);
};

const channels = batch.channels.map((channel) => channel.name);
check(
  JSON.stringify(channels) === JSON.stringify(expected.channels),
  `channels differ:\n  wasm:   ${JSON.stringify(channels)}\n  native: ${JSON.stringify(expected.channels)}`,
);
check(
  batch.samples.length === expected.rows.length,
  `wasm produced ${batch.samples.length} samples, native ${expected.rows.length}`,
);
// Anti-vacuity: the native test asserts the physics on these same rows; an
// empty or truncated run must not compare equal to nothing.
check(expected.rows.length > 390, `native.json holds only ${expected.rows.length} rows`);

let mismatches = 0;
const rows = Math.min(batch.samples.length, expected.rows.length);
for (let i = 0; i < rows; i++) {
  const sample = batch.samples[i];
  const want = expected.rows[i];
  const got = [Number(sample.time_ns), ...sample.values.map(bits)];
  if (JSON.stringify(got) !== JSON.stringify(want)) {
    mismatches += 1;
    if (mismatches <= 5) {
      failures.push(`row ${i}: wasm ${JSON.stringify(got)} vs native ${JSON.stringify(want)}`);
    }
  }
}
check(mismatches === 0, `${mismatches} of ${rows} rows differ from native.json`);

// ── Regulators and the supply-aware MCU ──
// Must equal REGULATOR_BATCH_CYCLES / REGULATOR_BATCHES and bor3_chip_yaml()
// in cosim_tests.rs.
const REG_BATCH_CYCLES = 21_000;
const REG_BATCHES = 40;
const regFixtures = resolve(repoRoot, 'crates/wasm/tests/fixtures/analog-regulator');
const regSystem = readFileSync(resolve(regFixtures, 'system.yaml'), 'utf8');
const regExpected = JSON.parse(readFileSync(resolve(regFixtures, 'native.json'), 'utf8'));
const bor3Chip = chipYaml.replace('supply_monitor:\n', 'supply_monitor:\n  bor_level: bor3\n');
check(bor3Chip !== chipYaml, 'stm32f401.yaml declares no supply_monitor to program BOR level 3 into');
const bootlog = readFileSync(resolve(repoRoot, 'tests/fixtures/stm32f401-supply-bootlog.elf'));
const reg = WasmSimulator.new_from_config(regSystem, bor3Chip, bootlog, null);
let held = '';
for (let i = 0; i < REG_BATCHES; i++) {
  reg.step_batch(REG_BATCH_CYCLES);
  held += reg.supply_status().held_in_reset ? 'H' : 'R';
}
const regBatch = reg.analog_trace_snapshot(0);
const supply = reg.supply_status();
const log = new DataView(Uint8Array.from(reg.read_memory(0x20000000, 0x30)).buffer);
const word = (i) => log.getUint32(4 * i, true);
const boots = word(0) === 0xb007c0de ? word(1) : 0;
const causes = [];
for (let i = 0; i < Math.min(boots, 8); i++) causes.push(word(4 + i).toString(16).padStart(8, '0'));

const regChannels = regBatch.channels.map((channel) => channel.name);
check(
  JSON.stringify(regChannels) === JSON.stringify(regExpected.channels),
  `regulator channels differ:\n  wasm:   ${JSON.stringify(regChannels)}\n  native: ${JSON.stringify(regExpected.channels)}`,
);
check(
  regBatch.samples.length === regExpected.rows.length,
  `regulator: wasm produced ${regBatch.samples.length} samples, native ${regExpected.rows.length}`,
);
check(regExpected.rows.length >= 1990, `analog-regulator/native.json holds only ${regExpected.rows.length} rows`);
let regMismatches = 0;
const regRows = Math.min(regBatch.samples.length, regExpected.rows.length);
for (let i = 0; i < regRows; i++) {
  const sample = regBatch.samples[i];
  const got = [Number(sample.time_ns), ...sample.values.map(bits)];
  if (JSON.stringify(got) !== JSON.stringify(regExpected.rows[i])) {
    regMismatches += 1;
    if (regMismatches <= 5) {
      failures.push(`regulator row ${i}: wasm ${JSON.stringify(got)} vs native ${JSON.stringify(regExpected.rows[i])}`);
    }
  }
}
check(regMismatches === 0, `${regMismatches} of ${regRows} regulator rows differ from native.json`);
check(held === regExpected.held, `held/running pattern: wasm ${held} vs native ${regExpected.held}`);
const gotSupply = {
  power_on_resets: supply.power_on_resets,
  brown_out_resets: supply.brown_out_resets,
  last_cause: supply.last_cause ?? null,
  held_in_reset: supply.held_in_reset,
};
const wantSupply = regExpected.supply;
for (const key of Object.keys(wantSupply)) {
  check(gotSupply[key] === wantSupply[key], `supply.${key}: wasm ${gotSupply[key]} vs native ${wantSupply[key]}`);
}
check(boots === regExpected.boots, `boots: wasm ${boots} vs native ${regExpected.boots}`);
check(
  JSON.stringify(causes) === JSON.stringify(regExpected.causes),
  `RCC_CSR at each boot: wasm ${JSON.stringify(causes)} vs native ${JSON.stringify(regExpected.causes)}`,
);
// Anti-vacuity: the native test asserts POR, BOR, POR; a run that never
// resets must not compare equal to a file that also never reset.
check(regExpected.boots === 3, `analog-regulator/native.json records ${regExpected.boots} boots, not 3`);
console.log(
  `[analog-regulator] ${regRows} samples × ${regChannels.length} channels compared bit-for-bit; ` +
    `supply ${held}; boots ${boots} (${causes.join(', ')})`,
);

console.log(
  `[analog-macros] ${rows} samples × ${channels.length} channels (${channels.join(', ')}) compared bit-for-bit`,
);
if (failures.length > 0) {
  console.error(`[analog-macros] FAIL (${failures.length}):`);
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}
console.log(
  '[analog-macros] PASS — the wasm build reproduces the native op-amp, comparator, zener, E/G, regulator and supply-reset runs exactly',
);
