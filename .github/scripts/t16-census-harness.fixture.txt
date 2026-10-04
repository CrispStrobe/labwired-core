/** Selected polled LSM303AGR guest on actual WASM; never a stub sensor proof. */
import {describe, it} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, mkdtempSync} from 'node:fs';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
import {performance} from 'node:perf_hooks';
import {buildLabwiredSystem} from '../src/labwired-bridge.js';
import {createLabwiredAdapter, plain} from '../src/labwired-adapter.js';

const fixture = fileURLToPath(new URL('./fixtures/labwired/microbit-motion/', import.meta.url));
const nodeDir = process.env.LABWIRED_WASM;
let hasGcc = false;
try { execFileSync('arm-none-eabi-gcc', ['--version'], {stdio: 'pipe'}); hasGcc = true; } catch {}
const skip = !nodeDir ? 'selected motion proof needs a newly built LABWIRED_WASM artifact'
    : !hasGcc ? 'selected motion proof needs arm-none-eabi-gcc' : false;
if (skip && process.env.LABWIRED_MOTION_REQUIRED === '1') throw Error(skip);
const requireRtx = process.env.LABWIRED_REQUIRE_MOTION_RTX === '1';
const wasm = skip ? null : createRequire(import.meta.url)(join(nodeDir, 'labwired_wasm.js'));
const firmware = skip ? null : compileGuest();

function compileGuest () {
    const output = join(mkdtempSync(join(tmpdir(), 'bw-motion-guest-')), 'motion.elf');
    execFileSync('arm-none-eabi-gcc', ['-mcpu=cortex-m4', '-mthumb', '-nostdlib',
        '-DMICROBIT_MOTION_IO', '-Wl,-T,board-io.ld', 'board-io.S', '-o', output],
    {cwd: fixture, stdio: 'pipe'});
    return new Uint8Array(readFileSync(output));
}

function makeAdapter () {
    const built = buildLabwiredSystem({chipKind: 'microbit_v2', boardVariant: 'lsm303agr',
        netlist: {parts: [{id: 'mb', kind: 'microbit'}], nets: []}});
    assert.equal(built.ok, true);
    const adapter = createLabwiredAdapter({wasm, chipYaml: built.chipYaml, systemYaml: built.systemYaml,
        firmware, firmwareOnly: true, clockHz: 64_000_000});
    const identities = adapter.discoverInputs().map(row => `${row.peripheral}.${row.key}`);
    for (const id of ['accelerometer', 'magnetometer']) {
        for (const axis of ['x', 'y', 'z']) assert.ok(identities.includes(`${id}.${axis}`));
    }
    adapter.sim.set_board_io_input('p11', true); // button B released (bridge physical high)
    return adapter;
}

const poses = [
    {accel: [1, -.5, .25], mag: [30, -15, 7.5]},
    {accel: [-.25, .125, -.75], mag: [-30, 0, 60]}
];
function setPose (adapter, index) {
    const pose = poses[index % 2];
    adapter.setInputs(['x', 'y', 'z'].flatMap((channel, axis) => [
        {component: 'accelerometer', channel, value: pose.accel[axis]},
        {component: 'magnetometer', channel, value: pose.mag[axis]}
    ]));
    adapter.sim.set_board_io_input('p5', index % 2 === 0); // A alternates released/pressed
    return pose;
}
function runCycles (adapter, cycles) {
    let remaining = cycles;
    while (remaining > 0) {
        const budget = Math.min(remaining, 64_000);
        const ran = adapter.sim.step_batch(budget);
        assert.ok(Number.isInteger(ran) && ran > 0 && ran <= budget, 'guest must make bounded progress');
        remaining -= ran;
    }
}
const halfAway = value => Math.sign(value) * Math.floor(Math.abs(value) + .5);
function receipt (adapter, pose, index) {
    const bytes = adapter.sim.read_memory(0x20000000, 0x138);
    const memory = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    assert.deepEqual([memory.getUint8(0x100), memory.getUint8(0x101)], [0x33, 0x40]);
    assert.equal(memory.getUint32(0x10c, true), 0, 'no guest transport error');
    assert.equal(memory.getUint32(0, true), index % 2, 'guest reads actual button level');
    assert.equal(memory.getUint32(0x124, true), 393217, 'actual one-byte TX / six-byte accel DMA');
    assert.equal(memory.getUint32(0x128, true), 393217, 'actual one-byte TX / six-byte mag DMA');
    const accel = [0, 1, 2].map(axis => memory.getInt16(0x110 + axis * 2, true));
    const mag = [0, 1, 2].map(axis => memory.getInt16(0x118 + axis * 2, true));
    assert.deepEqual(accel, pose.accel.map(value => Math.max(-512, Math.min(511, halfAway(value / .0039))) * 64));
    assert.deepEqual(mag, pose.mag.map(value => halfAway(value / .15)));
    const frame = plain(adapter.sim.get_display('led_matrix', true));
    assert.equal(frame.bytes.length, 25);
    for (let pixel = 0; pixel < 25; pixel++) {
        if (pixel % 6 === 0) assert.ok(frame.bytes[pixel] >= 155 && frame.bytes[pixel] <= 255);
        else assert.equal(frame.bytes[pixel], 0, `pixel ${pixel}: ${JSON.stringify(frame.bytes)}`);
    }
    return {accelRaw: accel, magRaw: mag, accelSamples: memory.getUint32(0x104, true),
        magSamples: memory.getUint32(0x108, true), scans: memory.getUint32(4, true)};
}

describe('selected micro:bit motion guest on actual WASM', {skip}, () => {
    it('reads both sensor identities, poses, actual DMA, matrix and buttons', () => {
        const adapter = makeAdapter();
        try {
            let previous = {accelSamples: 0, magSamples: 0, scans: 0};
            for (let index = 0; index < 2; index++) {
                const pose = setPose(adapter, index);
                runCycles(adapter, 2_000_000);
                const sample = receipt(adapter, pose, index);
                for (const key of ['accelSamples', 'magSamples', 'scans']) assert.ok(sample[key] > previous[key]);
                previous = sample;
            }
        } finally { adapter.sim.free(); }
    });

    it('all five 64M-cycle windows meet the unchanged 1x floor', {
        skip: !requireRtx ? 'enable LABWIRED_REQUIRE_MOTION_RTX for hosted qualification' : false
    }, () => {
        const adapter = makeAdapter();
        try {
            setPose(adapter, 0);
            runCycles(adapter, 8_000_000); // boot warm-up excluded from measured windows
            let previous = {accelSamples: 0, magSamples: 0, scans: 0};
            const measured = [];
            console.log('MICROBIT_WASM_GUEST_SHA256=' + createHash('sha256').update(firmware).digest('hex'));
            for (let index = 0; index < 5; index++) {
                const pose = setPose(adapter, index);
                const start = performance.now();
                runCycles(adapter, 64_000_000);
                const wallSeconds = (performance.now() - start) / 1000;
                const sample = receipt(adapter, pose, index);
                const rtx = 1 / wallSeconds;
                console.log('MICROBIT_WASM_SAMPLE ' + JSON.stringify({index, cycles: 64_000_000,
                    cpuHz: 64_000_000, wallSeconds, rtx, ...sample}));
                measured.push(rtx);
                for (const key of ['accelSamples', 'magSamples', 'scans']) assert.ok(sample[key] > previous[key]);
                previous = sample;
            }
            assert.ok(measured.every(rtx => Number.isFinite(rtx) && rtx >= 1),
                `all windows must meet 1x: ${JSON.stringify(measured)}`);
        } finally { adapter.sim.free(); }
    });
});
