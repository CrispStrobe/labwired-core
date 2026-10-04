import assert from 'node:assert/strict';
import {readFileSync, readdirSync, statSync, writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
assert.equal(process.env.GITHUB_ACTIONS,'true');
const root=process.argv[2];
const find=(dir,name)=>{
 const found=[];
 for(const entry of readdirSync(dir)){
  const path=join(dir,entry);
  if(statSync(path).isDirectory())found.push(...find(path,name));
  else if(entry===name)found.push(path);
 }
 return found;
};
const one=(leg,name)=>{
 const paths=find(join(root,'t16-discovery-census-'+leg),name);
 assert.equal(paths.length,1,name);return paths[0];
};
const read=(leg,name)=>JSON.parse(readFileSync(one(leg,name)));
const capture=Object.fromEntries(['off','a','b'].map(leg=>[leg,read(leg,'census-capture.json')]));
const guestSamples=report=>report.samples.map(({wallSeconds,rtx,...sample})=>sample);
for(const leg of ['off','a','b']){
 assert.equal(capture[leg].exitCode,0);
 assert.equal(capture[leg].diagnosticOnly,true);
 assert.equal(capture[leg].qualification,false);
 assert.equal(capture[leg].guestSha256,capture.off.guestSha256);
 assert.deepEqual(capture[leg].sharedGuestManifest,capture.off.sharedGuestManifest);
 assert.equal(capture[leg].guestSha256,capture[leg].sharedGuestManifest.elfSha256);
 const elf=Buffer.from(capture[leg].sharedGuestManifest.elfBase64,'base64');
 assert.equal(elf.length,capture[leg].sharedGuestManifest.elfBytes);
 assert.equal(createHash('sha256').update(elf).digest('hex'),capture[leg].guestSha256);
 assert.equal(capture[leg].originalHarnessSha256,capture.off.originalHarnessSha256);
 assert.deepEqual(guestSamples(capture[leg]),guestSamples(capture.off));
 const integration=readFileSync(one(leg,'integration.txt'),'utf8');
 assert.match(integration,/^# tests 108$/m);
 assert.match(integration,/^# fail 0$/m);
 assert.match(integration,/^# skipped 0$/m);
 assert.equal(read(leg,'BUILD-INFO.json').features.length,leg==='off'?0:1);
}
assert.deepEqual(capture.a.windows,capture.b.windows);
const hash=path=>createHash('sha256').update(readFileSync(path)).digest('hex');
for(const name of ['labwired_wasm_bg.wasm','labwired_wasm.js']){
 // Both nodejs and web output copies may exist; bind each relative target.
 const pa=find(join(root,'t16-discovery-census-a'),name).sort();
 const pb=find(join(root,'t16-discovery-census-b'),name).sort();
 assert.equal(pa.length,2);assert.equal(pb.length,2);
 for(let index=0;index<pa.length;index++)assert.equal(hash(pa[index]),hash(pb[index]));
}
const result={diagnosticOnly:true,qualification:false,
 actualGuestObservationsEquivalent:true,independentDiagnosticBuildBytesIdentical:true,
 independentCounterWindowsIdentical:true,integrationsPerLeg:108,
 windows:capture.a.windows,
 limitations:['Counts do not measure removable cost or qualify RTx',
 'Different-PC stale-generation lookup misses do not prove collision-caused work',
 'Motion workload only; RAM/GPIO census not yet captured']};
writeFileSync('census-equivalence.json',JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify(result));
