// Pure text transformation. Never loads or runs an engine.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
export const ORIGINAL_HARNESS_SHA256='af73787b849eddf697ed988b3a7c0a2fe25a938a561131498f1b7de38db7e3c5';
export function deriveMotionCensus(original, enabled) {
    assert.equal(typeof enabled,'boolean');
    const hash=text=>createHash('sha256').update(text).digest('hex');
    assert.equal(hash(original),ORIGINAL_HARNESS_SHA256,'Frozen harness bytes changed');
    const replace=(text,from,to)=>{
        assert.equal(text.split(from).length,2,'Unrecognized frozen timing-window context');
        return text.replace(from,to);
    };
    let derived=replace(original,'const start = performance.now();',
        `const startPc = adapter.sim.get_pc();\n                ${enabled?'adapter.sim.begin_t16_discovery_census();':''}\n                const start = performance.now();`);
    const context='const wallSeconds = (performance.now() - start) / 1000;\n                const sample = receipt(adapter, pose, index);';
    derived=replace(derived,context,context+
        `\n                ${enabled?"console.log('T16_DISCOVERY_WINDOW ' + JSON.stringify({index, cycles: 64_000_000, startPc, ...JSON.parse(adapter.sim.end_t16_discovery_census())}));":''}`);
    derived=replace(derived,'Number.isFinite(rtx) && rtx >= 1','Number.isFinite(rtx) && rtx > 0');
    derived=replace(derived,'all five 64M-cycle windows meet the unchanged 1x floor',
        'five diagnostic 64M-cycle windows retain guest checks; NOT 1x qualification');
    return {derived,originalHarnessSha256:hash(original),derivedHarnessSha256:hash(derived)};
}
