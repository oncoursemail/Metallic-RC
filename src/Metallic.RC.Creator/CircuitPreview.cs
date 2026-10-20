using System.Globalization;

namespace Metallic.RC.Creator;

public static class CircuitPreview
{
    public static PreviewResult Execute(PreviewRequest request)
    {
        var compilation = CircuitCompiler.Compile(request.Graph);
        var state = new SortedDictionary<string, double>(StringComparer.Ordinal);
        var logs = new List<PreviewLog>();
        var steps = 0;
        if (!compilation.Validation.Valid)
            return new(false, null, steps, logs, state, compilation.Validation.Issues);
        var program = compilation.Program!;
        if (request.MaxSteps is < 1 or > CreatorLimits.MaxSteps)
            return new(false, program.Hash, 0, logs, state,
                [new("step_limit", $"Set MaxSteps between 1 and {CreatorLimits.MaxSteps}.")]);
        if ((request.InitialState?.Count ?? 0) > CreatorLimits.MaxInitialStateKeys)
            return new(false, program.Hash, 0, logs, state, [new("state_limit", "Too many initial state keys.")]);
        foreach (var (key, value) in request.InitialState ?? [])
        {
            if (!CircuitCompiler.IsStateKey(key) || !double.IsFinite(value))
                return new(false, program.Hash, 0, logs, state, [new("initial_state", "Initial state needs valid keys and finite numbers.")]);
            state[key] = value;
        }

        void Spend(string nodeId)
        {
            if (steps >= request.MaxSteps)
                throw new PreviewStop(new("budget_exceeded", "Preview stopped at its execution budget.", nodeId));
            steps++;
        }
        object Evaluate(string nodeId, int depth)
        {
            if (depth > CreatorLimits.MaxDataDepth)
                throw new PreviewStop(new("data_depth", "Preview data dependencies are too deep.", nodeId));
            Spend(nodeId);
            var node = program.Nodes[nodeId];
            object Input(string port) => Evaluate(node.Inputs[port].NodeId, depth + 1);
            var value = node.Kind switch
            {
                "constant.number" => (object)node.Config["value"].GetDouble(),
                "constant.boolean" => node.Config["value"].GetBoolean(),
                "constant.string" => node.Config["value"].GetString()!,
                "number.add" => (double)Input("a") + (double)Input("b"),
                "number.multiply" => (double)Input("a") * (double)Input("b"),
                "number.lessThan" => (double)Input("a") < (double)Input("b"),
                "state.getNumber" => state.GetValueOrDefault(node.Config["key"].GetString()!, 0),
                _ => throw new PreviewStop(new("invalid_operation", "This node cannot produce a data value.", nodeId))
            };
            if (value is double number && !double.IsFinite(number))
                throw new PreviewStop(new("non_finite_result", "This operation produced a non-finite number.", nodeId));
            return value;
        }

        try
        {
            string? current = program.EntryNodeId;
            while (current is not null)
            {
                Spend(current);
                var node = program.Nodes[current];
                var nextPort = "next";
                switch (node.Kind)
                {
                    case "event.start": break;
                    case "flow.branch":
                        nextPort = (bool)Evaluate(node.Inputs["condition"].NodeId, 0) ? "true" : "false";
                        break;
                    case "debug.log":
                        var value = Evaluate(node.Inputs["value"].NodeId, 0);
                        var type = value is double ? "number" : value is bool ? "boolean" : "string";
                        var text = value switch
                        {
                            double number => number.ToString("G17", CultureInfo.InvariantCulture),
                            bool boolean => boolean ? "true" : "false",
                            _ => (string)value
                        };
                        logs.Add(new(node.Id, type, value, text));
                        break;
                    case "state.setNumber":
                        var key = node.Config["key"].GetString()!;
                        if (!state.ContainsKey(key) && state.Count >= CreatorLimits.MaxStateKeys)
                            throw new PreviewStop(new("state_limit", "Preview state reached its key limit.", node.Id));
                        state[key] = (double)Evaluate(node.Inputs["value"].NodeId, 0);
                        break;
                    default:
                        throw new PreviewStop(new("invalid_execution", "This node cannot execute in the control flow.", node.Id));
                }
                current = node.Next.GetValueOrDefault(nextPort);
            }
            return new(true, program.Hash, steps, logs, state, []);
        }
        catch (PreviewStop stopped)
        {
            return new(false, program.Hash, steps, logs, state, [stopped.Issue]);
        }
    }

    private sealed class PreviewStop(CircuitIssue issue) : Exception
    {
        public CircuitIssue Issue { get; } = issue;
    }
}
