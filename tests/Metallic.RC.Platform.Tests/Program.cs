using System.Net;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.Extensions.DependencyInjection;
using Metallic.RC.Creator;
using Metallic.RC.Platform;

internal static class Program
{
    private static int assertions;

    private static async Task<int> Main()
    {
        var data = Path.Combine(Path.GetTempPath(), "metallic-platform-acceptance-" + Guid.NewGuid().ToString("N"));
        try
        {
            CreatorChecks();
            await HttpChecks(data);
            Console.WriteLine($"PASS: {assertions} assertions; typed CV3, bounded preview, real loopback HTTP, authentication, ETags and restart persistence.");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("FAIL: " + error);
            return 1;
        }
        finally
        {
            if (Directory.Exists(data)) Directory.Delete(data, true);
        }
    }

    private static void Check(bool condition, string message)
    {
        assertions++;
        if (!condition) throw new InvalidOperationException(message);
    }

    private static CircuitNode Node(string id, string kind, string? configKey = null, object? value = null) => new()
    {
        Id = id, Kind = kind,
        Config = configKey is null ? [] : new() { [configKey] = JsonSerializer.SerializeToElement(value) }
    };

    private static CircuitEdge Edge(string from, string port, string to, string input) => new()
    {
        FromNode = from, FromPort = port, ToNode = to, ToPort = input
    };

    private static CircuitGraph Sample() => new()
    {
        EntryNodeId = "start",
        Nodes =
        [
            Node("start", "event.start"), Node("two", "constant.number", "value", 2),
            Node("three", "constant.number", "value", 3), Node("sum", "number.add"),
            Node("set", "state.setNumber", "key", "score"), Node("get", "state.getNumber", "key", "score"),
            Node("limit", "constant.number", "value", 10), Node("less", "number.lessThan"),
            Node("branch", "flow.branch"), Node("log", "debug.log"),
            Node("failure", "constant.string", "value", "wrong branch"), Node("falseLog", "debug.log")
        ],
        Edges =
        [
            Edge("two", "value", "sum", "a"), Edge("three", "value", "sum", "b"),
            Edge("start", "next", "set", "exec"), Edge("sum", "value", "set", "value"),
            Edge("set", "next", "branch", "exec"), Edge("get", "value", "less", "a"),
            Edge("limit", "value", "less", "b"), Edge("less", "value", "branch", "condition"),
            Edge("branch", "true", "log", "exec"), Edge("get", "value", "log", "value"),
            Edge("branch", "false", "falseLog", "exec"), Edge("failure", "value", "falseLog", "value")
        ]
    };

    private static void HasIssue(ValidationResult result, string code, string? nodeId = null)
    {
        Check(!result.Valid && result.ProgramHash is null, "Invalid graphs must not produce executable programs.");
        Check(result.Issues.Any(i => i.Code == code && (nodeId is null || i.NodeId == nodeId)),
            "Expected a useful " + code + " issue at " + (nodeId ?? "graph") + ".");
    }

    private static void CreatorChecks()
    {
        var graph = Sample();
        var nodes = graph.Nodes!;
        var edges = graph.Edges!;
        var validation = CircuitCompiler.Validate(graph);
        Check(validation.Valid && validation.ProgramHash?.Length == 64, "Sample must compile with a SHA256 program hash.");
        var reordered = graph with { Nodes = nodes.AsEnumerable().Reverse().ToList(), Edges = edges.AsEnumerable().Reverse().ToList() };
        Check(CircuitCompiler.Validate(reordered).ProgramHash == validation.ProgramHash, "Canonical hash must ignore editor array order.");
        using var sameNumber = JsonDocument.Parse("2.000e0");
        var equivalent = graph with { Nodes = nodes.Select(n => n.Id == "two" ? n with
            { Config = new() { ["value"] = sameNumber.RootElement.Clone() } } : n).ToList() };
        Check(CircuitCompiler.Validate(equivalent).ProgramHash == validation.ProgramHash, "Equivalent numeric spellings must have the same hash.");
        var preview = CircuitPreview.Execute(new() { Graph = graph, InitialState = new() { ["score"] = 99 } });
        Check(preview.Succeeded && preview.State["score"] == 5 && preview.Logs.Count == 1 && preview.Logs[0].Text == "5",
            "Preview must execute arithmetic, state mutation and the correct conditional branch.");
        Check(preview.Steps == 11, "Preview must count control and data evaluations, including repeated state reads.");
        var again = CircuitPreview.Execute(new() { Graph = reordered, InitialState = new() { ["score"] = 99 } });
        Check(JsonSerializer.Serialize(preview, PlatformJson.Options) == JsonSerializer.Serialize(again, PlatformJson.Options),
            "Preview results must be deterministic across graph ordering.");
        var budget = CircuitPreview.Execute(new() { Graph = graph, MaxSteps = 3 });
        Check(!budget.Succeeded && budget.Steps == 3 && budget.Issues.Any(i => i.Code == "budget_exceeded"), "Execution must stop exactly at its budget.");
        Check(!budget.State.ContainsKey("score"), "An interrupted calculation must not write a partial state value.");
        Check(CircuitPreview.Execute(new() { Graph = graph, MaxSteps = 0 }).Issues.Any(i => i.Code == "step_limit"), "Reject zero execution budget.");
        Check(CircuitPreview.Execute(new() { Graph = graph, MaxSteps = CreatorLimits.MaxSteps + 1 }).Issues.Any(i => i.Code == "step_limit"), "Reject unbounded execution budget.");
        Check(CircuitPreview.Execute(new() { Graph = graph, InitialState = new() { ["score"] = double.NaN } }).Issues.Any(i => i.Code == "initial_state"), "Initial state must be finite.");
        Check(CircuitPreview.Execute(new() { Graph = graph, InitialState = new() { ["../score"] = 1 } }).Issues.Any(i => i.Code == "initial_state"), "Initial state keys are bounded identifiers.");

        var badType = graph with { Nodes = nodes.Select(n => n.Id == "two" ? Node("two", "constant.string", "value", "two") : n).ToList() };
        HasIssue(CircuitCompiler.Validate(badType), "port_type", "sum");
        HasIssue(CircuitCompiler.Validate(graph with { Nodes = [..nodes, nodes[0]] }), "duplicate_node", "start");
        HasIssue(CircuitCompiler.Validate(graph with { Edges = [..edges, Edge("two", "missing", "sum", "a")] }), "output_port", "two");
        HasIssue(CircuitCompiler.Validate(graph with { Edges = [..edges, Edge("two", "value", "sum", "missing")] }), "input_port", "sum");
        HasIssue(CircuitCompiler.Validate(graph with { Edges = edges.Where(e => e.ToNode != "sum" || e.ToPort != "a").ToList() }), "required_input", "sum");
        HasIssue(CircuitCompiler.Validate(graph with { Edges = [..edges, Edge("gone", "value", "sum", "a")] }), "missing_node", "sum");
        HasIssue(CircuitCompiler.Validate(graph with { Edges = [..edges, Edge("three", "value", "sum", "a")] }), "multiple_input", "sum");
        HasIssue(CircuitCompiler.Validate(graph with { Edges = [..edges, Edge("sum", "value", "sum", "a")] }), "cycle", "sum");
        HasIssue(CircuitCompiler.Validate(graph with { Edges = [..edges, Edge("log", "next", "set", "exec")] }), "cycle", "set");
        HasIssue(CircuitCompiler.Validate(graph with { Nodes = [..nodes, Node("extra", "constant.string", "value", new string('x', CreatorLimits.MaxStringLength + 1))] }), "config_value", "extra");
        HasIssue(CircuitCompiler.Validate(graph with { Nodes = Enumerable.Range(0, CreatorLimits.MaxNodes + 1).Select(i => Node("n" + i, "event.start")).ToList() }), "graph_size");
        HasIssue(CircuitCompiler.Validate(null), "graph_required");
        HasIssue(CircuitCompiler.Validate(graph with { Nodes = null }), "graph_shape");
        HasIssue(CircuitCompiler.Validate(graph with { Nodes = [..nodes, null!] }), "null_node");

        var overflow = graph with { Nodes = nodes.Select(n => n.Id is "two" or "three" ? Node(n.Id, "constant.number", "value", double.MaxValue) : n).ToList() };
        var overflowResult = CircuitPreview.Execute(new() { Graph = overflow });
        Check(!overflowResult.Succeeded && overflowResult.Issues.Any(i => i.Code == "non_finite_result" && i.NodeId == "sum"), "Overflow must produce a node-level error instead of invalid JSON.");

        var deep = new CircuitGraph { EntryNodeId = "start", Nodes = [Node("start", "event.start"), Node("zero", "constant.number", "value", 0), Node("log", "debug.log")], Edges = [Edge("start", "next", "log", "exec")] };
        var previous = "zero";
        for (var i = 0; i < 70; i++)
        {
            var id = "sum" + i;
            deep.Nodes.Add(Node(id, "number.add"));
            deep.Edges.Add(Edge(previous, "value", id, "a"));
            deep.Edges.Add(Edge("zero", "value", id, "b"));
            previous = id;
        }
        deep.Edges.Add(Edge(previous, "value", "log", "value"));
        Check(CircuitCompiler.Validate(deep).Valid, "Deep graph remains structurally valid.");
        var deepResult = CircuitPreview.Execute(new() { Graph = deep, MaxSteps = 4096 });
        Check(!deepResult.Succeeded && deepResult.Issues.Any(i => i.Code == "data_depth"), "Recursive data evaluation must stop at its depth bound.");
    }

    private static async Task HttpChecks(string data)
    {
        var neverStartedData = Path.Combine(data, "never-started");
        await using (var neverStarted = PlatformApplication.Build(new(0, neverStartedData))) { }
        await using (var retry = await Host.Start(neverStartedData))
            Check((await retry.Send("GET", "/healthz")).Status == 200, "Disposing an unstarted application must release its store lease.");
        string id, savedToken, workspaceId;
        var graph = Sample();
        await using (var host = await Host.Start(data))
        {
            var index = await host.Send("GET", "/");
            Check(index.Status == 200 && index.Raw.Contains("Metallic", StringComparison.OrdinalIgnoreCase)
                && index.Raw.Contains("app.js", StringComparison.Ordinal), "The packaged root document must be served before API fallback routing.");
            var script = await host.Send("GET", "/app.js");
            Check(script.Status == 200 && script.Raw.Contains("/api/", StringComparison.Ordinal), "The packaged editor script must be served as a static asset.");
            var unknownApi = await host.Send("GET", "/api/v1/does-not-exist");
            Check(unknownApi.Status == 404 && unknownApi.Body.GetProperty("error").GetProperty("code").GetString() == "not_found", "Missing API routes must return a JSON 404 instead of the editor document.");
            var health = await host.Send("GET", "/healthz");
            Check(health.Status == 200 && health.Body.GetProperty("scope").GetString() == "local-authoring", "Loopback health endpoint must be real.");
            Check(!health.Raw.Contains(data, StringComparison.Ordinal), "Health must not expose local filesystem paths.");
            var capabilityResponse = (await host.Send("GET", "/api/v1/capabilities")).Body;
            workspaceId = capabilityResponse.GetProperty("workspaceId").GetString()!;
            Check(workspaceId.Length == 64 && !capabilityResponse.GetRawText().Contains(data, StringComparison.Ordinal), "Workspace identity must be a hash without exposing the data path.");
            var capabilities = capabilityResponse.GetProperty("features");
            Check(capabilities.GetProperty("cv3Validation").GetBoolean() && capabilities.GetProperty("roomDrafts").GetBoolean(), "Report available original authoring capabilities.");
            Check(!capabilities.GetProperty("compiledClientAdapter").GetBoolean() && !capabilities.GetProperty("multiplayer").GetBoolean() && !capabilities.GetProperty("voice").GetBoolean(), "Never claim client integration, multiplayer or voice.");
            Check((await host.Send("GET", "/api/session")).Status == 403, "Session requires explicit local-client header.");
            savedToken = await host.Session();
            Check(savedToken.Length == 64, "Local authentication token must have 256 bits of entropy.");
            var storedSession = JsonDocument.Parse(await File.ReadAllTextAsync(Path.Combine(data, "local-session.json")));
            using (storedSession)
                Check(storedSession.RootElement.GetProperty("token").GetString() == savedToken && storedSession.RootElement.GetProperty("port").GetInt32() == host.Address.Port, "Private session file must identify the current host.");
            if (!OperatingSystem.IsWindows())
            {
                Check(File.GetUnixFileMode(data) == (UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute), "State directory must be user-only.");
                Check(File.GetUnixFileMode(Path.Combine(data, "local-session.json")) == (UnixFileMode.UserRead | UnixFileMode.UserWrite), "Token file must be user-only.");
            }
            Check((await host.Send("POST", "/api/v1/rooms", new { name = "Draft" })).Status == 401, "Writes require a current token.");
            Check((await host.Send("POST", "/api/v1/rooms", new { name = "Draft" }, new string('0', 64))).Status == 401, "Wrong token must fail.");
            Check((await host.Send("POST", "/api/v1/rooms", new { name = "Draft" }, new string('z', 64))).Status == 401, "Malformed token must fail without internal error.");
            Check((await host.Send("GET", "/api/session", headers: new() { ["X-Metallic-Client"] = "desktop", ["Origin"] = "https://example.com" })).Status == 403, "Foreign Origin cannot obtain a local token.");
            Check((await host.Send("GET", "/api/session", headers: new() { ["X-Metallic-Client"] = "desktop", ["Sec-Fetch-Site"] = "cross-site" })).Status == 403, "Fetch metadata must reject cross-site access.");
            Check((await host.Send("GET", "/api/session", headers: new() { ["X-Metallic-Client"] = "desktop", ["Sec-Fetch-Site"] = "same-site" })).Status == 403, "Other localhost origins are not the same-origin app.");
            Check((await host.Send("GET", "/healthz", headers: new() { ["Host"] = "rebind.invalid:" + host.Address.Port })).Status == 421, "Reject DNS rebinding Host headers.");
            Check((await host.Send("GET", "/healthz", headers: new() { ["Origin"] = "null" })).Status == 403, "Reject opaque browser origins.");
            Check((await host.Send("GET", "/api/session", headers: new() { ["X-Metallic-Client"] = "desktop", ["Origin"] = host.Address.GetLeftPart(UriPartial.Authority), ["Sec-Fetch-Site"] = "same-origin" })).Status == 200, "Same-origin app can bootstrap its session.");
            Check((await host.Send("OPTIONS", "/api/v1/rooms")).Status == 405, "Cross-origin preflight is not enabled.");
            Check((await host.Send("POST", "/api/v1/rooms", token: savedToken, raw: "{broken")).Status == 400, "Malformed JSON receives a bounded error.");
            Check((await host.Send("POST", "/api/v1/rooms", token: savedToken, raw: "{\"name\":\"A\",\"name\":\"B\"}")).Status == 400, "Reject duplicate JSON keys.");
            Check((await host.Send("POST", "/api/v1/rooms", new { name = "A", unexpected = true }, savedToken)).Status == 400, "Reject unknown DTO fields.");
            Check((await host.Send("POST", "/api/v1/rooms", token: savedToken, raw: "{\"name\":null}")).Status == 400, "Reject null required values.");
            Check((await host.Send("POST", "/api/v1/rooms", token: savedToken, raw: new string(' ', PlatformOptions.MaxRequestBytes + 1))).Status == 413, "Reject oversized request bodies.");
            Check((await host.Send("POST", "/api/v1/rooms", token: savedToken, raw: new string(' ', PlatformOptions.MaxRequestBytes + 1), chunked: true)).Status == 413, "Chunked transfer must not bypass the request limit.");
            Check((await host.Send("POST", "/api/v1/login", new { }, savedToken)).Status == 404, "No fake login API should exist.");

            var created = await host.Send("POST", "/api/v1/rooms", new { name = "  Metallic Test  ", description = "Persistent circuit", graph }, savedToken);
            Check(created.Status == 201 && created.ETag == "\"r1\"", "Create must return a durable draft with its first ETag.");
            id = created.Body.GetProperty("id").GetString()!;
            Check(created.Body.GetProperty("name").GetString() == "Metallic Test", "Trim room names at the store boundary.");
            var loaded = await host.Send("GET", "/api/v1/rooms/" + id);
            Check(loaded.Status == 200 && loaded.ETag == created.ETag, "Load must return the saved draft and ETag.");
            Check((await host.Send("GET", "/api/v1/rooms")).Body.GetProperty("rooms").GetArrayLength() == 1, "List only actual persisted drafts.");
            Check((await host.Send("PUT", "/api/v1/rooms/" + id, new { name = "Next", graph }, savedToken)).Status == 428, "Updates require If-Match.");
            Check((await host.Send("PUT", "/api/v1/rooms/" + id, new { name = "Next", graph }, savedToken, new() { ["If-Match"] = "\"r8\"" })).Status == 412, "Stale revisions must not overwrite drafts.");
            Check((await host.Send("PUT", "/api/v1/rooms/" + id, new { name = "Next", graph }, savedToken, new() { ["If-Match"] = "\"r01\"" })).Status == 400, "If-Match must use the exact strong ETag.");
            var updated = await host.Send("PUT", "/api/v1/rooms/" + id, new { name = "Updated", description = "Persist after restart", graph }, savedToken, new() { ["If-Match"] = loaded.ETag! });
            Check(updated.Status == 200 && updated.ETag == "\"r2\"", "A matching update increments the revision once.");
            var racers = await Task.WhenAll(
                host.Send("PUT", "/api/v1/rooms/" + id, new { name = "Writer A", graph }, savedToken, new() { ["If-Match"] = updated.ETag! }),
                host.Send("PUT", "/api/v1/rooms/" + id, new { name = "Writer B", graph }, savedToken, new() { ["If-Match"] = updated.ETag! }));
            Check(racers.Count(r => r.Status == 200 && r.ETag == "\"r3\"") == 1 && racers.Count(r => r.Status == 412) == 1, "Concurrent writers must produce one winner and one revision conflict.");
            Check(!Directory.GetFiles(Path.Combine(data, "rooms"), "*.tmp").Any(), "Atomic replacement must not leave completed-write temp files.");
            Check((await host.Send("GET", "/api/v1/rooms/not-a-guid")).Status == 404, "Draft IDs cannot identify arbitrary files.");
            Check((await host.Send("DELETE", "/api/v1/rooms/" + id, token: savedToken, headers: new() { ["If-Match"] = "\"r1\"" })).Status == 412, "Deletion must enforce optimistic revisions.");
            var compiled = await host.Send("POST", "/api/v1/cv3/validate", graph, savedToken);
            Check(compiled.Status == 200 && compiled.Body.GetProperty("valid").GetBoolean(), "HTTP graph compiler must validate a real graph.");
            var preview = await host.Send("POST", "/api/v1/cv3/preview", new { graph, maxSteps = 512 }, savedToken);
            Check(preview.Status == 200 && preview.Body.GetProperty("succeeded").GetBoolean() && preview.Body.GetProperty("state").GetProperty("score").GetDouble() == 5, "HTTP preview must execute the graph.");
            var invalidGraph = graph with { Edges = [] };
            var invalidDraft = await host.Send("POST", "/api/v1/rooms", new { name = "Work in progress", graph = invalidGraph }, savedToken);
            Check(invalidDraft.Status == 201, "Editors must be able to save an unfinished circuit draft.");
            var invalidPreview = await host.Send("POST", "/api/v1/cv3/preview", new { graph = invalidGraph }, savedToken);
            Check(invalidPreview.Status == 200 && !invalidPreview.Body.GetProperty("succeeded").GetBoolean(), "An unfinished draft cannot execute before validation succeeds.");
            Check((await host.Send("GET", "/api/v1/cv3/catalog")).Body.GetProperty("nodes").GetArrayLength() == 11, "Catalog must match the original implemented node set.");
            var leaseRejected = false;
            try { await using var duplicate = PlatformApplication.Build(new(0, data)); }
            catch (IOException) { leaseRejected = true; }
            Check(leaseRejected, "A second server must not own the same draft store.");
        }

        await using (var restarted = await Host.Start(data))
        {
            Check((await restarted.Send("GET", "/api/v1/capabilities")).Body.GetProperty("workspaceId").GetString() == workspaceId, "The workspace identity must survive host restart.");
            var loaded = await restarted.Send("GET", "/api/v1/rooms/" + id);
            Check(loaded.Status == 200 && loaded.ETag == "\"r3\"" && loaded.Body.GetProperty("graph").GetProperty("nodes").GetArrayLength() == graph.Nodes!.Count, "Room graph and revision must survive a complete host restart.");
            var freshToken = await restarted.Session();
            Check(freshToken != savedToken, "A restarted service must generate a fresh token.");
            Check((await restarted.Send("POST", "/api/v1/cv3/validate", graph, savedToken)).Status == 401, "Tokens from stopped instances must not authorize writes.");
            Check((await restarted.Send("DELETE", "/api/v1/rooms/" + id, token: freshToken, headers: new() { ["If-Match"] = loaded.ETag! })).Status == 204, "Matching delete must remove the draft.");
            Check((await restarted.Send("GET", "/api/v1/rooms/" + id)).Status == 404, "Deleted draft must immediately be absent.");
            var remaining = (await restarted.Send("GET", "/api/v1/rooms")).Body.GetProperty("rooms");
            var remainingId = remaining[0].GetProperty("id").GetString()!;
            await File.WriteAllTextAsync(Path.Combine(data, "rooms", remainingId + ".json"), "{damaged");
            var unavailable = await restarted.Send("GET", "/api/v1/rooms/" + remainingId);
            Check(unavailable.Status == 503 && unavailable.Body.GetProperty("error").GetProperty("code").GetString() == "storage_unavailable", "Corrupt storage must be unavailable, never an empty success or a client JSON error.");
            Check(!unavailable.Raw.Contains(data, StringComparison.Ordinal) && !unavailable.Raw.Contains("Exception", StringComparison.Ordinal), "Storage errors must redact paths and exception details.");
        }
        var corruptionRejected = false;
        try { await using var corrupt = PlatformApplication.Build(new(0, data)); }
        catch (IOException) { corruptionRejected = true; }
        Check(corruptionRejected, "Startup must reject a corrupt draft store without silently resetting it.");
    }

    private sealed record Reply(int Status, JsonElement Body, string? ETag, string Raw);

    private sealed class Host(WebApplication app, HttpClient client, Uri address) : IAsyncDisposable
    {
        public Uri Address { get; } = address;
        public static async Task<Host> Start(string data)
        {
            var app = PlatformApplication.Build(new(0, data));
            try
            {
                await app.StartAsync();
                var addresses = app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!;
                var address = new Uri(addresses.Addresses.Single());
                return new(app, new HttpClient { BaseAddress = address, Timeout = TimeSpan.FromSeconds(10) }, address);
            }
            catch { await app.DisposeAsync(); throw; }
        }

        public async Task<string> Session()
        {
            var reply = await Send("GET", "/api/session", headers: new() { ["X-Metallic-Client"] = "desktop" });
            Check(reply.Status == 200, "Local session bootstrap must succeed.");
            return reply.Body.GetProperty("token").GetString()!;
        }

        public async Task<Reply> Send(string method, string path, object? body = null, string? token = null,
            Dictionary<string, string>? headers = null, string? raw = null, bool chunked = false)
        {
            using var request = new HttpRequestMessage(new HttpMethod(method), path);
            if (token is not null) request.Headers.Add("X-Metallic-Token", token);
            foreach (var (name, value) in headers ?? []) request.Headers.TryAddWithoutValidation(name, value);
            if (raw is not null || body is not null)
            {
                var text = raw ?? JsonSerializer.Serialize(body, PlatformJson.Options);
                request.Content = chunked ? new ChunkedContent(Encoding.UTF8.GetBytes(text)) : new StringContent(text, Encoding.UTF8, "application/json");
            }
            using var response = await client.SendAsync(request);
            var content = await response.Content.ReadAsStringAsync();
            using var document = content.Length == 0 || response.Content.Headers.ContentType?.MediaType != "application/json"
                ? null : JsonDocument.Parse(content);
            var element = document?.RootElement.Clone() ?? default;
            return new((int)response.StatusCode, element, response.Headers.ETag?.ToString(), content);
        }

        public async ValueTask DisposeAsync()
        {
            client.Dispose();
            await app.StopAsync();
            await app.DisposeAsync();
        }
    }

    private sealed class ChunkedContent : HttpContent
    {
        private readonly byte[] bytes;
        public ChunkedContent(byte[] bytes)
        {
            this.bytes = bytes;
            Headers.ContentType = new MediaTypeHeaderValue("application/json");
        }
        protected override bool TryComputeLength(out long length) { length = 0; return false; }
        protected override Task SerializeToStreamAsync(Stream stream, TransportContext? context) => stream.WriteAsync(bytes).AsTask();
        public override string ToString() => "Bounded acceptance-test payload";
    }
}
