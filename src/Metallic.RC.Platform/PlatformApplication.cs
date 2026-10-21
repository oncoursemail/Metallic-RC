using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using Metallic.RC.Creator;

namespace Metallic.RC.Platform;

public static class PlatformApplication
{
    public static WebApplication Build(PlatformOptions options)
    {
        if (options.Port is < 0 or > 65535) throw new ArgumentException("Invalid listen port.");
        var builder = WebApplication.CreateBuilder(new WebApplicationOptions
        {
            Args = [], ContentRootPath = AppContext.BaseDirectory,
            WebRootPath = Path.Combine(AppContext.BaseDirectory, "wwwroot")
        });
        builder.Logging.ClearProviders();
        builder.WebHost.ConfigureKestrel(server =>
        {
            server.Listen(IPAddress.Loopback, options.Port);
            // The application enforces the exact JSON limit; the slightly higher
            // transport cap lets it return 413 before Kestrel aborts chunked input.
            server.Limits.MaxRequestBodySize = PlatformOptions.MaxRequestBytes + 8192;
            server.Limits.RequestHeadersTimeout = TimeSpan.FromSeconds(10);
            server.Limits.KeepAliveTimeout = TimeSpan.FromSeconds(30);
        });
        // Factory registration gives the host ownership even if startup fails or
        // the caller disposes the application before starting its listener.
        builder.Services.AddSingleton(_ => new RoomStore(options.DataDirectory));
        builder.Services.AddSingleton(_ => new LocalSecurity(options.DataDirectory));
        var app = builder.Build();
        var store = app.Services.GetRequiredService<RoomStore>();
        var security = app.Services.GetRequiredService<LocalSecurity>();
        app.Lifetime.ApplicationStopped.Register(store.Dispose);
        app.Lifetime.ApplicationStarted.Register(() =>
        {
            var addresses = app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()!;
            security.WriteSession(new Uri(addresses.Addresses.Single()).Port);
        });
        app.Use(async (context, next) =>
        {
            context.Response.Headers["X-Content-Type-Options"] = "nosniff";
            context.Response.Headers["Referrer-Policy"] = "no-referrer";
            context.Response.Headers["Content-Security-Policy"] =
                "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; object-src 'none'; form-action 'self'";
            context.Response.Headers.CacheControl = "no-store";
            try
            {
                LocalSecurity.ValidateRequest(context);
                if (context.Request.Method is not ("GET" or "HEAD" or "POST" or "PUT" or "DELETE"))
                    throw new PlatformFault(405, "method_not_allowed", "This method is not supported.");
                if (context.Request.Method is "POST" or "PUT" or "DELETE")
                    if (!security.IsAuthorized(context.Request))
                        throw new PlatformFault(401, "local_token_required", "A current local session token is required.");
                await next(context);
            }
            catch (PlatformFault fault)
            {
                await Error(context, fault.Status, fault.Code, fault.Message);
            }
            catch (BadHttpRequestException badRequest)
            {
                await Error(context, badRequest.StatusCode,
                    badRequest.StatusCode == 413 ? "request_size" : "bad_request",
                    badRequest.StatusCode == 413 ? "This request is too large." : "The request could not be read.");
            }
            catch (JsonException)
            {
                await Error(context, 400, "invalid_json", "The JSON does not match the expected request shape.");
            }
            catch (OperationCanceledException) when (context.RequestAborted.IsCancellationRequested) { }
            catch (IOException)
            {
                await Error(context, 503, "storage_unavailable", "The local data store is unavailable. No successful save is being reported.");
            }
            catch (UnauthorizedAccessException)
            {
                await Error(context, 503, "storage_unavailable", "The local data store is unavailable. Check its access permissions.");
            }
            catch (Exception)
            {
                await Error(context, 500, "internal_error", "The local service could not complete this request.");
            }
        });
        // Select API/fallback endpoints only after static file handling. Otherwise
        // a fallback selected by automatic routing prevents index.html serving.
        app.UseDefaultFiles();
        app.UseStaticFiles();
        app.UseRouting();
        app.MapGet("/healthz", () => Json(new { status = "ok", service = "metallic-rc-platform", scope = "local-authoring" }));
        app.MapGet("/api/session", (HttpContext context) => Json(new { token = security.SessionToken(context), apiVersion = "1" }));
        app.MapGet("/api/v1/capabilities", () => Json(new
        {
            product = "Metallic-RC", apiVersion = "1", mode = "local-authoring",
            workspaceId = WorkspaceIdentity(options.DataDirectory),
            features = new
            {
                roomDrafts = true, cv3Validation = true, cv3Preview = true,
                compiledClientAdapter = false, recRoomProtocol = false, multiplayer = false, voice = false, accountMigration = false
            },
            limits = new
            {
                requestBytes = PlatformOptions.MaxRequestBytes, graphNodes = CreatorLimits.MaxNodes,
                graphEdges = CreatorLimits.MaxEdges, previewSteps = CreatorLimits.MaxSteps,
                dataDepth = CreatorLimits.MaxDataDepth, stringLength = CreatorLimits.MaxStringLength
            }
        }));
        app.MapGet("/api/v1/cv3/catalog", () => Json(new
        {
            schemaVersion = 1, runtime = "metallic-cv3-preview-1", nodes = NodeCatalog.Definitions,
            note = "Original authoring and bounded runtime preview; no Rec Room client integration."
        }));
        app.MapPost("/api/v1/cv3/validate", async (HttpContext context) =>
        {
            var graph = await PlatformJson.ReadAsync<CircuitGraph>(context.Request);
            return Json(CircuitCompiler.Validate(graph));
        });
        app.MapPost("/api/v1/cv3/preview", async (HttpContext context) =>
        {
            var request = await PlatformJson.ReadAsync<PreviewRequest>(context.Request);
            return Json(CircuitPreview.Execute(request));
        });
        app.MapGet("/api/v1/rooms", () => Json(new { rooms = store.List(), storage = "local" }));
        app.MapGet("/api/v1/rooms/{id}", (string id, HttpContext context) =>
        {
            var room = store.Get(id);
            context.Response.Headers.ETag = ETag(room.Revision);
            return Json(room);
        });
        app.MapPost("/api/v1/rooms", async (HttpContext context) =>
        {
            var room = store.Create(await PlatformJson.ReadAsync<RoomWriteRequest>(context.Request));
            context.Response.Headers.ETag = ETag(room.Revision);
            context.Response.Headers.Location = "/api/v1/rooms/" + room.Id;
            return Json(room, 201);
        });
        app.MapPut("/api/v1/rooms/{id}", async (string id, HttpContext context) =>
        {
            var revision = ExpectedRevision(context.Request);
            var room = store.Update(id, revision, await PlatformJson.ReadAsync<RoomWriteRequest>(context.Request));
            context.Response.Headers.ETag = ETag(room.Revision);
            return Json(room);
        });
        app.MapDelete("/api/v1/rooms/{id}", (string id, HttpContext context) =>
        {
            store.Delete(id, ExpectedRevision(context.Request));
            return Results.NoContent();
        });
        app.MapFallback((HttpContext context) => Json(new
        {
            error = new { code = "not_found", message = "This resource does not exist." }
        }, 404));
        return app;
    }

    private static IResult Json(object value, int status = 200) => Results.Json(value, PlatformJson.Options, statusCode: status);
    private static string WorkspaceIdentity(string dataDirectory)
    {
        var canonical = Path.TrimEndingDirectorySeparator(Path.GetFullPath(dataDirectory));
        if (OperatingSystem.IsWindows()) canonical = canonical.ToUpperInvariant();
        return Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(canonical)));
    }
    private static string ETag(long revision) => "\"r" + revision.ToString(System.Globalization.CultureInfo.InvariantCulture) + "\"";
    private static long ExpectedRevision(HttpRequest request)
    {
        var text = request.Headers.IfMatch.ToString();
        if (string.IsNullOrEmpty(text))
            throw new PlatformFault(428, "revision_required", "Send the draft's ETag in If-Match.");
        if (text.Length is < 4 or > 22 || !text.StartsWith("\"r", StringComparison.Ordinal) || !text.EndsWith('"')
            || !long.TryParse(text.AsSpan(2, text.Length - 3), System.Globalization.NumberStyles.None,
                System.Globalization.CultureInfo.InvariantCulture, out var revision) || revision < 1 || text != ETag(revision))
            throw new PlatformFault(400, "revision_format", "Use the exact ETag returned when loading this draft.");
        return revision;
    }

    private static async Task Error(HttpContext context, int status, string code, string message)
    {
        if (context.Response.HasStarted) { context.Abort(); return; }
        context.Response.StatusCode = status;
        context.Response.ContentType = "application/json; charset=utf-8";
        await context.Response.WriteAsJsonAsync(new { error = new { code, message } }, PlatformJson.Options,
            context.RequestAborted);
    }
}
