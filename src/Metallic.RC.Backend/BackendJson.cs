using System.Text.Json;
using System.Text.Json.Serialization;

namespace Metallic.RC.Backend;
public static class BackendJson
{
    public static readonly JsonSerializerOptions Options=new(JsonSerializerDefaults.Web)
    {MaxDepth=16,UnmappedMemberHandling=JsonUnmappedMemberHandling.Disallow,PropertyNameCaseInsensitive=false};
    public static async Task<T> Read<T>(HttpRequest request)
    {
        if(!request.HasJsonContentType())throw new BackendFault(415,"json_required");
        using var bounded=new CancellationTokenSource(TimeSpan.FromSeconds(10));
        using var cancellation=CancellationTokenSource.CreateLinkedTokenSource(bounded.Token,request.HttpContext.RequestAborted);
        using var stream=new MemoryStream();byte[] buffer=new byte[8192];int count;
        while((count=await request.Body.ReadAsync(buffer,cancellation.Token))!=0)
        {
            if(stream.Length+count>BackendOptions.MaxRequestBytes)throw new BackendFault(413,"request_too_large");
            stream.Write(buffer,0,count);
        }
        byte[] bytes=stream.ToArray();RejectDuplicateProperties(bytes);
        return JsonSerializer.Deserialize<T>(bytes,Options)??throw new JsonException();
    }
    public static void RejectDuplicateProperties(ReadOnlySpan<byte> bytes)
    {
        var reader=new Utf8JsonReader(bytes,new JsonReaderOptions{MaxDepth=16});
        var stack=new Stack<HashSet<string>>();
        while(reader.Read())
        {
            if(reader.TokenType==JsonTokenType.StartObject)stack.Push(new(StringComparer.Ordinal));
            else if(reader.TokenType==JsonTokenType.EndObject)stack.Pop();
            else if(reader.TokenType==JsonTokenType.PropertyName&&!stack.Peek().Add(reader.GetString()!))throw new JsonException("Duplicate field.");
        }
    }
    public static long Revision(HttpRequest request)
    {
        string text=request.Headers.IfMatch.ToString();
        if(text.Length==0)throw new BackendFault(428,"revision_required");
        if(text.Length<4||!text.StartsWith("\"r",StringComparison.Ordinal)||!text.EndsWith('"')||
           !long.TryParse(text.AsSpan(2,text.Length-3),System.Globalization.NumberStyles.None,System.Globalization.CultureInfo.InvariantCulture,out long value)||value<1||text!="\"r"+value.ToString(System.Globalization.CultureInfo.InvariantCulture)+"\"")
            throw new BackendFault(400,"invalid_revision");
        return value;
    }
}
