#!/usr/bin/env node
// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT
//
// End-to-end gate for the CAN bridge through the real wasm artifact (the
// wasm-bindgen `--target nodejs` package is the same .wasm the playground
// loads), on the committed STM32H563 FDCAN/UDS ECU firmware.
//
//   1. record a scripted UDS session in the browser build;
//   2. replay it (no tester) twice: verdict "match", identical reports;
//      a tampered recording gives "mismatch" (negative control);
//   3. pause modes: DROP records the frame and the ECU never answers,
//      CAPTURE delivers after resume and the ECU answers, REPLAY ignores
//      live frames;
//   4. node reset during a multi-frame ISO-TP transfer: fault, reboot,
//      tester timeout, retry, recovery on one timeline;
//   5. snapshot/restore replays bridge calls exactly.
//
// Usage (after `wasm-pack build --target nodejs --dev --out-dir pkg` in
// crates/wasm/):  node scripts/test_browser_can_bridge.mjs

import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import assert from 'node:assert/strict';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..');
const pkgPath = resolve(repoRoot, 'crates/wasm/pkg/labwired_wasm.js');
if (!existsSync(pkgPath)) {
  console.error(`[can-bridge] ${pkgPath} missing — run wasm-pack build --target nodejs --dev --out-dir pkg in crates/wasm first.`);
  process.exit(1);
}
const { WasmSimulator, can_recording_summary } = await import(pkgPath);

const ex = (f) => readFileSync(resolve(repoRoot, 'examples/h563-can-replay', f), 'utf8');
const chipYaml = readFileSync(resolve(repoRoot, 'configs/chips/stm32h563.yaml'), 'utf8');
const elf = new Uint8Array(readFileSync(resolve(repoRoot, 'examples/h563-uds-ecu/firmware/h563_uds_ecu.elf')));
// The ECU alone: the H563 UDS board without its tester.
const ecuOnly = `name: "h563-ecu"\nchip: "../../configs/chips/stm32h563.yaml"\nexternal_devices: []\nboard_io: []\n`;
const open = (systemYaml) => {
  const sim = WasmSimulator.new_from_config(systemYaml, chipYaml, elf, {});
  sim.set_peripheral_tick_interval(1);
  return sim;
};
const report = (sim, id = 'bus') => JSON.parse(sim.can_bridge_report(id));
const decoder = new TextDecoder();

function runUntil(sim, pred, budget = 400) {
  for (let i = 0; i < budget; i++) {
    sim.step_batch(10_000);
    if (pred()) return true;
  }
  return false;
}
const testerDone = (sim) => () => report(sim).timeline.some((e) => e.lane === 'tester' && (e.kind === 'done' || e.kind === 'failed'));

// 1. Record.
const rec = open(ex('system-record.yaml'));
assert.ok(runUntil(rec, testerDone(rec)), 'the scripted session finishes');
const recReport = report(rec);
assert.ok(recReport.timeline.some((e) => e.kind === 'done'), 'the tester passed every step');
assert.equal(recReport.dropped_total, 0);
const recording = rec.can_bridge_recording('bus', 'jsonl');
const summary = JSON.parse(can_recording_summary(recording, 0));
assert.ok(summary.rx > 10 && summary.tx > 10, `recorded both directions: ${JSON.stringify(summary)}`);
assert.match(rec.can_bridge_recording('bus', 'candump'), /fdcan1-tx 7E8##1/);
console.log(`record: ${summary.frames} frames (${summary.rx} rx, ${summary.tx} tx)`);

// 2. Replay twice, then the tampered control.
function replay(text) {
  const sim = open(ecuOnly);
  sim.can_bridge_attach('bus', 'fdcan1', JSON.stringify({ pause_mode: 'replay', recording: text }));
  // A live frame is ignored in replay mode.
  assert.equal(sim.can_bridge_offer('bus', JSON.stringify({ id: '0x7E0', data: '021003' }), 1.0), 'dropped');
  runUntil(sim, () => report(sim).replay.verdict !== 'incomplete');
  sim.step_batch(50_000);
  return report(sim);
}
const r1 = replay(recording);
const r2 = replay(recording);
assert.equal(r1.replay.verdict, 'match', JSON.stringify(r1.replay));
assert.equal(r1.replay.max_cycle_skew, 0);
assert.equal(r1.dropped[0].reason, 'replay_ignores_live');
assert.deepEqual(r1, r2, 'two replays are identical');
const tampered = recording.replace('62f1904c4142', '62f1904c4143');
assert.notEqual(tampered, recording);
const bad = replay(tampered);
assert.equal(bad.replay.verdict, 'mismatch', 'a tampered recording is caught');
console.log(`replay: ${r1.replay.matched_tx}/${r1.replay.expected_tx} tx frames match, twice; tampered -> mismatch at tx ${bad.replay.first_mismatch.index}`);

// 3. Pause modes on a live bridge.
const vin = JSON.stringify({ id: '0x7E0', data: '0322f190' });
const answered = (sim) => report(sim, 'live').timeline.some((e) => e.dir === 'tx' && e.kind === 'frame' && e.data.includes('62f190'));
function bootLive(mode, capacity = 4) {
  const sim = open(ecuOnly);
  sim.can_bridge_attach('live', 'fdcan1', JSON.stringify({ pause_mode: mode, capture_capacity: capacity }));
  assert.ok(runUntil(sim, () => decoder.decode(sim.drain_uart_output()).includes('ECU_READY') || report(sim, 'live').timeline.some((e) => e.detail === 'ECU_READY')));
  sim.step_batch(20_000);
  return sim;
}
const drop = bootLive('drop');
drop.can_bridge_set_paused(true);
assert.equal(drop.can_bridge_offer('live', vin, 1234.5), 'dropped');
drop.can_bridge_set_paused(false);
runUntil(drop, () => answered(drop), 20);
assert.equal(answered(drop), false, 'DROP: the ECU never sees the frame');
const d = report(drop, 'live').dropped[0];
assert.deepEqual([d.reason, d.id, d.dlc, d.data, d.host_time_ms], ['paused_drop', 0x7e0, 4, '0322f190', 1234.5]);
const cap = bootLive('capture', 1);
cap.can_bridge_set_paused(true);
assert.equal(cap.can_bridge_offer('live', vin, 1), 'captured');
assert.equal(cap.can_bridge_offer('live', JSON.stringify({ id: '0x7E0', data: '023e00' }), 2), 'dropped', 'capacity 1 overflows');
cap.can_bridge_set_paused(false);
assert.ok(runUntil(cap, () => answered(cap), 20), 'CAPTURE: delivered after resume, the ECU answers');
const cr = report(cap, 'live');
assert.equal(cr.capture.overflowed_total, 1);
assert.equal(cr.dropped[0].reason, 'capture_overflow');
assert.match(cr.pause_note, /does not preserve real-time interaction/);
console.log('pause: drop recorded exactly and never delivered; capture delivered after resume; overflow recorded');

// 4. Node reset during the 603-byte ISO-TP transfer.
const fault = open(ex('system-reset-fault.yaml'));
assert.ok(runUntil(fault, testerDone(fault)));
const fr = report(fault);
const kinds = fr.timeline.map((e) => `${e.lane}:${e.kind}`);
const at = (k) => kinds.indexOf(k);
assert.ok(at('fault:node_reset') >= 0 && at('fault:node_reset') < at('tester:timeout'), kinds.join(' '));
assert.ok(at('tester:timeout') < at('tester:retry') && at('tester:retry') < at('tester:done'), kinds.join(' '));
assert.ok(fr.timeline.slice(at('fault:node_reset')).some((e) => e.lane === 'console' && e.detail === 'ECU_READY'), 'the ECU rebooted');
console.log(`fault: node_reset @${fr.faults[0].applied_cycle} -> reboot -> tester timeout -> retry -> done`);

// 5. Snapshot/restore replays bridge calls: save after a pause/offer/resume,
// run on, restore, run the same again -> the same report.
const snap = bootLive('capture');
assert.equal(snap.snapshot_unavailable_reason(), undefined);
snap.can_bridge_set_paused(true);
snap.can_bridge_offer('live', vin, 5);
snap.can_bridge_set_paused(false);
const mid = JSON.parse(snap.snapshot_save('mid')).id;
snap.step_batch(50_000);
const straight = report(snap, 'live');
assert.ok(answered(snap));
snap.snapshot_restore(mid);
snap.step_batch(50_000);
assert.deepEqual(report(snap, 'live'), straight, 'restore replays the bridge calls exactly');
console.log('snapshot: restore reproduces the bridge state');
console.log('OK');
