// Hosted-only diagnostic derivative, never ordinary qualification.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {deriveMotionCensus} from './t16-census-harness.mjs';
assert.equal(process.env.GITHUB_ACTIONS, 'true', 'No local engine execution');
assert(!process.env.NODE_OPTIONS, 'Default Node flags required');
const board=resolve(process.argv[2]), wasm=resolve(process.argv[3]);
const enabled=process.argv[4]==='on';
const hash=data=>createHash('sha256').update(data).digest('hex');
const original=readFileSync(join(board,'test/labwired-microbit-motion.test.mjs'),'utf8');
const module=createRequire(import.meta.url)(join(wasm,'labwired_wasm.js'));
assert.equal(typeof module.WasmSimulator.prototype.begin_t16_discovery_census, enabled?'function':'undefined');
assert.equal(typeof module.WasmSimulator.prototype.end_t16_discovery_census, enabled?'function':'undefined');
const {derived}=deriveMotionCensus(original,enabled);
// The sole removed assertion is the timing floor in this diagnostic derivative.
// Guest checks, budget, pose changes, warmup and five intervals are preserved.
const path=join(board,'test/diagnostic-t16-census-generated.test.mjs');
writeFileSync(path,derived);
const report={schema:'labwired.t16-census-capture.v1',diagnosticOnly:true,enabled,
 originalHarnessSha256:hash(original),derivedHarnessSha256:hash(derived),
 wasmSha256:hash(readFileSync(join(wasm,'labwired_wasm_bg.wasm'))),
 glueSha256:hash(readFileSync(join(wasm,'labwired_wasm.js'))),node:process.version,
 flags:[],qualification:false,startedAt:new Date().toISOString()};
writeFileSync('census-capture.json',JSON.stringify(report,null,2)+'\n');
const result=spawnSync(process.execPath,[path],{cwd:board,encoding:'utf8',
 timeout:240000,maxBuffer:8*1024*1024,env:{...process.env,LABWIRED_WASM:wasm,
 LABWIRED_MOTION_REQUIRED:'1',LABWIRED_REQUIRE_MOTION_RTX:'1'}});
writeFileSync('census-stdout.txt',result.stdout||'');
writeFileSync('census-stderr.txt',result.stderr||'');
Object.assign(report,{exitCode:result.status,signal:result.signal,error:result.error?.message,
 completedAt:new Date().toISOString()});
writeFileSync('census-capture.json',JSON.stringify(report,null,2)+'\n');
assert.equal(result.status,0,result.stderr);
const rows=(result.stdout||'').split('\n').filter(l=>l.startsWith('T16_DISCOVERY_WINDOW '))
 .map(l=>JSON.parse(l.slice('T16_DISCOVERY_WINDOW '.length)));
assert.equal(rows.length,enabled?5:0);
const samples=(result.stdout||'').split('\n').filter(l=>l.startsWith('MICROBIT_WASM_SAMPLE '))
 .map(l=>JSON.parse(l.slice('MICROBIT_WASM_SAMPLE '.length)));
assert.equal(samples.length,5);
for(const [index,row] of rows.entries()){
 assert.equal(row.index,index);assert.equal(row.overflow,false);
 assert.equal(row.diagnostic_only,true);assert.equal(row.memo_slots,64);
 const count=Object.fromEntries(Object.entries(row.counts).map(([k,v])=>{
  assert.match(v,/^\d+$/);return [k,BigInt(v)];}));
 assert(count.calls>0n,'Instrumentation liveness');
 assert.equal(count.calls,count.positive_reuse+count.no_positive_span);
 assert.equal(count.no_positive_span,count.memo_hit+count.memo_empty+
  count.memo_same_generation_collision+count.memo_same_pc_stale_generation+
  count.memo_different_pc_stale_generation);
 assert.equal(count.no_positive_span,count.memo_hit+count.admission_refused+
  count.discovered+count.search_exhausted);
 assert.equal(count.positive_reuse+count.discovered,
  count.runtime_rejected+count.branch_exit+count.budget_exit);
}
Object.assign(report,{windows:rows,samples,
 guestSha256:(result.stdout||'').match(/MICROBIT_WASM_GUEST_SHA256=([a-f0-9]{64})/)?.[1]});
assert(report.guestSha256);
writeFileSync('census-capture.json',JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report));
