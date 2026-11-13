#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Two isolated model-source mutations; actual guest wrong-memory admission."""
from pathlib import Path
import hashlib,json,os,re,subprocess,sys
ROOT=Path(__file__).resolve().parents[2]
# Match complete executable statements, allowing Rust formatting changes while
# keeping the checked range and NOR-program operation specific. Unknown model
# changes and duplicate sites must fail before running a mutant guest.
MUTATIONS = {
 'omit-erase-commit': (
  rb'(?m)^[ \t]*(self\s*\.\s*flash\s*\.\s*data\s*\[\s*address\s*\.\.\s*end\s*\]\s*\.\s*fill\s*\(\s*0xff\s*\)\s*;)[ \t]*$',
  b'let _ = address; // isolated actual model erase commit removed'),
 'omit-program-commit': (
  rb'(?m)^[ \t]*(\*\s*old\s*&=\s*new\s*;)[ \t]*$',
  b'let _ = (old, new); // isolated actual model program commit removed'),
}

def rust_code_only(source):
 """Keep byte offsets, but hide Rust comments and literals from site matching.

 This is a lexer, not a Rust parser. Unsupported/unterminated literals fail
 closed; nested block comments and arbitrary raw-string hash counts matter
 because model documentation can contain complete executable-looking lines.
 """
 masked = bytearray(source)
 i = 0
 while i < len(source):
  start = i
  if source.startswith(b'//', i):
   end = source.find(b'\n', i)
   i = len(source) if end < 0 else end
  elif source.startswith(b'/*', i):
   depth = 1
   i += 2
   while i < len(source) and depth:
    if source.startswith(b'/*', i):
     depth += 1; i += 2
    elif source.startswith(b'*/', i):
     depth -= 1; i += 2
    else:
     i += 1
   if depth:
    raise ValueError('unterminated Rust block comment')
  else:
   raw = re.match(rb'(?:br|cr|r)(#*)"', source[i:])
   if raw:
    delimiter = b'"' + raw.group(1)
    end = source.find(delimiter, i + raw.end())
    if end < 0:
     raise ValueError('unterminated Rust raw string')
    i = end + len(delimiter)
   elif source[i:i+1] == b'"':
    i += 1
    while i < len(source):
     if source[i:i+1] == b'\\':
      i += 2
     elif source[i:i+1] == b'"':
      i += 1
      break
     else:
      i += 1
    else:
     raise ValueError('unterminated Rust string')
   elif source[i:i+1] == b"'":
    # Distinguish character literals (including escaped quotes) from lifetimes.
    char = re.match(rb"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|[^\r\n])|[^'\\\r\n])'", source[i:])
    if not char:
     i += 1
     continue
    i += char.end()
   else:
    i += 1
    continue
  # Non-whitespace sentinels also prevent matching across a hidden token.
  for offset in range(start, i):
   if source[offset:offset+1] not in (b'\r', b'\n'):
    masked[offset] = ord('~')
 return bytes(masked)

def mutate_source(original, name):
 pattern, replacement = MUTATIONS[name]
 matches = list(re.finditer(pattern, rust_code_only(original)))
 if len(matches) != 1:
  raise ValueError(f'unique actual model mutation site: {name} ({len(matches)} matches)')
 start, end = matches[0].span(1)
 return original[:start] + replacement + original[end:]

def main():
 if os.getenv('GITHUB_ACTIONS')!='true':raise ValueError('hosted-only mutant guest')
 profile=sys.argv[1];out=ROOT/sys.argv[2]
 if profile not in ['default','event-scheduler'] or out.exists():raise ValueError('profile/new output')
 out.mkdir();p=ROOT/'crates/core/src/bus/samd51_storage.rs';original=p.read_bytes()
 receipts=[]
 try:
  for name in MUTATIONS:
   raw=mutate_source(original,name);p.write_bytes(raw)
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
