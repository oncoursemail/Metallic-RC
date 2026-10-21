from pathlib import Path
import re,shutil,json,subprocess
R=Path(__file__).resolve().parent;W=R.parent/'rework-20261020/rea-steam-decoding';P=W/'startup-contract-fix-20261021';O=W/'native-login-writer-rea-20261021'
O.mkdir(exist_ok=False)
s=(P/'build_targets.py').read_text();s=re.sub(r'^GROUPS=.*$','GROUPS='+repr({'login-writer':[126719]}),s,flags=re.M)
(O/'build_targets.py').write_text(s,encoding='utf8',newline='\n')
for name in ['rea_session.py','run_contract_rea.py']:shutil.copyfile(P/name,O/name)
selection={'methods':[{'methodIndex':126719,'rva':0x96069e0,'length':0x9606eb0-0x96069e0,'reaRanges':[],'rangeBasis':'Full contiguous body extent to the next independently mapped formatter entry, including split compiler ranges and traps.'}]}
path=O/'writer-selection.json';path.write_text(json.dumps(selection))
checker=W/'deep-rea-20261021/SelectedInstructionAudit/bin/Release/net10.0/SelectedInstructionAudit.dll'
subprocess.run([str(R.parent/'tooling/dotnet-10.0.402/dotnet.exe'),str(checker),str(W/'runs/recovery-/20261020T212503.738773Z/decoded/GameAssembly.code-restored.analysis.dll'),str(path),str(O/'instructions')],check=True)
subprocess.run([r'C:\Python315\python.exe',str(O/'build_targets.py')],check=True)
subprocess.run([r'C:\Python315\python.exe',str(O/'run_contract_rea.py')],check=True)
