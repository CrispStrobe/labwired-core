// Source-text tests only: no emulator imports or guest execution.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {deriveMotionCensus} from './t16-census-harness.mjs';
const original=readFileSync(new URL('./t16-census-harness.fixture.txt',import.meta.url),'utf8');
for(const enabled of [false,true])test('unique timing-window transformation, enabled='+enabled,()=>{
    const {derived}=deriveMotionCensus(original,enabled);
    const marker="    it('all five 64M-cycle windows";
    assert.equal(derived.slice(0,derived.indexOf("    it('five diagnostic")),
        original.slice(0,original.indexOf(marker)),'All earlier semantic tests untouched');
    assert.equal((derived.match(/const sample = receipt\(adapter, pose, index\);/g)||[]).length,2);
    assert.equal((derived.match(/begin_t16_discovery_census\(\)/g)||[]).length,enabled?1:0);
    assert.equal((derived.match(/end_t16_discovery_census\(\)/g)||[]).length,enabled?1:0);
    assert(derived.includes('runCycles(adapter, 8_000_000);'));
    assert(derived.includes('runCycles(adapter, 64_000_000);'));
    assert(derived.includes('for (let index = 0; index < 5; index++)'));
    assert(derived.includes('assert.ok(sample[key] > previous[key])'));
    assert(!derived.includes('Number.isFinite(rtx) && rtx >= 1'));
    assert(derived.includes('Number.isFinite(rtx) && rtx > 0'));
});
test('changed frozen input rejected before any execution or write',()=>{
    assert.throws(()=>deriveMotionCensus(original+'\n',true),/Frozen harness bytes changed/);
    assert.throws(()=>deriveMotionCensus(original.replace('64_000_000','63_000_000'),false),/Frozen harness bytes changed/);
    assert.throws(()=>deriveMotionCensus(original,'on'));
});
