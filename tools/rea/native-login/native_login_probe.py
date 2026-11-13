"""Evaluate the exact generated formatter's control flow in a bounded x64 guest.
The IL2CPP allocator, metadata resolver, GC barrier and JsonReader/Writer routines
are declared modeled boundaries. This does not execute the original JSON parser,
OS exception unwinder, live client, or service. Unknown guest targets fail closed.
"""
from pathlib import Path
import sys,json,hashlib,struct,re,sqlite3,collections
R=Path(__file__).resolve().parent;W=R.parent;D=W/'runs/recovery-/20261020T212503.738773Z/decoded'
sys.path.insert(0,str(W/'verification-pass-20261021/vendor'))
from unicorn import Uc,UcError,UC_ARCH_X86,UC_MODE_64,UC_HOOK_CODE,UC_HOOK_MEM_READ_UNMAPPED,UC_HOOK_MEM_WRITE_UNMAPPED,UC_HOOK_MEM_FETCH_UNMAPPED
from unicorn.x86_const import *
B=0x180000000;HEAP=0x60000000;STACK=0x70000000;SP=STACK+0x18000;STOP=0x50000000
SHA='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
class NativeFormatter:
 def __init__(self):
  self.image=(D/'GameAssembly.code-restored.analysis.dll').read_bytes();assert hashlib.sha256(self.image).hexdigest()==SHA
  self.meta=(D/'global-metadata.v31.normalized.dat').read_bytes();assert hashlib.sha256(self.meta).hexdigest()=='d675ea92bfb0086c0238e56702e91413fcc663c81fce364253749edbf6fcec9f'
  self.sections=json.loads((D.parent/'capture/capture-report.json').read_text())['gameAssemblyDiskLayout']['sections'];self.roles=json.loads((D/'metadata-normalization.json').read_text())['headerMapping']
  self.uc=Uc(UC_ARCH_X86,UC_MODE_64);self.pages=set();self.uc.mem_map(HEAP,0x1000000);self.uc.mem_map(STACK,0x20000);self.uc.mem_map(STOP,4096)
  self.heap=HEAP;self.meta_heap=HEAP+0xc00000;self.strings={0:None};self.type_tags={};self.aliases=[];self.prefix_calls=[];self.slot_records=[];self.steps=0;self.all_steps=0;self.status='idle';self.context={}
  self.ranges={126718: (157314736, 157315991), 126719: (157313504, 157314736), 126720: (157312384, 157313504)}
  self.boundaries={0xc400d0:'metadata',0xc40430:'type_init',0xc40350:'allocate',0xc3f460:'array_new',0xd0a4c0:'object_ctor',0xd10590:'object_ctor',0xc3f3e0:'write_barrier',0xa510cb0:'dictionary_ctor',0xa510460:'dictionary_add',0xa515ad0:'encode_key',0xa5159a0:'prefix_comma',0xa516160:'prefix_begin',0xa531a30:'write_raw',0xa515130:'write_string',0x50bff50:'ensure_buffer',0x50c00f0:'write_null',0xa513620:'read_null',0xa5132c0:'begin_object',0xa5136e0:'empty_object',0xa514290:'next_property',0xa514500:'property_segment',0xa510b10:'lookup_key',0xa513de0:'read_string',0xa512540:'skip_value',0xc40390:'bounds_error',0xc403a0:'null_error'}
  self.boundaries.update({0x3bd94d0:'nested_resolver',0x3d80:'nested_read',0xc3d410:'nested_method_lookup',0x50001000-B:'nested_write'})
  self.uc.mem_map(0x50001000,4096)
  self.nested_object=self.meta_alloc();self.putq(self.nested_object,self.meta_alloc())
  self.nested_method=self.meta_alloc();self.putq(self.nested_method,0x50001000);self.putq(self.nested_method+8,0)
  self.uc.hook_add(UC_HOOK_MEM_READ_UNMAPPED|UC_HOOK_MEM_WRITE_UNMAPPED|UC_HOOK_MEM_FETCH_UNMAPPED,self.unmapped)
  self.uc.hook_add(UC_HOOK_CODE,self.step)
  self.regs=[UC_X86_REG_RAX,UC_X86_REG_RBX,UC_X86_REG_RCX,UC_X86_REG_RDX,UC_X86_REG_RSI,UC_X86_REG_RDI,UC_X86_REG_RBP,UC_X86_REG_RSP,UC_X86_REG_R8,UC_X86_REG_R9,UC_X86_REG_R10,UC_X86_REG_R11,UC_X86_REG_R12,UC_X86_REG_R13,UC_X86_REG_R14,UC_X86_REG_R15]
  for reg in self.regs:self.uc.reg_write(reg,0)
  self.uc.reg_write(UC_X86_REG_EFLAGS,2);self.baseline=self.uc.context_save()
  self.formatter=self.alloc(0x100);self.call(126718,[self.formatter]);assert self.status=='ok',self.context
  self.dictionary=self.qword(self.formatter+0x10);self.names_array=self.qword(self.formatter+0x18)
  assert self.qword(self.names_array+0x18)==8
  self.alias_map={r['name']:r['ordinal']for r in self.aliases};assert len(self.alias_map)==len(self.aliases)
  self.canonical=[]
  for i in range(8):
   p=self.qword(self.names_array+0x20+8*i);data=self.byte_array(p);obj=json.loads((data+(b'null}' if i==0 else b'null}')).decode() if i==0 else ('{'+data.decode()[1:]+'null}'))
   name=next(iter(obj));self.canonical.append(name);assert self.alias_map[name]==i
  self.case_heap=self.heap;self.constructor_steps=self.steps
 def raw(self,rva,n):
  for s in self.sections:
   if s['rva']<=rva and rva+n<=s['rva']+s['rawSize']:return self.image[s['rawOffset']+rva-s['rva']:s['rawOffset']+rva-s['rva']+n]
  raise ValueError('Unbacked source '+hex(rva))
 def map_source(self,address):
  page=address&~4095
  if page in self.pages:return
  rva=page-B
  if not 0<=rva<0x12000000:raise ValueError('Unknown guest address '+hex(address))
  block=bytearray(4096);matched=False
  for s in self.sections:
   lo=max(rva,s['rva']);hi=min(rva+4096,s['rva']+s['virtualSize'])
   if lo<hi:
    matched=True;read_end=min(hi,s['rva']+s['rawSize'])
    if read_end>lo:block[lo-rva:read_end-rva]=self.raw(lo,read_end-lo)
  if not matched:raise ValueError('Address outside admitted source sections '+hex(address))
  self.uc.mem_map(page,4096);self.uc.mem_write(page,bytes(block));self.pages.add(page)
 def unmapped(self,uc,access,address,size,value,unused):
  try:
   for page in range(address&~4095,(address+size+4095)&~4095,4096):self.map_source(page)
   return True
  except Exception as exc:self.context['memoryFailure']=str(exc);return False
 def qword(self,a):return struct.unpack('<Q',self.uc.mem_read(a,8))[0]
 def putq(self,a,v):self.uc.mem_write(a,struct.pack('<Q',v))
 def alloc(self,size=0x200):
  p=self.heap;self.heap=(p+size+15)&~15
  if self.heap>=HEAP+0xb00000:raise ValueError('Guest allocation bound')
  self.uc.mem_write(p,b'\0'*(self.heap-p));return p
 def meta_alloc(self,size=0x200):
  p=self.meta_heap;self.meta_heap=(p+size+15)&~15
  if self.meta_heap>=HEAP+0x1000000:raise ValueError('Metadata allocation bound')
  self.uc.mem_write(p,b'\0'*(self.meta_heap-p));return p
 def string(self,text):
  if text is None:return 0
  p=self.alloc(32+len(text.encode('utf-16-le','surrogatepass')));self.strings[p]=text;return p
 def byte_array_new(self,data):
  p=self.alloc(32+len(data));self.putq(p+24,len(data));self.uc.mem_write(p+32,bytes(data));return p
 def byte_array(self,p):
  n=self.qword(p+24);assert n<=131072
  return bytes(self.uc.mem_read(p+32,n))
 def literal(self,tagged):
  assert tagged>>29==5;index=(tagged&0x1ffffffe)>>1;q=self.roles['stringLiteral'];size,start=struct.unpack_from('<II',self.meta,q['offset']+index*8);q=self.roles['stringLiteralData'];assert start+size<=q['size'];return self.meta[q['offset']+start:q['offset']+start+size].decode('utf-8')
 def resolve(self,slot):
  self.map_source(slot);v=self.qword(slot)
  if HEAP<=v<HEAP+0x1000000:return
  if v<2**32 and v&1:
   tag=v>>29
   if tag==5:p=self.meta_alloc(0x200);self.strings[p]=self.literal(v)
   else:p=self.meta_alloc(0x200);self.uc.mem_write(p+0xec,struct.pack('<I',1));self.type_tags[p]=v
   self.putq(slot,p);self.slot_records.append({'slot':hex(slot),'tagged':hex(v),'tag':tag,'literal':self.strings.get(p)})
  else:raise ValueError('Unexpected unresolved metadata slot '+hex(slot)+' '+hex(v))
 def ret(self,value=0):
  sp=self.uc.reg_read(UC_X86_REG_RSP);ip=self.qword(sp);self.uc.reg_write(UC_X86_REG_RSP,sp+8)
  for reg in [UC_X86_REG_RCX,UC_X86_REG_RDX,UC_X86_REG_R8,UC_X86_REG_R9,UC_X86_REG_R10,UC_X86_REG_R11]:self.uc.reg_write(reg,0x123456789abcdef0)
  self.uc.reg_write(UC_X86_REG_RAX,value);self.uc.reg_write(UC_X86_REG_RIP,ip)
 def write(self,writer,data):
  p=self.qword(writer);n=struct.unpack('<I',self.uc.mem_read(writer+8,4))[0];cap=self.qword(p+24);assert n+len(data)<=cap
  self.uc.mem_write(p+32+n,data);self.uc.mem_write(writer+8,struct.pack('<I',n+len(data)))
 def step(self,uc,address,size,unused):
  self.steps+=1;self.all_steps+=1;rva=address-B
  if self.steps>30000:raise ValueError('Instruction budget')
  if self.active[0]<=rva and rva+size<=self.active[1]:return
  kind=self.boundaries.get(rva)
  if not kind:raise ValueError('Unknown guest code boundary '+hex(address))
  a=uc.reg_read(UC_X86_REG_RCX);b=uc.reg_read(UC_X86_REG_RDX);c=uc.reg_read(UC_X86_REG_R8);d=uc.reg_read(UC_X86_REG_R9)
  if kind=='nested_resolver':self.ret(self.nested_object);return
  if kind=='nested_method_lookup':
   assert a==self.nested_object and c==0
   self.ret(self.nested_method);return
  if kind=='nested_read':
   token=self.context['tokens'][self.context['cursor']]
   if token['value'] is not None:self.status='nested_hint_outside_profile';uc.emu_stop();return
   self.context['cursor']+=1;self.ret(0);return
  if kind=='nested_write':
   assert a==self.nested_object
   if c!=0:self.status='nested_hint_outside_profile';uc.emu_stop();return
   self.write(b,b'null');self.ret();return
  if kind=='metadata':self.resolve(a);self.ret();return
  if kind=='type_init':self.uc.mem_write(a+0xec,struct.pack('<I',1));self.ret();return
  if kind=='allocate':self.ret(self.alloc(0x300));return
  if kind=='array_new':
   assert b<=128;p=self.alloc(32+b*8);self.putq(p+24,b);self.ret(p);return
  if kind in ('object_ctor','dictionary_ctor','write_barrier'):self.ret();return
  if kind in ('encode_key','prefix_begin','prefix_comma'):
   text=self.strings[a];assert isinstance(text,str)and text.isascii()
   data=text.encode() if kind=='encode_key' else (b'{' if kind=='prefix_begin' else b',')+json.dumps(text,separators=(',',':')).encode()+b':'
   if kind!='encode_key':self.prefix_calls.append({'name':text,'kind':kind})
   self.ret(self.byte_array_new(data));return
  if kind=='dictionary_add':self.aliases.append({'name':self.byte_array(b).decode(),'ordinal':c&0xffffffff});self.ret();return
  if kind=='write_raw':self.write(a,self.byte_array(b));self.ret();return
  if kind=='write_string':
   self.context.setdefault('writtenStrings',[]).append(self.strings[b]);self.write(a,json.dumps(self.strings[b],ensure_ascii=False,separators=(',',':')).encode());self.ret();return
  if kind=='ensure_buffer':assert b+c<=self.qword(self.qword(a)+24);self.ret();return
  if kind=='write_null':self.write(a,b'null');self.ret();return
  if kind=='read_null':self.ret(int(self.context['tokens'] is None));return
  if kind=='begin_object':self.ret();return
  if kind=='empty_object':self.ret(int(self.context['tokens']==[]));return
  if kind=='next_property':self.ret(int(self.context['cursor']>=len(self.context['tokens'])));return
  if kind=='property_segment':
   self.putq(a,self.context['cursor']+1);self.putq(a+8,0);self.putq(a+16,0);self.ret(a);return
  if kind=='lookup_key':
   token=self.context['tokens'][self.context['cursor']];name=token['name'];ordinal=self.alias_map.get(name)
   if ordinal is not None:self.uc.mem_write(c,struct.pack('<i',ordinal))
   self.context.setdefault('lookups',[]).append({'name':name,'ordinal':ordinal});self.ret(int(ordinal is not None));return
  if kind=='read_string':
   token=self.context['tokens'][self.context['cursor']];value=token['value']
   if value is not None and not isinstance(value,str):self.status='known_value_type_error';uc.emu_stop();return
   self.context['cursor']+=1;self.ret(self.string(value));return
  if kind=='skip_value':self.context['cursor']+=1;self.context['skips']+=1;self.ret();return
  if kind in ('null_error','bounds_error'):self.status=kind;uc.emu_stop();return
  raise ValueError('Unhandled boundary '+kind)
 def call(self,mid,args):
  self.uc.context_restore(self.baseline);self.uc.mem_write(SP-0x1000,b'\0'*0x2000);self.putq(SP,STOP);self.uc.reg_write(UC_X86_REG_RSP,SP)
  for reg,value in zip([UC_X86_REG_RCX,UC_X86_REG_RDX,UC_X86_REG_R8,UC_X86_REG_R9],args):self.uc.reg_write(reg,value)
  self.active=self.ranges[mid];self.steps=0;self.status='running'
  try:self.uc.emu_start(B+self.active[0],STOP,count=30000)
  except UcError as e:raise RuntimeError(str(e)+' '+str(self.context))from e
  if self.status=='running':
   if self.uc.reg_read(UC_X86_REG_RIP)!=STOP:raise ValueError('Guest did not return')
   self.status='ok'
  return self.uc.reg_read(UC_X86_REG_RAX)
 def deserialize(self,tokens):
  self.heap=self.case_heap;self.context={'tokens':tokens,'cursor':0,'skips':0};reader=self.alloc();ptr=self.call(126720,[self.formatter,reader,0])
  fields=None if not ptr or self.status!='ok' else [self.strings[self.qword(ptr+16+8*i)]for i in range(8)]
  return {'status':self.status,'fields':fields,'cursor':self.context['cursor'],'skips':self.context['skips'],'instructions':self.steps,'lookups':self.context.get('lookups',[])}
 def serialize(self,values):
  self.heap=self.case_heap;self.context={};value=self.alloc(0x200)if values is not None else 0
  if values is not None:
   assert len(values)==8
   for i,s in enumerate(values):self.putq(value+16+8*i,self.string(s))
  writer=self.alloc();buffer=self.byte_array_new(b'\0'*131072);self.putq(writer,buffer);self.uc.mem_write(writer+8,b'\0'*4)
  self.call(126719,[self.formatter,writer,value,0]);n=struct.unpack('<I',self.uc.mem_read(writer+8,4))[0]
  return {'status':self.status,'json':bytes(self.uc.mem_read(buffer+32,n)).decode(),'writtenStrings':self.context.get('writtenStrings',[]),'instructions':self.steps}
if __name__=='__main__':
 f=NativeFormatter()
 print(json.dumps({'aliases':f.aliases,'canonical':f.canonical,'ranges':f.ranges,'constructorInstructions':f.constructor_steps}),flush=True)
 for tokens in [None,[],[{'name':'error','value':'invalid_grant'}],[{'name':'code_hint','value':None}]]:
  print(json.dumps({'input':tokens,'output':f.deserialize(tokens)}),flush=True)
 print(json.dumps(f.serialize([None]*8)),flush=True)
