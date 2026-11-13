from pathlib import Path
import subprocess
root=Path(__file__).resolve().parent
s=(root/'build_rea_function_capsule.py').read_text()
s=s.replace("pages=sorted(set(rva & ~4095 for _,rva,_,_ in methods))", "methods.append(('UnityEngine.Object.op_Inequality',0xb415990,252,253383))\npages=sorted(set(rva & ~4095 for _,rva,_,_ in methods))")
s=s.replace("0x8664,4,0,0,0,240,0x2022", "0x8664,len(pages)+1,0,0,0,240,0x2022")
s=s.replace('14,0,3*4096,4096,0', '14,0,len(pages)*4096,4096,0')
s=s.replace("methods[-1][1],pages[0]", "methods[2][1],pages[0]")
s=s.replace('i<3', 'i<len(pages)')
s=s.replace('ExactCodePages.analysis.dll','ExactCodePages.with-tail.analysis.dll')
s=s.replace('code-page-provenance.json','code-page-with-tail-provenance.json')
s=s.replace('three exact code pages','four exact code pages including the verified Unity inequality tail-call target')
compile(s,str(root/'build_rea_tail_capsule.generated.py'),'exec')
(root/'build_rea_tail_capsule.generated.py').write_text(s,encoding='utf-8',newline='\n')
p=subprocess.run([r'C:\Python315\python.exe',str(root/'build_rea_tail_capsule.generated.py')],timeout=30)
raise SystemExit(p.returncode)
