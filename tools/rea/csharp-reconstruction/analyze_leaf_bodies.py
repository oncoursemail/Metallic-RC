"""Conservative typed x64 leaf-body reconstruction. Never executes native bytes.
Only exact branch/call-free load/store/constant/return forms are admitted.
This is sequential leaf semantics, not game-level, exception, or concurrency proof.
"""
from pathlib import Path
import json,struct,hashlib,collections,datetime,sys
ROOT=Path(__file__).resolve().parent
REFERENCE=ROOT/'typed-reference-v3'
SOURCE=ROOT.parent/'runs/recovery-/20261020T212503.738773Z/decoded'
OUT=ROOT/'leaf-bodies';OUT.mkdir(exist_ok=False)
status=json.loads((REFERENCE/'reconstruction-status.json').read_text());assert status['status']=='typed_csharp_reference_reconstructed'
b=(SOURCE/'GameAssembly.code-restored.analysis.dll').read_bytes();assert hashlib.sha256(b).hexdigest()=='a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066'
cap=json.loads((SOURCE.parent/'capture/capture-report.json').read_text());ss=cap['gameAssemblyDiskLayout']['sections']
meta=(SOURCE/'global-metadata.v31.normalized.dat').read_bytes();tables=json.loads((SOURCE/'metadata-normalization.json').read_text())['headerMapping']
def table(role,fmt):q=tables[role];return list(struct.iter_unpack(fmt,meta[q['offset']:q['offset']+q['size']]))
tds=table('typeDefinitions','<16i8H2I');mds=table('methods','<IiiiiiI4H')
nts=[json.loads(x) for x in (REFERENCE/'native-types.jsonl').open(encoding='utf-8')];assert len(nts)==201385
fieldmap=collections.defaultdict(list)
for line in (REFERENCE/'field-layouts.jsonl').open(encoding='utf-8'):
 f=json.loads(line)
 if not f['isStatic'] and not f['declaringTypeIsValueType'] and f['offset']>=0:fieldmap[(f['declaringTypeIndex'],f['offset'])].append(f)
INT={'IL2CPP_TYPE_BOOLEAN':1,'IL2CPP_TYPE_CHAR':2,'IL2CPP_TYPE_I1':1,'IL2CPP_TYPE_U1':1,'IL2CPP_TYPE_I2':2,'IL2CPP_TYPE_U2':2,'IL2CPP_TYPE_I4':4,'IL2CPP_TYPE_U4':4,'IL2CPP_TYPE_I8':8,'IL2CPP_TYPE_U8':8,'IL2CPP_TYPE_I':8,'IL2CPP_TYPE_U':8}
REF={'IL2CPP_TYPE_CLASS','IL2CPP_TYPE_STRING','IL2CPP_TYPE_OBJECT','IL2CPP_TYPE_SZARRAY','IL2CPP_TYPE_ARRAY'}
def isref(t):return t['kind'] in REF or (t['kind']=='IL2CPP_TYPE_GENERICINST' and not t['valuetype'])
def addr(rva):
 for s in ss:
  if s['name'] in ('.text','il2cpp') and s['rva']<=rva and rva+32<=s['rva']+s['virtualSize']:return s['rawOffset']+rva-s['rva']
 return None
# prefix, disp8 ModRM, disp32 ModRM, width, operation, floating register format.
SPECS=[(b'\x48\x8b',0x41,0x81,8,'get','integer'),(b'\x8b',0x41,0x81,4,'get','integer'),(b'\x0f\xb6',0x41,0x81,1,'get','integer'),(b'\x0f\xbe',0x41,0x81,1,'get','integer'),(b'\x0f\xb7',0x41,0x81,2,'get','integer'),(b'\x0f\xbf',0x41,0x81,2,'get','integer'),(b'\x8a',0x41,0x81,1,'get','integer'),(b'\xf3\x0f\x10',0x41,0x81,4,'get','float'),(b'\xf2\x0f\x10',0x41,0x81,8,'get','float'),(b'\x48\x89',0x51,0x91,8,'set','integer'),(b'\x89',0x51,0x91,4,'set','integer'),(b'\x88',0x51,0x91,1,'set','integer'),(b'\x66\x89',0x51,0x91,2,'set','integer'),(b'\xf3\x0f\x11',0x49,0x89,4,'set','float'),(b'\xf2\x0f\x11',0x49,0x89,8,'set','float')]
def decode(code):
 for prefix,m8,m32,width,operation,fmt in SPECS:
  if not code.startswith(prefix):continue
  pos=len(prefix);mode=code[pos];dispbytes=1 if mode==m8 else 4 if mode==m32 else 0
  if not dispbytes:continue
  end=pos+1+dispbytes
  if code[end]!=0xc3:continue
  offset=int.from_bytes(code[pos+1:end],'little',signed=True)
  if offset<0:return None
  return {'operation':operation,'offset':offset,'width':width,'format':fmt,'length':end+1}
 if code[:3] in (b'\x33\xc0\xc3',b'\x31\xc0\xc3'):return {'operation':'constant','value':0,'width':4,'length':3}
 if code[:1]==b'\xb8' and code[5]==0xc3:return {'operation':'constant','value':int.from_bytes(code[1:5],'little'),'width':4,'length':6}
 if code[:1]==b'\xb0' and code[2]==0xc3:return {'operation':'constant','value':code[1],'width':1,'length':3}
 if code[0]==0xc3:return {'operation':'return_void','length':1}
 return None
# Test every admitted encoding at both displacement widths, plus refusal controls.
tests=[]
for p,m8,m32,w,op,fmt in SPECS:
 for mode,disp in ((m8,(32).to_bytes(1,'little')),(m32,(512).to_bytes(4,'little'))):
  code=p+bytes([mode])+disp+b'\xc3'+b'\xcc'*32;d=decode(code);assert d and d['operation']==op and d['width']==w and d['offset']==int.from_bytes(disp,'little');tests.append({'bytes':code[:d['length']].hex(),'accepted':True})
for code in [b'\x48\x8b\x41\x20\xe9',b'\xeb\x00\x48\x8b\x41\x20\xc3',b'\xe8\x00\x00\x00\x00\xc3',b'\x48\x8b\x42\x20\xc3',b'\x48\x8b\x41\xff\xc3']:
 assert decode(code+b'\xcc'*32) is None;tests.append({'bytes':code.hex(),'accepted':False})
counts=collections.Counter();rejected=collections.Counter();scanned=0
with (OUT/'leaf-reconstruction.jsonl').open('x',encoding='utf-8',newline='\n') as output:
 for line in (REFERENCE/'method-signatures.jsonl').open(encoding='utf-8'):
  m=json.loads(line);mi=m['methodIndex'];t=tds[m['declaringTypeIndex']];scanned+=1
  if m['nativeRva'] is None:rejected['null_pointer']+=1;continue
  if m['name'] in ('.ctor','.cctor','Finalize') or m['returnByref'] or t[6]>=0 or mds[mi][5]>=0:rejected['constructor_byref_or_generic']+=1;continue
  rva=int(m['nativeRva'],16);raw=addr(rva)
  if raw is None:rejected['out_of_code_range']+=1;continue
  candidate=decode(b[raw:raw+32])
  if candidate is None:rejected['unsupported_native_body']+=1;continue
  operation=candidate['operation'];ret=nts[mds[mi][2]];proof={'methodIndex':mi,'assembly':m['assembly'],'declaringTypeIndex':m['declaringTypeIndex'],'declaringType':m['declaringType'],'method':m['name'],'token':m['token'],'returnType':m['returnType'],'rva':m['nativeRva'],'sourceOffset':raw,'translation':candidate,'parameters':m['parameters']}
  if operation in ('get','set'):
   if m['flags']&16 or ((ret['kind']=='IL2CPP_TYPE_VOID')!=(operation=='set')) or len(m['parameters'])!=(0 if operation=='get' else 1):rejected['signature_not_accessor_shape']+=1;continue
   fs=fieldmap.get((m['declaringTypeIndex'],candidate['offset']),[])
   if len(fs)!=1:rejected['no_unique_direct_instance_field']+=1;continue
   f=fs[0];ft=nts[f['nativeTypeIndex']]
   if ft['byref']:rejected['byref_field']+=1;continue
   expected=m['returnType'] if operation=='get' else m['parameters'][0]['type']
   if f['type']!=expected or (operation=='set' and m['parameters'][0]['byref']):rejected['field_signature_type_mismatch']+=1;continue
   if candidate['format']=='float':compatible=ft['kind']==('IL2CPP_TYPE_R4' if candidate['width']==4 else 'IL2CPP_TYPE_R8')
   else:compatible=INT.get(ft['kind'])==candidate['width'] or (operation=='get' and candidate['width']==8 and isref(ft))
   if not compatible:rejected['field_width_or_reference_store']+=1;continue
   proof['field']={k:f[k] for k in ('fieldIndex','name','type','offset','token','attributes')}
  elif operation=='constant':
   if len(m['parameters']) or ret['byref']:rejected['constant_signature']+=1;continue
   width=INT.get(ret['kind']);value=candidate['value']
   if isref(ret) and value==0:proof['translation']['managedConstant']='null'
   elif width and width<=candidate['width'] and not(ret['kind']=='IL2CPP_TYPE_BOOLEAN' and value not in (0,1)):proof['translation']['managedConstant']='integer_bits'
   else:rejected['unsupported_constant_type']+=1;continue
  elif operation=='return_void':
   if ret['kind']!='IL2CPP_TYPE_VOID':rejected['ret_with_unknown_value']+=1;continue
  code=b[raw:raw+candidate['length']];proof['nativeBytes']=code.hex();proof['nativeBytesSha256']=hashlib.sha256(code).hexdigest();proof['validation']='exact_supported_instruction_sequence_plus_recovered_type_and_field_mapping';proof['gameplayVerified']=False
  output.write(json.dumps(proof,ensure_ascii=True)+'\n');counts[operation]+=1
summary={'status':'bounded_leaf_translations_identified','allMethodDefinitionsScanned':scanned,'accepted':sum(counts.values()),'operations':dict(counts),'rejected':dict(rejected),'decoderTestsPassed':len(tests),'sourceSha256':hashlib.sha256(b).hexdigest(),'proofFileSha256':hashlib.sha256((OUT/'leaf-reconstruction.jsonl').read_bytes()).hexdigest(),'limits':['Not complete decompilation: all complex branches, calls, constructors, generic bodies and unresolved cases are excluded.','Translations establish typed sequential leaf behavior; exception timing, concurrency, cross-platform code generation and gameplay equivalence are not tested.','No native instruction bytes or game assemblies were executed.','Reference stores are excluded to avoid inventing GC write-barrier semantics.']}
(OUT/'leaf-analysis.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n');(OUT/'decoder-tests.json').write_text(json.dumps(tests,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(summary))
