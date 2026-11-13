using System.Net.Http.Headers;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.Extensions.DependencyInjection;
using Metallic.RC.Backend;

if (args.Length != 1) throw new ArgumentException("Pass a new result.json path.");
string output = Path.GetFullPath(args[0]);
if (File.Exists(output)) throw new IOException("Evidence output already exists.");
string root = Path.Combine(Path.GetTempPath(), "mrc-pose-contract-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(root);
var checks = new List<string>();
var sockets = new List<ClientWebSocket>();
WebApplication? app = null;
HttpClient? client = null;
void Check(bool condition, string name)
{
    if (!condition) throw new InvalidOperationException(name);
    checks.Add(name);
}
async Task<JsonElement> Request(string path, object body, string? token = null, string? admin = null)
{
    using var request = new HttpRequestMessage(HttpMethod.Post, path);
    request.Content = new StringContent(JsonSerializer.Serialize(body), Encoding.UTF8, "application/json");
    if (token is not null) request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", token);
    if (admin is not null) request.Headers.Add("X-Metallic-Admin", admin);
    using var response = await client!.SendAsync(request);
    response.EnsureSuccessStatusCode();
    using var document = JsonDocument.Parse(await response.Content.ReadAsByteArrayAsync());
    return document.RootElement.Clone();
}
async Task<ClientWebSocket> Connect(string room, string token)
{
    var socket = new ClientWebSocket();
    sockets.Add(socket);
    socket.Options.SetRequestHeader("Authorization", "Bearer " + token);
    var address = new UriBuilder(client!.BaseAddress!) { Scheme = "ws", Path = "/metallic/v1/rooms/" + room + "/socket" }.Uri;
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(8));
    await socket.ConnectAsync(address, timeout.Token);
    return socket;
}
async Task<JsonElement> Next(ClientWebSocket socket, string type)
{
    byte[] buffer = new byte[32768];
    int length = 0;
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(8));
    WebSocketReceiveResult result;
    do
    {
        result = await socket.ReceiveAsync(new ArraySegment<byte>(buffer, length, buffer.Length - length), timeout.Token);
        if (result.MessageType != WebSocketMessageType.Text) throw new IOException("Expected text event.");
        length += result.Count;
    } while (!result.EndOfMessage);
    using var document = JsonDocument.Parse(buffer.AsMemory(0, length));
    var message = document.RootElement.Clone();
    Check(message.GetProperty("type").GetString() == type, "next event is " + type);
    return message;
}
async Task Send(ClientWebSocket socket, string json, bool fragmented = false)
{
    byte[] bytes = Encoding.UTF8.GetBytes(json);
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
    if (fragmented)
    {
        int split = bytes.Length / 2;
        await socket.SendAsync(bytes.AsMemory(0, split), WebSocketMessageType.Text, false, timeout.Token);
        await socket.SendAsync(bytes.AsMemory(split), WebSocketMessageType.Text, true, timeout.Token);
    }
    else await socket.SendAsync(bytes.AsMemory(), WebSocketMessageType.Text, true, timeout.Token);
}
try
{
    app = BackendApplication.Build(new BackendOptions(0, root));
    await app.StartAsync();
    string address = app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!.Addresses.Single();
    Check(address.StartsWith("http://127.0.0.1:", StringComparison.Ordinal), "isolated loopback listener");
    client = new HttpClient(new SocketsHttpHandler { UseProxy = false, AllowAutoRedirect = false })
    { BaseAddress = new Uri(address), Timeout = TimeSpan.FromSeconds(10) };
    string admin = File.ReadAllText(Path.Combine(root, "admin.key")).Trim();
    string password = Convert.ToHexString(System.Security.Cryptography.RandomNumberGenerator.GetBytes(24));
    var tokens = new List<string>();
    string? actor = null;
    foreach (string username in new[] { "sender", "observer", "lateobserver" })
    {
        var account = await Request("/metallic/v1/auth/register", new { username, password, displayName = username }, admin: admin);
        if (actor is null) actor = account.GetProperty("id").GetString();
        var login = await Request("/metallic/v1/auth/login", new { username, password });
        tokens.Add(login.GetProperty("accessToken").GetString()!);
    }
    var room = await Request("/metallic/v1/rooms", new { name = "Pose contract regression", capacity = 3, isPublic = true }, token: tokens[0]);
    string id = room.GetProperty("id").GetString()!;
    var sender = await Connect(id, tokens[0]);
    await Next(sender, "snapshot"); await Next(sender, "joined");
    var observer = await Connect(id, tokens[1]);
    await Next(observer, "snapshot"); await Next(observer, "joined"); await Next(sender, "joined");
    long sequence = 1;
    await Send(sender, "{\"type\":\"pose\",\"sequence\":1,\"x\":12,\"y\":23,\"z\":34}");
    var initial = await Next(observer, "pose"); await Next(sender, "pose");
    long revision = initial.GetProperty("revision").GetInt64();
    string[] omissions = ["x", "y", "z", "all_coordinates", "type", "sequence"];
    double lastX = 12, lastY = 23, lastZ = 34;
    foreach (string omission in omissions)
    {
        sequence++;
        var partial = new Dictionary<string, object>
        { ["type"] = "pose", ["sequence"] = sequence, ["x"] = -10, ["y"] = -20, ["z"] = -30 };
        if (omission == "all_coordinates") { partial.Remove("x"); partial.Remove("y"); partial.Remove("z"); }
        else partial.Remove(omission);
        await Send(sender, JsonSerializer.Serialize(partial), fragmented: omission == "all_coordinates");
        var rejected = await Next(sender, "error");
        Check(rejected.GetProperty("code").GetString() == "invalid_json", omission + " omission rejected");
        lastX = sequence == 7 ? 0 : sequence + 10;
        lastY = sequence == 7 ? 0 : sequence + 20;
        lastZ = sequence == 7 ? 0 : sequence + 30;
        await Send(sender, JsonSerializer.Serialize(new { type = "pose", sequence, x = lastX, y = lastY, z = lastZ }), fragmented: omission == "z");
        var seen = await Next(observer, "pose");
        var echoed = await Next(sender, "pose");
        Check(seen.GetProperty("sequence").GetInt64() == sequence && echoed.GetProperty("sequence").GetInt64() == sequence,
            omission + " rejection does not consume sequence");
        Check(seen.GetProperty("revision").GetInt64() == ++revision, omission + " rejection does not broadcast or advance revision");
        Check(seen.GetProperty("x").GetDouble() == lastX && seen.GetProperty("y").GetDouble() == lastY && seen.GetProperty("z").GetDouble() == lastZ,
            omission + " observer receives only complete accepted coordinates");
    }
    var late = await Connect(id, tokens[2]);
    var snapshot = await Next(late, "snapshot");
    var stored = snapshot.GetProperty("peers").EnumerateArray().Single(p => p.GetProperty("accountId").GetString() == actor);
    Check(stored.GetProperty("sequence").GetInt64() == sequence && stored.GetProperty("x").GetDouble() == lastX &&
        stored.GetProperty("y").GetDouble() == lastY && stored.GetProperty("z").GetDouble() == lastZ,
        "late observer snapshot contains last accepted complete pose including explicit zeroes");
    var report = new { status = "pose_contract_real_transport_passed", checks = checks.Count, cases = omissions,
        tests = checks, actualRecRoomClientConnected = false, externalServiceRequests = 0 };
    await using var evidence = new FileStream(output, FileMode.CreateNew, FileAccess.Write);
    await JsonSerializer.SerializeAsync(evidence, report, new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks, actualRecRoomClientConnected = false }));
}
finally
{
    foreach (var socket in sockets) { socket.Abort(); socket.Dispose(); }
    client?.Dispose();
    if (app is not null) { await app.StopAsync(); await app.DisposeAsync(); }
    if (Directory.Exists(root)) Directory.Delete(root, true);
}
