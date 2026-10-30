using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Metallic.RC.Backend;
using Microsoft.Data.Sqlite;

if (args.Length != 2 || args[0] != "--self-test") throw new ArgumentException("Use --self-test NEW-result.json.");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");
var checks = new List<string>();
void Check(bool value, string name) { if (!value) throw new InvalidOperationException(name); checks.Add(name); }
void Reject(Action action, string code = "native_session_rejected", int status = 401)
{
    bool rejected = false;
    try { action(); } catch (BackendFault e) { rejected = e.Code == code && e.Status == status; }
    Check(rejected, "static rejection: " + code);
}
void Invalid(Action action, string label)
{ bool rejected = false; try { action(); } catch (IOException) { rejected = true; } Check(rejected,label); }
string name = "metallic-native-sessions-" + Guid.NewGuid().ToString("N");
string root = Path.Combine(Path.GetTempPath(),name); Directory.CreateDirectory(root);
string Folder() { string value = Path.Combine(root,Guid.NewGuid().ToString("N")); Directory.CreateDirectory(value); return value; }
string Db(string folder) => Path.Combine(folder,"backend.sqlite");
void Owned(string folder)
{
    if (!Path.GetFullPath(folder).StartsWith(Path.GetFullPath(root)+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase))
        throw new InvalidOperationException("Synthetic fixture escaped its generated root.");
}
SqliteConnection Open(string folder)
{
    Owned(folder);var db=new SqliteConnection(new SqliteConnectionStringBuilder{DataSource=Db(folder),Pooling=false}.ToString());db.Open();return db;
}
object? Sql(string folder,string text)
{using var db=Open(folder);using var c=db.CreateCommand();c.CommandText=text;return c.ExecuteScalar();}
long Count(string folder,string text)=>(long)(Sql(folder,text)??throw new InvalidOperationException("Expected fixture integer."));
BackendStore Store(string folder,bool identities=true,bool sessions=false)=>new(new BackendOptions(0,folder){EnableNativeIdentityMapping=identities,EnableNativeSessions=sessions});
string Clone(string folder)
{
    Owned(folder);using var owner=new FileStream(Path.Combine(folder,"backend.lock"),FileMode.OpenOrCreate,FileAccess.ReadWrite,FileShare.None);
    using(var db=Open(folder))using(var c=db.CreateCommand())
    {c.CommandText="PRAGMA wal_checkpoint(TRUNCATE);";using var r=c.ExecuteReader();if(!r.Read()||r.GetInt64(0)!=0)throw new IOException("Fixture WAL checkpoint busy.");}
    if(File.Exists(Db(folder)+"-wal")&&new FileInfo(Db(folder)+"-wal").Length!=0)throw new IOException("Fixture WAL not drained.");
    string copy=Folder();File.Copy(Db(folder),Db(copy));File.Copy(Path.Combine(folder,".metallic-backend"),Path.Combine(copy,".metallic-backend"));return copy;
}
string Token()=>Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(48));
string Hash(string value)=>Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(value)));
NativeSessionMaterial Material(string access,string refresh,int accessSeconds=1800,int refreshSeconds=86400)=>
    new(access,refresh,DateTimeOffset.UtcNow.AddSeconds(accessSeconds),DateTimeOffset.UtcNow.AddSeconds(refreshSeconds));
AccountView Register(BackendStore store,string password)=>store.Register(new("user_"+Guid.NewGuid().ToString("N")[..12],password,"Synthetic profile"));
try
{
    string directory=Folder(), password=Token();AccountView account,other;int id,otherId;SessionView owned;
    using(var store=Store(directory,false))
    {
        account=Register(store,password);other=Register(store,password);owned=store.Login(new(account.Username,password));
        Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,1,Material(Token(),Token())),"native_sessions_disabled",503);
    }
    Check(Count(directory,"PRAGMA user_version;")==1,"default off new database stays v1");
    string v1=Clone(directory);
    Invalid(()=>{using var unused=Store(v1,true,true);},"v1 cannot silently migrate for native session admission");
    Check(Count(v1,"PRAGMA user_version;")==1&&Count(v1,"SELECT count(*) FROM sqlite_schema WHERE name='native_account_ids';")==0,"v1 refusal preserves original schema");
    using(var store=Store(directory)) {id=store.GetOrCreateNativeAccountId(account.Id);otherId=store.GetOrCreateNativeAccountId(other.Id);}
    Check(Count(directory,"PRAGMA user_version;")==2,"explicit identity step creates v2");string v2=Clone(directory);
    using(var store=Store(v2,false))Check(store.Authenticate("Bearer "+owned.AccessToken).AccountId==account.Id,"v2 default off preserves owned sessions");
    Check(Count(v2,"PRAGMA user_version;")==2&&Count(v2,"SELECT count(*) FROM sqlite_schema WHERE name='native_sessions';")==0,"default off v2 has no session schema");

    string access=Token(),refresh=Token(),nextAccess=Token(),nextRefresh=Token();NativeSessionView first;
    using(var store=Store(directory,true,true))
    {
        Reject(()=>store.CreateNativeSession(new(other.Username,password),account.Id,id,Material(access,refresh)));
        Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,otherId,Material(access,refresh)));
        Reject(()=>store.CreateNativeSession(new(account.Username,Token()),account.Id,id,Material(access,refresh)),"invalid_credentials");
        Check(Count(directory,"SELECT count(*) FROM native_sessions;")==0,"wrong credentials or identity create no native family");
        long ownedBefore=Count(directory,"SELECT count(*) FROM sessions;");
        first=store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(access,refresh));
        Check(first.NativeAccountId==id&&first.AccountId==account.Id&&first.Generation==0,"issue binds verified account and durable native ID");
        Check(Count(directory,"SELECT count(*) FROM sessions;")==ownedBefore,"native issue does not mint an unrelated owned session");
        Check(store.AuthenticateNativeSession(access,account.Id,id).SessionId==first.SessionId,"exact stored access accepted");
        foreach(string bad in new[]{refresh,Token(),access.ToUpperInvariant()," "+access,access+"x","short",new string('x',16385),new string('x',31)+"\ud800"})
            Reject(()=>store.AuthenticateNativeSession(bad,account.Id,id));
        Reject(()=>store.AuthenticateNativeSession(access,other.Id,id));Reject(()=>store.AuthenticateNativeSession(access,account.Id,otherId));
        Reject(()=>store.Authenticate("Bearer "+access),"session_required");
        Check(JsonSerializer.Serialize(Material(access,refresh))=="{}"&&!Material(access,refresh).ToString().Contains(access,StringComparison.Ordinal),"material has no JSON or incidental text secret output");
        Check((string)Sql(directory,"SELECT access_hash FROM native_sessions;")! == Hash(access),"database stores access SHA256 only");
        Check((string)Sql(directory,"SELECT refresh_hash FROM native_sessions;")! == Hash(refresh),"database stores refresh SHA256 only");
        Reject(()=>store.RotateNativeSession(refresh,other.Id,id,Material(nextAccess,nextRefresh)));
        Reject(()=>store.RotateNativeSession(refresh,account.Id,otherId,Material(nextAccess,nextRefresh)));
        Reject(()=>store.RotateNativeSession(refresh,account.Id,id,Material(access,Token())));
        Reject(()=>store.RotateNativeSession(refresh,account.Id,id,Material(Token(),Token(),-1)));
        Check(store.AuthenticateNativeSession(access,account.Id,id).Generation==0,"failed rotations preserve original access and refresh generation");
        var rotated=store.RotateNativeSession(refresh,account.Id,id,Material(nextAccess,nextRefresh));
        Check(rotated.Generation==1&&rotated.SessionId==first.SessionId,"refresh rotates within same durable family");
        Reject(()=>store.AuthenticateNativeSession(access,account.Id,id));
        Check(store.AuthenticateNativeSession(nextAccess,account.Id,id).Generation==1,"new access works after rotation");
    }
    Check(Count(directory,"PRAGMA user_version;")==3,"explicit v2 to v3 migration committed");
    using(var store=Store(directory,false))
    {
        Check(store.Authenticate("Bearer "+owned.AccessToken).AccountId==account.Id,"v3 default off preserves owned session API");
        Reject(()=>store.AuthenticateNativeSession(nextAccess,account.Id,id),"native_sessions_disabled",503);
    }
    using(var store=Store(directory,true,true))
    {
        Check(store.AuthenticateNativeSession(nextAccess,account.Id,id).SessionId==first.SessionId,"native access survives restart");
        Reject(()=>store.RotateNativeSession(refresh,account.Id,id,Material(Token(),Token())));
        Reject(()=>store.AuthenticateNativeSession(nextAccess,account.Id,id));
        Reject(()=>store.RotateNativeSession(nextRefresh,account.Id,id,Material(Token(),Token())));
        Check(Count(directory,"SELECT revoked FROM native_sessions;")==1,"consumed refresh replay durably revokes current family");
        Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(access,Token())));
        string a=Token(),r=Token();store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(a,r));
        store.RevokeNativeSession(a,account.Id,id);Reject(()=>store.AuthenticateNativeSession(a,account.Id,id));
        Reject(()=>store.RotateNativeSession(r,account.Id,id,Material(Token(),Token())));
    }

    // Race two real store calls. One consumption succeeds; the second is a
    // replay and revokes the family, including the first caller's replacement.
    using(var store=Store(directory,true,true))
    {
        string a=Token(),r=Token();var created=store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(a,r));
        int success=0,rejected=0;var issued=new System.Collections.Concurrent.ConcurrentBag<string>();
        Parallel.For(0,2,_=>{string replacement=Token();try{store.RotateNativeSession(r,account.Id,id,Material(replacement,Token()));Interlocked.Increment(ref success);issued.Add(replacement);}catch(BackendFault e)when(e.Code=="native_session_rejected"){Interlocked.Increment(ref rejected);}});
        Check(success==1&&rejected==1,"concurrent refresh has exactly one consumption and one replay rejection");
        foreach(string token in issued)Reject(()=>store.AuthenticateNativeSession(token,account.Id,id));
        Check(Count(directory,"SELECT revoked FROM native_sessions WHERE session_id='"+created.SessionId+"';")==1,"race replay revocation committed");
    }

    using(var store=Store(directory,true,true))
    {
        foreach(var pair in new[]{(0,86400),(7200,86400),(1800,1799),(1800,31*86400)})
            Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(Token(),Token(),pair.Item1,pair.Item2)));
        Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(new string('x',16383)+"é",Token())));
        string same=Token();Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(same,same)));
        string a=Token(),r=Token();store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(a,r));
        _=Sql(directory,"UPDATE native_sessions SET created_at=created_at-100,access_expires_at="+(DateTimeOffset.UtcNow.ToUnixTimeSeconds()-1)+" WHERE access_hash='"+Hash(a)+"';");
        Reject(()=>store.AuthenticateNativeSession(a,account.Id,id));
        string replacement=Token();store.RotateNativeSession(r,account.Id,id,Material(replacement,Token()));
        Check(store.AuthenticateNativeSession(replacement,account.Id,id).Generation==1,"expired access can refresh within live family");
    }
    string expired=Clone(v2),expiredAccess=Token(),expiredRefresh=Token();
    using(var store=Store(expired,true,true))store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(expiredAccess,expiredRefresh));
    _=Sql(expired,"UPDATE native_sessions SET created_at=1,access_expires_at=2,refresh_expires_at=3;");
    using(var store=Store(expired,true,true))
    {
        Check(Count(expired,"SELECT revoked FROM native_sessions;")==0,"expiry fixture has a previously valid unrevoked family");
        Reject(()=>store.AuthenticateNativeSession(expiredAccess,account.Id,id));
        Reject(()=>store.RotateNativeSession(expiredRefresh,account.Id,id,Material(Token(),Token())));
    }
    string anchor=Clone(v2),anchorAccess=Token(),anchorRefresh=Token();
    using(var store=Store(anchor,true,true))
    {
        store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(anchorAccess,anchorRefresh));
        _=Sql(anchor,"UPDATE native_sessions SET created_at=created_at-86400;");
        Reject(()=>store.RotateNativeSession(anchorRefresh,account.Id,id,Material(Token(),Token(),1800,30*86400)));
        Check(store.AuthenticateNativeSession(anchorAccess,account.Id,id).Generation==0,"failed family-lifetime extension leaves original access valid");
        store.RotateNativeSession(anchorRefresh,account.Id,id,Material(Token(),Token(),1800,28*86400));
        Check(Count(anchor,"SELECT generation FROM native_sessions;")==1,"rotation within original family deadline accepted");
    }

    string cap=Clone(v2);using(var store=Store(cap,true,true))
    {
        var accesses=new List<string>();
        for(int i=0;i<8;i++){string a=Token();store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(a,Token()));accesses.Add(a);}
        Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(Token(),Token())),"native_session_limit",429);
        store.RevokeNativeSession(accesses[0],account.Id,id);
        store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(Token(),Token()));
        Check(Count(cap,"SELECT count(*) FROM native_sessions;")==9,"active cap can release capacity without deleting history");
    }
    string rotations=Clone(v2);using(var store=Store(rotations,true,true))
    {
        string a=Token(),r=Token();store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(a,r));
        for(int i=1;i<=32;i++){string next=Token();var view=store.RotateNativeSession(r,account.Id,id,Material(Token(),next));r=next;Check(view.Generation==i,"bounded rotation generation "+i);}
        Reject(()=>store.RotateNativeSession(r,account.Id,id,Material(Token(),Token())),"native_rotation_limit",429);
        Check(Count(rotations,"SELECT count(*) FROM native_session_tokens;")==66,"rotation cap retains bounded access and refresh history");
    }
    string total=Clone(v2);using(var store=Store(total,true,true)){}
    using(var db=Open(total))using(var tx=db.BeginTransaction())
    {
        for(int i=0;i<1024;i++)
        {
            string sid=Guid.NewGuid().ToString("N"),a=Hash(Token()),r=Hash(Token());long now=DateTimeOffset.UtcNow.ToUnixTimeSeconds();
            using var c=db.CreateCommand();c.Transaction=tx;c.CommandText="INSERT INTO native_sessions VALUES($id,$account,$native,$a,$r,$now,$ae,$re,0,1); INSERT INTO native_session_tokens VALUES($a,$id,0,0); INSERT INTO native_session_tokens VALUES($r,$id,1,0);";
            foreach(var p in new (string,object)[]{("$id",sid),("$account",account.Id),("$native",id),("$a",a),("$r",r),("$now",now),("$ae",now+100),("$re",now+200)})c.Parameters.AddWithValue(p.Item1,p.Item2);
            c.ExecuteNonQuery();
        }
        tx.Commit();
    }
    using(var store=Store(total,true,true))Reject(()=>store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(Token(),Token())),"native_session_limit",429);
    Check(Count(total,"SELECT count(*) FROM native_sessions;")==1024,"durable family cap never prunes revoked token history");

    string collision=Clone(v2);_=Sql(collision,"CREATE INDEX native_session_tokens_session ON accounts(username);");
    Invalid(()=>{using var unused=Store(collision,true,true);},"migration collision fails");
    Check(Count(collision,"PRAGMA user_version;")==2&&Count(collision,"SELECT count(*) FROM sqlite_schema WHERE name='native_sessions';")==0,"failed migration rolls back all new DDL and version");
    foreach(string mutation in new[]{"CREATE TRIGGER unexpected_session AFTER INSERT ON native_sessions BEGIN SELECT 1; END;",
        "DROP INDEX native_sessions_account;", "DELETE FROM native_session_tokens WHERE rowid=(SELECT min(rowid) FROM native_session_tokens);",
        "UPDATE native_sessions SET native_id="+otherId+";", "UPDATE native_sessions SET access_expires_at=0;", "PRAGMA user_version=4;"})
    {
        string malformed=Clone(directory);_=Sql(malformed,mutation);
        foreach(bool enabled in new[]{false,true})Invalid(()=>{using var unused=Store(malformed,enabled,enabled);},"malformed v3 rejected with native APIs "+enabled);
    }
    string afterOpen=Clone(v2);using(var store=Store(afterOpen,true,true))
    {
        _=Sql(afterOpen,"CREATE TRIGGER redirect_native_session AFTER INSERT ON native_sessions BEGIN UPDATE native_sessions SET account_id='"+other.Id+"',native_id="+otherId+" WHERE session_id=new.session_id; END;");
        Invalid(()=>store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(Token(),Token())),"after-open ownership-redirection trigger rejected before issue");
        Check(Count(afterOpen,"SELECT count(*) FROM native_sessions;")==0,"unexpected trigger commits no session or wrong ownership");
    }

    string backupAccess=Token(),backupRefresh=Token();string restore=Folder();
    using(var store=Store(directory,true,true))
    {
        store.CreateNativeSession(new(account.Username,password),account.Id,id,Material(backupAccess,backupRefresh));
        using var metadata=JsonDocument.Parse(JsonSerializer.Serialize(store.Backup()));
        File.Copy(Path.Combine(directory,"backups",metadata.RootElement.GetProperty("file").GetString()!),Db(restore));
    }
    File.Copy(Path.Combine(directory,".metallic-backend"),Path.Combine(restore,".metallic-backend"));
    using(var store=Store(restore,true,true))
    {
        Check(store.AuthenticateNativeSession(backupAccess,account.Id,id).AccountId==account.Id,"consistent backup restores native identity and exact session binding");
        Check(store.Authenticate("Bearer "+owned.AccessToken).AccountId==account.Id,"backup preserves old owned session independently");
        Reject(()=>store.RotateNativeSession(refresh,account.Id,id,Material(Token(),Token())));
    }
    byte[] database=File.ReadAllBytes(Db(restore));
    Check(!Encoding.UTF8.GetString(database).Contains(backupAccess,StringComparison.Ordinal)&&!Encoding.UTF8.GetString(database).Contains(backupRefresh,StringComparison.Ordinal),"backup contains hashes rather than plaintext token material");
    var report=new{status="native_sessions_self_tests_passed",checks=checks.Count,tests=checks,nativeClientExecuted=false,servicesStarted=false,nativeRoutesEnabled=false,wireTokenFormatImplemented=false,installedDatabaseAccessed=false};
    using(var output=new FileStream(args[1],FileMode.CreateNew))JsonSerializer.Serialize(output,report,new JsonSerializerOptions{WriteIndented=true});
    Console.WriteLine(JsonSerializer.Serialize(new{report.status,report.checks}));
}
finally
{
    string full=Path.GetFullPath(root),temp=Path.GetFullPath(Path.GetTempPath());
    if(!string.Equals(full,Path.GetFullPath(Path.Combine(temp,name)),StringComparison.OrdinalIgnoreCase)||
        !name.StartsWith("metallic-native-sessions-",StringComparison.Ordinal)||!Guid.TryParseExact(name["metallic-native-sessions-".Length..],"N",out _)||
        !full.StartsWith(temp.TrimEnd(Path.DirectorySeparatorChar)+Path.DirectorySeparatorChar,StringComparison.OrdinalIgnoreCase)||
        (File.GetAttributes(full)&FileAttributes.ReparsePoint)!=0)throw new InvalidOperationException("Temporary cleanup target is invalid.");
    Directory.Delete(full,true);
}
