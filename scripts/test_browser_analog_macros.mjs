#!/usr/bin/env node
// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT
//
// Browser gate for the analog engine's op-amp, comparator, zener/TVS and
// dependent-source support.
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

console.log(
  `[analog-macros] ${rows} samples × ${channels.length} channels (${channels.join(', ')}) compared bit-for-bit`,
);
if (failures.length > 0) {
  console.error(`[analog-macros] FAIL (${failures.length}):`);
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}
console.log('[analog-macros] PASS — the wasm build reproduces the native op-amp, comparator, zener and E/G trace exactly');
