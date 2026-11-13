using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Metallic.RC.Backend;
using Metallic.RC.Compatibility;
using Microsoft.Data.Sqlite;

if(args.Length!=2||args[0]!="--self-test")throw new ArgumentException("Use --self-test NEW-result.json.");
if(File.Exists(args[1]))throw new IOException("Evidence output already exists.");
var checks=new List<string>();
void Check(bool value,string label){if(!value)throw new InvalidOperationException(label);checks.Add(label);}
void Fault(Action action,int status,string code){bool failed=false;try{action();}catch(BackendFault e){failed=e.Status==status&&e.Code==code;}Check(failed,"static failure: "+code);}
void Invalid(Action action,string label){bool failed=false;try{action();}catch(IOException e){failed=!e.Message.Contains("synthetic-sensitive",StringComparison.Ordinal)&&e.InnerException is null;}Check(failed,label);}
string name="metallic-native-profiles-"+Guid.NewGuid().ToString("N"),root=Path.Combine(Path.GetTempPath(),name);Directory.CreateDirectory(root);
string Folder(){string f=Path.Combine(root,Guid.NewGuid().ToString("N"));Directory.CreateDirectory(f);return f;}
string Db(string f)=>Path.Combine(f,"backend.sqlite");
void Owned(string f){if(!Path.GetFullPath(f).StartsWith(Path.GetFullPath(root)+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase))throw new InvalidOperationException("Fixture escaped generated root.");}
SqliteConnection Open(string f){Owned(f);var db=new SqliteConnection(new SqliteConnectionStringBuilder{DataSource=Db(f),Pooling=false}.ToString());db.Open();return db;}
object? Sql(string f,string text){using var db=Open(f);using var c=db.CreateCommand();c.CommandText=text;return c.ExecuteScalar();}
long Count(string f,string sql)=>(long)(Sql(f,sql)??throw new InvalidOperationException("Expected fixture integer."));
BackendStore Store(string f,bool identities=true,bool sessions=false,bool profiles=false)=>new(new BackendOptions(0,f){EnableNativeIdentityMapping=identities,EnableNativeSessions=sessions,EnableNativeAccountProfiles=profiles});
string Clone(string f)
{
    Owned(f);using var owner=new FileStream(Path.Combine(f,"backend.lock"),FileMode.OpenOrCreate,FileAccess.ReadWrite,FileShare.None);
    using(var db=Open(f))using(var c=db.CreateCommand()){c.CommandText="PRAGMA wal_checkpoint(TRUNCATE);";using var r=c.ExecuteReader();if(!r.Read()||r.GetInt64(0)!=0)throw new IOException("Fixture checkpoint busy.");}
    if(File.Exists(Db(f)+"-wal")&&new FileInfo(Db(f)+"-wal").Length!=0)throw new IOException("Fixture WAL not drained.");
    string copy=Folder();File.Copy(Db(f),Db(copy));File.Copy(Path.Combine(f,".metallic-backend"),Path.Combine(copy,".metallic-backend"));
    File.Copy(Path.Combine(f,"admin.key"),Path.Combine(copy,"admin.key"));return copy;
}
string Token()=>Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(48));
NativeSelfAccountResponseModel Model(AccountView a,int id,int variant=0)=>new(
    variant==0?null:"synthetic-sensitive@example.invalid",variant==0?null:"synthetic-sensitive-phone",variant==0?null:123456789,
    variant==0?0:int.MinValue,variant==0?null:-1,variant==0?null:false,variant==0?0:7,id,a.Username,a.DisplayName,
    variant==0?null:"猫",variant==0?null:"https://synthetic.invalid/profile",variant==0?null:"https://synthetic.invalid/banner",
    variant!=0,variant!=0,variant==0?0:int.MaxValue,variant==0?0:-1,variant==0?0:3155378975999999999,variant!=0,variant==0?null:false);
void Blob(string folder,string account,byte[] bytes)
{using var db=Open(folder);using var c=db.CreateCommand();c.CommandText="PRAGMA ignore_check_constraints=ON; UPDATE native_account_profiles SET payload=$bytes WHERE account_id=$id;";c.Parameters.AddWithValue("$bytes",bytes);c.Parameters.AddWithValue("$id",account);c.ExecuteNonQuery();}
try
{
    string f=Folder(),password=Token();AccountView a,b;SessionView ownedSession;int id,bid;
    using(var s=Store(f,false)){a=s.Register(new("user_"+Guid.NewGuid().ToString("N")[..12],password,"Synthetic profile"));b=s.Register(new("user_"+Guid.NewGuid().ToString("N")[..12],password,"Other synthetic"));ownedSession=s.Login(new(a.Username,password));}
    string admin=File.ReadAllText(Path.Combine(f,"admin.key")).Trim(),v1=Clone(f);
    Invalid(()=>{using var unused=Store(v1,true,false,true);},"profile activation refuses v1 before identity migration");
    Check(Count(v1,"PRAGMA user_version;")==1&&Count(v1,"SELECT count(*) FROM sqlite_schema WHERE name='native_account_ids';")==0,"v1 refusal preserves earlier schema");
    using(var s=Store(f)){id=s.GetOrCreateNativeAccountId(a.Id);bid=s.GetOrCreateNativeAccountId(b.Id);}
    string v2=Clone(f);
    Invalid(()=>{using var unused=Store(v2,true,true,true);},"profile activation refuses v2 before session migration");
    Check(Count(v2,"PRAGMA user_version;")==2&&Count(v2,"SELECT count(*) FROM sqlite_schema WHERE name='native_sessions';")==0,"v2 refusal preserves earlier session state");
    string access=Token(),refresh=Token();
    using(var s=Store(f,true,true))s.CreateNativeSession(new(a.Username,password),a.Id,id,new(access,refresh,DateTimeOffset.UtcNow.AddMinutes(30),DateTimeOffset.UtcNow.AddDays(1)));
    string v3=Clone(f);
    using(var s=Store(v3,false))
    {
        Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,id,0,Model(a,id)),503,"native_profiles_disabled");
        Check(s.Authenticate("Bearer "+ownedSession.AccessToken).AccountId==a.Id,"default off v3 preserves old owned sessions");
    }
    Check(Count(v3,"PRAGMA user_version;")==3&&Count(v3,"SELECT count(*) FROM sqlite_schema WHERE name='native_account_profiles';")==0,"default off v3 creates no profile table");
    using(var s=Store(f,true,true,true))
    {
        foreach(string wrong in new[]{"",Token(),ownedSession.AccessToken,access})Fault(()=>s.WriteNativeAccountProfile(wrong,a.Id,id,0,Model(a,id)),401,"local_admin_key_required");
        Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,bid,0,Model(a,id)),401,"native_session_rejected");
        Fault(()=>s.WriteNativeAccountProfile(admin,b.Id,id,0,Model(a,id)),401,"native_session_rejected");
        foreach(var bad in new[]{Model(a,bid),Model(a with{Username="unrelated"},id),Model(a with{DisplayName="unrelated"},id)})
            Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,id,0,bad),400,"native_profile_identity_mismatch");
        Fault(()=>s.ReadNativeAccountProfile(admin,a.Id,id),404,"native_profile_missing");
        Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,id,-1,Model(a,id)),400,"native_profile_revision_required");
        var first=s.WriteNativeAccountProfile(admin,a.Id,id,0,Model(a,id));
        Check(first.Revision==1&&first.OwnedRevision==a.Revision,"explicit initial profile binds both revisions");
        Check(first.GetResponseBytes().AsSpan().SequenceEqual(NativeSelfAccountResponseProfile.Write(Model(a,id))),"stored bytes come from unchanged writer");
        using(var json=JsonDocument.Parse(first.GetResponseBytes()))
        {
            Check(json.RootElement.EnumerateObject().Count()==20,"all twenty explicit fields retained");
            foreach(string key in new[]{"Email","Phone","Birthday","ParentAccountId","IsFakeJuniorBirthday","DisplayEmoji","ProfileImage","BannerImage","IsJunior"})Check(json.RootElement.GetProperty(key).ValueKind==JsonValueKind.Null,"explicit null retained: "+key);
            Check(json.RootElement.GetProperty("createdAt").GetInt64()==0&&!json.RootElement.GetProperty("TreatAsJunior").GetBoolean()&&json.RootElement.GetProperty("IdentityFlags").GetInt32()==0,"explicit zero and false retained without inferred history");
        }
        byte[] copy=first.GetResponseBytes();copy[0]=0;Check(first.GetResponseBytes()[0]==(byte)'{',"payload copy cannot mutate stored view");
        using(var metadata=JsonDocument.Parse(JsonSerializer.Serialize(first)))Check(metadata.RootElement.EnumerateObject().Count()==2,"view JSON contains revision metadata only");
        Check(!first.ToString().Contains("Synthetic",StringComparison.Ordinal),"incidental output redacts profile data");
        Check(s.NativeAccountProfileForSession(access,a.Id,id).Revision==1,"real stored native session authorizes bound profile read");
        foreach(string wrong in new[]{ownedSession.AccessToken,refresh,Token()})Fault(()=>s.NativeAccountProfileForSession(wrong,a.Id,id),401,"native_session_rejected");
        Fault(()=>s.NativeAccountProfileForSession(access,b.Id,bid),401,"native_session_rejected");
        Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,id,0,Model(a,id)),412,"native_profile_revision_conflict");
        var second=s.WriteNativeAccountProfile(admin,a.Id,id,1,Model(a,id,1));
        Check(second.Revision==2,"revision-checked explicit update");
        Check(second.GetResponseBytes().AsSpan().SequenceEqual(NativeSelfAccountResponseProfile.Write(Model(a,id,1))),"Unicode nullable dates undefined enums and flags preserved exactly");
        var m=Model(a,id,1);
        var invalid=new NativeSelfAccountResponseModel("\ud800",m.Phone,m.BirthdayUtcTicks,m.JuniorState,m.ParentAccountId,m.IsFakeJuniorBirthday,m.AvailableUsernameChanges,m.AccountId,m.UserName,m.DisplayName,m.DisplayEmoji,m.ProfileImage,m.BannerImage,m.TreatAsJunior,m.HasBirthday,m.PersonalPronouns,m.IdentityFlags,m.CreatedAtUtcTicks,m.IsMetaPlatformBlocked,m.IsJunior);
        Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,id,2,invalid),400,"native_profile_value_invalid");
        Check(s.ReadNativeAccountProfile(admin,a.Id,id).Revision==2,"invalid explicit field preserves prior profile");
    }
    Check(Count(f,"PRAGMA user_version;")==4,"atomic explicit v3 to v4 migration");
    using(var s=Store(f,false))
    {
        Fault(()=>s.ReadNativeAccountProfile(admin,a.Id,id),503,"native_profiles_disabled");
        Check(s.Profile(a.Id)==a&&s.Authenticate("Bearer "+ownedSession.AccessToken).AccountId==a.Id,"v4 default off preserves owned APIs");
    }
    using(var s=Store(f,true,false,true))Fault(()=>s.NativeAccountProfileForSession(access,a.Id,id),503,"native_sessions_disabled");
    using(var s=Store(f,true,true,true))
    {
        Check(s.ReadNativeAccountProfile(admin,a.Id,id).Revision==2,"restart preserves explicit profile");
        int won=0,lost=0;
        Parallel.For(0,2,i=>{try{s.WriteNativeAccountProfile(admin,a.Id,id,2,Model(a,id,i));Interlocked.Increment(ref won);}catch(BackendFault e)when(e.Code=="native_profile_revision_conflict"){Interlocked.Increment(ref lost);}});
        Check(won==1&&lost==1&&s.ReadNativeAccountProfile(admin,a.Id,id).Revision==3,"competing writers cannot silently overwrite a revision");
        byte[] before=s.ReadNativeAccountProfile(admin,a.Id,id).GetResponseBytes();
        a=s.UpdateProfile(s.Authenticate("Bearer "+ownedSession.AccessToken),new("Renamed synthetic"),a.Revision);
        Fault(()=>s.ReadNativeAccountProfile(admin,a.Id,id),409,"native_profile_stale");
        Fault(()=>s.NativeAccountProfileForSession(access,a.Id,id),409,"native_profile_stale");
        Check(((byte[])Sql(f,"SELECT payload FROM native_account_profiles WHERE account_id='"+a.Id+"';")!).AsSpan().SequenceEqual(before),"owned rename does not silently rewrite explicit profile data");
    }
    using(var s=Store(f,true,true,true))
    {
        Check(s.Profile(a.Id).DisplayName==a.DisplayName,"stale profile preserved at startup without breaking owned API");
        Fault(()=>s.ReadNativeAccountProfile(admin,a.Id,id),409,"native_profile_stale");
        var updated=s.WriteNativeAccountProfile(admin,a.Id,id,3,Model(a,id,1));
        Check(updated.Revision==4&&updated.OwnedRevision==a.Revision,"explicit owner rewrite clears stale state without inventing data");
        s.WriteNativeAccountProfile(admin,b.Id,bid,0,Model(b,bid));
    }
    string good=Clone(f);
    byte[] valid=NativeSelfAccountResponseProfile.Write(Model(a,id,1));
    string text=Encoding.UTF8.GetString(valid);
    var malformed=new List<byte[]>{Array.Empty<byte>(),new byte[65537],Encoding.UTF8.GetBytes("null"),Encoding.UTF8.GetBytes("[]"),new byte[]{255},
        Encoding.UTF8.GetBytes(text[..^1]+",\"Email\":null}"),Encoding.UTF8.GetBytes(text.Replace("\"Phone\"","\"UnknownField\"",StringComparison.Ordinal)),
        Encoding.UTF8.GetBytes(text.Replace("synthetic-sensitive@example.invalid","\\ud800",StringComparison.Ordinal)),Encoding.UTF8.GetBytes(" "+text)};
    foreach(string key in new[]{"Email","Phone","Birthday","JuniorState","ParentAccountId","IsFakeJuniorBirthday","AvailableUsernameChanges","AccountId","UserName","DisplayName","DisplayEmoji","ProfileImage","BannerImage","TreatAsJunior","HasBirthday","PersonalPronouns","IdentityFlags","createdAt","IsMetaPlatformBlocked","IsJunior"})
    {
        var obj=JsonNode.Parse(valid)!.AsObject();obj.Remove(key);malformed.Add(Encoding.UTF8.GetBytes(obj.ToJsonString()));
    }
    foreach((string key,JsonNode? value) in new (string,JsonNode?)[]{("AccountId",JsonValue.Create("wrong")),("TreatAsJunior",JsonValue.Create(1)),("IsJunior",JsonValue.Create("false")),("createdAt",JsonValue.Create(long.MaxValue)),("Email",JsonValue.Create(4)),("Phone",JsonValue.Create(new string('x',16385)))})
    {var obj=JsonNode.Parse(valid)!.AsObject();obj[key]=value;malformed.Add(Encoding.UTF8.GetBytes(obj.ToJsonString()));}
    foreach(byte[] bytes in malformed)
    {
        string bad=Clone(good);Blob(bad,a.Id,bytes);
        foreach(bool on in new[]{false,true})Invalid(()=>{using var unused=Store(bad,on,on,on);},"bad bounded canonical payload rejected with feature "+on);
    }
    string selected=Clone(good);using(var s=Store(selected,true,true,true))
    {
        Blob(selected,b.Id,Encoding.UTF8.GetBytes("bad-json"));
        Check(s.ReadNativeAccountProfile(admin,a.Id,id).Revision==4,"ordinary read validates selected payload rather than decoding every other profile");
        Invalid(()=>s.ReadNativeAccountProfile(admin,b.Id,bid),"selected malformed payload rejected after open");
    }
    string collision=Clone(v3);_=Sql(collision,"CREATE INDEX native_account_profiles_native ON accounts(username);");
    Invalid(()=>{using var unused=Store(collision,true,false,true);},"migration second-DDL collision rejected");
    Check(Count(collision,"PRAGMA user_version;")==3&&Count(collision,"SELECT count(*) FROM sqlite_schema WHERE name='native_account_profiles';")==0,"failed profile migration rolls back table and version");
    foreach(string mutation in new[]{"DROP INDEX native_account_profiles_native;","UPDATE native_account_profiles SET native_id="+bid+" WHERE account_id='"+a.Id+"';","PRAGMA ignore_check_constraints=ON;UPDATE native_account_profiles SET revision=0;","CREATE TRIGGER profile_noop AFTER UPDATE ON native_account_profiles BEGIN SELECT 1;END;","PRAGMA user_version=5;"})
    {
        string bad=Clone(good);
        try{_=Sql(bad,mutation);}catch(SqliteException e)when(e.SqliteErrorCode==19){Check(true,"database constraint refuses duplicate native ID");continue;}
        Invalid(()=>{using var unused=Store(bad);},"malformed metadata/schema rejected even when feature off");
    }
    string trigger=Clone(v3);using(var s=Store(trigger,true,true,true))
    {
        _=Sql(trigger,"CREATE TRIGGER profile_redirect AFTER INSERT ON native_account_profiles BEGIN UPDATE native_account_profiles SET account_id='"+b.Id+"',native_id="+bid+" WHERE account_id=new.account_id;END;");
        Invalid(()=>s.WriteNativeAccountProfile(admin,a.Id,id,0,Model(a,id)),"after-open redirection trigger refused before write");
        Check(Count(trigger,"SELECT count(*) FROM native_account_profiles;")==0,"trigger rejection commits no profile or misbinding");
    }
    string maxRevision=Clone(good);_=Sql(maxRevision,"UPDATE native_account_profiles SET revision=9223372036854775807 WHERE account_id='"+a.Id+"';");
    using(var s=Store(maxRevision,true,true,true))Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,id,long.MaxValue,Model(a,id)),409,"native_profile_revision_exhausted");

    string cap=Clone(v3);using(var s=Store(cap,true,true,true)){}
    using(var db=Open(cap))using(var tx=db.BeginTransaction())
    {
        for(int i=0;i<1024;i++)
        {
            string account=Guid.NewGuid().ToString("N"),username="fixture_"+i;int nid=i+3;
            var av=new AccountView(account,username,"Explicit synthetic",1);byte[] bytes=NativeSelfAccountResponseProfile.Write(Model(av,nid));
            using var c=db.CreateCommand();c.Transaction=tx;c.CommandText="INSERT INTO accounts VALUES($a,$u,$n,'synthetic-unused-hash',1);INSERT INTO native_account_ids(native_id,account_id) VALUES($id,$a);INSERT INTO native_account_profiles VALUES($a,$id,1,1,$bytes);";
            c.Parameters.AddWithValue("$a",account);c.Parameters.AddWithValue("$u",username);c.Parameters.AddWithValue("$n",av.DisplayName);c.Parameters.AddWithValue("$id",nid);c.Parameters.AddWithValue("$bytes",bytes);c.ExecuteNonQuery();
        }
        tx.Commit();
    }
    using(var s=Store(cap,true,true,true))Fault(()=>s.WriteNativeAccountProfile(admin,a.Id,id,0,Model(s.Profile(a.Id),id)),429,"native_profile_limit");
    _=Sql(cap,"INSERT INTO native_account_profiles SELECT account_id,native_id,1,1,x'7B7D' FROM native_account_ids WHERE native_id="+id+";");
    Invalid(()=>{using var unused=Store(cap);},"row cap rejects oversized persisted profile table before decoding");

    string restore=Folder();using(var s=Store(f,true,true,true))
    {using var meta=JsonDocument.Parse(JsonSerializer.Serialize(s.Backup()));File.Copy(Path.Combine(f,"backups",meta.RootElement.GetProperty("file").GetString()!),Db(restore));}
    File.Copy(Path.Combine(f,".metallic-backend"),Path.Combine(restore,".metallic-backend"));
    using(var s=Store(restore,true,true,true))
    {
        Check(s.NativeAccountProfileForSession(access,a.Id,id).GetResponseBytes().AsSpan().SequenceEqual(valid),"consistent backup restores explicit profile plus session and identity binding");
        Check(s.Authenticate("Bearer "+ownedSession.AccessToken).AccountId==a.Id,"backup preserves old owned session alongside profile");
        s.RevokeNativeSession(access,a.Id,id);Fault(()=>s.NativeAccountProfileForSession(access,a.Id,id),401,"native_session_rejected");
    }
    var report=new{status="native_account_profiles_self_tests_passed",checks=checks.Count,tests=checks,nativeClientExecuted=false,servicesStarted=false,nativeRoutesEnabled=false,installedDatabaseAccessed=false,syntheticExplicitInputsOnly=true};
    using(var output=new FileStream(args[1],FileMode.CreateNew))JsonSerializer.Serialize(output,report,new JsonSerializerOptions{WriteIndented=true});
    Console.WriteLine(JsonSerializer.Serialize(new{report.status,report.checks}));
}
finally
{
    string full=Path.GetFullPath(root),temp=Path.GetFullPath(Path.GetTempPath());
    if(!string.Equals(full,Path.GetFullPath(Path.Combine(temp,name)),StringComparison.OrdinalIgnoreCase)||!name.StartsWith("metallic-native-profiles-",StringComparison.Ordinal)||
        !Guid.TryParseExact(name["metallic-native-profiles-".Length..],"N",out _)||!full.StartsWith(temp.TrimEnd(Path.DirectorySeparatorChar)+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase)||
        (File.GetAttributes(full)&FileAttributes.ReparsePoint)!=0)throw new InvalidOperationException("Temporary cleanup target is invalid.");
    Directory.Delete(full,true);
}
