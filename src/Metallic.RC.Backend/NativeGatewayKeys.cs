using System.Globalization;
using System.Security.Cryptography;
using Microsoft.Data.Sqlite;
using Metallic.RC.Compatibility;

namespace Metallic.RC.Backend;

public sealed partial class BackendStore
{
    private readonly bool nativeGatewayEnabled;
    private void NativeGatewayRequired()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (!nativeGatewayEnabled) throw new BackendFault(501,"native_contract_not_implemented");
    }
    internal byte[] LoginNativeGateway(LoginRequest credentials,CancellationToken cancellationToken)
    {
        NativeGatewayRequired();
        cancellationToken.ThrowIfCancellationRequested();
        NativeTokenResponse response;
        lock(gate)
        {
            cancellationToken.ThrowIfCancellationRequested();
            using var db=Open();using var tx=db.BeginTransaction();
            cancellationToken.ThrowIfCancellationRequested();
            var account=VerifyCredentialsInTransaction(credentials,db,tx);
            cancellationToken.ThrowIfCancellationRequested();
            NativeGatewayKeys.Validate(db,tx);
            var raw=Scalar(db,"SELECT native_id FROM native_account_ids WHERE account_id=$id;",tx,("$id",account.Id));
            if(raw is not long native||native is <1 or >int.MaxValue)throw new BackendFault(501,"native_identity_not_initialized");
            int id=(int)native;
            _=NativeAccountProfiles.Read(db,tx,account.Id,id);
            byte[] master=NativeGatewayKeys.Master(db,tx);
            try
            {
                var now=DateTimeOffset.FromUnixTimeSeconds(DateTimeOffset.UtcNow.ToUnixTimeSeconds());
                response=NativeTokenResponse.Create(id,master,now,now.AddHours(1),now.AddDays(30));
                response.UseSessionMaterial((material,key)=>
                {
                    var session=NativeSessions.Create(db,tx,account.Id,id,material);
                    NativeGatewayKeys.Add(db,tx,session,key);
                });
                cancellationToken.ThrowIfCancellationRequested();tx.Commit();
            }
            finally{CryptographicOperations.ZeroMemory(master);}
        }
        // No credential response can escape before both durable writes commit.
        return response.WriteTransportBody();
    }
    internal byte[] ReadNativeGatewayAccount(string access,string signature,string rawTarget)
    {
        NativeGatewayRequired();
        if(!NativeGateway.AccountTarget(rawTarget))throw new BackendFault(501,"native_request_outside_profile");
        Span<byte> supplied=stackalloc byte[32];
        if(signature.Length!=44||!Convert.TryFromBase64String(signature,supplied,out int count)||count!=32||Convert.ToBase64String(supplied)!=signature)
            throw new BackendFault(401,"native_signature_rejected");
        lock(gate)
        {
            using var db=Open();using var tx=db.BeginTransaction();NativeGatewayKeys.Validate(db,tx);
            var session=NativeSessions.AuthenticateIssuedToken(db,tx,access);
            byte[] key=NativeGatewayKeys.RequestKey(db,tx,session);
            try
            {
                byte[] expected=Convert.FromBase64String(NativeRequestSigningProfile.Compute(key,rawTarget,null));
                if(!CryptographicOperations.FixedTimeEquals(supplied,expected))throw new BackendFault(401,"native_signature_rejected");
                var profile=NativeAccountProfiles.Read(db,tx,session.AccountId,session.NativeAccountId);
                tx.Commit();return profile.GetResponseBytes();
            }
            finally{CryptographicOperations.ZeroMemory(key);}
        }
    }
}

// Plaintext secret BLOBs are confined to the existing owner-only SQLite/backup
// boundary. This is not encryption at rest or protection from a local DB admin.
internal static class NativeGatewayKeys
{
    private const string MasterSql="""
        CREATE TABLE native_gateway_master(
            slot INTEGER PRIMARY KEY CHECK(slot=1),
            secret BLOB NOT NULL CHECK(length(secret)=32)
        ) STRICT
        """;
    private const string KeysSql="""
        CREATE TABLE native_gateway_request_keys(
            session_id TEXT PRIMARY KEY REFERENCES native_sessions(session_id),
            generation INTEGER NOT NULL CHECK(generation BETWEEN 0 AND 32),
            secret BLOB NOT NULL CHECK(length(secret)=32)
        ) STRICT
        """;
    private const string IndexSql="CREATE UNIQUE INDEX native_gateway_request_keys_secret ON native_gateway_request_keys(secret)";
    private static IOException Invalid()=>new("Native gateway key schema or data is invalid.");
    private static string Canonical(string s)=>s.Replace("\r\n","\n",StringComparison.Ordinal);
    private static SqliteCommand Command(SqliteConnection db,SqliteTransaction? tx,string sql,params object[] values)
    {
        var c=db.CreateCommand();c.Transaction=tx;c.CommandText=sql;
        for(int i=0;i<values.Length;i++)c.Parameters.AddWithValue("$p"+i,values[i]);return c;
    }
    private static object? Scalar(SqliteConnection db,SqliteTransaction? tx,string sql,params object[] values)
    {using var c=Command(db,tx,sql,values);return c.ExecuteScalar();}
    private static void Execute(SqliteConnection db,SqliteTransaction? tx,string sql,params object[] values)
    {using var c=Command(db,tx,sql,values);c.ExecuteNonQuery();}
    internal static void Migrate(SqliteConnection db)
    {
        using var tx=db.BeginTransaction();byte[] master=RandomNumberGenerator.GetBytes(32);
        try
        {
            if(master.All(b=>b==0))throw Invalid();
            foreach(string sql in new[]{MasterSql,KeysSql,IndexSql})Execute(db,tx,Canonical(sql));
            Execute(db,tx,"INSERT INTO native_gateway_master VALUES(1,$p0);",master);
            Validate(db,tx);Execute(db,tx,"PRAGMA user_version=5;");tx.Commit();
        }
        catch(SqliteException){throw new IOException("Native gateway migration failed; original version preserved.");}
        finally{CryptographicOperations.ZeroMemory(master);}
    }
    internal static void Validate(SqliteConnection db,SqliteTransaction? tx)
    {
        NativeAccountProfiles.Validate(db,tx,allPayloads:false);
        try
        {
            foreach(var d in new[]{("table","native_gateway_master",MasterSql),("table","native_gateway_request_keys",KeysSql),("index","native_gateway_request_keys_secret",IndexSql)})
                if(Scalar(db,tx,"SELECT sql FROM sqlite_schema WHERE type=$p0 AND name=$p1;",d.Item1,d.Item2)is not string sql||Canonical(sql)!=Canonical(d.Item3))throw Invalid();
            if(Scalar(db,tx,"SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name IN('native_gateway_master','native_gateway_request_keys') COLLATE NOCASE;")is not null)throw Invalid();
            if(Convert.ToInt64(Scalar(db,tx,"SELECT count(*) FROM native_gateway_master;"),CultureInfo.InvariantCulture)!=1||
                Convert.ToInt64(Scalar(db,tx,"SELECT count(*) FROM native_gateway_request_keys;"),CultureInfo.InvariantCulture)>NativeSessions.MaxFamilies)throw Invalid();
            byte[] master=Master(db,tx);
            try
            {
                var sessions=new List<(string Id,int Generation)>();
                using(var c=Command(db,tx,"SELECT k.session_id,k.generation,typeof(k.secret),length(k.secret),s.generation FROM native_gateway_request_keys k LEFT JOIN native_sessions s ON s.session_id=k.session_id;"))
                using(var r=c.ExecuteReader())while(r.Read())
                {
                    if(r.GetValue(0)is not string id||!NativeIdentityMap.IsAccountId(id)||r.GetValue(1)is not long gen||gen is <0 or >32||
                        r.GetString(2)!="blob"||r.GetValue(3)is not long length||length!=32||r.GetValue(4)is not long current||gen>current)throw Invalid();
                    sessions.Add((id,(int)gen));
                }
                foreach(var session in sessions)
                {
                    byte[] key=KeyBytes(db,tx,session.Id,session.Generation);
                    try{if(CryptographicOperations.FixedTimeEquals(master,key))throw Invalid();}
                    finally{CryptographicOperations.ZeroMemory(key);}
                }
            }
            finally{CryptographicOperations.ZeroMemory(master);}
        }
        catch(SqliteException){throw Invalid();}
    }
    internal static byte[] Master(SqliteConnection db,SqliteTransaction? tx)
    {
        using var c=Command(db,tx,"SELECT secret FROM native_gateway_master WHERE slot=1 AND typeof(secret)='blob' AND length(secret)=32;");
        using var r=c.ExecuteReader();if(!r.Read())throw Invalid();byte[] bytes=r.GetFieldValue<byte[]>(0);
        if(bytes.All(b=>b==0)){CryptographicOperations.ZeroMemory(bytes);throw Invalid();}return bytes;
    }
    private static byte[] KeyBytes(SqliteConnection db,SqliteTransaction? tx,string id,int generation)
    {
        using var c=Command(db,tx,"SELECT secret FROM native_gateway_request_keys WHERE session_id=$p0 AND generation=$p1 AND typeof(secret)='blob' AND length(secret)=32;",id,generation);
        using var r=c.ExecuteReader();if(!r.Read())throw Invalid();byte[] bytes=r.GetFieldValue<byte[]>(0);
        if(bytes.All(b=>b==0)){CryptographicOperations.ZeroMemory(bytes);throw Invalid();}return bytes;
    }
    internal static void Add(SqliteConnection db,SqliteTransaction tx,NativeSessionView session,ReadOnlySpan<byte> key)
    {
        if(key.Length!=32||session.Generation!=0)throw Invalid();byte[] copy=key.ToArray();
        try
        {
            Execute(db,tx,"INSERT INTO native_gateway_request_keys VALUES($p0,$p1,$p2);",session.SessionId,session.Generation,copy);
            Validate(db,tx);
        }
        finally{CryptographicOperations.ZeroMemory(copy);}
    }
    internal static byte[] RequestKey(SqliteConnection db,SqliteTransaction tx,NativeSessionView session)
    {
        object? generation=Scalar(db,tx,"SELECT generation FROM native_gateway_request_keys WHERE session_id=$p0;",session.SessionId);
        if(generation is not long current||current!=session.Generation)throw new BackendFault(501,"native_request_key_unavailable");
        return KeyBytes(db,tx,session.SessionId,session.Generation);
    }
}
