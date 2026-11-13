using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using Microsoft.AspNetCore.Identity;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.Options;

namespace Metallic.RC.Backend;

// Durable Metallic-owned accounts/rooms. This is not the still-unmapped RecNet login protocol.
public sealed partial class BackendStore : IDisposable
{
    private readonly object gate = new();
    private readonly FileStream owner;
    private readonly string directory;
    private readonly string connectionString;
    private readonly PasswordHasher<string> passwords = new(Options.Create(new PasswordHasherOptions { IterationCount = 600000 }));
    private readonly string dummyHash;
    private readonly byte[] adminHash;
    private readonly bool nativeIdentityEnabled;
    private bool disposed;
    public string InstanceId { get; } = Guid.NewGuid().ToString("N");

    public BackendStore(BackendOptions options)
    {
        nativeIdentityEnabled = options.EnableNativeIdentityMapping;
        nativeSessionsEnabled = options.EnableNativeSessions;
        nativeProfilesEnabled = options.EnableNativeAccountProfiles;
        nativeGatewayEnabled = options.EnableNativeGateway;
        if (nativeGatewayEnabled && (!nativeIdentityEnabled || !nativeSessionsEnabled || !nativeProfilesEnabled))
            throw new ArgumentException("Native gateway requires explicit identity, session and profile options.");
        if (nativeSessionsEnabled && !nativeIdentityEnabled)
            throw new ArgumentException("Native sessions require explicit native identity mapping.");
        if (nativeProfilesEnabled && !nativeIdentityEnabled)
            throw new ArgumentException("Native profiles require explicit native identity mapping.");
        directory = options.DataDirectory;
        BackendFiles.Prepare(directory);
        owner = new FileStream(Path.Combine(directory, "backend.lock"), FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
        try
        {
            string keyFile = Path.Combine(directory, "admin.key");
            if (!File.Exists(keyFile)) BackendFiles.WriteAtomic(keyFile, Encoding.ASCII.GetBytes(Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(32))));
            string key = File.ReadAllText(keyFile).Trim();
            if (!Regex.IsMatch(key, "\\A[0-9a-f]{64}\\z", RegexOptions.CultureInvariant)) throw new IOException("Invalid admin key.");
            adminHash = SHA256.HashData(Encoding.UTF8.GetBytes(key));
            connectionString = new SqliteConnectionStringBuilder { DataSource = Path.Combine(directory, "backend.sqlite"), Pooling = false, DefaultTimeout = 5 }.ToString();
            using var db = Open();
            Execute(db, "PRAGMA journal_mode=WAL;");
            long version = Convert.ToInt64(Scalar(db, "PRAGMA user_version;"), CultureInfo.InvariantCulture);
            if (version is not (0 or 1 or 2 or 3 or 4 or 5)) throw new IOException("Unsupported backend database version; not changing it.");
            if (nativeGatewayEnabled && version is not (4 or 5))
                throw new IOException("Native gateway requires an existing validated v4 profile database.");
            if (nativeProfilesEnabled && version is not (3 or 4 or 5))
                throw new IOException("Native profiles require an existing validated v3 session database.");
            if (nativeSessionsEnabled && version is not (2 or 3 or 4 or 5))
                throw new IOException("Native sessions require an existing validated v2 identity database.");
            if (version == 0)
            {
                using var transaction = db.BeginTransaction();
                Execute(db, """
                    CREATE TABLE accounts(id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, display_name TEXT NOT NULL, password_hash TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1);
                    CREATE TABLE sessions(token_hash TEXT PRIMARY KEY, account_id TEXT NOT NULL REFERENCES accounts(id), expires_at INTEGER NOT NULL);
                    CREATE INDEX sessions_account ON sessions(account_id);
                    CREATE TABLE rooms(id TEXT PRIMARY KEY, owner_id TEXT NOT NULL REFERENCES accounts(id), name TEXT NOT NULL, capacity INTEGER NOT NULL CHECK(capacity BETWEEN 1 AND 32), is_public INTEGER NOT NULL CHECK(is_public IN(0,1)), revision INTEGER NOT NULL DEFAULT 1);
                    PRAGMA user_version=1;
                    """, transaction);
                transaction.Commit();
                version = 1;
            }
            if (!Equals(Scalar(db, "PRAGMA quick_check;"), "ok")) throw new IOException("Backend database integrity check failed; original preserved.");
            if (version == 1 && nativeIdentityEnabled)
            {
                NativeIdentityMap.Migrate(db);
                version = 2;
            }
            // A previously opted-in database remains usable by the owned APIs
            // when this feature is off, but an invalid mapping is never adopted.
            if (version is 2 or 3 or 4 or 5) NativeIdentityMap.Validate(db, null);
            if (version == 2 && nativeSessionsEnabled) { NativeSessions.Migrate(db); version = 3; }
            if (version is 3 or 4 or 5) NativeSessions.Validate(db, null);
            if (version == 3 && nativeProfilesEnabled) { NativeAccountProfiles.Migrate(db); version = 4; }
            if (version is 4 or 5) NativeAccountProfiles.Validate(db, null, allPayloads: true);
            if (version == 4 && nativeGatewayEnabled) { NativeGatewayKeys.Migrate(db); version = 5; }
            if (version == 5) NativeGatewayKeys.Validate(db, null);
            dummyHash = passwords.HashPassword("dummy", Convert.ToHexString(RandomNumberGenerator.GetBytes(32)));
        }
        catch { owner.Dispose(); throw; }
    }
    private SqliteConnection Open()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        var db = new SqliteConnection(connectionString);
        try { db.Open(); Execute(db, "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;"); return db; }
        catch { db.Dispose(); throw; }
    }
    private static SqliteCommand Command(SqliteConnection db, string sql, SqliteTransaction? tx, params (string Name, object Value)[] args)
    {
        var command = db.CreateCommand(); command.CommandText = sql; command.Transaction = tx;
        foreach (var (name, value) in args) command.Parameters.AddWithValue(name, value);
        return command;
    }
    private static int Execute(SqliteConnection db, string sql, SqliteTransaction? tx = null, params (string Name, object Value)[] args)
    { using var command = Command(db, sql, tx, args); return command.ExecuteNonQuery(); }
    private static object? Scalar(SqliteConnection db, string sql, SqliteTransaction? tx = null, params (string Name, object Value)[] args)
    { using var command = Command(db, sql, tx, args); return command.ExecuteScalar(); }
    private static string Username(string? value)
    {
        if (value is null || !Regex.IsMatch(value, "\\A[A-Za-z0-9_]{3,24}\\z", RegexOptions.CultureInvariant)) throw new BackendFault(400, "invalid_username");
        return value.ToLowerInvariant();
    }
    private static string Display(string? value, int max = 48)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Length > max || value.Any(char.IsControl)) throw new BackendFault(400, "invalid_name");
        return value.Trim();
    }
    private static void Password(string? value)
    { if (value is null || value.Length is < 12 or > 256) throw new BackendFault(400, "password_length_12_to_256_required"); }
    public bool IsAdmin(string candidate) => candidate.Length == 64 && CryptographicOperations.FixedTimeEquals(adminHash, SHA256.HashData(Encoding.UTF8.GetBytes(candidate)));
    public AccountView Register(RegisterRequest request)
    {
        string username = Username(request.Username), display = Display(request.DisplayName); Password(request.Password);
        string id = Guid.NewGuid().ToString("N"), hash = passwords.HashPassword(id, request.Password);
        lock (gate)
        {
            using var db = Open();
            try { Execute(db, "INSERT INTO accounts(id,username,display_name,password_hash) VALUES($id,$user,$display,$hash)", null, ("$id",id),("$user",username),("$display",display),("$hash",hash)); }
            catch (SqliteException ex) when (ex.SqliteErrorCode == 19) { throw new BackendFault(409, "username_taken"); }
        }
        return new(id, username, display, 1);
    }
    // Password verification only: no session, identity allocation or profile write.
    internal AccountView VerifyCredentials(LoginRequest request)
    {
        string username = Username(request.Username); Password(request.Password);
        (AccountView? Account,string Hash) record;
        lock (gate)
        {
            using var db = Open(); record=CredentialRecord(db,null,username);
        }
        return CheckedCredential(record.Account,record.Hash,request.Password);
    }
    private (AccountView? Account,string Hash) CredentialRecord(SqliteConnection db,SqliteTransaction? tx,string username)
    {
        using var command=Command(db,"SELECT id,username,display_name,revision,password_hash FROM accounts WHERE username=$user",tx,("$user",username));
        using var reader=command.ExecuteReader();
        return reader.Read()?(new AccountView(reader.GetString(0),reader.GetString(1),reader.GetString(2),reader.GetInt64(3)),reader.GetString(4)):(null,dummyHash);
    }
    private AccountView CheckedCredential(AccountView? account,string hash,string password)
    {
        var checkedPassword = passwords.VerifyHashedPassword(account?.Id ?? "dummy", hash, password);
        if (account is null || checkedPassword == PasswordVerificationResult.Failed) throw new BackendFault(401, "invalid_credentials");
        return account;
    }
    private AccountView VerifyCredentialsInTransaction(LoginRequest request,SqliteConnection db,SqliteTransaction tx)
    {
        string username=Username(request.Username);Password(request.Password);
        var record=CredentialRecord(db,tx,username);
        return CheckedCredential(record.Account,record.Hash,request.Password);
    }
    public SessionView Login(LoginRequest request)
    {
        var account = VerifyCredentials(request);
        string token = Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(32));
        var expires = DateTimeOffset.UtcNow.AddHours(1);
        lock (gate)
        {
            using var db = Open(); using var transaction = db.BeginTransaction();
            Execute(db, "DELETE FROM sessions WHERE expires_at <= $now", transaction, ("$now",DateTimeOffset.UtcNow.ToUnixTimeSeconds()));
            long count = Convert.ToInt64(Scalar(db,"SELECT COUNT(*) FROM sessions WHERE account_id=$id",transaction,("$id",account.Id)),CultureInfo.InvariantCulture);
            if (count >= 16) throw new BackendFault(429,"session_limit");
            Execute(db,"INSERT INTO sessions VALUES($hash,$id,$expires)",transaction,("$hash",BackendFiles.Hash(token)),("$id",account.Id),("$expires",expires.ToUnixTimeSeconds()));
            transaction.Commit();
        }
        return new(account, token, expires);
    }
    public Principal Authenticate(string authorization)
    {
        if (!authorization.StartsWith("Bearer ", StringComparison.Ordinal) || authorization.Length != 71) throw new BackendFault(401,"session_required");
        string token = authorization[7..];
        if (!Regex.IsMatch(token,"\\A[0-9a-f]{64}\\z",RegexOptions.CultureInvariant)) throw new BackendFault(401,"session_required");
        return AuthenticateHash(BackendFiles.Hash(token));
    }
    public Principal AuthenticateHash(string hash)
    {
        lock(gate)
        {
            using var db=Open(); using var cmd=Command(db,"SELECT account_id,expires_at FROM sessions WHERE token_hash=$hash AND expires_at>$now",null,("$hash",hash),("$now",DateTimeOffset.UtcNow.ToUnixTimeSeconds()));
            using var reader=cmd.ExecuteReader();
            if(!reader.Read())throw new BackendFault(401,"session_expired_or_revoked");
            return new(reader.GetString(0),hash,DateTimeOffset.FromUnixTimeSeconds(reader.GetInt64(1)));
        }
    }
    public void Logout(Principal user)
    { lock(gate){using var db=Open();Execute(db,"DELETE FROM sessions WHERE token_hash=$hash",null,("$hash",user.TokenHash));} }
    public AccountView Profile(string id)
    {
        lock(gate)
        {
            using var db=Open();using var cmd=Command(db,"SELECT id,username,display_name,revision FROM accounts WHERE id=$id",null,("$id",id));using var rd=cmd.ExecuteReader();
            if(!rd.Read())throw new BackendFault(404,"account_not_found");return new(rd.GetString(0),rd.GetString(1),rd.GetString(2),rd.GetInt64(3));
        }
    }
    public int GetOrCreateNativeAccountId(string accountId)
    {
        if (!nativeIdentityEnabled) throw new BackendFault(409, "native_identity_disabled");
        if (!NativeIdentityMap.IsAccountId(accountId)) throw new BackendFault(400, "invalid_account_id");
        lock (gate)
        {
            using var db = Open(); using var transaction = db.BeginTransaction();
            int id = NativeIdentityMap.GetOrCreate(db, transaction, accountId);
            transaction.Commit();
            return id;
        }
    }
    public string AccountIdForNative(int nativeId)
    {
        if (!nativeIdentityEnabled) throw new BackendFault(409, "native_identity_disabled");
        if (nativeId <= 0) throw new BackendFault(400, "invalid_native_account_id");
        lock (gate)
        {
            using var db = Open(); using var transaction = db.BeginTransaction();
            string id = NativeIdentityMap.Owner(db, transaction, nativeId);
            transaction.Commit();
            return id;
        }
    }
    public AccountView UpdateProfile(Principal user, ProfileWrite request, long revision)
    {
        string name=Display(request.DisplayName);
        lock(gate)
        {
            using var db=Open();if(Execute(db,"UPDATE accounts SET display_name=$name,revision=revision+1 WHERE id=$id AND revision=$revision",null,("$name",name),("$id",user.AccountId),("$revision",revision))!=1)throw new BackendFault(412,"revision_conflict");
            return Profile(user.AccountId);
        }
    }
    public RoomView CreateRoom(Principal user, RoomWrite request)
    {
        string name=Display(request.Name,64);if(request.Capacity is <1 or >32)throw new BackendFault(400,"invalid_capacity");
        string id=Guid.NewGuid().ToString("N");
        lock(gate)
        {
            using var db=Open();if(Convert.ToInt64(Scalar(db,"SELECT COUNT(*) FROM rooms WHERE owner_id=$owner",null,("$owner",user.AccountId)),CultureInfo.InvariantCulture)>=100)throw new BackendFault(429,"room_limit");
            Execute(db,"INSERT INTO rooms VALUES($id,$owner,$name,$cap,$public,1)",null,("$id",id),("$owner",user.AccountId),("$name",name),("$cap",request.Capacity),("$public",request.IsPublic?1:0));
            return new(id,user.AccountId,name,request.Capacity,request.IsPublic,1);
        }
    }
    public IReadOnlyList<RoomView> Rooms(Principal user)
    {
        lock(gate)
        {
            using var db=Open();using var cmd=Command(db,"SELECT id,owner_id,name,capacity,is_public,revision FROM rooms WHERE is_public=1 OR owner_id=$owner ORDER BY id LIMIT 200",null,("$owner",user.AccountId));using var rd=cmd.ExecuteReader();var list=new List<RoomView>();
            while(rd.Read())list.Add(ReadRoom(rd));return list;
        }
    }
    private static RoomView ReadRoom(SqliteDataReader rd)=>new(rd.GetString(0),rd.GetString(1),rd.GetString(2),rd.GetInt32(3),rd.GetInt32(4)!=0,rd.GetInt64(5));
    public RoomView Room(Principal user,string id)
    {
        lock(gate)
        {
            using var db=Open();using var cmd=Command(db,"SELECT id,owner_id,name,capacity,is_public,revision FROM rooms WHERE id=$id",null,("$id",id));using var rd=cmd.ExecuteReader();
            if(!rd.Read())throw new BackendFault(404,"room_not_found");var room=ReadRoom(rd);
            if(!room.IsPublic&&room.OwnerId!=user.AccountId)throw new BackendFault(404,"room_not_found");return room;
        }
    }
    public object Backup()
    {
        lock(gate)
        {
            string folder=Path.Combine(directory,"backups");BackendFiles.PrivateDirectory(folder);
            string name="backend-"+DateTimeOffset.UtcNow.ToString("yyyyMMddTHHmmssfffZ",CultureInfo.InvariantCulture)+"-"+Guid.NewGuid().ToString("N")+".sqlite";
            string path=Path.Combine(folder,name);using(var db=Open())using(var destination=new SqliteConnection(new SqliteConnectionStringBuilder{DataSource=path,Pooling=false}.ToString()))
            {
                destination.Open();db.BackupDatabase(destination);
                if(!Equals(Scalar(destination,"PRAGMA quick_check;"),"ok"))throw new IOException("Backup integrity check failed.");
            }
            long size=new FileInfo(path).Length;string sha;using(var stream=File.OpenRead(path))sha=Convert.ToHexStringLower(SHA256.HashData(stream));
            foreach(var old in new DirectoryInfo(folder).GetFiles("backend-*.sqlite").OrderByDescending(f=>f.Name,StringComparer.Ordinal).Skip(8))old.Delete();
            return new{file=name,sha256=sha,bytes=size,integrity="ok"};
        }
    }
    public void WriteRuntime(int port)
    {
        BackendFiles.WriteAtomic(Path.Combine(directory,"runtime.json"),System.Text.Json.JsonSerializer.SerializeToUtf8Bytes(new{service="metallic-rc-backend",port,processId=Environment.ProcessId,instanceId=InstanceId,startedUtc=DateTimeOffset.UtcNow,clientIntegrated=false}));
    }
    public void Dispose(){lock(gate){if(disposed)return;disposed=true;owner.Dispose();}}
}
