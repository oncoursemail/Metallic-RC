from pathlib import Path
import hashlib,json,subprocess,os
ROOT=Path(__file__).resolve().parent
OUT=ROOT/'ManagedProbe';OUT.mkdir(exist_ok=False)
source=ROOT.parent/'deep-rea-20261021/RecoveredCodec/RecoveredCodec.cs'
code=source.read_bytes();(OUT/'ObservedByteCodec.cs').write_bytes(code)
(OUT/'ManagedProbe.csproj').write_text('''<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable><CheckForOverflowUnderflow>false</CheckForOverflowUnderflow><Deterministic>true</Deterministic></PropertyGroup></Project>\n''',encoding='utf-8')
(OUT/'Program.cs').write_text('''using System.Text.Json;
using Metallic.Reconstruction;
if (args.Length is < 2 or > 3) throw new ArgumentException("ManagedProbe cases.jsonl NEW-results.jsonl [upfront-mutant]");
using var output = new StreamWriter(new FileStream(args[1], FileMode.CreateNew));
int count=0;
foreach (string line in File.ReadLines(args[0])) {
    using var json=JsonDocument.Parse(line); var row=json.RootElement;
    int id=row.GetProperty("id").GetInt32(), index=row.GetProperty("index").GetInt32();
    string operation=row.GetProperty("operation").GetString()!;
    string? encoded=row.GetProperty("bytesHex").GetString();
    byte[]? bytes=encoded is null?null:Convert.FromHexString(encoded);
    int value=row.GetProperty("value").GetInt32();
    if(bytes?.Length>4096 || ++count>30000) throw new InvalidDataException("Fixture bound exceeded");
    string status="ok";int? result=null;string? exception=null;
    try {
        if(operation=="read") result=ObservedByteCodec.ReadInt32(bytes,index);
        else if(operation=="write") {
            // Deliberate negative-control rewrite: it must NOT match partial stores.
            if(args.Length==3 && args[2]=="upfront-mutant" && bytes is not null &&
               (index<0 || index>bytes.Length-4)) throw new IndexOutOfRangeException();
            ObservedByteCodec.WriteInt32(bytes!,index,value);
        } else throw new InvalidDataException("Unknown operation");
    } catch(IndexOutOfRangeException e) {status="bounds";exception=e.GetType().FullName;}
      catch(NullReferenceException e) {status="null";exception=e.GetType().FullName;}
    output.WriteLine(JsonSerializer.Serialize(new {id,status,value=result,bytesHex=bytes is null?null:Convert.ToHexStringLower(bytes),exceptionType=exception}));
}
Console.WriteLine(JsonSerializer.Serialize(new {status="completed",cases=count,originalNativeCodeExecuted=false}));
''',encoding='utf-8',newline='\n')
dotnet=r'C:\Users\alexi\AppData\Local\Metallic-RC\tooling\dotnet-10.0.402\dotnet.exe'
env=os.environ.copy();env.update(DOTNET_CLI_TELEMETRY_OPTOUT='1',DOTNET_NOLOGO='1')
p=subprocess.run([dotnet,'build',str(OUT/'ManagedProbe.csproj'),'-c','Release','--nologo','-v','minimal'],capture_output=True,text=True,timeout=90,env=env)
(ROOT/'managed-build.log').write_text(p.stdout+p.stderr,encoding='utf-8')
assert p.returncode==0,p.stdout+p.stderr
manifest={'status':'built','codecSourceSha256':hashlib.sha256(code).hexdigest(),'codecSourceCopiedFrom':str(source),'projectFiles':[{ 'file':p.name,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in OUT.iterdir() if p.is_file()],'dotnet':dotnet,'output':str(OUT/'bin/Release/net10.0/ManagedProbe.dll'),'gameAssembliesLoaded':False}
(ROOT/'managed-probe-build.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(manifest),flush=True)
