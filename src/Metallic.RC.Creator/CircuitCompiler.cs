using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Metallic.RC.Creator;

internal sealed record InputBinding(string NodeId, string Port);
internal sealed record CompiledNode(
    string Id, string Kind, IReadOnlyDictionary<string, JsonElement> Config,
    IReadOnlyDictionary<string, InputBinding> Inputs, IReadOnlyDictionary<string, string> Next);
internal sealed record CompiledCircuit(string EntryNodeId, string Hash, IReadOnlyDictionary<string, CompiledNode> Nodes);
internal sealed record Compilation(ValidationResult Validation, CompiledCircuit? Program);

public static partial class CircuitCompiler
{
    [GeneratedRegex("^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$", RegexOptions.CultureInvariant)]
    private static partial Regex NodeIdPattern();
    [GeneratedRegex("^[A-Za-z_][A-Za-z0-9_.-]{0,63}$", RegexOptions.CultureInvariant)]
    private static partial Regex StateKeyPattern();

    public static bool IsStateKey(string? key) => key is not null && StateKeyPattern().IsMatch(key);
    public static ValidationResult Validate(CircuitGraph? graph) => Compile(graph).Validation;

    internal static Compilation Compile(CircuitGraph? graph)
    {
        var issues = new List<CircuitIssue>();
        var nodeCount = graph?.Nodes?.Count ?? 0;
        var edgeCount = graph?.Edges?.Count ?? 0;
        Compilation Failed() => new(new(false, issues, null, nodeCount, edgeCount), null);
        if (graph is null)
        {
            issues.Add(new("graph_required", "Provide a circuit graph."));
            return Failed();
        }
        if (graph.SchemaVersion != 1)
            issues.Add(new("schema_version", "Only circuit schema version 1 is supported."));
        if (graph.Nodes is null || graph.Edges is null)
        {
            issues.Add(new("graph_shape", "Nodes and edges must be arrays."));
            return Failed();
        }
        if (nodeCount is < 1 or > CreatorLimits.MaxNodes || edgeCount > CreatorLimits.MaxEdges)
        {
            issues.Add(new("graph_size", $"Use 1–{CreatorLimits.MaxNodes} nodes and at most {CreatorLimits.MaxEdges} edges."));
            return Failed();
        }

        var nodes = new Dictionary<string, CircuitNode>(StringComparer.Ordinal);
        var definitions = new Dictionary<string, NodeDefinition>(StringComparer.Ordinal);
        foreach (var node in graph.Nodes)
        {
            if (node is null)
            {
                issues.Add(new("null_node", "A node cannot be null."));
                continue;
            }
            if (node.Id is null || !NodeIdPattern().IsMatch(node.Id))
            {
                issues.Add(new("node_id", "Node IDs need 1–64 letters, digits, underscores or hyphens.", node.Id));
                continue;
            }
            if (!nodes.TryAdd(node.Id, node))
            {
                issues.Add(new("duplicate_node", "Node IDs must be unique.", node.Id));
                continue;
            }
            if (node.Kind is null || !NodeCatalog.ByKind.TryGetValue(node.Kind, out var definition))
            {
                issues.Add(new("unknown_kind", "Choose a node kind from the supported catalog.", node.Id));
                continue;
            }
            definitions[node.Id] = definition;
            ValidateConfig(node, definition, issues);
        }

        if (graph.EntryNodeId is null || !nodes.TryGetValue(graph.EntryNodeId, out var entry) || entry.Kind != "event.start")
            issues.Add(new("entry_node", "EntryNodeId must identify an event.start node.", graph.EntryNodeId));
        if (nodes.Values.Count(n => n.Kind == "event.start") != 1)
            issues.Add(new("start_count", "A graph must contain exactly one event.start node."));

        var incoming = new Dictionary<(string Node, string Port), List<CircuitEdge>>();
        var outgoing = new Dictionary<(string Node, string Port), List<CircuitEdge>>();
        var uniqueEdges = new HashSet<(string, string, string, string)>();
        var validEdges = new List<CircuitEdge>();
        foreach (var edge in graph.Edges)
        {
            if (edge is null || edge.FromNode is null || edge.ToNode is null || edge.FromPort is null || edge.ToPort is null)
            {
                issues.Add(new("edge_shape", "Each edge needs source and destination node IDs and port names."));
                continue;
            }
            if (!uniqueEdges.Add((edge.FromNode, edge.FromPort, edge.ToNode, edge.ToPort)))
            {
                issues.Add(new("duplicate_edge", "This connection already exists.", edge.ToNode, edge.ToPort));
                continue;
            }
            if (!definitions.TryGetValue(edge.FromNode, out var from) || !definitions.TryGetValue(edge.ToNode, out var to))
            {
                issues.Add(new("missing_node", "The connection refers to a missing or unsupported node.", edge.ToNode, edge.ToPort));
                continue;
            }
            var output = from.Outputs.FirstOrDefault(p => p.Name == edge.FromPort);
            var input = to.Inputs.FirstOrDefault(p => p.Name == edge.ToPort);
            if (output is null)
                issues.Add(new("output_port", "The source output port does not exist.", edge.FromNode, edge.FromPort));
            if (input is null)
                issues.Add(new("input_port", "The destination input port does not exist.", edge.ToNode, edge.ToPort));
            if (output is null || input is null) continue;
            if (output.Type != input.Type && !(input.Type == "value" && output.Type != "exec"))
            {
                issues.Add(new("port_type", $"Connect {input.Type} data to this port, not {output.Type}.", edge.ToNode, edge.ToPort));
                continue;
            }
            if (!incoming.TryGetValue((edge.ToNode, edge.ToPort), out var inc))
                incoming[(edge.ToNode, edge.ToPort)] = inc = [];
            if (!outgoing.TryGetValue((edge.FromNode, edge.FromPort), out var outs))
                outgoing[(edge.FromNode, edge.FromPort)] = outs = [];
            inc.Add(edge);
            outs.Add(edge);
            validEdges.Add(edge);
        }
        foreach (var (id, definition) in definitions)
        {
            foreach (var port in definition.Inputs)
            {
                var count = incoming.GetValueOrDefault((id, port.Name))?.Count ?? 0;
                if (port.Required && count == 0)
                    issues.Add(new("required_input", "Connect this required input.", id, port.Name));
                if (port.Type != "exec" && count > 1)
                    issues.Add(new("multiple_input", "A data input accepts one connection.", id, port.Name));
            }
            foreach (var port in definition.Outputs.Where(p => p.Type == "exec"))
                if ((outgoing.GetValueOrDefault((id, port.Name))?.Count ?? 0) > 1)
                    issues.Add(new("exec_fanout", "Use one next step per execution output.", id, port.Name));
        }

        // A topological pass rejects data and execution cycles before recursion or execution.
        var degrees = nodes.Keys.ToDictionary(k => k, _ => 0, StringComparer.Ordinal);
        var targets = nodes.Keys.ToDictionary(k => k, _ => new List<string>(), StringComparer.Ordinal);
        foreach (var edge in validEdges) { degrees[edge.ToNode]++; targets[edge.FromNode].Add(edge.ToNode); }
        var queue = new Queue<string>(degrees.Where(x => x.Value == 0).Select(x => x.Key).Order(StringComparer.Ordinal));
        var visited = 0;
        while (queue.TryDequeue(out var id))
        {
            visited++;
            foreach (var target in targets[id])
                if (--degrees[target] == 0) queue.Enqueue(target);
        }
        if (visited != nodes.Count)
            foreach (var id in degrees.Where(x => x.Value > 0).Select(x => x.Key).Order(StringComparer.Ordinal))
                issues.Add(new("cycle", "This node participates in or depends on a cycle. This preview supports acyclic graphs.", id));
        if (issues.Count > 0) return Failed();

        var hash = Convert.ToHexStringLower(SHA256.HashData(CanonicalBytes(graph)));
        var compiled = nodes.ToDictionary(kv => kv.Key, kv =>
        {
            var node = kv.Value;
            var dataInputs = new Dictionary<string, InputBinding>(StringComparer.Ordinal);
            foreach (var port in definitions[node.Id].Inputs.Where(p => p.Type != "exec"))
            {
                var edge = incoming[(node.Id, port.Name)][0];
                dataInputs[port.Name] = new(edge.FromNode, edge.FromPort);
            }
            var next = new Dictionary<string, string>(StringComparer.Ordinal);
            foreach (var port in definitions[node.Id].Outputs.Where(p => p.Type == "exec"))
                if (outgoing.TryGetValue((node.Id, port.Name), out var links))
                    next[port.Name] = links[0].ToNode;
            return new CompiledNode(node.Id, node.Kind,
                node.Config!.ToDictionary(x => x.Key, x => x.Value.Clone(), StringComparer.Ordinal), dataInputs, next);
        }, StringComparer.Ordinal);
        // The entry ID was checked above; no program is produced when validation fails.
        return new(new(true, [], hash, nodeCount, edgeCount), new(graph.EntryNodeId!, hash, compiled));
    }

    private static void ValidateConfig(CircuitNode node, NodeDefinition definition, List<CircuitIssue> issues)
    {
        if (node.Config is null)
        {
            issues.Add(new("config_shape", "Node config must be an object.", node.Id));
            return;
        }
        foreach (var key in node.Config.Keys)
            if (!definition.ConfigKeys.Contains(key, StringComparer.Ordinal))
                issues.Add(new("unknown_config", "This setting is not supported by the node.", node.Id, key));
        foreach (var key in definition.ConfigKeys)
            if (!node.Config.ContainsKey(key))
                issues.Add(new("missing_config", "Provide this node setting.", node.Id, key));
        if (node.Config.TryGetValue("value", out var value))
        {
            var valid = node.Kind switch
            {
                "constant.number" => value.ValueKind == JsonValueKind.Number && value.TryGetDouble(out var n) && double.IsFinite(n),
                "constant.boolean" => value.ValueKind is JsonValueKind.True or JsonValueKind.False,
                "constant.string" => value.ValueKind == JsonValueKind.String && value.GetString()!.Length <= CreatorLimits.MaxStringLength,
                _ => false
            };
            if (!valid)
                issues.Add(new("config_value", "Provide a finite number, Boolean, or bounded text matching this constant's type.", node.Id, "value"));
        }
        if (node.Config.TryGetValue("key", out var stateKey)
            && (stateKey.ValueKind != JsonValueKind.String || !IsStateKey(stateKey.GetString())))
            issues.Add(new("state_key", "Use a state key of 1–64 letters, digits, underscores, dots or hyphens, starting with a letter or underscore.", node.Id, "key"));
    }

    private static byte[] CanonicalBytes(CircuitGraph graph)
    {
        using var stream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(stream))
        {
            writer.WriteStartObject();
            writer.WriteNumber("schemaVersion", graph.SchemaVersion);
            writer.WriteString("entryNodeId", graph.EntryNodeId);
            writer.WriteStartArray("nodes");
            foreach (var node in graph.Nodes!.OrderBy(n => n.Id, StringComparer.Ordinal))
            {
                writer.WriteStartObject();
                writer.WriteString("id", node.Id);
                writer.WriteString("kind", node.Kind);
                writer.WriteStartObject("config");
                foreach (var (key, value) in node.Config!.OrderBy(p => p.Key, StringComparer.Ordinal))
                {
                    writer.WritePropertyName(key);
                    if (value.ValueKind == JsonValueKind.Number)
                    {
                        var number = value.GetDouble();
                        writer.WriteNumberValue(number == 0 ? 0 : number);
                    }
                    else value.WriteTo(writer);
                }
                writer.WriteEndObject();
                writer.WriteEndObject();
            }
            writer.WriteEndArray();
            writer.WriteStartArray("edges");
            foreach (var edge in graph.Edges!.OrderBy(e => e.FromNode, StringComparer.Ordinal)
                .ThenBy(e => e.FromPort, StringComparer.Ordinal).ThenBy(e => e.ToNode, StringComparer.Ordinal)
                .ThenBy(e => e.ToPort, StringComparer.Ordinal))
            {
                writer.WriteStartObject();
                writer.WriteString("fromNode", edge.FromNode);
                writer.WriteString("fromPort", edge.FromPort);
                writer.WriteString("toNode", edge.ToNode);
                writer.WriteString("toPort", edge.ToPort);
                writer.WriteEndObject();
            }
            writer.WriteEndArray();
            writer.WriteEndObject();
        }
        return stream.ToArray();
    }
}
