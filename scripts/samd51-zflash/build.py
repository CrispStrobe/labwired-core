#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Hosted-only actual include-adapted ZFlash link; never guest execution."""
import hashlib,json,os,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
SAMDPIN='62a3a408ac544a4e34bf0beb3932683a2c114e26'
COREPIN='2ed19d64ec3359143409a4b0a9758b4aeef3ee7d'
def need(ok,reason):
 if not ok:raise ValueError(reason)
def sha(b):return hashlib.sha256(b).hexdigest()
def git(path,*args):return subprocess.check_output(['git','-C',str(path),*args])
def run(samd,core,framework,out):
 need(os.getenv('GITHUB_ACTIONS')=='true','hosted-only ZFlash build')
 for p in [samd,core,framework,out]:need(p.is_relative_to(ROOT) and p!=ROOT,'owned checkout paths')
 need(not out.exists(),'new output directory')
 need(git(samd,'rev-parse','HEAD').decode().strip()==SAMDPIN,'exact codal source')
 need(git(core,'rev-parse','HEAD').decode().strip()==COREPIN,'exact core source')
 need(not git(ROOT,'status','--porcelain','--untracked-files=no'),'clean engine inputs')
 # Reuse the qualified include-only compiler producer. It retains original
 # driver/header/dependency bytes and notices, and rejects unknown includes.
 core_in_samd=samd/'owned-zflash-core'
 framework_in_samd=samd/'owned-zflash-framework'
 need(core==core_in_samd and framework==framework_in_samd,'closed checkout layout')
 subprocess.run([sys.executable,'-B',str(samd/'pxt-storage-compile/compile_flash.py'),str(core),str(framework),str(samd/'zflash-compile')],check=True)
 old=samd/'zflash-compile';out.mkdir()
 (out/'LICENSE-engine').write_bytes((ROOT/'LICENSE').read_bytes())
 report=json.loads((old/'compile-report.json').read_bytes())
 own=ROOT/'scripts/samd51-zflash';commands=[]
 def command(argv):
  argv=[str(p) for p in argv];res=subprocess.run(argv,cwd=ROOT,capture_output=True,timeout=120)
  commands.append(dict(argv=argv,returncode=res.returncode,stdout=res.stdout.decode(errors='replace'),stderr=res.stderr.decode(errors='replace')))
  (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
  need(res.returncode==0,'actual build command failed: '+json.dumps(commands[-1]))
 flags=['-mcpu=cortex-m4','-mthumb','-ffreestanding','-fno-builtin','-ffunction-sections','-fdata-sections','-nostdinc','-nostdinc++','-DBW_SIMULATION_CODAL_SERVICES=1','-DBW_PXT_FLASH_COMPILE=1','-DSAMD51=1']
 for p in [samd/'pxt-storage-compile',old,samd/'runtime-services/reviewed-toolchain',*[core/'inc'/part for part in ['core','types','driver-models','drivers']]]:flags+=['-I',p.relative_to(ROOT).as_posix()]
 rel=lambda p:p.relative_to(ROOT).as_posix()
 command(['arm-none-eabi-g++',*flags,'-std=c++11','-O2','-fno-exceptions','-fno-rtti','-fno-unwind-tables','-fno-threadsafe-statics','-fno-tree-loop-distribute-patterns','-MD','-MF',rel(out/'probe.d'),'-c',rel(own/'probe.cpp'),'-o',rel(out/'probe.o')])
 command(['arm-none-eabi-gcc','-mcpu=cortex-m4','-mthumb','-c',rel(own/'startup.S'),'-o',rel(out/'startup.o')])
 command(['arm-none-eabi-g++','-mcpu=cortex-m4','-mthumb','-nostdlib','-Wl,--gc-sections','-Wl,--no-undefined','-Wl,-Map='+rel(out/'control.map'),'-T',rel(own/'control.ld'),rel(out/'startup.o'),rel(out/'probe.o'),rel(old/'SAMDFlash-storage.o'),'-o',rel(out/'control.elf')])
 # Every prerequisite used by the authored caller must already be selected
 # by the real driver producer, except the one explicitly authored caller.
 sys.path.insert(0,str(samd/'consumer-build'));import build as consumer
 selected={row['sha256'] for row in report['source_inputs']}
 deps=[]
 for name in consumer.dependencies((out/'probe.d').read_text()):
  path=(ROOT/name).resolve();raw=path.read_bytes();digest=sha(raw)
  need(digest in selected or path==own/'probe.cpp','unselected caller dependency')
  deps.append(dict(path=path.relative_to(ROOT).as_posix(),sha256=digest))
 provenance=dict(status='ACTUAL_ZFLASH_LINK_ONLY',engine_tested=git(ROOT,'rev-parse','HEAD').decode().strip(),samd_source=SAMDPIN,core_source=COREPIN,framework_source='31abf23d118f35010fb75122e60a1eb2b6dffe9f',elf_sha256=sha((out/'control.elf').read_bytes()),driver_compile_report=report,probe_dependencies=deps,own_sources=[dict(path=rel(p),sha256=sha(p.read_bytes())) for p in [own/'probe.cpp',own/'startup.S',own/'control.ld',own/'build.py']],guest_executed=False,full_board=False,physical_accuracy=False,licence_clearance=False)
 (out/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
 print('ACTUAL_ZFLASH_LINK_ONLY '+provenance['elf_sha256'])
if __name__=='__main__':
 need(len(sys.argv)==5,'samd core framework out required');run(*(Path(a).resolve() for a in sys.argv[1:]))
