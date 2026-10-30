using System.Globalization;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Metallic.RC.Backend;
using Metallic.RC.Compatibility;

if (args.Length != 2 || args[0] != "--self-test") throw new ArgumentException("Use --self-test NEW-result.json.");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");
var checks = new List<string>();
void Check(bool ok, string name) { if (!ok) throw new InvalidOperationException(name); checks.Add(name); }
void Reject(Action action, string code)
{
    bool rejected = false;
    try { action(); } catch (NativeTokenResponseException e) { rejected = e.Code == code && e.Message == code; }
    Check(rejected, "static refusal: " + code);
}
byte[] Decode(string value)
{
    string text = value.Replace('-', '+').Replace('_', '/');
    return Convert.FromBase64String(text + new string('=', (4 - text.Length % 4) % 4));
}
DateTimeOffset issued = DateTimeOffset.FromUnixTimeSeconds(1793232000);
byte[] master = Enumerable.Range(1, 32).Select(i => (byte)i).ToArray(); // Synthetic test key only.
byte[] originalMaster = master.ToArray();
NativeTokenResponse Make(int id = 42) => NativeTokenResponse.Create(id, master, issued, issued.AddHours(1), issued.AddDays(30));
var seenTokens = new HashSet<string>(StringComparer.Ordinal);
var seenRequests = new HashSet<string>(StringComparer.Ordinal);
var seenRefresh = new HashSet<string>(StringComparer.Ordinal);
var seenJti = new HashSet<string>(StringComparer.Ordinal);
foreach (int id in new[] { 1, 42, int.MaxValue })
{
    var response = Make(id);
    byte[] wire = response.WriteTransportBody();
    using var body = JsonDocument.Parse(wire);
    var root = body.RootElement;
    Check(root.EnumerateObject().Select(p => p.Name).SequenceEqual(LoginResponseContract.CanonicalNames), "canonical eight-field envelope and order");
    foreach (string name in new[] { "Error", "error_description", "code_hint", "login_context", "hcc" })
        Check(root.GetProperty(name).ValueKind == JsonValueKind.Null, "no fabricated optional response field: " + name);
    string access = root.GetProperty("access_token").GetString()!;
    string refresh = root.GetProperty("refresh_token").GetString()!;
    string key = root.GetProperty("Key").GetString()!;
    string[] parts = access.Split('.');
    Check(parts.Length == 3 && parts.All(p => p.Length > 0 && p.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_')), "three nonempty unpadded Base64url JWT segments");
    Check(Encoding.UTF8.GetString(Decode(parts[0])) == "{\"alg\":\"HS256\",\"typ\":\"JWT\"}", "fixed HS256 JWT header");
    byte[] signature = HMACSHA256.HashData(originalMaster, Encoding.ASCII.GetBytes(parts[0] + "." + parts[1]));
    Check(CryptographicOperations.FixedTimeEquals(signature, Decode(parts[2])), "independently recomputed HMAC-SHA256 signature");
    using var claims = JsonDocument.Parse(Decode(parts[1]));
    var c = claims.RootElement;
    Check(c.EnumerateObject().Select(p => p.Name).SequenceEqual(new[] { "sub", "iss", "aud", "iat", "exp", "jti" }), "only explicit local-policy claims");
    Check(c.GetProperty("sub").ValueKind == JsonValueKind.String && int.Parse(c.GetProperty("sub").GetString()!, CultureInfo.InvariantCulture) == id, "second-segment JSON sub is an Int32 string");
    Check(c.GetProperty("iss").GetString() == NativeTokenResponse.Issuer && c.GetProperty("aud").GetString() == NativeTokenResponse.Audience, "fixed Metallic-owned issuer and audience");
    Check(c.GetProperty("iat").GetInt64() == issued.ToUnixTimeSeconds() && c.GetProperty("exp").GetInt64() == issued.AddHours(1).ToUnixTimeSeconds(), "explicit whole-second UTC NumericDates");
    string jti = c.GetProperty("jti").GetString()!;
    Check(Decode(jti).Length == 16, "CSPRNG jti is 128 bits");
    Check(Decode(refresh).Length == 48 && refresh.Length == 64 && refresh.All(x => char.IsAsciiLetterOrDigit(x) || x is '-' or '_'), "opaque refresh has 384 random bits and bounded encoding");
    byte[] request = Convert.FromBase64String(key);
    Check(request.Length == 32 && key == Convert.ToBase64String(request) && !request.SequenceEqual(originalMaster), "independent 32-byte request key uses canonical standard Base64");
    byte[] changedInput = Encoding.ASCII.GetBytes(parts[0] + "." + parts[1]); changedInput[^1] ^= 1;
    Check(!CryptographicOperations.FixedTimeEquals(HMACSHA256.HashData(originalMaster, changedInput), Decode(parts[2])), "changed signed content fails independent signature check");
    byte[] wrongKey = originalMaster.ToArray(); wrongKey[0] ^= 1;
    Check(!CryptographicOperations.FixedTimeEquals(HMACSHA256.HashData(wrongKey, Encoding.ASCII.GetBytes(parts[0] + "." + parts[1])), Decode(parts[2])), "wrong master key fails independent signature check");
    Check(!CryptographicOperations.FixedTimeEquals(HMACSHA256.HashData(request, Encoding.ASCII.GetBytes(parts[0] + "." + parts[1])), Decode(parts[2])), "request signing key cannot verify the JWT signature");
    Check(LoginResponseContract.Read(wire) is { } parsed && parsed.AccessToken == access && parsed.RefreshToken == refresh && parsed.Key == key, "existing formatter reads its exact response");
    Check(LoginResponseContract.Classify(LoginResponseContract.Read(wire), 200) == LoginResponseDisposition.RequiresTokenAndAccountValidation, "response is not proof of completed login");
    Check(JsonSerializer.Serialize(response) == "{}" && response.GetType().GetProperties().Length == 0, "default JSON and public properties disclose no material or dates");
    Check(response.ToString() == "NativeTokenResponse { credential values redacted }", "incidental text is static and redacted");
    int callbacks = 0;
    response.UseSessionMaterial((material, rawKey) =>
    {
        callbacks++;
        Check(rawKey.SequenceEqual(request), "explicit callback receives matching request key");
        string Property(string name) => (string)typeof(NativeSessionMaterial).GetProperty(name, BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(material)!;
        Check(Property("AccessToken") == access && Property("RefreshToken") == refresh, "explicit session-material handoff matches wire tokens");
        var expiry = (DateTimeOffset)typeof(NativeSessionMaterial).GetProperty("AccessExpiresAt", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(material)!;
        var family = (DateTimeOffset)typeof(NativeSessionMaterial).GetProperty("RefreshExpiresAt", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(material)!;
        Check(expiry == issued.AddHours(1) && family == issued.AddDays(30), "material carries exact admitted expiries");
        Check(JsonSerializer.Serialize(material) == "{}", "session-material callback stays redacted by default");
        // Even advanced ref access can mutate only the temporary callback buffer.
        ref byte first = ref MemoryMarshal.GetReference(rawKey); first ^= 255;
    });
    Check(callbacks == 1 && response.WriteTransportBody().SequenceEqual(wire), "callback cannot change retained request key");
    wire[0] = 0;
    Check(response.WriteTransportBody()[0] == (byte)'{', "body export has no shared mutable backing array");
    Check(seenTokens.Add(access) && seenRequests.Add(key) && seenRefresh.Add(refresh) && seenJti.Add(jti), "independent generations use distinct random material");
}
Check(master.SequenceEqual(originalMaster), "caller master key is not modified");
var stable = Make(); byte[] stableBody = stable.WriteTransportBody();
using (var sameInput = JsonDocument.Parse(stableBody))
{
    var body = sameInput.RootElement;
    Check(seenTokens.Add(body.GetProperty("access_token").GetString()!) && seenRefresh.Add(body.GetProperty("refresh_token").GetString()!) &&
        seenRequests.Add(body.GetProperty("Key").GetString()!), "identical account and timestamps still produce fresh material");
}
master.AsSpan().Fill(255);
Check(stable.WriteTransportBody().SequenceEqual(stableBody), "issued material does not retain caller key storage");
master = originalMaster.ToArray();
bool callbackThrown = false;
try { stable.UseSessionMaterial((_, _) => throw new InvalidOperationException("synthetic_callback_failure")); }
catch (InvalidOperationException e) when (e.Message == "synthetic_callback_failure") { callbackThrown = true; }
Check(callbackThrown && stable.WriteTransportBody().SequenceEqual(stableBody), "throwing secret consumer cannot corrupt response");
bool nullRejected = false; try { stable.UseSessionMaterial(null!); } catch (ArgumentNullException) { nullRejected = true; }
Check(nullRejected, "missing secret consumer is refused");
foreach (int id in new[] { 0, -1, int.MinValue }) Reject(() => Make(id), "native_token_id_invalid");
foreach (int count in new[] { 0, 1, 31, 33, 64 }) Reject(() => NativeTokenResponse.Create(1, new byte[count], issued, issued.AddHours(1), issued.AddDays(1)), "native_token_server_key_invalid");
Reject(() => NativeTokenResponse.Create(1, new byte[32], issued, issued.AddHours(1), issued.AddDays(1)), "native_token_server_key_invalid");
foreach (var times in new[] {
    (issued.AddTicks(1), issued.AddHours(1), issued.AddDays(1)),
    (issued, issued.AddHours(1).AddTicks(1), issued.AddDays(1)),
    (issued, issued.AddHours(1), issued.AddDays(1).AddTicks(1)),
    (issued.ToOffset(TimeSpan.FromHours(1)), issued.AddHours(1), issued.AddDays(1)) })
    Reject(() => NativeTokenResponse.Create(1, master, times.Item1, times.Item2, times.Item3), "native_token_time_invalid");
foreach (var times in new[] {
    (issued, issued, issued.AddDays(1)),
    (issued, issued.AddSeconds(-1), issued.AddDays(1)),
    (issued, issued.AddSeconds(3601), issued.AddDays(1)),
    (issued, issued.AddHours(1), issued.AddSeconds(3599)),
    (issued, issued.AddHours(1), issued.AddDays(30).AddSeconds(1)),
    (DateTimeOffset.FromUnixTimeSeconds(-1), issued, issued),
    (DateTimeOffset.FromUnixTimeSeconds(253402300799 - 30 * 86400 + 1), DateTimeOffset.FromUnixTimeSeconds(253402300799), DateTimeOffset.FromUnixTimeSeconds(253402300799)) })
    Reject(() => NativeTokenResponse.Create(1, master, times.Item1, times.Item2, times.Item3), "native_token_expiry_invalid");
var epoch = DateTimeOffset.UnixEpoch;
Check(NativeTokenResponse.Create(1, master, epoch, epoch.AddSeconds(1), epoch.AddSeconds(1)).WriteTransportBody().Length < 2048, "minimum Unix and lifetime boundaries admitted with bounded body");
var upper = DateTimeOffset.FromUnixTimeSeconds(253402300799 - 30 * 86400);
Check(NativeTokenResponse.Create(int.MaxValue, master, upper, upper.AddHours(1), upper.AddDays(30)).WriteTransportBody().Length < 2048, "maximum supported family date boundary admitted");
using (var output = new FileStream(args[1], FileMode.CreateNew)) JsonSerializer.Serialize(output, new {
    status = "native_token_response_self_tests_passed", checks = checks.Count, tests = checks,
    servicesStarted = false, databaseTouched = false, nativeClientExecuted = false, routesEnabled = false, nativeLoginVerified = false
}, new JsonSerializerOptions { WriteIndented = true });
Console.WriteLine(JsonSerializer.Serialize(new { status = "native_token_response_self_tests_passed", checks = checks.Count }));
