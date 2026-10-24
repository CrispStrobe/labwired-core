#!/usr/bin/env python3
"""One hosted instruction-position diagnostic; never changes the performance gate."""
from pathlib import Path
import gzip,hashlib,importlib.util,json,os,re,shutil,subprocess,sys,time
PARENT='3270808e9f22be54be7b99adc2dc6c8d1982ce23';SOURCE='01e50371b9a7d341142399b84932b402fcac306b'
FIXTURE='b9015534eafb5207aef3bd7bd2e8ac332b33df5b4ab9f045c0faca4033bbadb6'
def need(ok,why):
 if not ok:raise ValueError(why)
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  while b:=f.read(1024*1024):h.update(b)
 return h.hexdigest()
def seal(path):
 need(path.stat().st_size<1024**3,'bounded retained original');original=dict(bytes=path.stat().st_size,sha256=digest(path));dest=path.with_suffix(path.suffix+'.gz')
 with path.open('rb') as src,gzip.GzipFile(filename=str(dest),mode='wb',mtime=0) as out:shutil.copyfileobj(src,out)
 h=hashlib.sha256();size=0
 with gzip.open(dest,'rb') as f:
  while b:=f.read(1024*1024):h.update(b);size+=len(b)
 need(size==original['bytes'] and h.hexdigest()==original['sha256'],'lossless original compression/readback')
 path.unlink();return dict(original=original,compressed=dict(bytes=dest.stat().st_size,sha256=digest(dest)),file=dest.name)
def execute(args,cwd,out,label,env=None):
 with (out/(label+'.stdout')).open('xb') as stdout,(out/(label+'.stderr')).open('xb') as stderr:
  p=subprocess.run([str(x) for x in args],cwd=cwd,env=env,stdout=stdout,stderr=stderr,timeout=1800)
 need(p.returncode==0,'diagnostic command failed: '+label)
 return None
def main(parent,candidate,out):
 need(os.environ.get('GITHUB_ACTIONS')=='true','heavy builds/guest execution are hosted only');need(not out.exists(),'fresh original output');out.mkdir()
 preflight=dict(free_bytes=shutil.disk_usage(out).free,mem_available_bytes=int(next(x.split()[1] for x in Path('/proc/meminfo').read_text().splitlines() if x.startswith('MemAvailable:')))*1024,load=list(os.getloadavg()));need(preflight['free_bytes']>5*1024**3 and preflight['mem_available_bytes']>2*1024**3,'hosted build resource admission');(out/'preflight.json').write_text(json.dumps(preflight,indent=2))
 for folder,pin in [(parent,PARENT),(candidate,SOURCE)]:
  need(subprocess.check_output(['git','-C',str(folder),'rev-parse','HEAD']).decode().strip()==pin,'exact selected source')
  need(not subprocess.check_output(['git','-C',str(folder),'status','--porcelain','--untracked-files=no']),'clean selected source')
 keys=['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_PROFILE_RELEASE_OPT_LEVEL','CARGO_PROFILE_RELEASE_LTO','CARGO_PROFILE_RELEASE_CODEGEN_UNITS'];need(all(not os.environ.get(k) for k in keys),'unmodified native release profile')
 execute(['cargo','+1.95.0','build','--release','--locked','-p','labwired-cli','--features','event-scheduler'],candidate,out,'native-build')
 binary=out/'candidate-labwired';shutil.copy2(candidate/'target/release/labwired',binary)
 spec=importlib.util.spec_from_file_location('frozen_fixture_builder',parent/'scripts/perf/board_perf.py');helper=importlib.util.module_from_spec(spec);sys.modules[spec.name]=helper;spec.loader.exec_module(helper)
 firmware,skipped=helper.build_fixtures({'nrf'});need(not skipped,'original nrf fixture');fixture=out/'perf-spin-nrf';shutil.copyfile(firmware['nrf'],fixture);need(digest(fixture)==FIXTURE,'same original fixture bytes, no regenerated expectation')
 chip=out/'nrf52832.yaml';shutil.copyfile(parent/'configs/chips/nrf52832.yaml',chip)
 for path in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','scripts/perf/board_perf.py','configs/chips/nrf52832.yaml']:
  need((parent/path).read_bytes()==(candidate/path).read_bytes(),'unchanged original measurement/build input '+path)
 for label,args in [('rustc',['rustc','+1.95.0','-vV']),('valgrind',['valgrind','--version']),('symbols',['nm','-S','--defined-only',binary]),('disassembly',['objdump','-d','-C',binary])]:execute(args,candidate,out,label)
 disassembly=seal(out/'disassembly.stdout');env=dict(os.environ);env['LABWIRED_ARM_SINGLE_STEP']='1';measurements=[]
 for steps in [200000,1200000]:
  label='steps-'+str(steps);raw=out/(label+'.callgrind');stdout=(out/(label+'.stdout')).open('xb');stderr=(out/(label+'.stderr')).open('xb')
  args=['valgrind','--tool=callgrind','--callgrind-out-file='+str(raw),'--cache-sim=no','--branch-sim=no','--dump-instr=yes','--compress-pos=no','--compress-strings=no',str(binary),'run','--chip',str(chip),'--firmware',str(fixture),'--max-steps',str(steps)]
  p=subprocess.Popen(args,cwd=candidate,env=env,stdout=stdout,stderr=stderr);mappings=[]
  for _ in range(1000):
   try:
    lines=Path('/proc',str(p.pid),'maps').read_text().splitlines()
    for line in lines:
     fields=line.split(None,5)
     if len(fields)==6 and fields[5]==str(binary):
      start,end=fields[0].split('-');row=dict(start=int(start,16),end=int(end,16),permissions=fields[1],offset=int(fields[2],16))
      if row not in mappings:mappings.append(row)
   except FileNotFoundError:pass
   if any('x' in row['permissions'] for row in mappings) or p.poll() is not None:break
   time.sleep(0.01)
  try:code=p.wait(timeout=1800)
  except subprocess.TimeoutExpired:
   p.kill();p.wait();stdout.close();stderr.close();raise
  stdout.close();stderr.close();need(code==0 and mappings,'actual command completed with observed client ELF mapping')
  text=(out/(label+'.stderr')).read_text();refs=helper.IREFS_RE.search(text);need(refs is not None and not helper.BATCHED_RE.search(text),'original single-step loop/count')
  original=seal(raw);measurements.append(dict(steps=steps,irefs=int(refs.group(1).replace(',','')),command=args,client_text_mappings=mappings,callgrind=original))
 report=dict(status='CURRENT_SOURCE_INSTRUCTION_DIAGNOSTIC_ONLY',source=SOURCE,fixture_source=PARENT,fixture_sha256=digest(fixture),chip_sha256=digest(chip),binary_sha256=digest(binary),disassembly=disassembly,measurements=measurements,baseline_changed=False,threshold_changed=False,gate_qualification=False,parent_reexecuted=False)
 (out/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(dict(status=report['status'],source=SOURCE,slope=(measurements[1]['irefs']-measurements[0]['irefs'])/1000000,parent_reexecuted=False)))
if __name__=='__main__':
 need(len(sys.argv)==4,'parent-fixture-source candidate-source fresh-output');main(*(Path(x).resolve() for x in sys.argv[1:]))
