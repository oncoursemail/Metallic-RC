using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Metallic.RC.Compatibility;

if (args.Length is <2 or >3) throw new ArgumentException("Use --self-test NEW-result.json or fixtures.jsonl NEW-results.jsonl [unix-seconds|unspecified-kind|declared-enums-only|omit-birthday-null|first-wins].");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");
SelfAccountPrefixResult Read(string json) => SelfAccountPrefixContract.ReadWithDefaultNumericScalars(Encoding.UTF8.GetBytes(json));
var options = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };
options.Converters.Add(new JsonStringEnumConverter());

if (args[0] == "--self-test")
{
    if (args.Length != 2) throw new ArgumentException("Self-test does not accept mutations.");
    var tests = new List<string>();
    void Check(bool condition,string name) { if (!condition) throw new Exception(name); tests.Add(name); }
    Check(Read("null").Status == "root_null","root null");
    Check(Read("{}").Status == "root_empty_object_null","empty-object null");
    Check(Read("{\"unknown\":null}").Fields!.Count == 0,"no invented absent fields");
    var birthdayNull = Read("{\"Birthday\":null}");
    Check(birthdayNull.Fields!.ContainsKey("Birthday") && birthdayNull.Fields["Birthday"] is null,"Birthday null remains present");
    foreach (string name in new[]{"Birthday","createdAt"})
    {
        foreach (long ticks in new[]{0L,1L,3_155_378_975_999_999_999L})
        {
            var result = Read("{\"" + name + "\":" + ticks + "}");
            Check(result.Fields![name] is SelfAccountDateTimePrefixValue date && date.Ticks == ticks && date.Kind == DateTimeKind.Utc,"UTC ticks: " + name + ticks);
        }
        foreach (string invalid in new[]{"-1","3155378976000000000","9223372036854775807","-9223372036854775808"})
            Check(Read("{\"" + name + "\":" + invalid + "}").Status == "known_value_range_error","date range: " + name + invalid);
        foreach (string invalid in new[]{"9223372036854775808","1.0","1e0"})
            Check(Read("{\"" + name + "\":" + invalid + "}").Status == "known_value_type_error","date Int64 lexical profile: " + name + invalid);
        foreach (string outside in new[]{"\"2026-10-28T12:30:00Z\"","true","false","{}","[]"})
            Check(Read("{\"" + name + "\":" + outside + "}").Status == "nested_value_outside_profile","nonnumeric date outside: " + name + outside);
    }
    Check(Read("{\"createdAt\":null}").Status == "nested_value_outside_profile","nonnull DateTime null remains outside");
    foreach (string name in new[]{"JuniorState","PersonalPronouns","IdentityFlags"})
    {
        foreach (int value in new[]{int.MinValue,-1,0,3,777,int.MaxValue})
            Check(Read("{\"" + name + "\":" + value + "}").Fields![name] is int number && number == value,"enum underlying value, including undefined: " + name + value);
        foreach (string invalid in new[]{"null","true","false","1.0","2147483648","-2147483649","{}","[]"})
            Check(Read("{\"" + name + "\":" + invalid + "}").Status == "known_value_type_error","enum strict Int32: " + name + invalid);
        Check(Read("{\"" + name + "\":\"None\"}").Status == "nested_value_outside_profile","enum names stay outside: " + name);
    }
    var duplicate = Read("{\"Birthday\":1,\"birthday\":null,\"CreatedAt\":0,\"createdat\":1,\"JuniorState\":1,\"juniorstate\":-7}");
    Check(duplicate.Fields!["Birthday"] is null,"last Birthday alias wins with null");
    Check(duplicate.Fields["createdAt"] is SelfAccountDateTimePrefixValue dateLast && dateLast.Ticks == 1,"last createdAt alias wins");
    Check(duplicate.Fields["JuniorState"] is -7,"last enum alias wins");
    Check(Read("{\"BIRTHDAY\":0,\"JUNIORSTATE\":0,\"CREATEDAT\":0}").Fields!.Count == 0,"casefold not added");
    Check(Read("{\"Birthd\\u0061y\":0,\"JuniorSt\\u0061te\":0}").Fields!.Count == 0,"escaped aliases not widened");
    foreach (string name in new[]{"Birthday","JuniorState","PersonalPronouns","IdentityFlags","createdAt"})
    {
        byte[] raw = Encoding.UTF8.GetBytes("{\"" + name + "\":0}");
        Check(SelfAccountPrefixContract.Read(raw).Status == "nested_value_outside_profile","12-field reader unchanged: " + name);
        Check(SelfAccountPrefixContract.ReadWithDefaultNullablePrimitives(raw).Status == "nested_value_outside_profile","15-field reader unchanged: " + name);
    }
    var all = Read("{\"Email\":null,\"Phone\":null,\"Birthday\":0,\"JuniorState\":-1,\"ParentAccountId\":null,\"IsFakeJuniorBirthday\":false,\"AvailableUsernameChanges\":0,\"AccountId\":1,\"UserName\":null,\"DisplayName\":null,\"DisplayEmoji\":null,\"ProfileImage\":null,\"BannerImage\":null,\"TreatAsJunior\":false,\"HasBirthday\":true,\"PersonalPronouns\":777,\"IdentityFlags\":-1,\"createdAt\":1,\"IsMetaPlatformBlocked\":false,\"IsJunior\":null}");
    Check(all.Status == "prefix_complete" && all.Fields!.Count == 20,"all 20 observed fields in explicit numeric profile");
    var diagnostic = (SelfAccountDateTimePrefixValue)all.Fields!["Birthday"]!;
    Check(!typeof(SelfAccountDateTimePrefixValue).GetProperty("Ticks")!.CanWrite,"diagnostic ticks are immutable");
    Check(!diagnostic.ToString().Contains("Ticks",StringComparison.Ordinal),"diagnostic incidental output redacts values");
    bool immutable = false;
    try { ((IDictionary<string,object?>)all.Fields).Add("extra",0); } catch (NotSupportedException) { immutable = true; }
    Check(immutable,"result map is read-only");
    foreach (byte[] invalid in new byte[][] { [.. "{\"unknown\":\""u8,0xff,.. "\"}"u8], Encoding.UTF8.GetBytes("{\"Email\":\"\\uD800\"}"), Encoding.UTF8.GetBytes("{\"Birthday\":0,}") })
    {
        bool rejected = false;
        try { SelfAccountPrefixContract.ReadWithDefaultNumericScalars(invalid); } catch (JsonException) { rejected = true; }
        Check(rejected,"shared lexical admission");
    }
    var report = new { status = "self_account_numeric_prefix_self_tests_passed", checks = tests.Count, tests, originalGameExecuted = false, accountConstructed = false, nativeLoginVerified = false };
    using var stream = new FileStream(args[1],FileMode.CreateNew);
    JsonSerializer.Serialize(stream,report,new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks }));
    return;
}

string? mutation = args.Length == 3 ? args[2] : null;
if (mutation is not (null or "unix-seconds" or "unspecified-kind" or "declared-enums-only" or "omit-birthday-null" or "first-wins")) throw new ArgumentException("Unknown negative control.");
string FirstWins(string input)
{
    if (mutation != "first-wins") return input;
    using var doc = JsonDocument.Parse(input);
    if (doc.RootElement.ValueKind != JsonValueKind.Object) return input;
    using var stream = new MemoryStream();
    using (var writer = new Utf8JsonWriter(stream))
    {
        writer.WriteStartObject();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var property in doc.RootElement.EnumerateObject())
        {
            if (!seen.Add(property.Name)) continue;
            property.WriteTo(writer);
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
    if (++cases > 20_000 || line.Length > 2*SelfAccountPrefixContract.MaxJsonBytes) throw new InvalidDataException("Fixture budget exceeded.");
    using var doc = JsonDocument.Parse(line);
    int id = doc.RootElement.GetProperty("id").GetInt32();
    if (id < 0 || !ids.Add(id)) throw new InvalidDataException("Invalid or duplicate fixture identity.");
    string raw = doc.RootElement.GetProperty("inputJson").GetString() ?? throw new InvalidDataException("Missing input document.");
    // Native observations remain unread; comparison is external to this executable.
    try
    {
        var result = Read(FirstWins(raw));
        string status = result.Status;
        Dictionary<string,object?>? fields = result.Fields?.ToDictionary();
        if (fields is not null && mutation is not null)
        {
            foreach (var entry in fields.ToArray())
            {
                if (entry.Value is SelfAccountDateTimePrefixValue date)
                {
                    if (mutation == "unix-seconds") fields[entry.Key] = new { ticks = (decimal)date.Ticks*10_000_000+621_355_968_000_000_000, kind = "Utc" };
                    if (mutation == "unspecified-kind") fields[entry.Key] = new { ticks = date.Ticks, kind = "Unspecified" };
                }
                if (mutation == "omit-birthday-null" && entry.Key == "Birthday" && entry.Value is null) fields.Remove(entry.Key);
                if (mutation == "declared-enums-only" && entry.Value is int number && entry.Key is "JuniorState" or "PersonalPronouns" or "IdentityFlags")
                {
                    int[] declared = entry.Key == "JuniorState" ? [0,1,2] : entry.Key == "PersonalPronouns" ? [0,1,2,4,8,16,32] : [0,1,2,4,8,16,32,64,128,256,512];
                    if (!declared.Contains(number)) { status = "known_value_type_error"; fields = null; break; }
                }
            }
        }
        output.WriteLine(JsonSerializer.Serialize(new { id, status, fields },options));
    }
    catch (SelfAccountPrefixException error) { output.WriteLine(JsonSerializer.Serialize(new { id, status = error.Code, fields = (object?)null },options)); }
    catch (JsonException) { output.WriteLine(JsonSerializer.Serialize(new { id, status = "invalid_json", fields = (object?)null },options)); }
}
Console.WriteLine(JsonSerializer.Serialize(new { status = "self_account_numeric_prefix_fixture_run_completed", cases, negativeControl = mutation, originalGameExecuted = false }));
