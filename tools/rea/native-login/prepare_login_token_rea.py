from pathlib import Path
import re,shutil,subprocess
R=Path(__file__).resolve().parent;W=R.parent/'rework-20261020/rea-steam-decoding';P=W/'startup-contract-fix-20261021';O=W/'native-login-token-rea-20261021'
O.mkdir(exist_ok=False)
s=(P/'build_targets.py').read_text();s=re.sub(r'^GROUPS=.*$','GROUPS='+repr({'token-consumer':[149439,149491],'password-request':[149472,146099]}),s,flags=re.M)
(O/'build_targets.py').write_text(s,encoding='utf8',newline='\n')
for name in ['rea_session.py','run_contract_rea.py']:shutil.copyfile(P/name,O/name)
subprocess.run([r'C:\Python315\python.exe',str(O/'build_targets.py')],check=True)
subprocess.run([r'C:\Python315\python.exe',str(O/'run_contract_rea.py')],check=True)
