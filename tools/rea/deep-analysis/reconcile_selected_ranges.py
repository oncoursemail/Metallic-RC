"""An entry unwind record is not necessarily the complete native function.
Audit the union span of the entry record and actual REA body ranges, retaining
additional compiler fragments and every gap. Never silently trim REA output.
"""
from pathlib import Path
import json,sqlite3
ROOT=Path(__file__).resolve().parent
rows={}
for f in ('rea-analysis-summary.json','followup-analysis-summary.json'):
    d=json.loads((ROOT/f).read_text());assert d['status']=='completed'
    for m in d['results']:rows[m['methodIndex']]=m
with sqlite3.connect((ROOT/'Metallic-RC.Analysis.sqlite').as_uri()+'?mode=ro',uri=True) as db:
    db.row_factory=sqlite3.Row;out=[];notes=[]
    for mid,m in sorted(rows.items()):
        start=int(m['typedMetadataSignature']['nativeRva'],16);ranges=m['analyzedBody']['ranges']
        assert all(int(x['start'],16)>=0x180000000+start for x in ranges)
        rea_end=max(int(x['end'],16)-0x180000000+1 for x in ranges)
        entry=m['typedMetadataSignature']['range'];entry_end=entry['end_rva'] if entry else start
        end=max(rea_end,entry_end);assert 0<end-start<100000
        fragments=[dict(x) for x in db.execute('SELECT start_rva,end_rva,unwind_rva,sha256 FROM native_ranges WHERE start_rva>=? AND start_rva<? ORDER BY start_rva',(start,end))]
        neighbors=[dict(x) for x in db.execute('SELECT id,type,name,rva FROM methods WHERE rva>? AND rva<? ORDER BY rva LIMIT 20',(start,end))]
        out.append({'methodIndex':mid,'rva':start,'length':end-start,'rangeBasis':'entry_unwind_plus_REA_body_span' if entry else 'bounded_REA_leaf_body_no_unwind_record','reaRanges':ranges,'reaEvidenceId':m['evidenceId']})
        notes.append({'methodIndex':mid,'entryUnwindBytes':entry_end-start if entry else None,'reaSpanBytes':rea_end-start,'auditedBytes':end-start,'compilerFragments':fragments,'otherMetadataEntriesWithinSpan':neighbors,'expandedBeyondFirstUnwind':rea_end>entry_end if entry else False})
(ROOT/'selected-instruction-input-v2.json').write_text(json.dumps({'methods':out},indent=2)+'\n',encoding='utf-8',newline='\n')
(ROOT/'range-reconciliation.json').write_text(json.dumps({'status':'explicit_union_spans','methods':notes,'limits':['These spans cover source bytes; they are not proofs that every byte is reachable.','Unwind records can split a function, while decompiler bodies can omit exception paths. Both boundaries remain separately recorded.']},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'methods':len(out),'totalSelectedBytes':sum(m['length'] for m in out),'expandedMethods':[m['methodIndex'] for m in notes if m['expandedBeyondFirstUnwind']],'spansContainingOtherMetadataEntries':[m['methodIndex'] for m in notes if m['otherMetadataEntriesWithinSpan']]}))
