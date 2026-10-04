// Hosted artifact reuse, with original failed capture gate retained as failure.
import assert from 'node:assert/strict';
import {readFileSync,readdirSync,statSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
assert.equal(process.env.GITHUB_ACTIONS,'true');
const leg=process.argv[2];assert(['off','a','b'].includes(leg));
const root=resolve('captures/t16-discovery-census-'+leg);
const source='1b044ea97d2a72c43f1b15a147d64a479620f0ee',run=37201401969;
const hash=data=>createHash('sha256').update(data).digest('hex');
const api=async suffix=>{
    const response=await fetch('https://api.github.com/repos/CrispStrobe/labwired-core/'+suffix,
        {headers:{Authorization:'Bearer '+process.env.GITHUB_TOKEN,Accept:'application/vnd.github+json'}});
    assert(response.ok,'GitHub evidence unavailable: '+response.status);return response.json();
};
const snapshot=await api('actions/runs/'+run);
assert.equal(snapshot.head_sha,source);assert.equal(snapshot.status,'completed');
assert.equal(snapshot.conclusion,'failure'); // Original workflow NOT reclassified.
const jobs=(await api('actions/runs/'+run+'/jobs?per_page=100')).jobs;
assert.equal(jobs.find(j=>j.name==='correctness')?.conclusion,'success');
const job=jobs.find(j=>j.name==='build ('+leg+')');assert(job);
assert.equal(job.conclusion,'failure');
for(const name of ['Build feature-off control or explicitly enabled diagnostic',
    'All 108 actual-WASM integrations including seven same-PC proofs'])
    assert.equal(job.steps.find(s=>s.name===name)?.conclusion,'success',name);
assert.deepEqual(job.steps.filter(s=>s.conclusion==='failure').map(s=>s.name),
    ['Five diagnostic windows, never ordinary RTx qualification']);
const find=(dir,name)=>readdirSync(dir).flatMap(entry=>{
    const path=join(dir,entry);return statSync(path).isDirectory()?find(path,name):entry===name?[path]:[];
});
const one=name=>{const paths=find(root,name);assert.equal(paths.length,1,name);return paths[0];};
assert.equal(find(root,'census-capture.json').length,0,'Original failure must precede capture');
assert.equal(find(root,'census-stdout.txt').length,0,'Do not repeat already measured windows');
const info=JSON.parse(readFileSync(one('BUILD-INFO.json')));
assert.equal(info.ref,source);assert.equal(info.parentTool,'d1f2c8ebfc5e4e76d5fc81f930588c13de13e974');
assert.equal(info.diagnosticOnly,true);
assert.deepEqual(info.features,leg==='off'?[]:['t16-discovery-census']);
const glue=find(root,'labwired_wasm.js').filter(p=>p.includes('/nodejs/'));
assert.equal(glue.length,1);
const wasm=glue[0].replace(/labwired_wasm\.js$/,'labwired_wasm_bg.wasm');
for(const [name,path] of [['labwired_wasm.js',glue[0]],['labwired_wasm_bg.wasm',wasm]]){
    const bytes=readFileSync(path);
    assert.equal(bytes.length,info.targets.nodejs[name].bytes);
    assert.equal(hash(bytes),info.targets.nodejs[name].sha256);
}
const integration=readFileSync(one('integration.txt'),'utf8');
assert.match(integration,/^# tests 108$/m);assert.match(integration,/^# fail 0$/m);assert.match(integration,/^# skipped 0$/m);
const artifact=(await api('actions/runs/'+run+'/artifacts?per_page=100')).artifacts
    .filter(a=>a.name==='t16-discovery-census-'+leg);
assert.equal(artifact.length,1);assert.equal(artifact[0].expired,false);
const evidence={schema:'labwired.t16-census-artifact-reuse.v1',run,source,leg,
    originalWorkflowConclusion:snapshot.conclusion,originalCaptureConclusion:job.conclusion,
    successfulBuildAnd108Integrations:true,priorWindowCount:0,qualification:false,
    artifact:artifact[0],job,buildInfo:info,integrationSha256:hash(integration),
    originalHarnessTool:'d1f2c8ebfc5e4e76d5fc81f930588c13de13e974'};
writeFileSync('artifact-reuse.json',JSON.stringify(evidence,null,2)+'\n');
writeFileSync('engine-directory.txt',resolve(glue[0],'..')+'\n');
console.log(JSON.stringify(evidence));
