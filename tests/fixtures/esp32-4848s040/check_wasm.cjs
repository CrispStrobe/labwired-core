// SPDX-License-Identifier: MIT
// Functional software qualification; no release or physical-board claim.
// The source-owned guest, not this host, initializes and paints the panel.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const repo = path.resolve(__dirname, '../../..');
assert.ok(process.argv[2], 'usage: node check_wasm.cjs /path/to/nodejs-engine');
const engine = path.resolve(process.argv[2]);
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const read = name => fs.readFileSync(path.join(repo, name));
const plain = value => value instanceof Map
  ? Object.fromEntries([...value].map(([key, value]) => [key, plain(value)]))
  : Array.isArray(value) ? value.map(plain) : value;
const wasm = require(path.join(engine, 'labwired_wasm.js'));
const firmware = read('tests/fixtures/esp32-4848s040/probe.elf');
assert.equal(sha256(firmware), 'fc5563c16da88dc609c5ada36b17c72a611cc4998675d502572318c1657c419f');
const system = read('configs/systems/esp32-4848s040.yaml');
const chip = read('configs/chips/esp32s3.yaml');
const blobs = {
  esp32s3_irom: new Uint8Array(read('crates/core/roms/esp32s3/esp32s3_rom.bin')),
  esp32s3_drom: new Uint8Array(read('crates/core/roms/esp32s3/esp32s3_drom.bin')),
};
const report = {
  qualification: 'functional software test only; no release qualification, hardware capture, physical timing, or arbitrary vendor firmware claim',
  engine: Object.fromEntries(['labwired_wasm.js', 'labwired_wasm_bg.wasm'].map(name => {
    const bytes = fs.readFileSync(path.join(engine, name));
    return [name, {bytes: bytes.length, sha256: sha256(bytes)}];
  })),
  fixture: {bytes: firmware.length, sha256: sha256(firmware)},
  systemSha256: sha256(system), chipSha256: sha256(chip),
  roms: Object.fromEntries(Object.entries(blobs).map(([name, bytes]) => [name, {bytes: bytes.length, sha256: sha256(bytes)}])),
  steps: [],
};
let sim;
let uart = '';
let reportedCycles = 0;
const start = performance.now();
const word = at => Buffer.from(sim.read_memory(at, 4)).readUInt32LE();
const display = () => {
  const d = plain(sim.get_display('display', true));
  assert.equal(d.id, 'display');
  assert.equal(d.kind, 'framebuffer');
  assert.equal(d.format, 'rgb565_be');
  assert.equal(d.width, 480);
  assert.equal(d.height, 480);
  d.meta = plain(d.meta);
  d.bytes = Buffer.from(d.bytes);
  assert.equal(d.bytes.length, 480 * 480 * 2);
  return d;
};
const until = count => {
  const limit = reportedCycles + 20_000_000;
  while (word(0x3fc91800) < count) {
    assert.ok(reportedCycles < limit, 'guest frame counter bound');
    assert.ok(performance.now() - start < 120_000, 'wall-time bound');
    const cycles = sim.step_batch(1000);
    assert.ok(cycles > 0, 'guest must make progress');
    reportedCycles += cycles;
    uart += Buffer.from(sim.drain_uart_output()).toString();
  }
  const d = display();
  report.steps.push({counter: word(0x3fc91800), reportedCycles, pc: `0x${sim.get_pc().toString(16)}`,
    frameSha256: sha256(d.bytes), pixel0: d.bytes.readUInt16BE(0), meta: d.meta});
  return d;
};
try {
  sim = wasm.WasmSimulator.new_from_config(system.toString(), chip.toString(), new Uint8Array(firmware), blobs);
  report.inputs = plain(sim.list_inputs());
  for (const key of ['x', 'y', 'pressed']) {
    assert.ok(report.inputs.some(i => i.peripheral === 'touch' && i.key === key));
  }
  const initial = until(1);
  for (const [i, color] of [[0,0xf800],[479,0x07e0],[479*480,0x001f],[480*480-1,0xffff]]) {
    assert.equal(initial.bytes.readUInt16BE(i*2), color, `corner ${i}`);
  }
  assert.equal(initial.meta.frames_received, 1);
  assert.equal(initial.meta.backlight_on, true);
  assert.equal(initial.meta.sleeping, false);
  sim.set_inputs([
    {component:'touch', channel:'x', value:123},
    {component:'touch', channel:'y', value:456},
    {component:'touch', channel:'pressed', value:1},
  ]);
  assert.equal(until(2).bytes.readUInt16BE(0), 123);
  sim.set_inputs([
    {component:'touch', channel:'x', value:321},
    {component:'touch', channel:'y', value:47},
  ]);
  assert.equal(until(3).bytes.readUInt16BE(0), 321);
  sim.set_inputs([{component:'touch', channel:'pressed', value:0}]);
  assert.equal(until(4).bytes.readUInt16BE(0), 0xf800);
  // Functional RGB completes before the first 115200-baud character shifts.
  // Let the actual UART FIFO drain; do not treat a FIFO write as sent output.
  const uartLimit = reportedCycles + 2_000_000;
  while (!(uart.includes('S040 READY') && uart.includes('S040 TOUCH')) && reportedCycles < uartLimit) {
    reportedCycles += sim.step_batch(10_000);
    uart += Buffer.from(sim.drain_uart_output()).toString();
  }
  assert.ok(uart.includes('S040 READY'));
  assert.ok(uart.includes('S040 TOUCH'));

  // The public WASM read/routing path must keep bank-1 pads distinct from
  // their lower-bank bit indices. Native tests separately cover drive/release.
  const lowBankBefore = word(0x60004004);
  const out = value => { const b=Buffer.alloc(4); b.writeUInt32LE(value); return new Uint8Array(b); };
  const refs = [40,43,44].map(pin => ({kind:'gpio', peripheral:'gpio', pin}));
  for (const ref of refs) {
    const mask = 2 ** (ref.pin - 32);
    sim.write_memory(0x60009004 + ref.pin * 4, out(0x1b00));
    sim.write_memory(0x60004030, out(mask));
    sim.write_memory(0x60004014, out(mask));
    assert.equal(plain(sim.sample_logic_signals([ref]))[0].value, true);
    assert.equal(plain(sim.pin_routing([ref]))[0].mode, 'output');
    sim.write_memory(0x60004018, out(mask));
    assert.equal(plain(sim.sample_logic_signals([ref]))[0].value, false);
  }
  assert.equal(word(0x60004004), lowBankBefore);
  for (const pin of [22,23,24,25,49,50,51,52,53]) {
    assert.ok(plain(sim.sample_logic_signals([{kind:'gpio',peripheral:'gpio',pin}]))[0].value == null, 'unbonded pad is unavailable');
  }
  assert.equal(typeof sim.set_gpio_external, 'function', 'real external-drive API required');
  const sample = pin => plain(sim.sample_logic_signals([{kind:'gpio',peripheral:'gpio',pin}]))[0].value;
  for (const pin of [1,2,40,43,44]) {
    const bank1 = pin >= 32, mask = 2 ** (pin % 32);
    sim.write_memory(0x60004000 + (bank1 ? 0x34 : 0x28), out(mask));
    sim.write_memory(0x60009004 + pin * 4, out(0x1b00));
    sim.set_gpio_external('gpio', pin, false);
    assert.equal(sample(pin), false);
    sim.set_gpio_external('gpio', pin, true);
    assert.equal(sample(pin), true);
    sim.set_gpio_external('gpio', pin, null);
    assert.equal(sample(pin), true, 'internal pull-up after actual release');
    sim.write_memory(0x60009004 + pin * 4, out(0x1200));
    assert.equal(sample(pin), false, 'release did not leave a held external high');
    sim.write_memory(0x60009004 + pin * 4, out(0x1b00));
  }
  sim.set_gpio_external('gpio', 8, false);
  sim.set_gpio_external('gpio', 40, true);
  assert.equal(sample(8), false, 'GPIO40 must not alias GPIO8');
  sim.set_gpio_external('gpio', 8, null);
  sim.set_gpio_external('gpio', 40, false);
  for (const pin of [-1,0.5,22,23,24,25,49,53,255,256,296,2**32+40,NaN,Infinity,'40',null,undefined]) {
    assert.throws(() => sim.set_gpio_external('gpio', pin, true), `bad pin ${String(pin)}`);
    assert.equal(sample(40), false, 'invalid pin must not mutate GPIO40');
  }
  for (const value of [0,1,'high',{},undefined,NaN]) {
    assert.throws(() => sim.set_gpio_external('gpio', 40, value), 'level must be exactly boolean/null');
    assert.equal(sample(40), false);
  }
  assert.throws(() => sim.set_gpio_external('missing', 40, true));
  assert.throws(() => sim.set_gpio_external('uart0', 0, true));
  sim.write_memory(0x60004074 + 40 * 4, out(1 << 2));
  sim.write_memory(0x60004014, out(1 << 8));
  sim.write_memory(0x60004030, out(1 << 8));
  assert.equal(sample(40), false, 'open-drain high follows an external low');
  sim.set_gpio_external('gpio', 40, null);
  assert.equal(sample(40), true, 'released open drain with pull-up');
  sim.write_memory(0x60004034, out(1 << 8));
  sim.set_gpio_external('gpio', 40, false);
  const held = JSON.parse(sim.snapshot_save('external-low'));
  sim.set_gpio_external('gpio', 40, null);
  const released = JSON.parse(sim.snapshot_save('external-released'));
  sim.set_gpio_external('gpio', 40, true);
  sim.snapshot_restore(held.id);
  assert.equal(sample(40), false, 'journal restores held input');
  sim.snapshot_restore(released.id);
  sim.write_memory(0x60009004 + 40 * 4, out(0x1200));
  assert.equal(sample(40), false, 'journal restores actual release, not held high');
  sim.write_memory(0x60009004 + 40 * 4, out(0x1b00));
  assert.equal(sample(40), true);
  report.h1 = {pins:[1,2,40,43,44], distinctBanks:true, absentPadsRefused:true,
    strictJsArguments:true, highLowReleased:true, openDrain:true, journalRestore:true};
  sim.free();
  sim = null;
  sim = wasm.WasmSimulator.new_from_config(system.toString(), chip.toString(), new Uint8Array(firmware), blobs);
  assert.equal(display().meta.frames_received, 0);
  assert.equal(display().meta.backlight_on, false);
  assert.equal(until(1).bytes.readUInt16BE(0), 0xf800);
  report.freshConstructorReset = true;
  report.passed = true;
} catch (error) {
  report.passed = false;
  report.error = error.stack || String(error);
  process.exitCode = 1;
} finally {
  report.uart = uart;
  report.elapsedWallMs = performance.now() - start;
  if (sim) sim.free();
  console.log(JSON.stringify(report, null, 2));
}
