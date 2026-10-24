#!/usr/bin/env python3
"""Hosted-only frozen nRF52832 parent/candidate attribution; changes no gate."""
from pathlib import Path
import hashlib,importlib.util,json,os,re,shutil,subprocess,sys
PARENT='3270808e9f22be54be7b99adc2dc6c8d1982ce23';CANDIDATE='6e7be97375eb58f091cbf048c58d2a674fdf30af'
def need(ok,why):
 if not ok:raise ValueError(why)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def command(args,cwd,out,env=None):
 p=subprocess.run([str(a) for a in args],cwd=cwd,env=env,capture_output=True,timeout=1800)
 out.parent.mkdir(parents=True,exist_ok=True);out.with_suffix('.stdout').write_bytes(p.stdout);out.with_suffix('.stderr').write_bytes(p.stderr)
 need(p.returncode==0,'diagnostic command failed: '+str(args[0]));return p

def main(parent,candidate,out):
 need(os.environ.get('GITHUB_ACTIONS')=='true','hosted-only engine builds and attribution')
 need(not out.exists(),'fresh diagnostic originals');out.mkdir()
 pins={};inputs=['Cargo.lock','Cargo.toml','rust-toolchain.toml','scripts/perf/board_perf.py','configs/chips/nrf52832.yaml']
 inputs+=subprocess.check_output(['git','-C',str(parent),'ls-files','crates/firmware-perf-spin']).decode().splitlines()
 for path in inputs:need((parent/path).read_bytes()==(candidate/path).read_bytes(),'same original diagnostic input '+path)
 for name,folder in [('parent',parent),('candidate',candidate)]:
  pin=subprocess.check_output(['git','-C',str(folder),'rev-parse','HEAD']).decode().strip();need(pin.startswith(PARENT) if name=='parent' else pin==CANDIDATE,'frozen source identity')
  need(not subprocess.check_output(['git','-C',str(folder),'status','--porcelain','--untracked-files=no']),'clean source');pins[name]=pin
  command(['cargo','+1.95.0','build','--release','--locked','-p','labwired-cli','--features','event-scheduler'],folder,out/(name+'-build'))
  exe=folder/'target/release/labwired';shutil.copyfile(exe,out/(name+'-labwired'))
  command(['nm','-C','-S',exe],folder,out/(name+'-symbols'))
  p=command(['objdump','-d','-C',exe],folder,out/(name+'-disassembly'))
  import gzip
  with gzip.GzipFile(filename=str(out/(name+'-disassembly.stdout.gz')),mode='wb',mtime=0) as f:f.write(p.stdout)
  (out/(name+'-disassembly.stdout')).unlink()
 spec=importlib.util.spec_from_file_location('frozen_fixture_producer',parent/'scripts/perf/board_perf.py');helper=importlib.util.module_from_spec(spec);sys.modules[spec.name]=helper;spec.loader.exec_module(helper)
 fixture,skipped=helper.build_fixtures({'nrf'});need(not skipped,'one original fixture');firmware=out/'perf-spin-nrf';shutil.copyfile(fixture['nrf'],firmware);chip=out/'nrf52832.yaml';shutil.copyfile(parent/'configs/chips/nrf52832.yaml',chip)
 for args,name in [(['rustc','+1.95.0','-vV'],'rustc'),(['cargo','+1.95.0','--version'],'cargo'),(['valgrind','--version'],'valgrind'),(['objdump','--version'],'binutils')]:command(args,parent,out/name)
 env=dict(os.environ);env['LABWIRED_ARM_SINGLE_STEP']='1';measurements={}
 for name,folder in [('parent',parent),('candidate',candidate)]:
  refs=[]
  for steps in [200000,1200000]:
   prefix=out/(name+'-'+str(steps));cg=prefix.with_suffix('.callgrind')
   args=['valgrind','--tool=callgrind','--callgrind-out-file='+str(cg),'--cache-sim=no','--branch-sim=no',str(out/(name+'-labwired')),'run','--chip',str(chip),'--firmware',str(firmware),'--max-steps',str(steps)]
   p=command(args,folder,prefix,env);match=helper.IREFS_RE.search(p.stderr.decode());need(match is not None and not helper.BATCHED_RE.search(p.stderr.decode()),'actual unchanged step-loop measurement')
   refs.append(int(match.group(1).replace(',','')))
   command(['callgrind_annotate','--auto=no','--inclusive=no','--threshold=99',cg],folder,out/(name+'-'+str(steps)+'-self-cost'))
  slope=(refs[1]-refs[0])/1000000;measurements[name]=dict(irefs=refs,instructions_per_step=slope,baseline=1279.2,baseline_delta=slope/1279.2-1,binary_sha256=sha(out/(name+'-labwired')))
 report=dict(status='DIAGNOSTIC_ATTRIBUTION_ONLY_NOT_GATE_QUALIFICATION',sources=pins,inputs={p:sha(parent/p) for p in inputs},fixture_sha256=sha(firmware),chip_sha256=sha(chip),measurements=measurements,candidate_parent_delta=measurements['candidate']['instructions_per_step']/measurements['parent']['instructions_per_step']-1,baseline_changed=False,threshold_changed=False)
 (out/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
if __name__=='__main__':
 need(len(sys.argv)==4,'parent candidate fresh output');main(*(Path(s).resolve() for s in sys.argv[1:]))
