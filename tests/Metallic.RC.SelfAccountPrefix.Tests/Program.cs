using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;

if (args.Length is <2 or >3)
    throw new ArgumentException("Use --self-test NEW-result.json or fixtures.jsonl NEW-results.jsonl [casefold|first-wins|empty-object|swap-int-fields].");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");

if (args[0] == "--self-test")
{
    if (args.Length != 2) throw new ArgumentException("Self-test does not accept a mutation.");
    int checks = 0;
    var names = new List<string>();
    void Check(bool condition,string name)
    {
        checks++;
        if (!condition) throw new Exception(name);
        names.Add(name);
    }
    SelfAccountPrefixResult Read(string json) => SelfAccountPrefixContract.Read(Encoding.UTF8.GetBytes(json));
    void Reject(string json,string code)
    {
        bool rejected = false;
        try { Read(json); }
        catch (SelfAccountPrefixException error) { rejected = error.Code == code; }
        catch (JsonException) { rejected = code == "invalid_json"; }
        Check(rejected,"reject " + code);
    }
    void RejectBytes(byte[] json,string code)
    {
        bool rejected = false;
        try { SelfAccountPrefixContract.Read(json); }
        catch (SelfAccountPrefixException error) { rejected = error.Code == code; }
        catch (JsonException) { rejected = code == "invalid_json"; }
        Check(rejected,"reject bytes " + code);
    }

    Check(Read("null").Status == "root_null","root null");
    Check(Read("{}").Status == "root_empty_object_null","empty object returns null profile");
    var unknown = Read("{\"unknown\":{\"nested\":[1,true,null]}}");
    Check(unknown.Status == "prefix_complete" && unknown.Fields!.Count == 0,"unknown-only nonempty object");
    var duplicate = Read("{\"Email\":\"first\",\"email\":null}");
    Check(duplicate.Fields!.ContainsKey("Email") && duplicate.Fields["Email"] is null,"explicit string null is present and wins");
    Check(!duplicate.Fields.ContainsKey("Phone"),"absent fields are omitted");
    Check(Read("{\"UserName\":\"first\",\"username\":\"last\"}").Fields!["UserName"] is "last","duplicate alias last wins");
    Check(Read("{\"EMAIL\":\"ignored\"}").Fields!.Count == 0,"no blanket casefold");
    Check(Read("{\"Ema\\u0069l\":\"ignored\"}").Fields!.Count == 0,"escaped property names are not widened into raw aliases");
    Check(Read("{\"AccountId\":-2147483648,\"availableUsernameChanges\":2147483647}").Fields!["AccountId"] is int.MinValue,"signed Int32 lower bound");
    Check(Read("{\"AccountId\":-2147483648,\"availableUsernameChanges\":2147483647}").Fields!["AvailableUsernameChanges"] is int.MaxValue,"signed Int32 upper bound");
    foreach (string invalid in new[]{"null","true","\"1\"","1.0","1e0","2147483648","-2147483649"})
        Check(Read("{\"AccountId\":" + invalid + "}").Status == "known_value_type_error","strict Int32: " + invalid);
    foreach (string invalid in new[]{"null","0","1","\"false\"","{}","[]"})
        Check(Read("{\"TreatAsJunior\":" + invalid + "}").Status == "known_value_type_error","strict Boolean: " + invalid);
    var booleans = Read("{\"TreatAsJunior\":true,\"hasbirthday\":false,\"IsMetaPlatformBlocked\":true}");
    Check(booleans.Fields!["TreatAsJunior"] is true && booleans.Fields["HasBirthday"] is false && booleans.Fields["IsMetaPlatformBlocked"] is true,"three native Boolean branches");
    foreach (string invalid in new[]{"0","true","{}","[]"})
        Check(Read("{\"DisplayName\":" + invalid + "}").Status == "known_value_type_error","strict string/null: " + invalid);
    foreach (string field in new[]{"Birthday","JuniorState","ParentAccountId","IsFakeJuniorBirthday","PersonalPronouns","IdentityFlags","createdAt","IsJunior"})
        foreach (string value in new[]{"null","0","\"synthetic\""})
            Check(Read("{\"" + field + "\":" + value + "}").Status == "nested_value_outside_profile","nested value stays outside: " + field);
    Check(Read("{\"CreatedAt\":null}").Status == "nested_value_outside_profile","observed uppercase CreatedAt alias");
    Check(SelfAccountPrefixContract.CanonicalNames.Count == 20 && SelfAccountPrefixContract.CanonicalNames[17] == "createdAt","canonical mixed-case field list");
    Reject("[]","object_or_null_required");
    Reject("true","object_or_null_required");
    Reject("", "missing_json_value");
    Reject("null {}","invalid_json");
    Reject("{} {}","invalid_json");
    Reject("{\"Email\":\"x\",}","invalid_json");
    Reject("{\"unknown\": [}","invalid_json");
    Reject("{/*comment*/}","invalid_json");
    RejectBytes([.. "{\"Email\":\""u8,0xff,.. "\"}"u8],"invalid_utf8");
    RejectBytes([.. "{\"unknown\":\""u8,0xff,.. "\"}"u8],"invalid_utf8");
    RejectBytes([.. "{\""u8,0xff,.. "\":null}"u8],"invalid_utf8");
    RejectBytes([.. "{\"Email\":\""u8,0xc0,0xaf,.. "\"}"u8],"invalid_utf8");
    RejectBytes([.. "{\"Email\":\""u8,0xed,0xa0,0x80,.. "\"}"u8],"invalid_utf8");
    RejectBytes([.. "{\"unknown\":\""u8,0xc2,.. "\"}"u8],"invalid_utf8");
    Reject("{\"Email\":\"\\uD800\"}","invalid_string");
    Reject("{\"Email\":\"\\uDC00\"}","invalid_string");
    Reject("{\"Email\":\"\\uD800\\uD800\"}","invalid_string");
    Check(Read("{\"Email\":\"\\uD83D\\uDE00\"}").Fields!["Email"] is "\U0001f600","valid escaped surrogate pair");
    Check(Read("{\"Email\":\"\U0001f600\"}").Fields!["Email"] is "\U0001f600","valid four-byte UTF8 string");
    Reject("{\"unknown\":" + new string('[',SelfAccountPrefixContract.MaxDepth) + "0" + new string(']',SelfAccountPrefixContract.MaxDepth) + "}","invalid_json");
    Reject(new string(' ',SelfAccountPrefixContract.MaxJsonBytes+1),"response_too_large");
    string property = "\"unknown\":null";
    Reject("{" + string.Join(',',Enumerable.Repeat(property,SelfAccountPrefixContract.MaxProperties+1)) + "}","property_budget_exceeded");
    Check(Read("{" + string.Join(',',Enumerable.Repeat(property,SelfAccountPrefixContract.MaxProperties)) + "}").Fields!.Count == 0,"property admission boundary");
    var privateValues = Read("{\"Email\":\"synthetic-secret\",\"Phone\":\"synthetic-phone\"}");
    Check(!privateValues.ToString().Contains("synthetic",StringComparison.Ordinal),"incidental formatting redacts values");
    bool readOnly = false;
    try { ((IDictionary<string,object?>)privateValues.Fields!).Add("extra",1); }
    catch (NotSupportedException) { readOnly = true; }
    Check(readOnly,"returned presence map is read-only");

    var result = new { status = "self_account_prefix_self_tests_passed", checks, tests = names,
        nativeAccountConstructed = false, nativeLoginVerified = false, originalGameExecuted = false };
    using var stream = new FileStream(args[1],FileMode.CreateNew);
    JsonSerializer.Serialize(stream,result,new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { result.status, checks }));
    return;
}

string? mutation = args.Length == 3 ? args[2] : null;
if (mutation is not (null or "casefold" or "first-wins" or "empty-object" or "swap-int-fields"))
    throw new ArgumentException("Unknown negative control.");

string Mutate(string input)
{
    if (mutation is null) return input;
    using var document = JsonDocument.Parse(input);
    if (document.RootElement.ValueKind != JsonValueKind.Object) return input;
    if (mutation == "empty-object" && !document.RootElement.EnumerateObject().Any())
        return "{\"unknown\":null}";
    using var stream = new MemoryStream();
    using (var writer = new Utf8JsonWriter(stream))
    {
        writer.WriteStartObject();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var property in document.RootElement.EnumerateObject())
        {
            string name = property.Name;
            string? canonical = SelfAccountPrefixContract.CanonicalNames.FirstOrDefault(value => value.Equals(name,StringComparison.OrdinalIgnoreCase));
            if (mutation == "casefold" && canonical is not null) name = canonical;
            if (mutation == "first-wins" && canonical is not null && !seen.Add(canonical)) continue;
            if (mutation == "swap-int-fields" && canonical is "AccountId" or "AvailableUsernameChanges")
                name = canonical == "AccountId" ? "AvailableUsernameChanges" : "AccountId";
            writer.WritePropertyName(name);
            property.Value.WriteTo(writer);
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
    // Expected observations are deliberately not read by this executable.
    try
    {
        var result = SelfAccountPrefixContract.Read(Encoding.UTF8.GetBytes(Mutate(input)));
        output.WriteLine(JsonSerializer.Serialize(new { id, status = result.Status, fields = result.Fields }));
    }
    catch (SelfAccountPrefixException error)
    { output.WriteLine(JsonSerializer.Serialize(new { id, status = error.Code, fields = (object?)null })); }
    catch (JsonException)
    { output.WriteLine(JsonSerializer.Serialize(new { id, status = "invalid_json", fields = (object?)null })); }
}
Console.WriteLine(JsonSerializer.Serialize(new { status = "self_account_prefix_fixture_run_completed", cases,
    negativeControl = mutation, syntheticFixtureValuesOnly = true, nativeLoginVerified = false }));
