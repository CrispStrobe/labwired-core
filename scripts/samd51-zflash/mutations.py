#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Two isolated model-source mutations; actual guest wrong-memory admission."""
from pathlib import Path
import hashlib,json,os,subprocess,sys
ROOT=Path(__file__).resolve().parents[2]
def main():
 if os.getenv('GITHUB_ACTIONS')!='true':raise ValueError('hosted-only mutant guest')
 profile=sys.argv[1];out=ROOT/sys.argv[2]
 if profile not in ['default','event-scheduler'] or out.exists():raise ValueError('profile/new output')
 out.mkdir();p=ROOT/'crates/core/src/bus/samd51_storage.rs';original=p.read_bytes()
 changes={'omit-erase-commit':(b'self.flash.data[address..address + 8192].fill(0xff);',b'let _ = address; // isolated actual model erase commit removed'),
          'omit-program-commit':(b'*old &= new;',b'let _ = (old, new); // isolated actual model program commit removed')}
 receipts=[]
 try:
  for name,(before,after) in changes.items():
   if original.count(before)!=1:raise ValueError('unique actual model mutation site')
   raw=original.replace(before,after,1);p.write_bytes(raw)
   (out/(name+'.diff')).write_bytes(subprocess.check_output(['git','diff','--','crates/core/src/bus/samd51_storage.rs'],cwd=ROOT))
   command=['cargo','test','--locked','-p','labwired-core','--test','samd51_zflash_guest']
   if profile!='default':command+=['--features',profile]
   command+=['--','--ignored','--exact','actual_zflash_driver_program_erase_and_refusals','--nocapture']
   result=subprocess.run(command,cwd=ROOT,env={**os.environ,'ZFLASH_MUTANT':name},capture_output=True,timeout=600)
   (out/(name+'.stdout')).write_bytes(result.stdout);(out/(name+'.stderr')).write_bytes(result.stderr)
   marker=b'ZFLASH_MUTANT_WRONG_ERASE ' if name=='omit-erase-commit' else b'ZFLASH_MUTANT_WRONG_PROGRAM '
   if result.returncode!=0 or result.stdout.count(marker)!=1 or b'ACTUAL_ZFLASH_FINITE_PASS' in result.stdout:
    raise ValueError('specific real guest wrong memory result not admitted: '+name)
   receipts.append(dict(kind=name,profile=profile,returncode=result.returncode,original_sha256=hashlib.sha256(original).hexdigest(),mutated_sha256=hashlib.sha256(raw).hexdigest(),command=command))
   p.write_bytes(original)
 finally:p.write_bytes(original)
 (out/'mutation-report.json').write_text(json.dumps(receipts,indent=2)+'\n')
 print('TWO_ACTUAL_ZFLASH_MODEL_MUTANTS_DETECTED '+profile)
if __name__=='__main__':main()
