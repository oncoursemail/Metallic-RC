namespace Metallic.RC.Creator;

public static class NodeCatalog
{
    public static IReadOnlyList<NodeDefinition> Definitions { get; } = Array.AsReadOnly<NodeDefinition>(
    [
        new("event.start", "On preview start", [], [new("next", "exec")], []),
        new("constant.number", "Number", [], [new("value", "number")], ["value"]),
        new("constant.boolean", "Boolean", [], [new("value", "boolean")], ["value"]),
        new("constant.string", "Text", [], [new("value", "string")], ["value"]),
        new("number.add", "Add", [new("a", "number", true), new("b", "number", true)], [new("value", "number")], []),
        new("number.multiply", "Multiply", [new("a", "number", true), new("b", "number", true)], [new("value", "number")], []),
        new("number.lessThan", "Less than", [new("a", "number", true), new("b", "number", true)], [new("value", "boolean")], []),
        new("flow.branch", "Branch", [new("exec", "exec", true), new("condition", "boolean", true)],
            [new("true", "exec"), new("false", "exec")], []),
        new("debug.log", "Preview log", [new("exec", "exec", true), new("value", "value", true)], [new("next", "exec")], []),
        new("state.setNumber", "Set preview number", [new("exec", "exec", true), new("value", "number", true)],
            [new("next", "exec")], ["key"]),
        new("state.getNumber", "Get preview number", [], [new("value", "number")], ["key"])
    ]);

    internal static readonly IReadOnlyDictionary<string, NodeDefinition> ByKind =
        Definitions.ToDictionary(x => x.Kind, StringComparer.Ordinal);
}
