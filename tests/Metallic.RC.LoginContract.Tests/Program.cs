using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;

if(args.Length is <2 or >3)throw new ArgumentException("Use --self-test NEW-result.json or fixtures.jsonl NEW-results.jsonl [casefold|first-wins|empty-object].");
if(File.Exists(args[1]))throw new IOException("Evidence output already exists.");
string?[]? Fields(LoginResponseProfile? value)=>value is null?null:LoginResponseContract.Values(value);
if(args[0]=="--self-test")
{
    int checks=0;var names=new List<string>();
    void Check(bool condition,string name){checks++;if(!condition)throw new Exception(name);names.Add(name);}
    void Reject(string json,string code)
    {
        bool failed=false;try{LoginResponseContract.Read(Encoding.UTF8.GetBytes(json));}catch(LoginProfileException e){failed=e.Code==code;}catch(JsonException){failed=code=="invalid_json";}
        Check(failed,"reject "+code);
    }
    Check(LoginResponseContract.Read("null"u8)is null,"root null");Check(LoginResponseContract.Read("{}"u8)is null,"empty root");
    Check(LoginResponseContract.Read("{\"unknown\":{\"nested\":[1,true]}}"u8)is not null,"unknown-only object retained");
    Check(LoginResponseContract.Read("{\"Error\":\"first\",\"error\":null}"u8)!.Error is null,"duplicate alias explicit-null wins");
    Check(LoginResponseContract.Read("{\"ACCESS_TOKEN\":\"unrecognized\"}"u8)!.AccessToken is null,"not blanket casefold");
    Check(LoginResponseContract.Read("{\"access\\u005ftoken\":\"outside-key-profile\"}"u8)!.AccessToken is null,"raw alias profile not silently widened");
    Reject("{\"code_hint\":{\"Text\":\"test\"}}","nested_hint_outside_verified_profile");
    Reject("{\"Error\":123}","known_value_type_error");Reject("[]","object_or_null_required");Reject("null {}","invalid_json");Reject("{\"Error\":\"x\",}","invalid_json");
    Reject(new string(' ',LoginResponseContract.MaxJsonBytes+1),"response_too_large");
    var value=new LoginResponseProfile("test-error","test-description","synthetic-access","synthetic-refresh","synthetic-key","synthetic-context","synthetic-challenge");
    byte[] bytes=LoginResponseContract.Write(value);using var written=JsonDocument.Parse(bytes);
    Check(written.RootElement.EnumerateObject().Select(p=>p.Name).SequenceEqual(LoginResponseContract.CanonicalNames),"canonical mixed-case order");
    Check(written.RootElement.GetProperty("code_hint").ValueKind==JsonValueKind.Null,"nullable hint written explicitly");
    Check(LoginResponseContract.Read(bytes)==value,"round-trip admitted profile");
    Check(!value.ToString().Contains("synthetic-",StringComparison.Ordinal),"incidental object formatting does not leak values");
    Check(Encoding.UTF8.GetString(LoginResponseContract.Write(null))=="null","writer null");
    Check(LoginResponseContract.Classify(null,200)==LoginResponseDisposition.MissingResponse,"missing response never success");
    Check(LoginResponseContract.Classify(new(),200)==LoginResponseDisposition.RequiresTokenAndAccountValidation,"empty fields never completed login");
    foreach(int status in new[]{200,201,302,399,400,401,403,429,500,599})
    {
        Check(LoginResponseContract.Classify(new(),status)==(status>=400?LoginResponseDisposition.Rejected:LoginResponseDisposition.RequiresTokenAndAccountValidation),"HTTP failure boundary");
        Check(LoginResponseContract.Classify(new(Error:"bad"),status)==LoginResponseDisposition.Rejected,"nonempty error rejected");
        Check(LoginResponseContract.Classify(new(ErrorDescription:"security_code required"),status)==LoginResponseDisposition.ContinueSecurityCode,"security continuation observed before generic failure");
        Check(LoginResponseContract.Classify(new(Error:"continue_2fa"),status)==LoginResponseDisposition.ContinueSecurityCode,"two-factor continuation");
        Check(LoginResponseContract.Classify(new(ErrorDescription:"invalid security_code"),status)==LoginResponseDisposition.RetrySecurityCode,"security retry");
        Check(LoginResponseContract.Classify(new(ErrorDescription:"invalid 2fa_code"),status)==LoginResponseDisposition.RetrySecurityCode,"two-factor retry");
        Check(LoginResponseContract.Classify(new(ErrorDescription:"hcr required"),status)==LoginResponseDisposition.AdditionalChallengeRequired,"challenge work not skipped");
        Check(LoginResponseContract.Classify(new(ErrorDescription:"hcr required"),status,originalRequestHasChallenge:true)==(status>=400?LoginResponseDisposition.Rejected:LoginResponseDisposition.RequiresTokenAndAccountValidation),"already supplied challenge not retried automatically");
    }
    foreach(string error in new[]{"authorization_pending","slow_down"})
    {
        Check(LoginResponseContract.Classify(new(Error:error),400,true)==LoginResponseDisposition.RemoteAuthorizationPending,"remote pending preserved");
        Check(LoginResponseContract.Classify(new(Error:error),400,false)==LoginResponseDisposition.Rejected,"nonremote pending not accepted");
        Check(LoginResponseContract.Classify(new(Error:error),200,true)==LoginResponseDisposition.Rejected,"pending requires observed status");
    }
    bool invalidStatus=false;try{LoginResponseContract.Classify(new(),0);}catch(ArgumentOutOfRangeException){invalidStatus=true;}Check(invalidStatus,"invalid transport status rejected");
    var result=new{status="login_contract_self_tests_passed",checks,tests=names,nativeClassifierEquivalenceProved=false,tokenIssued=false,actualGameClientExecuted=false};
    File.WriteAllText(args[1],JsonSerializer.Serialize(result,new JsonSerializerOptions{WriteIndented=true})+"\n");Console.WriteLine(JsonSerializer.Serialize(new{result.status,checks}));return;
}

string? mutation=args.Length==3?args[2]:null;
if(mutation is not(null or "casefold" or "first-wins" or "empty-object"))throw new ArgumentException("Unknown negative control.");
string Mutate(string input)
{
    using var parsed=JsonDocument.Parse(input);
    if(parsed.RootElement.ValueKind!=JsonValueKind.Object)return input;
    if(mutation=="empty-object"&&!parsed.RootElement.EnumerateObject().Any())return "{\"unknown\":null}";
    if(mutation is not("casefold" or "first-wins"))return input;
    using var stream=new MemoryStream();using(var writer=new Utf8JsonWriter(stream))
    {
        writer.WriteStartObject();var seen=new HashSet<string>(StringComparer.Ordinal);
        foreach(var property in parsed.RootElement.EnumerateObject())
        {
            string name=property.Name;
            string? canonical=LoginResponseContract.CanonicalNames.FirstOrDefault(n=>n.Equals(name,StringComparison.OrdinalIgnoreCase));
            if(mutation=="casefold"&&canonical is not null)name=canonical;
            if(mutation=="first-wins"&&canonical is not null&&!seen.Add(canonical))continue;
            writer.WritePropertyName(name);property.Value.WriteTo(writer);
        }
        writer.WriteEndObject();
    }
    return Encoding.UTF8.GetString(stream.ToArray());
}
using var output=new StreamWriter(new FileStream(args[1],FileMode.CreateNew));int cases=0;
foreach(string line in File.ReadLines(args[0]))
{
    if(++cases>20000||line.Length>2*LoginResponseContract.MaxJsonBytes)throw new InvalidDataException("Fixture budget exceeded.");
    using var document=JsonDocument.Parse(line);var row=document.RootElement;int id=row.GetProperty("id").GetInt32();
    string operation=row.GetProperty("operation").GetString()!;
    try
    {
        if(operation=="read")
        {
            string input=row.GetProperty("inputJson").GetString()!;if(mutation is not null)input=Mutate(input);
            var value=LoginResponseContract.Read(Encoding.UTF8.GetBytes(input));
            output.WriteLine(JsonSerializer.Serialize(new{id,status="ok",fields=Fields(value)}));
        }
        else if(operation=="write")
        {
            string?[]? fields=row.GetProperty("fields").ValueKind==JsonValueKind.Null?null:row.GetProperty("fields").EnumerateArray().Select(x=>x.ValueKind==JsonValueKind.Null?null:x.GetString()).ToArray();
            if(fields is not null&&(fields.Length!=8||fields[5]is not null))throw new InvalidDataException("Unsupported writer fixture.");
            LoginResponseProfile? value=fields is null?null:new(fields[0],fields[1],fields[2],fields[3],fields[4],fields[6],fields[7]);
            output.WriteLine(JsonSerializer.Serialize(new{id,status="ok",json=Encoding.UTF8.GetString(LoginResponseContract.Write(value))}));
        }
        else throw new InvalidDataException("Unknown fixture operation.");
    }
    catch(LoginProfileException error)
    {output.WriteLine(JsonSerializer.Serialize(new{id,status=error.Code=="nested_hint_outside_verified_profile"?"nested_hint_outside_profile":error.Code,fields=(string?[]?)null}));}
}
Console.WriteLine(JsonSerializer.Serialize(new{status="fixture_run_completed",cases,negativeControl=mutation,syntheticFixtureValuesOnly=true}));
