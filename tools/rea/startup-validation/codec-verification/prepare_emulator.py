"""Acquire a pinned CPU emulator into this analysis workspace only."""
from pathlib import Path
import urllib.request,json,hashlib,zipfile,sys
ROOT=Path(__file__).resolve().parent
VENDOR=ROOT/'vendor'
if VENDOR.exists():raise RuntimeError('Vendor output exists; verify rather than overwrite')
url='https://pypi.org/pypi/unicorn/2.1.4/json'
req=urllib.request.Request(url,headers={'User-Agent':'Metallic-RC-offline-verification'})
with urllib.request.urlopen(req,timeout=25) as f:info=json.load(f)
filename='unicorn-2.1.4-cp37-abi3-win_amd64.whl'
item=next(x for x in info['urls'] if x['filename']==filename)
expected='d7107500c64ce5c168fbff6bef9485b5db1350050036f4cea568650cf8bdbdf5'
assert item['digests']['sha256']==expected and item['url'].startswith('https://files.pythonhosted.org/')
with urllib.request.urlopen(item['url'],timeout=45) as f:data=f.read(20_000_000)
assert len(data)==item['size'] and hashlib.sha256(data).hexdigest()==expected
wheel=ROOT/filename
with wheel.open('xb') as f:f.write(data)
VENDOR.mkdir()
with zipfile.ZipFile(wheel) as z:
 for member in z.infolist():
  target=(VENDOR/member.filename).resolve()
  if not target.is_relative_to(VENDOR.resolve()) or member.is_dir():
   if member.is_dir():continue
   raise ValueError('Unexpected archive path')
  target.parent.mkdir(parents=True,exist_ok=True)
  with target.open('xb') as f:f.write(z.read(member))
sys.path.insert(0,str(VENDOR));import unicorn
assert unicorn.__version__=='2.1.4'
# New synthetic fixture only: MOV EAX,42; RET, stopped at sentinel return address.
from unicorn import Uc,UC_ARCH_X86,UC_MODE_64,UC_PROT_ALL
from unicorn.x86_const import UC_X86_REG_RSP,UC_X86_REG_RAX
uc=Uc(UC_ARCH_X86,UC_MODE_64);uc.mem_map(0x100000,4096);uc.mem_map(0x200000,4096)
uc.mem_write(0x100000,bytes.fromhex('b82a000000c3'));uc.reg_write(UC_X86_REG_RSP,0x200800);uc.mem_write(0x200800,(0x100100).to_bytes(8,'little'));uc.emu_start(0x100000,0x100100,count=10)
assert uc.reg_read(UC_X86_REG_RAX)==42
record={'package':'unicorn','version':unicorn.__version__,'wheel':filename,'sha256':expected,'bytes':len(data),'source':item['url'],'syntheticSmokePassed':True,'installLocation':str(VENDOR),'systemPythonChanged':False,'originalGameExecuted':False}
(ROOT/'emulator-provenance.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps(record),flush=True)
