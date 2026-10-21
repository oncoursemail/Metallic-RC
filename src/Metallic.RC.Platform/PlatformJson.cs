using System.Buffers;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Metallic.RC.Platform;

public static class PlatformJson
{
    public static JsonSerializerOptions Options { get; } = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = false,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
        RespectNullableAnnotations = true,
        MaxDepth = 32
    };

    public static async Task<T> ReadAsync<T>(HttpRequest request)
    {
        if (!request.HasJsonContentType())
            throw new PlatformFault(415, "json_required", "Send application/json.");
        // Read at most one byte beyond the API limit, including chunked requests.
        // This rejects oversized JSON before parsing and allows a useful 413 response.
        using var body = new MemoryStream();
        var buffer = ArrayPool<byte>.Shared.Rent(8192);
        try
        {
            while (true)
            {
                var remaining = PlatformOptions.MaxRequestBytes + 1 - (int)body.Length;
                var count = await request.Body.ReadAsync(buffer.AsMemory(0, Math.Min(buffer.Length, remaining)),
                    request.HttpContext.RequestAborted);
                if (count == 0) break;
                body.Write(buffer, 0, count);
                if (body.Length > PlatformOptions.MaxRequestBytes)
                    throw new PlatformFault(413, "request_size", "This request is larger than the local API limit.");
            }
        }
        finally { ArrayPool<byte>.Shared.Return(buffer); }
        using var document = JsonDocument.Parse(body.GetBuffer().AsMemory(0, (int)body.Length),
            new JsonDocumentOptions { MaxDepth = 32 });
        RejectDuplicateKeys(document.RootElement);
        return document.RootElement.Deserialize<T>(Options)
            ?? throw new PlatformFault(400, "body_required", "Provide a JSON request object.");
    }

    private static void RejectDuplicateKeys(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.Object)
        {
            var names = new HashSet<string>(StringComparer.Ordinal);
            foreach (var property in value.EnumerateObject())
            {
                if (!names.Add(property.Name))
                    throw new PlatformFault(400, "duplicate_property", "JSON property names must be unique.");
                RejectDuplicateKeys(property.Value);
            }
        }
        else if (value.ValueKind == JsonValueKind.Array)
            foreach (var item in value.EnumerateArray()) RejectDuplicateKeys(item);
    }
}
