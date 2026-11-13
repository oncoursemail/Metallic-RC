"""Normalize the captured build-specific metadata, retaining every repair.
Offline only. Does not execute the game or write into either Steam installation.
"""
from pathlib import Path
import json, struct, hashlib, collections, datetime, sys
ROOT=Path(__file__).resolve().parent
RUN=ROOT/'runs/recovery-/20261020T212503.738773Z'
EXPECTED='5af86c2e051811bf84f56461af8757bc767855645aa4ef8985487a3333ac71e4'
# Region roles inferred from this capture, then validated below against contents.
# All indices refer to the unique, independently measured header tiling.
ROLES={'stringLiteral':30,'stringLiteralData':24,'string':1,'events':28,'properties':10,
 'methods':15,'parameterDefaultValues':11,'fieldDefaultValues':2,
 'fieldAndParameterDefaultValueData':9,'fieldMarshaledSizes':0,'parameters':19,'fields':18,
 'genericParameters':13,'genericParameterConstraints':12,'genericContainers':3,
 'nestedTypes':20,'interfaces':22,'vtableMethods':26,'interfaceOffsets':7,'typeDefinitions':4,
 'images':16,'assemblies':8,'fieldRefs':17,'referencedAssemblies':6,'attributeData':27,
 'attributeDataRange':25,'unresolvedVirtualCallParameterTypes':5,'unresolvedVirtualCallParameterRanges':14,
 'windowsRuntimeTypeNames':21,'windowsRuntimeStrings':23,'exportedTypeDefinitions':29}

def normalize(data,sol,emit=True):
    regions=sol['regions']; checks=[]; repairs=[]
    def check(name,ok,detail=None):
        if not ok: raise ValueError(name+': '+str(detail))
        checks.append({'check':name,'passed':True,'detail':detail})
    check('Exact capture size',len(data)==52403096)
    check('Unique role assignment',set(ROLES.values())==set(range(31)))
    check('Table regions tile source',regions[0]['offset']==256 and all(a['offset']+a['size']==b['offset'] for a,b in zip(regions,regions[1:])) and regions[-1]['offset']+regions[-1]['size']==len(data))
    def reg(role):return regions[ROLES[role]]
    def raw(role):q=reg(role);return memoryview(data)[q['offset']:q['offset']+q['size']]
    def records(role,fmt):
        check(role+' record stride',len(raw(role))%struct.calcsize(fmt)==0)
        return list(struct.iter_unpack(fmt,raw(role)))
    blob=bytes(raw('string'))
    cache={}
    def text(idx,empty=False):
        if idx not in cache:
            if not 0<=idx<len(blob):raise ValueError('String index out of bounds: '+str(idx))
            end=blob.find(b'\0',idx)
            if end<0:raise ValueError('Unterminated identifier')
            value=blob[idx:end].decode('utf-8','strict')
            if any(ord(c)<32 for c in value):raise ValueError('Control character in identifier')
            cache[idx]=value
        v=cache[idx]
        if not empty and not v:raise ValueError('Unexpected empty identifier')
        return v
    types=records('typeDefinitions','<16i8H2I');methods=records('methods','<IiiiiiI4H')
    fields=records('fields','<IiI');params=records('parameters','<IIi')
    props=records('properties','<IiiII');events=records('events','<IiiiiI')
    images=records('images','<IiiIiiiIiI');assemblies=records('assemblies','<16I')
    gps=records('genericParameters','<iIhhHH');gcs=records('genericContainers','<4i')
    attrs=records('attributeDataRange','<II');lits=records('stringLiteral','<II')
    nesting=records('nestedTypes','<i');exported=records('exportedTypeDefinitions','<i')
    refs=records('referencedAssemblies','<i')
    for role,rows,token_index,tag,empty in [('methods',methods,6,6,False),('fields',fields,2,4,False),('parameters',params,1,8,True),('properties',props,4,23,False),('events',events,5,20,False),('typeDefinitions',types,25,2,False)]:
        check(role+' all token tags',all((x[token_index]>>24)==tag and ((x[token_index]&0xffffff)!=0 or (role=='parameters' and text(x[0],empty=True)=='')) for x in rows),{'records':len(rows),'unnamedZeroRowTokens':sum((x[token_index]&0xffffff)==0 for x in rows)})
        for x in rows:text(x[0],empty=empty)
        checks.append({'check':role+' all identifier indices and UTF8','passed':True,'records':len(rows)})
    for t in types:text(t[1],empty=True)
    check('Image/assembly count equality',len(images)==len(assemblies),len(images))
    check('Image/assembly reciprocal indices',all(x[1]>=0 and x[1]<len(assemblies) and assemblies[x[1]][0]==i for i,x in enumerate(images)))
    check('Assembly tokens, including generated synthetic assembly',all((a[1]>>24)==0x20 and ((a[1]&0xffffff)>0 or text(a[4])=='__Generated') for a in assemblies))
    cursor=0;attrcursor=0
    for i,x in enumerate(images):
        nm=text(x[0]);a=assemblies[x[1]];an=text(a[4])
        check('Image '+str(i)+' identity/ranges',((nm==an+'.dll') or (nm==an=='__Generated')) and x[2]==cursor and x[2]+x[3]<=len(types) and x[8]==attrcursor)
        cursor=x[2]+x[3];attrcursor=x[8]+x[9]
        check('Image '+str(i)+' attribute range',attrcursor<len(attrs))
        tokens=[v[0] for v in attrs[x[8]:attrcursor]]
        check('Image '+str(i)+' attribute token uniqueness',len(tokens)==len(set(tokens)))
    check('Images partition type table',cursor==len(types))
    check('Images partition attributes plus sentinel',attrcursor+1==len(attrs))
    member_specs=[('fields',fields,8,18),('methods',methods,9,16),('events',events,10,19),('properties',props,11,17),('nestedTypes',nesting,12,20),('interfaces',None,13,22),('vtableMethods',None,14,21),('interfaceOffsets',None,15,23)]
    strides={'interfaces':4,'vtableMethods':4,'interfaceOffsets':8}
    for role,rr,si,ci in member_specs:
        count=len(rr) if rr is not None else len(raw(role))//strides[role]
        coverage=bytearray(count)
        for ti,t in enumerate(types):
            start,n=t[si],t[ci]
            checkok=(start>=0 and start+n<=count) or (start==-1 and n==0)
            if not checkok:raise ValueError(f'{role} range outside table at type {ti}')
            if n:
                if any(coverage[start:start+n]):raise ValueError(role+' overlapping ownership')
                coverage[start:start+n]=b'\1'*n
                if role=='methods' and any(m[1]!=ti for m in methods[start:start+n]):raise ValueError('Method declaring type mismatch')
                if role=='properties' and any(not(-1<=p[k]<t[16]) for p in props[start:start+n] for k in (1,2)):raise ValueError('Property local method index')
                if role=='events' and any(not(-1<=e[k]<t[16]) for e in events[start:start+n] for k in (2,3,4)):raise ValueError('Event local method index')
        check(role+' complete nonoverlapping type ownership',all(coverage),count)
    check('Nested type indices',all(0<=x[0]<len(types) for x in nesting))
    check('Exported type indices',all(-1<=x[0]<len(types) for x in exported))
    cov=bytearray(len(params))
    for i,m in enumerate(methods):
        start,n=m[4],m[10]
        if not ((start>=0 and start+n<=len(params)) or (start==-1 and n==0)):raise ValueError('Parameter range')
        if n:
            if any(cov[start:start+n]):raise ValueError('Overlapping parameters')
            cov[start:start+n]=b'\1'*n
    check('Methods partition parameter table',all(cov),len(params))
    # Locate corrupted module placeholders only at named assembly image starts.
    module_name=blob.find(b'<Module>\0')
    check('Module identifier exists',module_name>=0 and (module_name==0 or blob[module_name-1]==0))
    corrupted=set()
    for image in images:
        ti=image[2];t=types[ti]
        if t[6]>=len(gcs):
            check('Corrupt module shape '+str(ti),t[25]==0x02000001 and t[0]==0 and t[3]==-1 and t[4]==-1 and not any(t[17:24]) and t[16]<=1 and (t[16]==0 or text(methods[t[9]][0])=='.cctor') and text(image[0]).endswith('.dll'))
            corrupted.add(ti)
            repairs.extend([{'offset':reg('typeDefinitions')['offset']+ti*88,'typeIndex':ti,'field':'nameIndex','old':t[0],'new':module_name}, {'offset':reg('typeDefinitions')['offset']+ti*88+24,'typeIndex':ti,'field':'genericContainerIndex','old':t[6],'new':-1}])
    for i,t in enumerate(types):
        if i not in corrupted:check('Type generic index '+str(i),-1<=t[6]<len(gcs))
    cov=bytearray(len(gps))
    for i,(owner,n,ismethod,start) in enumerate(gcs):
        ownerrows=methods if ismethod else types
        ownercol=5 if ismethod else 6
        if not (ismethod in (0,1) and 0<=owner<len(ownerrows) and ownerrows[owner][ownercol]==i and n>0 and 0<=start<=len(gps)-n):raise ValueError('Generic container cross reference')
        if any(cov[start:start+n]):raise ValueError('Generic overlap')
        cov[start:start+n]=b'\1'*n
        for num,gp in enumerate(gps[start:start+n]):
            if gp[0]!=i or gp[4]!=num:raise ValueError('Generic parameter owner/number')
            text(gp[1])
    check('Generic container/parameter reciprocity and full coverage',all(cov),len(gps))
    for m in methods:
        if not -1<=m[5]<len(gcs):raise ValueError('Method generic container')
    constraint_count=len(raw('genericParameterConstraints'))//4
    ranges=[(p[2],p[3]) for p in gps]
    check('Generic constraints bounds',all((a>=0 and a+b<=constraint_count) or(a==-1 and b==0) for a,b in ranges))
    check('Generic constraints exact end',max(a+b for a,b in ranges)==constraint_count)
    refcursor=0
    for a in assemblies:
        if a[2]==0xffffffff and a[3]==0:continue
        if a[2]!=refcursor:raise ValueError('Assembly references not a partition')
        refcursor=a[2]+a[3]
    check('Referenced assembly partition',refcursor==len(refs))
    check('All referenced assemblies in bounds',all(0<=x[0]<len(assemblies) for x in refs))
    for role,limit in [('fieldDefaultValues',len(fields)),('parameterDefaultValues',len(params)),('fieldMarshaledSizes',len(fields))]:
        rr=records(role,'<iii')
        check(role+' sorted unique member indices',all(0<=x[0]<limit for x in rr) and [x[0] for x in rr]==sorted(set(x[0] for x in rr)))
        check(role+' native type indices nonnegative',all(x[1]>=0 for x in rr))
        check(role+' data/size bounds',all((0<=x[2]<=256) if role=='fieldMarshaledSizes' else (-1<=x[2]<len(raw('fieldAndParameterDefaultValueData'))) for x in rr))
    cursor=0
    for n,o in lits:
        if o!=cursor:raise ValueError('Literal data not exact concatenation')
        cursor=o+n
    check('Literal data exact end with alignment padding',0<=len(raw('stringLiteralData'))-cursor<4 and len(raw('stringLiteralData'))%4==0,{'meaningfulBytes':cursor,'paddingBytes':len(raw('stringLiteralData'))-cursor})
    check('Attribute offsets monotonic',all(a[1]<=b[1] for a,b in zip(attrs,attrs[1:])))
    check('Attribute sentinel plus alignment',0<=len(raw('attributeData'))-attrs[-1][1]<4,{'meaningfulBytes':attrs[-1][1],'paddingBytes':len(raw('attributeData'))-attrs[-1][1]})
    uv=records('unresolvedVirtualCallParameterRanges','<ii');cursor=0
    for a,n in uv:
        if a!=cursor or n<0:raise ValueError('Unresolved call range partition')
        cursor=a+n
    check('Unresolved call parameter range coverage',cursor*4==len(raw('unresolvedVirtualCallParameterTypes')))
    check('Empty WinRT tables',len(raw('windowsRuntimeTypeNames'))==len(raw('windowsRuntimeStrings'))==0)
    check('Field references nonnegative',all(a>=0 and b>=0 for a,b in records('fieldRefs','<ii')))
    result=bytearray(data)
    header=[0xFAB11BAF,31]
    for role in ROLES:
        q=reg(role);header.extend((q['offset'],q['size']))
    struct.pack_into('<64I',result,0,*header)
    for p in repairs:struct.pack_into('<I',result,p['offset'],p['new']&0xffffffff)
    expected_changed=set(range(256))
    for p in repairs:expected_changed.update(range(p['offset'],p['offset']+4))
    actual_changed=[i for i,(a,b) in enumerate(zip(data,result)) if a!=b]
    check('Only header and listed module fields changed',set(actual_changed)<=expected_changed)
    counts={'images':len(images),'assemblies':len(assemblies),'types':len(types),'methods':len(methods),'fields':len(fields),'parameters':len(params),'properties':len(props),'events':len(events),'genericContainers':len(gcs),'genericParameters':len(gps),'stringLiterals':len(lits),'attributeRanges':len(attrs),'modulePlaceholderRepairs':len(corrupted)}
    summary={'schemaVersion':1,'sourceSha256':hashlib.sha256(data).hexdigest(),'normalizedSha256':hashlib.sha256(result).hexdigest(),'metadataVersion':31,'tableCount':31,'counts':counts,'checksPassed':len(checks),'headerMapping':{k:reg(k) for k in ROLES},'repairs':repairs,'changedByteCount':len(actual_changed),'validationScope':'Structural metadata table identities, record strides, token tags, UTF8 identifiers and reciprocal ownership/ranges. Not original C# source or independently verified native registration.','limits':['Two empty WinRT regions have no content identity; exchanging their empty offsets is semantically equivalent.','Default-value and attribute blob bytecode semantics are not executed here.','Native type indices and method code pointers require separate binary registration checks.','Independent upstream parser result is separate.']}
    if emit:
        out=RUN/'decoded';out.mkdir(exist_ok=False)
        (out/'global-metadata.v31.normalized.dat').write_bytes(result)
        (out/'metadata-normalization.json').write_text(json.dumps(summary,indent=2)+'\n')
        # Compact per-assembly structure index, without executable payloads.
        items=[]
        for i,img in enumerate(images):
            tt=types[img[2]:img[2]+img[3]]
            items.append({'imageIndex':i,'name':text(img[0]),'typeStart':img[2],'typeCount':img[3],'methodCount':sum(t[16] for t in tt),'fieldCount':sum(t[18] for t in tt)})
        (out/'assembly-index.json').write_text(json.dumps(items,indent=2)+'\n')
    return summary,result

if __name__=='__main__':
    c=json.loads((RUN/'capture/capture-report.json').read_text());item=next(x for x in c['artifacts'] if x['kind']=='metadata_candidate')
    assert item['coverage']['complete'] and c['metadataReadiness']['stableAcrossReads']
    data=(RUN/'capture'/item['file']).read_bytes();assert hashlib.sha256(data).hexdigest()==EXPECTED
    analysis=json.loads((RUN/'metadata-structural-inspection.json').read_text())
    assert analysis['headerSearch']['searchComplete'] and len(analysis['headerSearch']['solutions'])==1
    sol=analysis['headerSearch']['solutions'][0]
    summary,_=normalize(data,sol)
    print(json.dumps({k:summary[k] for k in ('sourceSha256','normalizedSha256','counts','checksPassed','changedByteCount')}))
