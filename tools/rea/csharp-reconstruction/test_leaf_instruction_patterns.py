from leaf_instruction_patterns import SPECS,decode
import json
tests=[]
for p,m8,m32,w,op,fmt in SPECS:
 for mode,disp in ((m8,(32).to_bytes(1,'little')),(m32,(512).to_bytes(4,'little'))):
  code=p+bytes([mode])+disp+b'\xc3'+b'\xcc'*32;d=decode(code);assert d and d['operation']==op and d['width']==w and d['offset']==int.from_bytes(disp,'little');tests.append({'bytes':code[:d['length']].hex(),'accepted':True})
for code in [b'\x48\x8b\x41\x20\xe9',b'\xeb\x00\x48\x8b\x41\x20\xc3',b'\xe8\x00\x00\x00\x00\xc3',b'\x48\x8b\x42\x20\xc3',b'\x48\x8b\x41\xff\xc3']:
 assert decode(code+b'\xcc'*32) is None;tests.append({'bytes':code.hex(),'accepted':False})

print(json.dumps({"status":"passed","cases":len(tests),"requiresGameFiles":False}))
