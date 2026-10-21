using System.Text.Json;
using Metallic.RC.Creator;

namespace Metallic.RC.Platform;

public sealed record RoomDraft
{
    public string Id { get; init; } = "";
    public string Name { get; init; } = "";
    public string Description { get; init; } = "";
    public long Revision { get; init; }
    public DateTimeOffset CreatedAt { get; init; }
    public DateTimeOffset UpdatedAt { get; init; }
    public CircuitGraph Graph { get; init; } = CircuitGraph.Empty();
}
public sealed record RoomSummary(string Id, string Name, string Description,
    long Revision, DateTimeOffset CreatedAt, DateTimeOffset UpdatedAt);
public sealed record RoomWriteRequest
{
    public string Name { get; init; } = "";
    public string Description { get; init; } = "";
    public CircuitGraph? Graph { get; init; }
}

public sealed class RoomStore : IDisposable
{
    private const int MaxRooms = 500;
    private const int MaxFileBytes = 524288;
    private readonly string directory;
    private readonly FileStream lease;
    private readonly object gate = new();
    public RoomStore(string dataDirectory)
    {
        var root = Path.GetFullPath(dataDirectory);
        PrivateFiles.CreateDirectory(root);
        lease = new FileStream(Path.Combine(root, "platform.lock"), FileMode.OpenOrCreate,
            FileAccess.ReadWrite, FileShare.None);
        directory = Path.Combine(root, "rooms");
        try
        {
            PrivateFiles.CreateDirectory(directory);
            var files = Directory.GetFiles(directory, "*.json");
            if (files.Length > MaxRooms) throw new IOException("Local draft capacity exceeded.");
            foreach (var file in files)
            {
                var id = Path.GetFileNameWithoutExtension(file);
                if (!IsId(id)) throw new IOException("Invalid draft filename.");
                _ = ReadExisting(id);
            }
        }
        catch { lease.Dispose(); throw; }
    }

    public IReadOnlyList<RoomSummary> List()
    {
        lock (gate)
            return Directory.GetFiles(directory, "*.json")
                .Select(file => ReadExisting(Path.GetFileNameWithoutExtension(file)))
                .OrderByDescending(room => room.UpdatedAt).ThenBy(room => room.Id, StringComparer.Ordinal)
                .Select(room => new RoomSummary(room.Id, room.Name, room.Description,
                    room.Revision, room.CreatedAt, room.UpdatedAt)).ToArray();
    }

    public RoomDraft Get(string id)
    {
        lock (gate) return ReadExisting(id);
    }

    public RoomDraft Create(RoomWriteRequest request)
    {
        ValidateWrite(request, false);
        lock (gate)
        {
            if (Directory.EnumerateFiles(directory, "*.json").Take(MaxRooms).Count() >= MaxRooms)
                throw new PlatformFault(409, "draft_capacity", "The local draft store has reached its room limit.");
            var now = DateTimeOffset.UtcNow;
            var room = new RoomDraft
            {
                Id = Guid.NewGuid().ToString("N"), Name = request.Name.Trim(), Description = request.Description,
                Revision = 1, CreatedAt = now, UpdatedAt = now, Graph = request.Graph ?? CircuitGraph.Empty()
            };
            WriteAtomic(room);
            return room;
        }
    }

    public RoomDraft Update(string id, long expectedRevision, RoomWriteRequest request)
    {
        ValidateWrite(request, true);
        lock (gate)
        {
            var current = ReadExisting(id);
            MatchRevision(current, expectedRevision);
            if (current.Revision == long.MaxValue)
                throw new PlatformFault(409, "revision_limit", "The draft revision limit has been reached.");
            var next = current with
            {
                Name = request.Name.Trim(), Description = request.Description, Graph = request.Graph!,
                Revision = current.Revision + 1, UpdatedAt = DateTimeOffset.UtcNow
            };
            WriteAtomic(next);
            return next;
        }
    }

    public void Delete(string id, long expectedRevision)
    {
        lock (gate)
        {
            MatchRevision(ReadExisting(id), expectedRevision);
            File.Delete(FilePath(id));
        }
    }

    private static void MatchRevision(RoomDraft room, long revision)
    {
        if (revision != room.Revision)
            throw new PlatformFault(412, "revision_conflict", "This draft changed. Reload it before saving or deleting.");
    }

    private static void ValidateWrite(RoomWriteRequest request, bool requireGraph)
    {
        if (string.IsNullOrWhiteSpace(request.Name) || request.Name.Length > 120)
            throw new PlatformFault(400, "room_name", "Use a room name of 1–120 characters.");
        if (request.Description is null || request.Description.Length > 4000)
            throw new PlatformFault(400, "room_description", "Use a description of at most 4,000 characters.");
        if (requireGraph && request.Graph is null)
            throw new PlatformFault(400, "graph_required", "Include the current graph when updating a draft.");
        if (request.Graph is { } graph && (graph.SchemaVersion != 1 || graph.Nodes is null || graph.Edges is null
            || graph.Nodes.Count > CreatorLimits.MaxNodes || graph.Edges.Count > CreatorLimits.MaxEdges))
            throw new PlatformFault(400, "graph_shape", "Use schema version 1 within the documented graph limits.");
    }

    private RoomDraft ReadExisting(string id)
    {
        if (!IsId(id)) throw new PlatformFault(404, "room_not_found", "This local room draft does not exist.");
        var path = FilePath(id);
        if (!File.Exists(path)) throw new PlatformFault(404, "room_not_found", "This local room draft does not exist.");
        var info = new FileInfo(path);
        if ((info.Attributes & FileAttributes.ReparsePoint) != 0 || info.Length > MaxFileBytes)
            throw new IOException("Invalid local draft file.");
        try
        {
            var room = JsonSerializer.Deserialize<RoomDraft>(File.ReadAllBytes(path), PlatformJson.Options);
            if (room is null || room.Id != id || room.Revision < 1 || room.Graph is null)
                throw new IOException("Invalid local draft data.");
            ValidateWrite(new() { Name = room.Name, Description = room.Description, Graph = room.Graph }, true);
            return room;
        }
        catch (JsonException error) { throw new IOException("Invalid local draft data.", error); }
        catch (PlatformFault error) { throw new IOException("Invalid local draft data.", error); }
    }

    private void WriteAtomic(RoomDraft room)
    {
        var bytes = JsonSerializer.SerializeToUtf8Bytes(room, PlatformJson.Options);
        if (bytes.Length > MaxFileBytes)
            throw new PlatformFault(413, "draft_size", "This draft is too large for local storage.");
        var target = FilePath(room.Id);
        if (File.Exists(target) && (File.GetAttributes(target) & FileAttributes.ReparsePoint) != 0)
            throw new IOException("Invalid local draft destination.");
        var temp = Path.Combine(directory, "." + room.Id + "." + Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            using (var stream = PrivateFiles.Create(temp)) { stream.Write(bytes); stream.Flush(true); }
            File.Move(temp, target, true);
        }
        finally { if (File.Exists(temp)) File.Delete(temp); }
    }

    private static bool IsId(string? id) => id is not null && Guid.TryParseExact(id, "N", out _);
    private string FilePath(string id) => Path.Combine(directory, id + ".json");
    public void Dispose() => lease.Dispose();
}
