// Build the unchanged original source once, exclusively on GitHub.
import assert from 'node:assert/strict';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
assert.equal(process.env.GITHUB_ACTIONS,'true');
const board=resolve(process.argv[2]), out=resolve('shared-guest');
const fixture=join(board,'test/fixtures/labwired/microbit-motion');
const hash=data=>createHash('sha256').update(data).digest('hex');
mkdirSync(out);
const args=['-mcpu=cortex-m4','-mthumb','-nostdlib','-DMICROBIT_MOTION_IO',
    '-Wl,-T,board-io.ld','board-io.S','-o',join(out,'motion.elf')];
execFileSync('arm-none-eabi-gcc',args,{cwd:fixture,stdio:'inherit'});
const bytes=readFileSync(join(out,'motion.elf'));
assert(bytes.length<32768,'Bound guest receipt size');
const manifest={schema:'labwired.t16-census-shared-guest.v1',
    boardRef:execFileSync('git',['rev-parse','HEAD'],{cwd:board,encoding:'utf8'}).trim(),
    compiler:execFileSync('arm-none-eabi-gcc',['--version'],{encoding:'utf8'}),args,
    sourceSha256:hash(readFileSync(join(fixture,'board-io.S'))),
    linkerSha256:hash(readFileSync(join(fixture,'board-io.ld'))),
    elfSha256:hash(bytes),elfBytes:bytes.length,elfBase64:bytes.toString('base64'),
    diagnosticOnly:true,qualification:false};
assert.equal(manifest.boardRef,'d1f2c8ebfc5e4e76d5fc81f930588c13de13e974');
writeFileSync(join(out,'guest-manifest.json'),JSON.stringify(manifest,null,2)+'\n');
console.log(JSON.stringify(manifest));
