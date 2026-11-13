from pathlib import Path
import importlib.util,json,struct,hashlib,time,datetime
r=Path(__file__).resolve().parent;run=r/'runs/recovery-/20261020T212503.738773Z'
spec=importlib.util.spec_from_file_location('decoder',r/'normalize_metadata_23191908.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
p=next((run/'capture').glob('*metadata-candidate*.bin'));data=p.read_bytes();j=json.loads((run/'metadata-structural-inspection.json').read_text());sol=j['headerSearch']['solutions'][0];rs=sol['regions'];records=[]
for name,region,relative,value in [('bad_method_token',15,24,0x01000001),('out_of_range_method_name',15,0,0xffffffff),('wrong_method_owner',15,4,0x0fffffff),('literal_overlap',30,12,12345),('bad_attribute_sentinel',25,rs[25]['size']-4,0xffffffff),('bad_image_assembly_link',16,4,0xffffffff),('bad_module_generic_shape',4,12,2)]:
 b=bytearray(data);struct.pack_into('<I',b,rs[region]['offset']+relative,value)
 try:m.normalize(bytes(b),sol,emit=False)
 except (ValueError,UnicodeError) as e:records.append({'case':name,'rejected':True,'reason':str(e)})
 else:raise AssertionError('Accepted corrupted metadata: '+name)
summary={'status':'passed','negativeCases':len(records),'cases':records,'decoderSha256':hashlib.sha256((r/'normalize_metadata_23191908.py').read_bytes()).hexdigest(),'sourceSha256':hashlib.sha256(data).hexdigest(),'sourceUnchanged':hashlib.sha256(p.read_bytes()).hexdigest()==m.EXPECTED,'finishedUtc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
(run/'decoded/metadata-negative-tests.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary))
