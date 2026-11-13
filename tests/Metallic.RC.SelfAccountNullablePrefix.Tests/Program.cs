using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;

if (args.Length is <2 or >3)
    throw new ArgumentException("Use --self-test NEW-result.json or fixtures.jsonl NEW-results.jsonl [reject-null|bool-as-int|omit-null|first-wins].");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");

SelfAccountPrefixResult Read(string json) => SelfAccountPrefixContract.ReadWithDefaultNullablePrimitives(Encoding.UTF8.GetBytes(json));
string[] nullableNames = ["ParentAccountId","IsFakeJuniorBirthday","IsJunior"];

if (args[0] == "--self-test")
{
    if (args.Length != 2) throw new ArgumentException("Self-test does not accept a mutation.");
    var tests = new List<string>();
    void Check(bool condition,string name)
    {
        if (!condition) throw new Exception(name);
        tests.Add(name);
    }
    void RejectBytes(byte[] json,string code)
    {
        bool rejected = false;
        try { SelfAccountPrefixContract.ReadWithDefaultNullablePrimitives(json); }
        catch (SelfAccountPrefixException error) { rejected = error.Code == code; }
        catch (JsonException) { rejected = code == "invalid_json"; }
        Check(rejected,"admission " + code);
    }
    Check(Read("null").Status == "root_null","root null");
    Check(Read("{}").Status == "root_empty_object_null","empty object null");
    Check(Read("{\"unknown\":null}").Fields!.Count == 0,"unknown nonempty object has no invented fields");
    foreach (string name in nullableNames)
    {
        string input = "{\"" + name + "\":null}";
        var result = Read(input);
        Check(result.Status == "prefix_complete" && result.Fields!.ContainsKey(name) && result.Fields[name] is null,"explicit nullable null present: " + name);
        Check(result.Fields!.Count == 1,"other nullable fields absent: " + name);
        Check(SelfAccountPrefixContract.Read(Encoding.UTF8.GetBytes(input)).Status == "nested_value_outside_profile","original profile unchanged: " + name);
    }
    foreach (string alias in new[]{"ParentAccountId","parentAccountId","parentaccountid"})
        Check(Read("{\"" + alias + "\":-2147483648}").Fields!["ParentAccountId"] is int.MinValue,"Int32 alias: " + alias);
    Check(Read("{\"ParentAccountId\":2147483647}").Fields!["ParentAccountId"] is int.MaxValue,"nullable Int32 upper bound");
    foreach (string invalid in new[]{"true","false","\"1\"","1.0","1e0","2147483648","-2147483649","{}","[]"})
        Check(Read("{\"ParentAccountId\":" + invalid + "}").Status == "known_value_type_error","strict nullable Int32: " + invalid);
    foreach (string name in new[]{"IsFakeJuniorBirthday","IsJunior"})
    {
        foreach (string value in new[]{"true","false"})
            Check(Read("{\"" + name + "\":" + value + "}").Fields![name] is bool actual && actual == (value == "true"),"nullable Boolean value: " + name + value);
        foreach (string invalid in new[]{"0","1","-1","\"false\"","{}","[]"})
            Check(Read("{\"" + name + "\":" + invalid + "}").Status == "known_value_type_error","strict nullable Boolean: " + name + invalid);
    }
    var duplicates = Read("{\"ParentAccountId\":7,\"parentaccountid\":null,\"IsJunior\":null,\"isjunior\":false,\"IsFakeJuniorBirthday\":false,\"isfakejuniorbirthday\":true}");
    Check(duplicates.Fields!["ParentAccountId"] is null,"last nullable Int32 alias wins with null");
    Check(duplicates.Fields["IsJunior"] is false,"last nullable Boolean alias wins with false");
    Check(duplicates.Fields["IsFakeJuniorBirthday"] is true,"last nullable Boolean alias wins with true");
    Check(Read("{\"PARENTACCOUNTID\":7,\"ISJUNIOR\":true}").Fields!.Count == 0,"no casefold expansion");
    Check(Read("{\"ParentAccount\\u0049d\":7,\"IsJun\\u0069or\":true}").Fields!.Count == 0,"no escaped alias expansion");
    foreach (string field in new[]{"Birthday","JuniorState","PersonalPronouns","IdentityFlags","createdAt"})
        foreach (string value in new[]{"null","0","\"synthetic\""})
            Check(Read("{\"" + field + "\":" + value + "}").Status == "nested_value_outside_profile","remaining nested value outside: " + field + value);
    var mixed = Read("{\"Email\":null,\"ParentAccountId\":-9,\"unknown\":[{\"IsJunior\":0}],\"IsJunior\":true,\"AccountId\":11}");
    Check(mixed.Fields!.Count == 4 && mixed.Fields["ParentAccountId"] is -9 && mixed.Fields["AccountId"] is 11 && mixed.Fields["IsJunior"] is true,"mixed native locals and unknown nested skip");
    Check(Read("{\"ParentAccountId\":true,\"Birthday\":null}").Status == "known_value_type_error","type failure precedes unsupported field");
    Check(Read("{\"Birthday\":null,\"ParentAccountId\":true}").Status == "nested_value_outside_profile","unsupported field stops before later type failure");
    RejectBytes([.. "{\"unknown\":\""u8,0xff,.. "\"}"u8],"invalid_utf8");
    RejectBytes(Encoding.UTF8.GetBytes("{\"Email\":\"\\uD800\"}"),"invalid_string");
    RejectBytes(Encoding.UTF8.GetBytes("{\"IsJunior\":null,}"),"invalid_json");
    RejectBytes(Encoding.UTF8.GetBytes("{} {}"),"invalid_json");
    RejectBytes(new byte[SelfAccountPrefixContract.MaxJsonBytes+1],"response_too_large");
    bool immutable = false;
    try { ((IDictionary<string,object?>)duplicates.Fields).Add("extra",1); }
    catch (NotSupportedException) { immutable = true; }
    Check(immutable,"presence map is read-only");
    var report = new { status = "self_account_nullable_prefix_self_tests_passed", checks = tests.Count, tests,
        nativeLoginVerified = false, originalGameExecuted = false, accountConstructed = false };
    using var stream = new FileStream(args[1],FileMode.CreateNew);
    JsonSerializer.Serialize(stream,report,new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks }));
    return;
}

string? mutation = args.Length == 3 ? args[2] : null;
if (mutation is not (null or "reject-null" or "bool-as-int" or "omit-null" or "first-wins"))
    throw new ArgumentException("Unknown negative control.");
string Mutate(string input)
{
    if (mutation is null) return input;
    using var doc = JsonDocument.Parse(input);
    if (doc.RootElement.ValueKind != JsonValueKind.Object) return input;
    using var stream = new MemoryStream();
    using (var writer = new Utf8JsonWriter(stream))
    {
        writer.WriteStartObject();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var property in doc.RootElement.EnumerateObject())
        {
            string name = property.Name;
            string? canonical = SelfAccountPrefixContract.CanonicalNames.FirstOrDefault(value => value.Equals(name,StringComparison.OrdinalIgnoreCase));
            bool nullable = nullableNames.Contains(canonical,StringComparer.Ordinal);
            bool isNull = property.Value.ValueKind == JsonValueKind.Null;
            if (mutation == "omit-null" && nullable && isNull) continue;
            if (mutation == "first-wins" && canonical is not null && !seen.Add(canonical)) continue;
            writer.WritePropertyName(name);
            if (mutation == "reject-null" && nullable && isNull) { writer.WriteStartObject(); writer.WriteEndObject(); }
            else if (mutation == "bool-as-int" && canonical is "IsJunior" or "IsFakeJuniorBirthday" && property.Value.ValueKind == JsonValueKind.Number && property.Value.TryGetInt32(out int number))
                writer.WriteBooleanValue(number != 0);
            else property.Value.WriteTo(writer);
        }
        writer.WriteEndObject();
    }
    return Encoding.UTF8.GetString(stream.ToArray());
}

using var output = new StreamWriter(new FileStream(args[1],FileMode.CreateNew));
var ids = new HashSet<int>();
int cases = 0;
foreach (string line in File.ReadLines(args[0]))
{
    if (++cases > 20_000 || line.Length > 2*SelfAccountPrefixContract.MaxJsonBytes)
        throw new InvalidDataException("Fixture budget exceeded.");
    using var document = JsonDocument.Parse(line);
    var row = document.RootElement;
    int id = row.GetProperty("id").GetInt32();
    if (id < 0 || !ids.Add(id)) throw new InvalidDataException("Duplicate or invalid fixture identity.");
    string input = row.GetProperty("inputJson").GetString() ?? throw new InvalidDataException("Fixture input is missing.");
    // The executable reads inputs only; native observations are compared externally.
    try
    {
        var result = Read(Mutate(input));
        output.WriteLine(JsonSerializer.Serialize(new { id, status = result.Status, fields = result.Fields }));
    }
    catch (SelfAccountPrefixException error)
    { output.WriteLine(JsonSerializer.Serialize(new { id, status = error.Code, fields = (object?)null })); }
    catch (JsonException)
    { output.WriteLine(JsonSerializer.Serialize(new { id, status = "invalid_json", fields = (object?)null })); }
}
Console.WriteLine(JsonSerializer.Serialize(new { status = "self_account_nullable_prefix_fixture_run_completed", cases, negativeControl = mutation,
    syntheticFixtureValuesOnly = true, nativeLoginVerified = false }));
