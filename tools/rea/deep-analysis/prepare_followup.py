from pathlib import Path
import subprocess
r=Path(__file__).resolve().parent
s=(r/'build_scoped_targets.py').read_text();a=s.index('GROUPS=');b=s.index('\nif not ',a)
s=s[:a]+'GROUPS='+repr({'service-codec-followup':[148123,380446,380444,387023,387055]})+s[b:]
s=s.replace("OUT=ROOT/'scoped-targets'","OUT=ROOT/'followup-targets'").replace("ROOT/'scoped-targets.json'","ROOT/'followup-targets.json'")
compile(s,str(r/'build_followup_targets.py'),'exec');(r/'build_followup_targets.py').write_text(s,encoding='utf-8',newline='\n')
s=(r/'run_deep_rea.py').read_text().replace("ROOT/'rea-dossiers'","ROOT/'followup-dossiers'").replace("ROOT/'scoped-targets.json'","ROOT/'followup-targets.json'").replace("ROOT/'scoped-targets'/","ROOT/'followup-targets'/").replace("ROOT/'rea-analysis-summary.json'","ROOT/'followup-analysis-summary.json'")
compile(s,str(r/'run_followup_rea.py'),'exec');(r/'run_followup_rea.py').write_text(s,encoding='utf-8',newline='\n')
p=subprocess.run([r'C:\Python315\python.exe',str(r/'build_followup_targets.py')],timeout=30)
raise SystemExit(p.returncode)
