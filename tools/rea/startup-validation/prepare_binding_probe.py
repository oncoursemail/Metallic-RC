"""Create a limited C# model of the observed service-binding call sequence.
It is not the async startup method, JSON wire decoder, or a network service.
"""
from pathlib import Path
import json,hashlib,subprocess,os
ROOT=Path(__file__).resolve().parent
DEEP=ROOT.parent/'deep-rea-20261021'
report=json.loads((DEEP/'findings/name-server-bindings.json').read_text())
rows=report['rows'];ordered=sorted(rows,key=lambda r:int(r['callAddress'],16))
assert len(rows)==31 and [r['serviceId'] for r in ordered]!=list(range(31))
OUT=ROOT/'BindingProbe';OUT.mkdir(exist_ok=False)
header='// Authored behavioral reconstruction from build 23191908; NOT original source.\n// Only the observed response-field to helper-call sequence is represented.\n// The supplied callback models the service updater; no network or engine work.\n'
lines=[header,'namespace Metallic.Reconstruction;','public sealed class ObservedServiceRoutes','{']
lines += ['    public string? '+r['serviceName']+' { get; init; }' for r in rows]
lines += ['}','public static class ObservedServiceBindingSequence','{','    public static void Apply(ObservedServiceRoutes routes, Action<int,string?> update)','    {']
lines += [f"        update({r['serviceId']}, routes.{r['serviceName']});" for r in ordered]
lines += ['    }','}']
(OUT/'ObservedServiceBindingSequence.cs').write_text('\n'.join(lines)+'\n',encoding='utf-8',newline='\n')
(OUT/'BindingProbe.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable><Deterministic>true</Deterministic></PropertyGroup></Project>\n',encoding='utf-8')
program='''using System.Text.Json;
using Metallic.Reconstruction;
if(args.Length is <2 or >3) throw new ArgumentException("BindingProbe input.jsonl NEW-output.jsonl [sorted-mutant]");
using var output=new StreamWriter(new FileStream(args[1],FileMode.CreateNew));int count=0;
foreach(string line in File.ReadLines(args[0])) {
    using var j=JsonDocument.Parse(line);var input=j.RootElement;
    var values=input.GetProperty("values").EnumerateArray().Select(x=>x.GetString()).ToArray();
    if(values.Length!=31 || ++count>10000) throw new InvalidDataException("Fixture bounds");
    int id=input.GetProperty("id").GetInt32(),failAt=input.GetProperty("failAt").GetInt32();
    var routes=new ObservedServiceRoutes {
ASSIGNMENTS
    };
    var trace=new List<object>();string status="completed";
    void Update(int service,string? value) {
        int index=trace.Count;trace.Add(new {serviceId=service,value});
        if(index==failAt) throw new DeliberateCallbackFailure();
    }
    try {
        if(args.Length==3 && args[2]=="sorted-mutant") {
            // Deliberate wrong reconstruction used to test whether order matters.
            for(int service=0;service<31;service++) Update(service,values[service]);
        } else ObservedServiceBindingSequence.Apply(routes,Update);
    } catch(DeliberateCallbackFailure) {status="callback_stopped";}
    output.WriteLine(JsonSerializer.Serialize(new {id,status,trace}));
}
Console.WriteLine(JsonSerializer.Serialize(new {status="completed",cases=count,gameLoaded=false}));
sealed class DeliberateCallbackFailure:Exception {}
'''.replace('ASSIGNMENTS','\n'.join(f"        {r['serviceName']}=values[{r['serviceId']}]," for r in rows))
(OUT/'Program.cs').write_text(program,encoding='utf-8',newline='\n')
dotnet=r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\dotnet-10.0.402\dotnet.exe';env=os.environ.copy();env.update(DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_NOLOGO='1')
p=subprocess.run([dotnet,'build',str(OUT/'BindingProbe.csproj'),'-c','Release','--nologo'],capture_output=True,text=True,timeout=90,env=env)
(ROOT/'binding-probe-build.log').write_text(p.stdout+p.stderr,encoding='utf-8');assert p.returncode==0,p.stdout+p.stderr
manifest={'status':'built','methodIndex':148260,'helperMethodIndex':148123,'bindingEvidenceId':report['sourceEvidenceId'],'orderedServiceIds':[r['serviceId'] for r in ordered],'modelSha256':hashlib.sha256((OUT/'ObservedServiceBindingSequence.cs').read_bytes()).hexdigest(),'inputs':['31 observed response fields','typed service enum','REA verified call-site sequence'],'dotnet':dotnet,'program':str(OUT/'bin/Release/net10.0/BindingProbe.dll'),'isFullMethodReconstruction':False}
(ROOT/'binding-probe-build.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(manifest),flush=True)
