using System.Text.Json;
namespace Metallic.Reconstruction;

// Reconstructed ordering at the omitted cleanup block only. This is NOT a
// replacement implementation of the native exception dispatcher or unwinder.
public static class ObservedInitializationCleanup
{
    public static void Complete(ulong enumerator,ulong metadata,ulong savedException,
                                Action<ulong,ulong> dispose,Action<ulong> propagate)
    {
        dispose(enumerator,metadata);
        if(savedException!=0)propagate(savedException);
    }
}

public static class CleanupProbe
{
    public static void Run(string input,string output,string? mutant)
    {
        int count=0;using var writer=new StreamWriter(new FileStream(output,FileMode.CreateNew));
        foreach(string line in File.ReadLines(input)){
            using var json=JsonDocument.Parse(line);var c=json.RootElement;
            int id=c.GetProperty("id").GetInt32();ulong enumerator=c.GetProperty("enumerator").GetUInt64(),metadata=c.GetProperty("metadata").GetUInt64(),error=c.GetProperty("error").GetUInt64();
            var events=new List<object>();string status="return";
            void Dispose(ulong target,ulong context){if(mutant!="skip-cleanup")events.Add(new{operation="dispose",target,metadata=context});}
            void Propagate(ulong target){if(mutant!="drop-error"){events.Add(new{operation="propagate",target,metadata=0UL});status="propagate";}}
            ObservedInitializationCleanup.Complete(enumerator,metadata,error,Dispose,Propagate);
            writer.WriteLine(JsonSerializer.Serialize(new{id,status,events}));if(++count>10000)throw new InvalidDataException("Bound exceeded.");
        }
        Console.WriteLine(JsonSerializer.Serialize(new{status="completed",cases=count,nativeUnwindReimplemented=false}));
    }
}
