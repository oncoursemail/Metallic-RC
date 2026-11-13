using System.Text.Json;

namespace Metallic.RC.Creator;

public sealed record CircuitGraph
{
    public int SchemaVersion { get; init; } = 1;
    public string EntryNodeId { get; init; } = "";
    public List<CircuitNode>? Nodes { get; init; } = [];
    public List<CircuitEdge>? Edges { get; init; } = [];

    public static CircuitGraph Empty() => new()
    {
        EntryNodeId = "start",
        Nodes = [new() { Id = "start", Kind = "event.start" }]
    };
}

public sealed record CircuitNode
{
    public string Id { get; init; } = "";
    public string Kind { get; init; } = "";
    public Dictionary<string, JsonElement>? Config { get; init; } = new(StringComparer.Ordinal);
}

public sealed record CircuitEdge
{
    public string FromNode { get; init; } = "";
    public string FromPort { get; init; } = "";
    public string ToNode { get; init; } = "";
    public string ToPort { get; init; } = "";
}

public sealed record CircuitIssue(string Code, string Message, string? NodeId = null, string? Port = null);
public sealed record ValidationResult(
    bool Valid, IReadOnlyList<CircuitIssue> Issues, string? ProgramHash, int NodeCount, int EdgeCount);
public sealed record CircuitPort(string Name, string Type, bool Required = false);
public sealed record NodeDefinition(
    string Kind, string Name, IReadOnlyList<CircuitPort> Inputs,
    IReadOnlyList<CircuitPort> Outputs, IReadOnlyList<string> ConfigKeys);
public sealed record PreviewRequest
{
    public CircuitGraph? Graph { get; init; }
    public int MaxSteps { get; init; } = 512;
    public Dictionary<string, double>? InitialState { get; init; }
}
public sealed record PreviewLog(string NodeId, string ValueType, object Value, string Text);
public sealed record PreviewResult(
    bool Succeeded, string? ProgramHash, int Steps, IReadOnlyList<PreviewLog> Logs,
    IReadOnlyDictionary<string, double> State, IReadOnlyList<CircuitIssue> Issues);

public static class CreatorLimits
{
    public const int MaxNodes = 256;
    public const int MaxEdges = 1024;
    public const int MaxSteps = 4096;
    public const int MaxDataDepth = 64;
    public const int MaxStringLength = 4096;
    public const int MaxInitialStateKeys = 128;
    public const int MaxStateKeys = 256;
}
