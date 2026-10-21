"""Offline exact-build native registration reconstruction with exhaustive bounds checks.
Never loads the game as code. Never edits the captured or installed originals.
"""
from pathlib import Path
import struct,json,hashlib,collections,datetime
ROOT=Path(__file__).resolve().parent
SOURCE=ROOT.parent/'runs/recovery-/20261020T212503.738773Z/decoded'
OUT=ROOT/'verified-native';OUT.mkdir(exist_ok=False)
b=(SOURCE/'GameAssembly.code-restored.analysis.dll').read_bytes()
m=(SOURCE/'global-metadata.v31.normalized.dat').read_bytes()
assert hashlib.sha256(b).hexdigest()=='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
assert hashlib.sha256(m).hexdigest()=='d675ea92bfb0086c0238e56702e91413fcc663c81fce364253749edbf6fcec9f'
base=0x180000000
ss=json.loads((SOURCE.parent/'capture/capture-report.json').read_text())['gameAssemblyDiskLayout']['sections']
meta=json.loads((SOURCE/'metadata-normalization.json').read_text())['headerMapping']
def off(p,n=1):
 r=p-base
 for s in ss:
  if s['rva']<=r and r+n<=s['rva']+s['rawSize']:return s['rawOffset']+r-s['rva']
 raise ValueError('Outside disk-backed source: '+hex(p)+' length '+str(n))
def rd(p,fmt):return struct.unpack_from(fmt,b,off(p,struct.calcsize(fmt)))
def records(role,fmt):q=meta[role];return list(struct.iter_unpack(fmt,m[q['offset']:q['offset']+q['size']]))
q=meta['string'];strings=m[q['offset']:q['offset']+q['size']]
def text(i):return strings[i:strings.index(b'\0',i)].decode('utf8')
def cstring(p):q=off(p);end=b.index(0,q,q+4096);return b[q:end].decode('utf8')
td=records('typeDefinitions','<16i8H2I');md=records('methods','<IiiiiiI4H');fd=records('fields','<IiI');pd=records('parameters','<IIi');imgs=records('images','<IiiIiiiIiI');gps=records('genericParameters','<iIhhHH')
checks=[]
def check(label,condition,**detail):
 if not condition:raise ValueError(label+' '+str(detail))
 checks.append({'check':label,'passed':True,**detail})
# The count anchor appeared exactly twice in the inspected readonly/data regions;
# the contiguous standard registration layout is checked against all its arrays.
reg_addr=0x18ebf7c10;reg=rd(reg_addr,'<16Q')
labels=('genericClasses','genericInsts','genericMethodTable','types','methodSpecs','fieldOffsets','typeDefinitionsSizes','metadataUsages')
registration={k:{'count':reg[2*i],'pointer':hex(reg[2*i+1])} for i,k in enumerate(labels)}
check('Type and field-offset counts equal metadata definitions',reg[10]==reg[12]==len(td),count=len(td))
ptrs=rd(reg[7],'<'+str(reg[6])+'Q');ptr_index={p:i for i,p in enumerate(ptrs)}
check('All type pointers are distinct',len(ptr_index)==len(ptrs),count=len(ptrs))
types=[rd(p,'<QI') for p in ptrs]
valid={1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,17,18,19,20,21,22,24,25,28,29,30}
kindcounts=collections.Counter();unioncounts=collections.Counter()
for i,(data,bits) in enumerate(types):
 kind=(bits>>16)&255;kindcounts[kind]+=1
 if kind not in valid:raise ValueError('Unknown native type kind '+str(kind))
 if kind in (17,18):
  if data>=len(td):raise ValueError('Class type union index')
 elif kind in (19,30):
  if data>=len(gps):raise ValueError('Generic parameter union index')
 elif kind in (15,29):
  if data not in ptr_index:raise ValueError('Element type pointer absent')
 elif kind==20:
  element,rank,sizes,lower=rd(data,'<QBBB')
  if element not in ptr_index or not 1<=rank<=32 or sizes>rank or lower>rank:raise ValueError('Array descriptor')
 elif kind==21:
  typep,classargs,methodargs,cached=rd(data,'<4Q')
  if typep not in ptr_index:raise ValueError('Generic class definition pointer')
 unioncounts[kind]+=1
check('All native type unions bounded',True,count=len(types),kindCounts=dict(kindcounts))
for i,t in enumerate(td):
 data,bits=types[t[2]]
 if data!=i or ((bits>>16)&255) not in (1,2,3,4,5,6,7,8,9,10,11,12,13,14,17,18,22,24,25,28):raise ValueError('TypeDefinition/native byval reciprocity '+str(i))
check('Every type definition has reciprocal native byval descriptor',True,count=len(td))
for role,rr,col in [('fields',fd,1),('parameters',pd,2),('returnTypes',md,2)]:
 check(role+' native type indices valid',all(0<=r[col]<len(types) for r in rr),count=len(rr))
instptrs=rd(reg[3],'<'+str(reg[2])+'Q');instset=set(instptrs);argument_total=0
for p in instptrs:
 argc,argv=rd(p,'<QQ')
 if not 1<=argc<=128:raise ValueError('Generic instance argument count')
 args=rd(argv,'<'+str(argc)+'Q')
 if not all(x in ptr_index for x in args):raise ValueError('Generic argument type pointer')
 argument_total+=argc
check('Every generic instantiation and argument pointer valid',True,instances=len(instptrs),arguments=argument_total)
gcptrs=rd(reg[1],'<'+str(reg[0])+'Q');gcset=set(gcptrs)
for p in gcptrs:
 typep,ca,ma,cached=rd(p,'<4Q')
 if typep not in ptr_index or (ca and ca not in instset) or (ma and ma not in instset):raise ValueError('Generic class context')
check('Every generic class context bounded',True,count=len(gcptrs))
check('All generic-instance type unions reference known generic classes',all(data in gcset for data,bits in types if (bits>>16)&255==21))
spec_count=reg[8];specoff=off(reg[9],spec_count*12);specs=list(struct.iter_unpack('<3i',memoryview(b)[specoff:specoff+spec_count*12]))
check('Every generic method specification references existing definition/instances',all(0<=mi<len(md) and -1<=ci<len(instptrs) and -1<=gi<len(instptrs) for mi,ci,gi in specs),count=spec_count)
gt_count=reg[4];gtoff=off(reg[5],gt_count*16);gtable=list(struct.iter_unpack('<4i',memoryview(b)[gtoff:gtoff+gt_count*16]))
maxima=[max(x[i] for x in gtable) for i in range(4)]
check('Every generic method table row bounded',all(0<=a<spec_count and 0<=x<1123345 and -1<=y<124746 and -1<=z<172790 for a,x,y,z in gtable),count=gt_count,maxima=maxima)
# Count/pointer roles cross-checked against generic indexes and adjacent array ends.
canonical=[240,0x18bcd17e0,1123345,0x18d23bf20,0x18bf1c5d0,124746,0x18bbddd90,37309,0x18bcd1f60,0x18bd1ad50,0x18bd63b40,4889,0x18f8807a0,0,0,419,0]
code_ranges=[(base+s['rva'],base+s['rva']+s['virtualSize']) for s in ss if s['name'] in ('.text','il2cpp')]
code_checks=[]
for name,count,p in [('reversePInvoke',240,canonical[1]),('genericMethods',1123345,canonical[3]),('genericAdjustors',172790,canonical[4]),('invokers',124746,canonical[6]),('unresolvedVirtual',37309,canonical[8]),('unresolvedInstance',37309,canonical[9]),('unresolvedStatic',37309,canonical[10])]:
 values=rd(p,'<'+str(count)+'Q');bad=[i for i,x in enumerate(values) if x and not any(a<=x<z for a,z in code_ranges)]
 check(name+' all nonnull pointers in code',not bad,count=count,nulls=sum(x==0 for x in values));code_checks.append({'role':name,'count':count,'pointer':hex(p)})
check('Invoker exact adjacent boundary',canonical[6]+124746*8==canonical[1])
check('Reverse wrapper exact adjacent boundary',canonical[1]+240*8==canonical[8])
check('Adjustor exact adjacent boundary',canonical[4]+172790*8==reg[1])
check('Generic methods ends with alignment before method table',0<=reg[5]-(canonical[3]+1123345*8)<16)
mods=rd(0x18f87fa00,'<419Q');module_names={};owners=[0]*len(td)
for i,img in enumerate(imgs):owners[img[2]:img[2]+img[3]]=[i]*img[3]
maxrids=[0]*len(imgs)
for method in md:
 image=owners[method[1]];maxrids[image]=max(maxrids[image],method[6]&0xffffff)
for p in mods:
 namep,count,methods=rd(p,'<3Q');name=cstring(namep)
 if name in module_names:raise ValueError('Duplicate module name')
 module_names[name]={'pointer':p,'count':count,'methods':methods}
sortedmods=[]
for i,img in enumerate(imgs):
 name=text(img[0]);module=module_names[name]
 if module['count']!=maxrids[i]:raise ValueError('Module method-count mismatch '+name)
 sortedmods.append(module['pointer'])
check('419 unique modules match metadata by name/count',len(sortedmods)==len(module_names)==419)
fo=rd(reg[11],'<'+str(len(td))+'Q');sizes=rd(reg[13],'<'+str(len(td))+'Q');fieldstates=collections.Counter();typesizes=[]
for i,t in enumerate(td):
 typesizes.append(rd(sizes[i],'<4I'))
 if t[18]:
  if not fo[i]:fieldstates['missing_type_offset_array']+=t[18];continue
  offsets=rd(fo[i],'<'+str(t[18])+'i')
  for n in offsets:fieldstates['concrete' if n>=0 else 'sentinel']+=1
check('Field offset entries accounted for',sum(fieldstates.values())==len(fd),states=dict(fieldstates))
# Add a fresh readonly analysis-only PE section; no live/process/original changes.
pe=struct.unpack_from('<I',b,60)[0];opt=pe+24;st=opt+struct.unpack_from('<H',b,pe+20)[0];ns=struct.unpack_from('<H',b,pe+6)[0]
headersize=struct.unpack_from('<I',b,opt+60)[0]
check('Room for one new section header',st+(ns+1)*40<=headersize)
newrva=(max(s['rva']+s['virtualSize'] for s in ss)+4095)//4096*4096
rawptr=(len(b)+511)//512*512;modoffset=0x100
canonical[-1]=base+newrva+modoffset
payload=bytearray(4096);struct.pack_into('<17Q',payload,0,*canonical);struct.pack_into('<419Q',payload,modoffset,*sortedmods)
new=bytearray(b);new.extend(b'\0'*(rawptr-len(new)));new.extend(payload)
struct.pack_into('<H',new,pe+6,ns+1);struct.pack_into('<I',new,opt+56,newrva+4096)
p=st+ns*40;new[p:p+8]=b'.mrec\0\0\0';struct.pack_into('<IIIIIIHHI',new,p+8,4096,newrva,4096,rawptr,0,0,0,0,0x40000040)
path=OUT/'GameAssembly.registrations.analysis.dll';path.write_bytes(new)
result={'status':'native_registration_structural_validation_passed','sourceSha256':hashlib.sha256(b).hexdigest(),'metadataSha256':hashlib.sha256(m).hexdigest(),'analysisImageSha256':hashlib.sha256(new).hexdigest(),'analysisImageBytes':len(new),'output':str(path),'metadataRegistration':hex(reg_addr),'canonicalCodeRegistration':hex(base+newrva),'originalMetadataRegistration':registration,'canonicalCodeRegistrationWords':[hex(x) if x>=base else x for x in canonical],'sortedModuleCount':419,'allNativeTypes':len(types),'genericClassCount':len(gcptrs),'genericInstanceCount':len(instptrs),'genericArgumentCount':argument_total,'genericMethodSpecs':spec_count,'genericMethodTableRows':gt_count,'fieldOffsetStates':dict(fieldstates),'checks':checks,'codeArrays':code_checks,'patch':{'newSectionRva':hex(newrva),'newSectionRawOffset':rawptr,'originalDataSectionsUnmodified':True,'purpose':'Offline analysis compatibility only; never execute or replace original game','interopRoleEvidence':'Count 4889 and seven-qword entries at original pointer; retained for parser. This struct is not used for invoking code.'},'fullGameRebuild':False,'createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
(OUT/'native-registration-validation.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({k:v for k,v in result.items() if k not in ('checks','originalMetadataRegistration','canonicalCodeRegistrationWords')}))
