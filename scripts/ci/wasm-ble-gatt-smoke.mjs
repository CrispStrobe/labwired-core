#!/usr/bin/env node
// BLE connection + GATT through the REAL wasm engine (the browser's build),
// not a native test of the same Rust.
//
// Loads a `wasm-pack --target nodejs` build of crates/wasm, hands it the C3
// mask ROM the way the page does (`WasmWorld.register_esp32c3_rom`), builds a
// `WasmWorld` from resolved inputs (`new_from_resolved`, the page's call) with
// the stock Arduino `BLE_notify` GATT server on an ESP32-C3 and a
// `ble_central` "phone", steps it with `step_batch`, and reads the result back
// through `ble_centrals()` and `ble_air_trace()` — every binding the
// playground's BLE panel uses.
//
//   wasm-pack build crates/wasm --release --target nodejs --out-dir <pkg>
//   scripts/ci/fetch-c3-ble-flash.sh fixtures/esp32c3-ble scripts/ci/c3-ble-gatt-notify-flash.sha256
//   node scripts/ci/wasm-ble-gatt-smoke.mjs <pkg>              # phone vs BLE_notify
//   node scripts/ci/wasm-ble-gatt-smoke.mjs <pkg> --two-node   # BLE_client vs BLE_notify
//
// Exit 0 only if the phone connected, discovered the sketch's characteristic,
// read it, wrote it, received notifications and disconnected.
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const pkg = resolve(process.argv[2] ?? 'target/wasm-node');
const require = createRequire(import.meta.url);
const { WasmWorld } = require(resolve(pkg, 'labwired_wasm.js'));

const read = (p) => new Uint8Array(readFileSync(resolve(root, p)));
const UUID = 'beb5483e-36e1-4688-b7f5-ea07361b26a8';

WasmWorld.register_esp32c3_rom(
  read('crates/core/roms/esp32c3/esp32c3_rom.bin'),
  read('crates/core/roms/esp32c3/esp32c3_drom.bin'),
);

const systemYaml = readFileSync(resolve(root, 'configs/systems/esp32c3-devkit.yaml'), 'utf8');
const chipYaml = readFileSync(resolve(root, 'configs/chips/esp32c3.yaml'), 'utf8');
const flash = (name) => Array.from(read(`fixtures/esp32c3-ble/${name}.bin`));

// `--two-node`: the stock `BLE_client` sketch on a second C3 is the central
// instead of the phone; pass/fail on what the client prints.
if (process.argv.includes('--two-node')) {
  const world = WasmWorld.new_from_resolved(
    `
schema_version: "1.0"
name: two-c3-gatt
nodes:
  - { id: client, system: esp32c3-devkit.yaml, firmware: client.bin }
  - { id: server, system: esp32c3-devkit.yaml, firmware: server.bin }
interconnects:
  - type: ble_air
    nodes: [client, server]
`,
    [
      { id: 'client', system_yaml: systemYaml, chip_yaml: chipYaml, firmware: flash('c3-ble-gatt-client-flash') },
      { id: 'server', system_yaml: systemYaml, chip_yaml: chipYaml, firmware: flash('c3-ble-gatt-notify-flash') },
    ],
  );
  const started = Date.now();
  let client = '';
  for (let batch = 0; batch < 3000; batch++) {
    world.step_batch(200_000);
    client += Buffer.from(world.drain_uart_output('client')).toString('latin1');
    world.drain_uart_output('server');
    if ((client.match(/Notify callback for characteristic/g) ?? []).length >= 5) break;
  }
  const lines = client.split('\n').filter((l) => /BLE|Connected|Found|value|Notify|Setting/.test(l));
  console.log(lines.slice(0, 16).join('\n'));
  console.log(`sim time ${(world.time_ns() / 1e6).toFixed(1)} ms in ${((Date.now() - started) / 1000).toFixed(1)} s wall`);
  const want = [
    ' - Connected to server',
    ' - Found our service',
    ' - Found our characteristic',
    'The characteristic value was: ',
    'Setting new characteristic value to "Time since boot: ',
    'Notify callback for characteristic ' + UUID,
  ];
  let ok = true;
  for (const w of want) {
    const pass = client.includes(w);
    console.log(`${pass ? 'PASS' : 'FAIL'}  client printed ${JSON.stringify(w)}`);
    ok &&= pass;
  }
  process.exit(ok ? 0 : 1);
}

const env = `
schema_version: "1.0"
name: phone-lab
nodes:
  - { id: server, system: esp32c3-devkit.yaml, firmware: server.bin }
interconnects:
  - type: ble_central
    nodes: [server]
    config:
      id: phone
      target_name: ESP32
      script:
        - connect
        - discover
        - read: ${UUID}
        - write: { uuid: ${UUID}, text: hello }
        - subscribe: ${UUID}
        - wait_notify: { count: 3 }
        - disconnect
`;
const world = WasmWorld.new_from_resolved(env, [
  {
    id: 'server',
    system_yaml: systemYaml,
    chip_yaml: chipYaml,
    firmware: flash('c3-ble-gatt-notify-flash'),
  },
]);

const started = Date.now();
let phone;
for (let batch = 0; batch < 2000; batch++) {
  world.step_batch(200_000);
  phone = world.ble_centrals()[0];
  if (phone?.report?.script_done) break;
}
const secs = ((Date.now() - started) / 1000).toFixed(1);
const r = phone.report;
for (const l of r.log) console.log(`[phone ${String(l.t_us).padStart(9)} us] ${l.kind.padEnd(7)} ${l.text}`);
const air = world.ble_air_trace();
console.log(`air trace: ${air.length} frames, latest: ${air[0]?.text}`);
console.log(`sim time ${(world.time_ns() / 1e6).toFixed(1)} ms in ${secs} s wall`);

const chr = r.characteristics.find((c) => c.uuid === UUID);
const checks = [
  ['script finished', r.script_done],
  ['disconnected at the end', r.state === 'disconnected'],
  ['characteristic discovered with a CCCD', chr && chr.cccd_handle != null],
  ['one read of the 4-byte counter', r.reads.length === 1 && r.reads[0].value.length === 4],
  ['write acknowledged', r.writes_acked.length === 1],
  ['>= 3 notifications', r.notification_count >= 3],
  ['air trace carries notifications', air.some((f) => f.text.startsWith('ATT Handle Value Notification'))],
];
let ok = true;
for (const [name, pass] of checks) {
  console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}`);
  ok &&= Boolean(pass);
}
process.exit(ok ? 0 : 1);
