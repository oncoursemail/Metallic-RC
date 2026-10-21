using System.Text;using System.Text.Json;using Metallic.Reconstruction;
if(args.Length==2 && args[0]=="--self-test"){await ContractSelfTests.RunAsync(args[1]);return;}
if(args.Length is 3 or 4 && args[0]=="--cleanup-probe"){CleanupProbe.Run(args[1],args[2],args.Length==4?args[3]:null);return;}
if(args.Length!=2&&args.Length!=3)throw new ArgumentException("StartupContract cases.jsonl NEW-results.jsonl [test-mutant]");
using var output=new StreamWriter(new FileStream(args[1],FileMode.CreateNew));int count=0;
foreach(string line in File.ReadLines(args[0])){
    using var doc=JsonDocument.Parse(line);var row=doc.RootElement;int id=row.GetProperty("id").GetInt32();string operation=row.GetProperty("operation").GetString()!;
    string status="ok";string?[]? fields=null;string? json=null;
    try{
        if(operation=="read"){
            var response=DiscoveryContract.ReadCore(Encoding.UTF8.GetBytes(row.GetProperty("document").GetString()!),args.Length==3?args[2]:null);fields=response?.Values;
        }else if(operation=="write"){
            var f=row.GetProperty("fields");DiscoveryResponse? response=f.ValueKind==JsonValueKind.Null?null:new DiscoveryResponse();
            if(response is not null){int i=0;foreach(var value in f.EnumerateArray()){if(i>=32)throw new InvalidDataException("Too many fields.");response.Values[i++]=value.ValueKind==JsonValueKind.Null?null:value.GetString();}if(i!=32)throw new InvalidDataException("Expected 32 fields.");}
            json=Encoding.UTF8.GetString(DiscoveryContract.Write(response));
        }else throw new InvalidDataException("Unknown operation.");
    }catch(KnownValueTypeException){status="known_value_type_error";}catch(JsonException){status="json_error";}
    output.WriteLine(JsonSerializer.Serialize(new{id,status,fields,json}));if(++count>50000)throw new InvalidDataException("Case limit.");
}
Console.WriteLine(JsonSerializer.Serialize(new{status="completed",cases=count,originalGameExecuted=false}));
