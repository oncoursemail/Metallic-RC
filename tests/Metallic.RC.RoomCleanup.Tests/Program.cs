using System.Net.Http.Headers;
using System.Net.WebSockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.DependencyInjection;
using Metallic.RC.Backend;

if (args.Length != 1) throw new ArgumentException("Pass a new result.json path.");
string output = Path.GetFullPath(args[0]);
if (File.Exists(output)) throw new IOException("Evidence output already exists.");
string root = Path.Combine(Path.GetTempPath(), "mrc-room-cleanup-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(root);
var checks = new List<string>();
var sockets = new List<ClientWebSocket>();
WebApplication? app = null;
HttpClient? client = null;
string? admin = null;
bool tableRenamed = false;
void Check(bool condition, string name)
{
    if (!condition) throw new InvalidOperationException(name);
    checks.Add(name);
}
async Task<JsonElement> Request(HttpMethod method, string path, object? body = null, string? token = null, bool administrator = false)
{
    using var request = new HttpRequestMessage(method, path);
    if (body is not null) request.Content = new StringContent(JsonSerializer.Serialize(body), Encoding.UTF8, "application/json");
    if (token is not null) request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", token);
    if (administrator) request.Headers.Add("X-Metallic-Admin", admin);
    using var response = await client!.SendAsync(request);
    response.EnsureSuccessStatusCode();
    using var document = JsonDocument.Parse(await response.Content.ReadAsByteArrayAsync());
    return document.RootElement.Clone();
}
void RenameSessions(bool unavailable)
{
    // Fault injection touches only this executable's newly created disposable database.
    using var database = new SqliteConnection(new SqliteConnectionStringBuilder
    { DataSource = Path.Combine(root, "backend.sqlite"), Pooling = false }.ToString());
    database.Open();
    using var command = database.CreateCommand();
    command.CommandText = unavailable
        ? "ALTER TABLE sessions RENAME TO sessions_unavailable"
        : "ALTER TABLE sessions_unavailable RENAME TO sessions";
    command.ExecuteNonQuery();
    tableRenamed = unavailable;
}
async Task<int> ConnectionCount()
{
    var diagnostics = await Request(HttpMethod.Get, "/metallic/v1/admin/diagnostics", administrator: true);
    return diagnostics.GetProperty("connections").GetInt32();
}
async Task WaitForNoConnections(string name)
{
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
    while (await ConnectionCount() != 0)
    {
        if (timeout.IsCancellationRequested) throw new InvalidOperationException(name);
        await Task.Delay(20);
    }
    Check(true, name);
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
async Task WaitForMonitorAbort(ClientWebSocket socket)
{
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(8));
    try
    {
        var response = await socket.ReceiveAsync(new ArraySegment<byte>(new byte[4096]), timeout.Token);
        Check(response.MessageType == WebSocketMessageType.Close, "storage failure closes connection without a successful pose");
    }
    catch (WebSocketException)
    {
        Check(true, "storage failure aborts connection instead of hiding the failure");
    }
}
try
{
    app = BackendApplication.Build(new BackendOptions(0, root));
    await app.StartAsync();
    string address = app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!.Addresses.Single();
    Check(address.StartsWith("http://127.0.0.1:", StringComparison.Ordinal), "isolated loopback listener");
    client = new HttpClient(new SocketsHttpHandler { UseProxy = false, AllowAutoRedirect = false })
    { BaseAddress = new Uri(address), Timeout = TimeSpan.FromSeconds(10) };
    admin = File.ReadAllText(Path.Combine(root, "admin.key")).Trim();
    string password = Convert.ToHexString(RandomNumberGenerator.GetBytes(24));
    var account = await Request(HttpMethod.Post, "/metallic/v1/auth/register",
        new { username = "cleanupuser", password, displayName = "Cleanup test" }, administrator: true);
    var login = await Request(HttpMethod.Post, "/metallic/v1/auth/login", new { username = "cleanupuser", password });
    string token = login.GetProperty("accessToken").GetString()!;
    string actor = account.GetProperty("id").GetString()!;
    var room = await Request(HttpMethod.Post, "/metallic/v1/rooms",
        new { name = "Disposable cleanup fault test", capacity = 1, isPublic = true }, token: token);
    string roomId = room.GetProperty("id").GetString()!;
    string? previousEpoch = null;
    for (int cycle = 1; cycle <= 3; cycle++)
    {
        var socket = await Connect(roomId, token);
        var snapshot = await Next(socket, "snapshot");
        await Next(socket, "joined");
        Check(snapshot.GetProperty("peers").GetArrayLength() == 1 &&
            snapshot.GetProperty("accountId").GetString() == actor, "cycle " + cycle + " has only the admitted account");
        string epoch = snapshot.GetProperty("epoch").GetString()!;
        if (previousEpoch is not null) Check(epoch != previousEpoch, "cycle " + cycle + " recreates an empty live room after failure");
        previousEpoch = epoch;
        Check(await ConnectionCount() == 1, "cycle " + cycle + " accounts for its live connection");
        RenameSessions(true);
        await WaitForMonitorAbort(socket);
        // No input was sent after the rename: only the production monitor can encounter this fault.
        await WaitForNoConnections("cycle " + cycle + " releases membership after a monitor storage exception");
        RenameSessions(false);
        var profile = await Request(HttpMethod.Get, "/metallic/v1/me", token: token);
        Check(profile.GetProperty("id").GetString() == actor, "cycle " + cycle + " retains the original session after restoring storage");
    }
    var finalSocket = await Connect(roomId, token);
    var finalSnapshot = await Next(finalSocket, "snapshot");
    await Next(finalSocket, "joined");
    Check(finalSnapshot.GetProperty("epoch").GetString() != previousEpoch,
        "same account reconnects with a new live-room epoch after repeated storage failures");
    finalSocket.Abort();
    await WaitForNoConnections("ordinary client disconnect still releases membership");
    var health = await Request(HttpMethod.Get, "/healthz");
    Check(health.GetProperty("status").GetString() == "ok", "host remains available after isolated connection failures");
    var report = new { status = "room_cleanup_real_transport_passed", checks = checks.Count,
        monitorStorageFaultCycles = 3, tests = checks, actualRecRoomClientConnected = false, externalServiceRequests = 0 };
    await using var evidence = new FileStream(output, FileMode.CreateNew, FileAccess.Write);
    await JsonSerializer.SerializeAsync(evidence, report, new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks, actualRecRoomClientConnected = false }));
}
finally
{
    try { if (tableRenamed) RenameSessions(false); }
    finally
    {
        try
        {
            foreach (var socket in sockets) { socket.Abort(); socket.Dispose(); }
            client?.Dispose();
        }
        finally
        {
            try
            {
                if (app is not null)
                {
                    try { await app.StopAsync(); }
                    finally { await app.DisposeAsync(); }
                }
            }
            finally
            {
                string expectedParent = Path.GetFullPath(Path.GetTempPath()).TrimEnd(Path.DirectorySeparatorChar);
                if (Path.GetDirectoryName(root) != expectedParent || !Path.GetFileName(root).StartsWith("mrc-room-cleanup-", StringComparison.Ordinal))
                    throw new IOException("Unexpected disposable test directory.");
                if (Directory.Exists(root)) Directory.Delete(root, true);
            }
        }
    }
}
