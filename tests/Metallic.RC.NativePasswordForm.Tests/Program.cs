using System.Collections;
using System.Reflection;
using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;

if (args.Length != 2 || args[0] != "--self-test")
    throw new ArgumentException("Use --self-test NEW-result.json.");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");

var checks = new List<string>();
void Check(bool condition, string label)
{
    if (!condition) throw new InvalidOperationException(label);
    checks.Add(label);
}
string Synthetic(string category) => "synthetic-" + category + "-" + Guid.NewGuid().ToString("N");
string username = Synthetic("name"), password = Synthetic("value");
string[] manualNames = ["grant_type", "username", "password", "security_code", "2fa_code", "login_context", "require_password"];
string[] contextNames = ["client_id", "client_secret", "platform", "platform_id", "device_id", "device_class", "time", "ver", "cid", "asid", "locale", "isInitialLogin", "build_key"];
List<(string Key, string Value)> Basic() => [("grant_type", "password"), ("username", username), ("password", password)];
byte[] Form(IEnumerable<(string Key, string Value)> fields) => Encoding.ASCII.GetBytes(string.Join('&',
    fields.Select(field => Uri.EscapeDataString(field.Key) + "=" + Uri.EscapeDataString(field.Value))));
NativePasswordFormResult Read(byte[] body, string? contentType = NativePasswordFormProfile.ContentType) =>
    NativePasswordFormProfile.Read(body, contentType);
void NoExposure(NativePasswordFormResult result, IEnumerable<string> values)
{
    string[] serialized = [JsonSerializer.Serialize(result), JsonSerializer.Serialize<object>(result),
        JsonSerializer.Serialize(result, new JsonSerializerOptions { IncludeFields = true }), result.ToString()];
    foreach (string value in values.Where(value => value.Length > 8))
        Check(serialized.All(text => !text.Contains(value, StringComparison.Ordinal)), "automatic representations omit synthetic field values");
    using var document = JsonDocument.Parse(serialized[0]);
    var allowed = new HashSet<string>(["Status", "ReasonCode", "FieldNames", "FieldCount"], StringComparer.Ordinal);
    Check(document.RootElement.EnumerateObject().All(property => allowed.Contains(property.Name)), "public JSON contains only closed metadata properties");
    Check(!typeof(IEnumerable).IsAssignableFrom(result.GetType()), "result cannot be enumerated into field values");
}
void Rejected(byte[] body, string expected, string? contentType = NativePasswordFormProfile.ContentType)
{
    byte[] original = body.ToArray();
    var result = Read(body, contentType);
    Check(result.Status == "outside_profile" && result.ReasonCode == expected, "outside-profile decision: " + expected);
    Check(result.FieldCount == 0 && result.FieldNames.Count == 0, "rejected result contains no partial field metadata");
    foreach (string name in manualNames.Concat(contextNames))
        Check(!result.TryGetManualField(name, out string? value) && value is null, "rejected result cannot expose a previously parsed field: " + name);
    Check(body.AsSpan().SequenceEqual(original), "rejection does not change signed input bytes");
    NoExposure(result, [username, password]);
}

byte[] basic = Form(Basic()), basicSnapshot = basic.ToArray();
var parsed = Read(basic);
Check(parsed.Status == "within_profile" && parsed.ReasonCode is null && parsed.FieldCount == 3, "minimum password-form syntax profile matched, without authentication");
Check(parsed.TryGetManualField("username", out var nameValue) && nameValue == username, "username requires explicit field access");
Check(parsed.TryGetManualField("password", out var passwordValue) && passwordValue == password, "password requires explicit field access");
Check(basic.AsSpan().SequenceEqual(basicSnapshot), "successful read preserves signed bytes exactly");
Check(!parsed.TryGetManualField(null, out _) && !parsed.TryGetManualField("USERNAME", out _), "manual access does not infer aliases or accept a null name");
NoExposure(parsed, [username, password]);

var all = Basic();
var manualValues = manualNames.Skip(3).ToDictionary(key => key, key => Synthetic(key), StringComparer.Ordinal);
var contextValues = contextNames.ToDictionary(key => key, key => Synthetic("context"), StringComparer.Ordinal);
all.AddRange(manualValues.Select(pair => (pair.Key, pair.Value)));
all.AddRange(contextValues.Select(pair => (pair.Key, pair.Value)));
byte[] allBytes = Form(all), allSnapshot = allBytes.ToArray();
var complete = Read(allBytes);
Check(complete.Status == "within_profile" && complete.FieldCount == 20, "all seven manual and thirteen context names recognized");
Check(complete.FieldNames.SequenceEqual(all.Select(field => field.Key)), "metadata preserves only recognized field names in source order");
foreach (var pair in manualValues)
    Check(complete.TryGetManualField(pair.Key, out string? value) && value == pair.Value, "optional manual field remains opaque: " + pair.Key);
foreach (var pair in contextValues)
    Check(!complete.TryGetManualField(pair.Key, out string? value) && value is null, "context value discarded: " + pair.Key);
Check(allBytes.AsSpan().SequenceEqual(allSnapshot), "context validation preserves raw signed bytes");
NoExposure(complete, [username, password, .. manualValues.Values, .. contextValues.Values]);
foreach (var field in typeof(NativePasswordFormResult).GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic))
{
    object? stored = field.GetValue(complete);
    Check(stored is not byte[], "result retains no encoded body or decoded context byte array");
    if (stored is IDictionary dictionary)
    {
        Check(dictionary.Count == 7, "only manual values are held in the private value map");
        foreach (DictionaryEntry entry in dictionary)
            Check(!contextValues.Values.Contains(entry.Value), "private map retains no context string");
    }
}
bool immutable = false;
try { ((IList<string>)complete.FieldNames).Add("unknown"); }
catch (NotSupportedException) { immutable = true; }
Check(immutable && complete.FieldCount == 20, "public metadata names cannot be mutated");

var escaped = Basic();
string opaque = "\u96ea-" + char.ConvertFromUtf32(0x1f9e9) + "-e\u0301&=+/%";
escaped.Add(("login_context", opaque));
var unicode = Read(Form(escaped));
Check(unicode.TryGetManualField("login_context", out string? opaqueValue) && opaqueValue == opaque, "percent-decoded Unicode and reserved characters remain one opaque value");
Check(unicode.FieldCount == 4, "decoded separators never create new fields");
foreach (string value in new[] { "%26", " ", "e\u0301", "\u00e9", "+" })
{
    var fields = Basic(); fields.Add(("security_code", value));
    var result = Read(Form(fields));
    Check(result.Status == "within_profile" && result.TryGetManualField("security_code", out string? actual) && actual == value,
        "manual value decoded once with no trimming, Unicode normalization or MFA interpretation");
}
var plusUsername = Basic(); plusUsername[1] = ("username", username + "+");
Check(Read(Form(plusUsername)).TryGetManualField("username", out string? plus) && plus == username + "+", "encoded plus remains plus, never space");
string encodedBasic = Encoding.ASCII.GetString(basic);
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&security_code=+"), "literal_plus_outside_profile");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&login_context=%"), "malformed_percent_encoding");
foreach (string bad in new[] { "%0", "%GG", "%2g", "%g2" })
    Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&security_code=" + bad), "malformed_percent_encoding");
foreach (string bad in new[] { "%FF", "%C0%AF", "%F0%9F%8E", "%ED%A0%80", "%F4%90%80%80" })
{
    Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&security_code=" + bad), "invalid_utf8");
    Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&client_secret=" + bad), "invalid_utf8");
}
Rejected([.. basic, (byte)'&', (byte)'x', (byte)'=', 0xc3, 0xa9], "non_ascii_body");
foreach (string suffix in new[] { "&security_code=unescaped space", "&security_code=/", "&security_code=\r", "&security_code=\u007f" })
    Rejected(Encoding.ASCII.GetBytes(encodedBasic + suffix), "unescaped_character_outside_profile");
foreach (string suffix in new[] { "&", "&&security_code=x", "&security_code", "&=x", "&security_code=x=y" })
    Rejected(Encoding.ASCII.GetBytes(encodedBasic + suffix), "malformed_field");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&%75sername=" + Uri.EscapeDataString(username)), "duplicate_field");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&%2575sername=x"), "unknown_field");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&USERNAME=x"), "unknown_field");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&login_context=x&login_context=y"), "duplicate_field");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&client_secret=x&%63lient_secret=y"), "duplicate_field");
string unknownName = Synthetic("unknown");
var unknown = Read(Encoding.ASCII.GetBytes(encodedBasic + "&" + unknownName + "=x"));
Check(unknown.ReasonCode == "unknown_field" && !JsonSerializer.Serialize(unknown).Contains(unknownName, StringComparison.Ordinal), "unknown field name is not echoed into a diagnostic");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&" + unknownName + "=x"), "unknown_field");

foreach (string required in manualNames.Take(3)) Rejected(Form(Basic().Where(field => field.Key != required)), "required_field_missing");
foreach (string empty in new[] { "username", "password" })
    Rejected(Form(Basic().Select(field => field.Key == empty ? (field.Key, "") : field)), "empty_credentials");
var wrongGrant = Basic(); wrongGrant[0] = ("grant_type", "client_credentials");
Rejected(Form(wrongGrant), "grant_outside_profile");
Rejected([], "missing_body");
foreach (string? type in new string?[] { null, "", "application/json", "Application/x-www-form-urlencoded", NativePasswordFormProfile.ContentType + "; charset=utf-8", " " + NativePasswordFormProfile.ContentType, Synthetic("content-type") })
    Rejected(basic, "content_type_outside_profile", type);

foreach (int length in new[] { NativePasswordFormProfile.MaxValueBytes, NativePasswordFormProfile.MaxValueBytes + 1 })
{
    var fields = Basic(); fields.Add(("login_context", new string('v', length)));
    if (length == NativePasswordFormProfile.MaxValueBytes)
        Check(Read(Form(fields)).TryGetManualField("login_context", out string? value) && value!.Length == length, "value limit accepts exactly eight KiB decoded bytes");
    else Rejected(Form(fields), "value_budget_exceeded");
}
string fourByte = char.ConvertFromUtf32(0x1f9e9);
var wide = Basic(); wide.Add(("security_code", string.Concat(Enumerable.Repeat(fourByte, 2048))));
Check(Read(Form(wide)).Status == "within_profile", "value byte budget accepts 8192 bytes of valid four-byte UTF-8");
wide[^1] = ("security_code", wide[^1].Value + "x"); Rejected(Form(wide), "value_budget_exceeded");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&" + new string('k', 64) + "=x"), "unknown_field");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + "&" + new string('k', 65) + "=x"), "key_budget_exceeded");
Rejected(Encoding.ASCII.GetBytes(encodedBasic + string.Concat(Enumerable.Repeat("&security_code=x", 30))), "field_budget_exceeded");

byte[] SizedBody(int target)
{
    var fields = Basic(); fields.AddRange(contextNames.Select(name => (name, "")));
    int remaining = target - Form(fields).Length;
    for (int index = 3; index < fields.Count && remaining > 0; index++)
    {
        int count = Math.Min(remaining, NativePasswordFormProfile.MaxValueBytes);
        fields[index] = (fields[index].Key, new string('v', count)); remaining -= count;
    }
    if (remaining != 0) throw new InvalidOperationException("sized fixture did not fit authored field limits");
    return Form(fields);
}
byte[] exactBody = SizedBody(NativePasswordFormProfile.MaxBodyBytes);
Check(exactBody.Length == NativePasswordFormProfile.MaxBodyBytes && Read(exactBody).Status == "within_profile", "body budget accepts exactly 64 KiB without retaining context values");
Rejected(SizedBody(NativePasswordFormProfile.MaxBodyBytes + 1), "body_budget_exceeded");

var report = new { status = "native_password_form_profile_self_tests_passed", checks = checks.Count, tests = checks,
    input = "generated synthetic form bytes only", originalClientExecuted = false, credentialsUsed = false,
    authenticated = false, tokenIssued = false, operationalRouteChanged = false };
using (var output = new FileStream(args[1], FileMode.CreateNew))
    JsonSerializer.Serialize(output, report, new JsonSerializerOptions { WriteIndented = true });
Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks }));
