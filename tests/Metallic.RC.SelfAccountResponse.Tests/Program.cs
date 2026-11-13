using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;

if (args.Length is <2 or >3) throw new ArgumentException("Use --self-test NEW-result.json or fixtures.jsonl NEW-results.jsonl [drop-field|swap-ids|unix-seconds].");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");

NativeSelfAccountResponseModel FromJson(JsonElement m) => new(
    m.GetProperty("Email").GetString(),m.GetProperty("Phone").GetString(),
    m.GetProperty("Birthday").ValueKind == JsonValueKind.Null ? null : m.GetProperty("Birthday").GetInt64(),
    m.GetProperty("JuniorState").GetInt32(),
    m.GetProperty("ParentAccountId").ValueKind == JsonValueKind.Null ? null : m.GetProperty("ParentAccountId").GetInt32(),
    m.GetProperty("IsFakeJuniorBirthday").ValueKind == JsonValueKind.Null ? null : m.GetProperty("IsFakeJuniorBirthday").GetBoolean(),
    m.GetProperty("AvailableUsernameChanges").GetInt32(),m.GetProperty("AccountId").GetInt32(),
    m.GetProperty("UserName").GetString(),m.GetProperty("DisplayName").GetString(),m.GetProperty("DisplayEmoji").GetString(),
    m.GetProperty("ProfileImage").GetString(),m.GetProperty("BannerImage").GetString(),
    m.GetProperty("TreatAsJunior").GetBoolean(),m.GetProperty("HasBirthday").GetBoolean(),
    m.GetProperty("PersonalPronouns").GetInt32(),m.GetProperty("IdentityFlags").GetInt32(),m.GetProperty("createdAt").GetInt64(),
    m.GetProperty("IsMetaPlatformBlocked").GetBoolean(),
    m.GetProperty("IsJunior").ValueKind == JsonValueKind.Null ? null : m.GetProperty("IsJunior").GetBoolean());

if (args[0] == "--self-test")
{
    if (args.Length != 2) throw new ArgumentException("Self-test does not accept mutations.");
    var tests = new List<string>();
    void Check(bool condition,string name) { if (!condition) throw new Exception(name); tests.Add(name); }
    NativeSelfAccountResponseModel Sample() => new("synthetic@example.invalid",null,0,17,null,false,3,7,
        "synthetic","\"\\\n<>&\U0001f600",null,"synthetic-image",null,false,true,777,-1,1,false,null);
    NativeSelfAccountResponseModel StringCase(NativeSelfAccountResponseModel m,int field,string? v) => new(
        field == 0 ? v : m.Email,field == 1 ? v : m.Phone,m.BirthdayUtcTicks,m.JuniorState,m.ParentAccountId,m.IsFakeJuniorBirthday,
        m.AvailableUsernameChanges,m.AccountId,field == 8 ? v : m.UserName,field == 9 ? v : m.DisplayName,
        field == 10 ? v : m.DisplayEmoji,field == 11 ? v : m.ProfileImage,field == 12 ? v : m.BannerImage,
        m.TreatAsJunior,m.HasBirthday,m.PersonalPronouns,m.IdentityFlags,m.CreatedAtUtcTicks,m.IsMetaPlatformBlocked,m.IsJunior);
    NativeSelfAccountResponseModel DateCase(NativeSelfAccountResponseModel m,long? birthday,long created) => new(
        m.Email,m.Phone,birthday,m.JuniorState,m.ParentAccountId,m.IsFakeJuniorBirthday,m.AvailableUsernameChanges,m.AccountId,
        m.UserName,m.DisplayName,m.DisplayEmoji,m.ProfileImage,m.BannerImage,m.TreatAsJunior,m.HasBirthday,m.PersonalPronouns,
        m.IdentityFlags,created,m.IsMetaPlatformBlocked,m.IsJunior);
    void Reject(NativeSelfAccountResponseModel m,string code)
    {
        bool rejected = false;
        try { NativeSelfAccountResponseProfile.Write(m); }
        catch (NativeSelfAccountResponseException error) { rejected = error.Code == code && error.Message == code; }
        Check(rejected,"reject " + code);
    }
    var sample = Sample();
    byte[] bytes = NativeSelfAccountResponseProfile.Write(sample);
    using var document = JsonDocument.Parse(bytes);
    var root = document.RootElement;
    Check(root.EnumerateObject().Select(p => p.Name).SequenceEqual(SelfAccountPrefixContract.CanonicalNames),"all20 canonical keys in observed order");
    Check(root.GetProperty("Phone").ValueKind == JsonValueKind.Null && root.GetProperty("ParentAccountId").ValueKind == JsonValueKind.Null && root.GetProperty("IsJunior").ValueKind == JsonValueKind.Null,"explicit nullable values are emitted");
    Check(root.GetProperty("Birthday").GetInt64() == 0 && root.GetProperty("createdAt").GetInt64() == 1,"numeric UTC ticks without epoch conversion");
    Check(root.GetProperty("IsFakeJuniorBirthday").ValueKind == JsonValueKind.False && root.GetProperty("TreatAsJunior").ValueKind == JsonValueKind.False,"false remains distinct from null");
    Check(root.GetProperty("DisplayName").GetString() == sample.DisplayName,"Unicode, quotes, backslash and controls survive UTF8 output");
    Check(root.GetProperty("PersonalPronouns").GetInt32() == 777 && root.GetProperty("IdentityFlags").GetInt32() == -1,"undefined enum Int32 values remain explicit");
    Check(NativeSelfAccountResponseProfile.Write(sample).SequenceEqual(bytes),"deterministic output bytes");
    Check(!sample.ToString().Contains("synthetic",StringComparison.Ordinal),"incidental model output redacts values");
    var constructor = typeof(NativeSelfAccountResponseModel).GetConstructors().Single();
    Check(constructor.GetParameters().Length == 20 && constructor.GetParameters().All(p => !p.IsOptional && !p.HasDefaultValue),"all20 constructor arguments required");
    Check(typeof(NativeSelfAccountResponseModel).GetProperties().All(p => !p.CanWrite),"model properties are immutable");
    foreach (int field in new[]{0,1,8,9,10,11,12})
    {
        string name = SelfAccountPrefixContract.CanonicalNames[field];
        using var nullable = JsonDocument.Parse(NativeSelfAccountResponseProfile.Write(StringCase(sample,field,null)));
        Check(nullable.RootElement.GetProperty(name).ValueKind == JsonValueKind.Null,"string null emitted: " + name);
        using var empty = JsonDocument.Parse(NativeSelfAccountResponseProfile.Write(StringCase(sample,field,"")));
        Check(empty.RootElement.GetProperty(name).GetString() == "","empty string emitted: " + name);
        Reject(StringCase(sample,field,"\uD800"),"invalid_unicode");
        Reject(StringCase(sample,field,"\uDC00"),"invalid_unicode");
        Reject(StringCase(sample,field,new string('a',NativeSelfAccountResponseProfile.MaxStringUtf8Bytes+1)),"string_too_large");
    }
    Check(NativeSelfAccountResponseProfile.Write(StringCase(sample,0,new string('a',NativeSelfAccountResponseProfile.MaxStringUtf8Bytes))).Length <= NativeSelfAccountResponseProfile.MaxJsonBytes,"per-string ASCII admission boundary");
    Check(NativeSelfAccountResponseProfile.Write(StringCase(sample,0,string.Concat(Enumerable.Repeat("\U0001f600",4096)))).Length <= NativeSelfAccountResponseProfile.MaxJsonBytes,"per-string UTF8 boundary uses bytes");
    Reject(StringCase(sample,0,string.Concat(Enumerable.Repeat("\U0001f600",4097))),"string_too_large");
    Reject(StringCase(sample,0,new string('<',11_000)),"response_too_large");
    Reject(StringCase(sample,0,"prefix\uD800suffix"),"invalid_unicode");
    var boundaryModel = sample;
    foreach (int field in new[]{0,1,8,9,10,11,12}) boundaryModel = StringCase(boundaryModel,field,"");
    int fixedBytes = NativeSelfAccountResponseProfile.Write(boundaryModel).Length;
    foreach (int field in new[]{0,1,8}) boundaryModel = StringCase(boundaryModel,field,new string('a',NativeSelfAccountResponseProfile.MaxStringUtf8Bytes));
    int lastStringBytes = NativeSelfAccountResponseProfile.MaxJsonBytes-fixedBytes-3*NativeSelfAccountResponseProfile.MaxStringUtf8Bytes;
    Check(lastStringBytes is >0 and <NativeSelfAccountResponseProfile.MaxStringUtf8Bytes,"independent total-byte boundary fits per-string limits");
    boundaryModel = StringCase(boundaryModel,9,new string('a',lastStringBytes));
    Check(NativeSelfAccountResponseProfile.Write(boundaryModel).Length == NativeSelfAccountResponseProfile.MaxJsonBytes,"exact64KiB output accepted");
    Reject(StringCase(boundaryModel,9,new string('a',lastStringBytes+1)),"response_too_large");
    using var nullBirthday = JsonDocument.Parse(NativeSelfAccountResponseProfile.Write(DateCase(sample,null,0)));
    Check(nullBirthday.RootElement.GetProperty("Birthday").ValueKind == JsonValueKind.Null,"Birthday explicit null");
    using var maxDates = JsonDocument.Parse(NativeSelfAccountResponseProfile.Write(DateCase(sample,3_155_378_975_999_999_999,3_155_378_975_999_999_999)));
    Check(maxDates.RootElement.GetProperty("Birthday").GetInt64() == 3_155_378_975_999_999_999,"maximum UTC ticks");
    foreach (long invalid in new[]{-1L,3_155_378_976_000_000_000L,long.MinValue,long.MaxValue})
    {
        Reject(DateCase(sample,invalid,0),"ticks_out_of_range");
        Reject(DateCase(sample,null,invalid),"ticks_out_of_range");
    }
    bool nullModelRejected = false;
    try { NativeSelfAccountResponseProfile.Write(null!); } catch (ArgumentNullException) { nullModelRejected = true; }
    Check(nullModelRejected,"null model rejected");
    var report = new { status = "self_account_response_profile_self_tests_passed", checks = tests.Count, tests, originalGameExecuted = false, accountConstructed = false, nativeLoginVerified = false };
    using var stream = new FileStream(args[1],FileMode.CreateNew);
    JsonSerializer.Serialize(stream,report,new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks }));
    return;
}

string? mutation = args.Length == 3 ? args[2] : null;
if (mutation is not (null or "drop-field" or "swap-ids" or "unix-seconds")) throw new ArgumentException("Unknown negative control.");
byte[] Mutate(byte[] bytes)
{
    if (mutation is null) return bytes;
    using var doc = JsonDocument.Parse(bytes);
    using var stream = new MemoryStream();
    using (var writer = new Utf8JsonWriter(stream))
    {
        writer.WriteStartObject();
        foreach (var property in doc.RootElement.EnumerateObject())
        {
            if (mutation == "drop-field" && property.Name == "IdentityFlags") continue;
            writer.WritePropertyName(property.Name);
            if (mutation == "swap-ids" && property.Name is "AccountId" or "AvailableUsernameChanges")
                doc.RootElement.GetProperty(property.Name == "AccountId" ? "AvailableUsernameChanges" : "AccountId").WriteTo(writer);
            else if (mutation == "unix-seconds" && property.Name is "Birthday" or "createdAt" && property.Value.ValueKind == JsonValueKind.Number)
                writer.WriteNumberValue(property.Value.GetInt64()/10_000_000);
            else property.Value.WriteTo(writer);
        }
        writer.WriteEndObject();
    }
    return stream.ToArray();
}
using var output = new StreamWriter(new FileStream(args[1],FileMode.CreateNew));
int cases = 0;
var ids = new HashSet<int>();
foreach (string line in File.ReadLines(args[0]))
{
    if (++cases > 20_000 || line.Length > 4*NativeSelfAccountResponseProfile.MaxJsonBytes) throw new InvalidDataException("Fixture budget exceeded.");
    using var doc = JsonDocument.Parse(line);
    int id = doc.RootElement.GetProperty("id").GetInt32();
    if (id < 0 || !ids.Add(id)) throw new InvalidDataException("Invalid or duplicate identity.");
    // Only explicit model inputs are consumed; expected native observations stay external.
    var model = FromJson(doc.RootElement.GetProperty("model"));
    byte[] written = Mutate(NativeSelfAccountResponseProfile.Write(model));
    output.WriteLine(JsonSerializer.Serialize(new { id, status = "written", byteLength = written.Length, utf8Base64 = Convert.ToBase64String(written) }));
}
Console.WriteLine(JsonSerializer.Serialize(new { status = "self_account_response_fixture_run_completed", cases, negativeControl = mutation, originalGameExecuted = false }));
