"""Check real owned discovery HTTP bytes against captured formatter instructions.
Requires the exact private, hash-bound NativeFormatter workspace. This does not launch
or authenticate the original game. Its parser/allocator/GC callbacks remain modeled.
"""
from __future__ import annotations
import argparse, datetime, hashlib, importlib.util, json, pathlib, sys, urllib.request, urllib.parse

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl): return None

def main() -> None:
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--probe-dir',required=True,type=pathlib.Path)
    p.add_argument('--origin',default='http://127.0.0.1:4187/')
    p.add_argument('--output',required=True,type=pathlib.Path)
    args=p.parse_args();url=urllib.parse.urlsplit(args.origin)
    if url.scheme!='http' or url.hostname!='127.0.0.1' or not url.port or url.port<1024 or url.path not in ('','/') or url.username or url.password or url.query or url.fragment:
        raise ValueError('An explicit loopback-only origin is required.')
    args.output.mkdir(parents=True,exist_ok=False)
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
    with opener.open(urllib.parse.urljoin(args.origin,'nameserver?buildId=23191908'),timeout=5) as response:
        if response.status!=200 or response.headers.get_content_type()!='application/json':raise ValueError('Not a discovery JSON response.')
        body=response.read(1_048_577)
        if len(body)>1_048_576:raise ValueError('Response exceeds limit.')
    pairs=json.loads(body,object_pairs_hook=lambda items:items)
    if not isinstance(pairs,list) or len(pairs)!=32:raise ValueError('Expected 32 discovery fields.')
    probe=args.probe_dir/'native_contract_probe.py'
    spec=importlib.util.spec_from_file_location('private_native_formatter',probe)
    if not spec or not spec.loader:raise RuntimeError('Probe import unavailable.')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    formatter=module.NativeFormatter()
    assert [name for name,value in pairs]==formatter.canonical
    assert pairs[0][1] is None
    for name,value in pairs[1:]:
        parsed=urllib.parse.urlsplit(value)
        assert parsed.scheme=='http' and parsed.hostname=='127.0.0.1' and parsed.port==url.port and parsed.path.startswith('/recnet/')
    read=formatter.deserialize([{'name':name,'value':value} for name,value in pairs])
    assert read['status']=='ok' and read['fields']==[value for name,value in pairs] and read['cursor']==32 and read['skips']==0
    write=formatter.serialize(read['fields'])
    assert write['status']=='ok' and json.loads(write['json'],object_pairs_hook=lambda items:items)==pairs
    malformed=[{'name':name,'value':value}for name,value in pairs];malformed[2]['value']=123
    rejected=formatter.deserialize(malformed);assert rejected['status']=='known_value_type_error'
    (args.output/'discovery-response.json').write_bytes(body)
    report={'status':'live_http_discovery_matches_captured_formatter_profile','createdUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'steamBuildId':'23191908','sourceImageSha256':module.SHA,'httpResponseSha256':hashlib.sha256(body).hexdigest(),'httpBytes':len(body),'fields':32,'serviceAddresses':31,
            'nativeConstructorInstructions':formatter.constructor_steps,'nativeReadInstructions':read['instructions'],'nativeWriteInstructions':write['instructions'],
            'orderedFieldValuesMatched':True,'wrongTypeNegativeControlRejected':True,'nativeFormatterInstructionBytesEmulated':True,'actualOriginalGameExecuted':False,
            'originalClientLoginVerified':False,'modeledBoundaries':['JSON tokenization','metadata resolution','allocation','dictionary lookup','garbage collection barriers','string encoding'],
            'interpretation':'The live backend response satisfies the captured generated formatter profile. This is not original-client network, login, or gameplay acceptance.'}
    (args.output/'summary.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report))

if __name__=='__main__':main()
