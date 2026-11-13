"""Exact instruction matcher extracted from the executed recovery pass; no game inputs."""
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
