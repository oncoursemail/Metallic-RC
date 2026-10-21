using System.Text.Json;
using Metallic.Reconstruction;
int checks=0,cases=0;
void Check(bool value,string detail){checks++;if(!value)throw new Exception(detail);}
void Expect<T>(Action action,string detail) where T:Exception {bool hit=false;try{action();}catch(T){hit=true;}Check(hit,detail);}
string[] inputs={"", "x", "x/", "x//", "https://example.invalid", "https://example.invalid/a?x=y", "https://example.invalid/a#f", "\ud83c\udf1f"};
foreach(string value in inputs)foreach(char suffix in new[]{'/',':','x','\0'}){
    string actual=ObservedServiceAddressUpdate.EnsureTrailingCharacter(value,suffix);
    string expected=value.EndsWith(suffix)?value:value+suffix;
    Check(actual==expected,"Trailing character contents");
    if(value.EndsWith(suffix))Check(ReferenceEquals(value,actual),"Existing suffix preserves reference");
    cases++;
}
Expect<NullReferenceException>(()=>ObservedServiceAddressUpdate.EnsureTrailingCharacter(null!,'/'),"Null string behavior");
var rng=new Random(23191908);
for(int i=0;i<500;i++)foreach(bool loaded in new[]{false,true})foreach(bool alreadyChanged in new[]{false,true}){
    int id=rng.Next(0,31);string old=$"https://s{id}.example.invalid/p{i}/";
    string address=(i%3) switch {0=>old.TrimEnd('/'),1=>old, _=>$"https://s{id}.example.invalid/new{i}"};
    var routes=new Dictionary<int,Uri>{{id,new Uri(old)}};
    var context=new CachedResponseContext{CachedNameServerResponseLoaded=loaded,CachedNameServerResponseChanged=alreadyChanged};
    string expected=address.EndsWith('/')?address:address+"/";
    ObservedServiceAddressUpdate.Apply(routes,id,address,ref context);
    Check(routes[id].OriginalString==expected,"Stored URI original spelling");
    Check(context.CachedNameServerResponseLoaded==loaded,"Loaded flag is not overwritten");
    Check(context.CachedNameServerResponseChanged==(alreadyChanged||(loaded&&old!=expected)),"Changed flag is sticky and conditional");cases++;
}
{
    var routes=new Dictionary<int,Uri>{{1,new Uri("https://HOST.example.invalid/")}};
    var context=new CachedResponseContext{CachedNameServerResponseLoaded=true};
    Check(routes[1].Equals(new Uri("https://host.example.invalid/")),"Case fixture has equal URI semantics");
    ObservedServiceAddressUpdate.Apply(routes,1,"https://host.example.invalid/",ref context);
    Check(context.CachedNameServerResponseChanged,"OriginalString comparison must not be replaced by Uri.Equals");cases++;
}
{
    var routes=new Dictionary<int,Uri>();var context=new CachedResponseContext{CachedNameServerResponseLoaded=true};
    Expect<KeyNotFoundException>(()=>ObservedServiceAddressUpdate.Apply(routes,5,"https://empty.example.invalid",ref context),"Loaded-cache missing key fails before assignment");
    Check(routes.Count==0&&!context.CachedNameServerResponseChanged,"Failed lookup leaves state unchanged");cases++;
}
{
    var routes=new Dictionary<int,Uri>();var context=new CachedResponseContext();
    ObservedServiceAddressUpdate.Apply(routes,999,"https://test.example.invalid/a?x=y",ref context);
    Check(routes[999].OriginalString=="https://test.example.invalid/a?x=y/","Suffix is appended to whole string; not URL-path normalization");
    Check(!context.CachedNameServerResponseChanged,"Initial registration does not flag cached change");cases++;
    Uri before=routes[999];
    bool missing=false;try{ObservedServiceAddressUpdate.Apply(routes,999,null,ref context);}catch(Exception ex){missing=ex.GetType()==typeof(Exception);}
    Check(missing&&ReferenceEquals(before,routes[999]),"Missing address rejects before dictionary mutation");cases++;
    Expect<UriFormatException>(()=>ObservedServiceAddressUpdate.Apply(routes,999,"not-an-absolute-uri",ref context),"Invalid URI rejected");
    Check(ReferenceEquals(before,routes[999]),"URI parse failure leaves previous value");cases++;
}
var summary=new {status="service_helper_model_checks_passed",cases,checks,sourceMethods=new[]{148123,367496},cachedFlagFields=new[]{92806,92807},serviceDictionaryField=92689,nativeDifferentialExecuted=false,gameExecuted=false,networkRequests=0,limits=new[]{"Tests validate the newly authored C# model against the observed branch/field mapping, NOT the original Unity runtime or original BCL.","Original exception message, runtime initialization and full async name-server processing remain unresolved.","Numeric service mapping is not a confirmed HTTP or JSON wire schema."}};
File.WriteAllText(args[0],JsonSerializer.Serialize(summary,new JsonSerializerOptions{WriteIndented=true})+"\n");
Console.WriteLine(JsonSerializer.Serialize(summary));
