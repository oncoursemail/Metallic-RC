using System.Text.Json;
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
