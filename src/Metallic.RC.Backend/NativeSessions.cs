using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Data.Sqlite;

namespace Metallic.RC.Backend;

// Host-supplied exact material, not a native token/JWT producer. No public secret
// properties: JSON metadata and incidental ToString cannot disclose the values.
public sealed class NativeSessionMaterial(string accessToken, string refreshToken,
    DateTimeOffset accessExpiresAt, DateTimeOffset refreshExpiresAt)
{
    internal string AccessToken { get; } = accessToken;
    internal string RefreshToken { get; } = refreshToken;
    internal DateTimeOffset AccessExpiresAt { get; } = accessExpiresAt;
    internal DateTimeOffset RefreshExpiresAt { get; } = refreshExpiresAt;
    public override string ToString() => "NativeSessionMaterial { values redacted }";
}

public sealed class NativeSessionView(string sessionId, string accountId, int nativeAccountId,
    DateTimeOffset accessExpiresAt, DateTimeOffset refreshExpiresAt, int generation)
{
    public string SessionId { get; } = sessionId;
    public string AccountId { get; } = accountId;
    public int NativeAccountId { get; } = nativeAccountId;
    public DateTimeOffset AccessExpiresAt { get; } = accessExpiresAt;
    public DateTimeOffset RefreshExpiresAt { get; } = refreshExpiresAt;
    public int Generation { get; } = generation;
    public override string ToString() => "NativeSessionView { identifiers redacted }";
}

public sealed partial class BackendStore
{
    private readonly bool nativeSessionsEnabled;
    private void NativeSessionsRequired()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (!nativeSessionsEnabled) throw new BackendFault(503, "native_sessions_disabled");
    }

    // Credentials and exact durable identity are checked here; a caller-created
    // Principal, decoded claim or supplied native ID cannot authorize issuance.
    public NativeSessionView CreateNativeSession(LoginRequest credentials, string accountId,
        int nativeAccountId, NativeSessionMaterial material)
    {
        NativeSessionsRequired();
        var account = VerifyCredentials(credentials);
        if (!string.Equals(account.Id, accountId, StringComparison.Ordinal)) throw NativeSessions.Rejected();
        lock (gate)
        {
            using var db = Open(); using var tx = db.BeginTransaction();
            NativeSessions.Validate(db, tx);
            NativeSessions.Binding(db, tx, accountId, nativeAccountId);
            var result = NativeSessions.Create(db, tx, accountId, nativeAccountId, material);
            tx.Commit(); return result;
        }
    }

    public NativeSessionView AuthenticateNativeSession(string accessToken, string accountId, int nativeAccountId)
    {
        NativeSessionsRequired();
        lock (gate)
        {
            using var db = Open(); using var tx = db.BeginTransaction();
            NativeSessions.Validate(db, tx);
            var result = NativeSessions.Authenticate(db, tx, accessToken, accountId, nativeAccountId);
            tx.Commit(); return result;
        }
    }

    public NativeSessionView RotateNativeSession(string refreshToken, string accountId,
        int nativeAccountId, NativeSessionMaterial material)
    {
        NativeSessionsRequired();
        lock (gate)
        {
            using var db = Open(); using var tx = db.BeginTransaction();
            NativeSessions.Validate(db, tx);
            // A replay revocation is deliberately committed before rejection.
            var result = NativeSessions.Rotate(db, tx, refreshToken, accountId, nativeAccountId, material);
            tx.Commit(); return result ?? throw NativeSessions.Rejected();
        }
    }

    public void RevokeNativeSession(string accessToken, string accountId, int nativeAccountId)
    {
        NativeSessionsRequired();
        lock (gate)
        {
            using var db = Open(); using var tx = db.BeginTransaction();
            NativeSessions.Validate(db, tx);
            var session = NativeSessions.Authenticate(db, tx, accessToken, accountId, nativeAccountId);
            NativeSessions.Revoke(db, tx, session.SessionId); tx.Commit();
        }
    }
}

internal static class NativeSessions
{
    internal const int MaxFamilies = 1024, MaxActivePerAccount = 8, MaxRotations = 32;
    internal const long AccessSeconds = 3600, FamilySeconds = 30 * 86400;
    private const long MaxUnixSeconds = 253402300799;
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private const string TableSql = """
        CREATE TABLE native_sessions(
            session_id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL REFERENCES accounts(id),
            native_id INTEGER NOT NULL REFERENCES native_account_ids(native_id),
            access_hash TEXT NOT NULL UNIQUE,
            refresh_hash TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL,
            access_expires_at INTEGER NOT NULL,
            refresh_expires_at INTEGER NOT NULL,
            generation INTEGER NOT NULL CHECK(generation BETWEEN 0 AND 32),
            revoked INTEGER NOT NULL CHECK(revoked IN(0,1))
        ) STRICT
        """;
    private const string TokensSql = """
        CREATE TABLE native_session_tokens(
            token_hash TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES native_sessions(session_id),
            kind INTEGER NOT NULL CHECK(kind IN(0,1)),
            generation INTEGER NOT NULL CHECK(generation BETWEEN 0 AND 32),
            UNIQUE(session_id,kind,generation)
        ) STRICT
        """;
    private const string AccountIndex = "CREATE INDEX native_sessions_account ON native_sessions(account_id)";
    private const string TokenIndex = "CREATE INDEX native_session_tokens_session ON native_session_tokens(session_id)";
    internal static BackendFault Rejected() => new(401, "native_session_rejected");
    private static IOException Invalid() => new("Native session schema or data is invalid.");
    private static bool Hex(string? value, int length) => value is not null && value.Length == length && value.All(c => c is >= '0' and <= '9' or >= 'a' and <= 'f');
    private static long Now() => DateTimeOffset.UtcNow.ToUnixTimeSeconds();
    private static SqliteCommand Command(SqliteConnection db, SqliteTransaction? tx, string sql, params object[] values)
    {
        var c = db.CreateCommand(); c.Transaction = tx; c.CommandText = sql;
        for (int i = 0; i < values.Length; i++) c.Parameters.AddWithValue("$p" + i, values[i]);
        return c;
    }
    private static object? Scalar(SqliteConnection db, SqliteTransaction? tx, string sql, params object[] values)
    { using var c = Command(db, tx, sql, values); return c.ExecuteScalar(); }
    private static void Execute(SqliteConnection db, SqliteTransaction? tx, string sql, params object[] values)
    { using var c = Command(db, tx, sql, values); c.ExecuteNonQuery(); }
    private static string Canonical(string sql) => sql.Replace("\r\n", "\n", StringComparison.Ordinal);

    internal static void Migrate(SqliteConnection db)
    {
        using var tx = db.BeginTransaction();
        try
        {
            foreach (string sql in new[] { TableSql, TokensSql, AccountIndex, TokenIndex }) Execute(db, tx, Canonical(sql));
            Validate(db, tx); Execute(db, tx, "PRAGMA user_version=3;"); tx.Commit();
        }
        catch (SqliteException error) { throw new IOException("Native session migration failed; original version preserved.", error); }
    }

    internal static void Validate(SqliteConnection db, SqliteTransaction? tx)
    {
        NativeIdentityMap.Validate(db, tx);
        try
        {
            foreach (var row in new[] { ("table", "native_sessions", TableSql), ("table", "native_session_tokens", TokensSql),
                                       ("index", "native_sessions_account", AccountIndex), ("index", "native_session_tokens_session", TokenIndex) })
                if (Scalar(db, tx, "SELECT sql FROM sqlite_schema WHERE type=$p0 AND name=$p1;", row.Item1, row.Item2) is not string sql || Canonical(sql) != Canonical(row.Item3)) throw Invalid();
            if (Scalar(db, tx, "SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name IN('native_sessions','native_session_tokens') COLLATE NOCASE;") is not null) throw Invalid();
            using (var c = Command(db, tx, "PRAGMA foreign_key_check;")) using (var r = c.ExecuteReader()) if (r.Read()) throw Invalid();
            if (Convert.ToInt64(Scalar(db, tx, "SELECT count(*) FROM native_sessions;"), CultureInfo.InvariantCulture) > MaxFamilies ||
                Convert.ToInt64(Scalar(db, tx, "SELECT count(*) FROM native_session_tokens;"), CultureInfo.InvariantCulture) > MaxFamilies * (MaxRotations + 1) * 2) throw Invalid();
            var sessions = new List<Stored>();
            using (var c = Command(db, tx, "SELECT session_id,account_id,native_id,access_hash,refresh_hash,created_at,access_expires_at,refresh_expires_at,generation,revoked FROM native_sessions;"))
            using (var r = c.ExecuteReader()) while (r.Read()) sessions.Add(Read(r));
            foreach (var s in sessions)
            {
                try { Binding(db, tx, s.Account, s.NativeId); }
                catch (BackendFault) { throw Invalid(); }
                if (!Hex(s.Id, 32) || !Hex(s.Access, 64) || !Hex(s.Refresh, 64) || s.Access == s.Refresh ||
                    s.Created < 0 || s.Created > MaxUnixSeconds - FamilySeconds || s.AccessExpires <= s.Created ||
                    s.RefreshExpires < s.AccessExpires || s.RefreshExpires > s.Created + FamilySeconds || s.Generation is < 0 or > MaxRotations || s.Revoked is < 0 or > 1) throw Invalid();
                using var c = Command(db, tx, "SELECT token_hash,kind,generation FROM native_session_tokens WHERE session_id=$p0;", s.Id);
                using var r = c.ExecuteReader(); int count = 0;
                while (r.Read())
                {
                    if (r.GetValue(0) is not string hash || !Hex(hash, 64) || r.GetValue(1) is not long kind || kind is < 0 or > 1 ||
                        r.GetValue(2) is not long gen || gen < 0 || gen > s.Generation ||
                        (gen == s.Generation && hash != (kind == 0 ? s.Access : s.Refresh))) throw Invalid();
                    count++;
                }
                if (count != 2 * (s.Generation + 1)) throw Invalid();
            }
        }
        catch (SqliteException error) { throw new IOException("Native session schema or data is invalid.", error); }
    }

    internal static void Binding(SqliteConnection db, SqliteTransaction? tx, string account, int nativeId)
    {
        if (!NativeIdentityMap.IsAccountId(account) || nativeId <= 0 ||
            Scalar(db, tx, "SELECT account_id FROM native_account_ids WHERE native_id=$p0;", nativeId) is not string actual || actual != account) throw Rejected();
    }

    private sealed record Stored(string Id, string Account, int NativeId, string Access, string Refresh,
        long Created, long AccessExpires, long RefreshExpires, int Generation, int Revoked)
    { public override string ToString() => "StoredNativeSession { values redacted }"; }
    private static Stored Read(SqliteDataReader r)
    {
        if (Enumerable.Range(0, 10).Any(i => r.IsDBNull(i)) ||
            r.GetValue(2) is not long native || native is < 1 or > int.MaxValue ||
            r.GetValue(8) is not long generation || generation is < 0 or > MaxRotations ||
            r.GetValue(9) is not long revoked || revoked is < 0 or > 1) throw Invalid();
        return new(r.GetString(0),r.GetString(1),(int)native,r.GetString(3),r.GetString(4),r.GetInt64(5),r.GetInt64(6),r.GetInt64(7),(int)generation,(int)revoked);
    }
    private static Stored? Find(SqliteConnection db, SqliteTransaction tx, string hash)
    {
        using var c = Command(db, tx, "SELECT s.session_id,s.account_id,s.native_id,s.access_hash,s.refresh_hash,s.created_at,s.access_expires_at,s.refresh_expires_at,s.generation,s.revoked FROM native_session_tokens t JOIN native_sessions s ON s.session_id=t.session_id WHERE t.token_hash=$p0;", hash);
        using var r = c.ExecuteReader(); return r.Read() ? Read(r) : null;
    }
    private static bool Equal(string a, string b) => CryptographicOperations.FixedTimeEquals(Convert.FromHexString(a), Convert.FromHexString(b));
    private static string Hash(string? token)
    {
        if (token is null || token.Length is < 32 or > 16384 || token.Any(c => char.IsWhiteSpace(c) || char.IsControl(c))) throw Rejected();
        try
        {
            if (Utf8.GetByteCount(token) > 16384) throw Rejected();
            return Convert.ToHexStringLower(SHA256.HashData(Utf8.GetBytes(token)));
        }
        catch (EncoderFallbackException) { throw Rejected(); }
    }
    private static (string Access, string Refresh, long AccessExpires, long RefreshExpires) Material(SqliteConnection db,
        SqliteTransaction tx, NativeSessionMaterial value, long now, long created)
    {
        if (value is null) throw Rejected();
        string access = Hash(value.AccessToken), refresh = Hash(value.RefreshToken);
        long ae = value.AccessExpiresAt.ToUnixTimeSeconds(), re = value.RefreshExpiresAt.ToUnixTimeSeconds();
        if (access == refresh || now < 0 || now > MaxUnixSeconds - FamilySeconds || ae <= now || ae > now + AccessSeconds ||
            re < ae || re > created + FamilySeconds || created > now ||
            Scalar(db, tx, "SELECT 1 FROM native_session_tokens WHERE token_hash IN($p0,$p1);", access, refresh) is not null) throw Rejected();
        return (access, refresh, ae, re);
    }
    private static NativeSessionView View(Stored s) => new(s.Id,s.Account,s.NativeId,
        DateTimeOffset.FromUnixTimeSeconds(s.AccessExpires),DateTimeOffset.FromUnixTimeSeconds(s.RefreshExpires),s.Generation);
    private static void AddHashes(SqliteConnection db, SqliteTransaction tx, string id, int generation, string access, string refresh)
    {
        Execute(db, tx, "INSERT INTO native_session_tokens VALUES($p0,$p1,0,$p2);", access,id,generation);
        Execute(db, tx, "INSERT INTO native_session_tokens VALUES($p0,$p1,1,$p2);", refresh,id,generation);
    }
    internal static NativeSessionView Create(SqliteConnection db, SqliteTransaction tx, string account, int nativeId, NativeSessionMaterial material)
    {
        long now = Now(); var m = Material(db, tx, material, now, now);
        if (Convert.ToInt64(Scalar(db,tx,"SELECT count(*) FROM native_sessions;"),CultureInfo.InvariantCulture) >= MaxFamilies ||
            Convert.ToInt64(Scalar(db,tx,"SELECT count(*) FROM native_sessions WHERE account_id=$p0 AND revoked=0 AND refresh_expires_at>$p1;",account,now),CultureInfo.InvariantCulture) >= MaxActivePerAccount)
            throw new BackendFault(429,"native_session_limit");
        string id = Guid.NewGuid().ToString("N");
        Execute(db,tx,"INSERT INTO native_sessions VALUES($p0,$p1,$p2,$p3,$p4,$p5,$p6,$p7,0,0);",id,account,nativeId,m.Access,m.Refresh,now,m.AccessExpires,m.RefreshExpires);
        AddHashes(db,tx,id,0,m.Access,m.Refresh); Validate(db,tx);
        return new(id,account,nativeId,DateTimeOffset.FromUnixTimeSeconds(m.AccessExpires),DateTimeOffset.FromUnixTimeSeconds(m.RefreshExpires),0);
    }
    internal static NativeSessionView Authenticate(SqliteConnection db, SqliteTransaction tx, string accessToken, string account, int nativeId)
    {
        string hash = Hash(accessToken); var s = Find(db,tx,hash); long now = Now();
        if (s is null || !Equal(hash,s.Access) || s.Revoked != 0 || s.Created > now || s.AccessExpires <= now || s.Account != account || s.NativeId != nativeId) throw Rejected();
        Binding(db,tx,account,nativeId); return View(s);
    }
    // Gateway lookup derives identity from exact stored token state, never claims.
    internal static NativeSessionView AuthenticateIssuedToken(SqliteConnection db, SqliteTransaction tx, string accessToken)
    {
        var stored = Find(db,tx,Hash(accessToken));
        if (stored is null) throw Rejected();
        return Authenticate(db,tx,accessToken,stored.Account,stored.NativeId);
    }
    internal static NativeSessionView? Rotate(SqliteConnection db, SqliteTransaction tx, string refreshToken, string account, int nativeId, NativeSessionMaterial material)
    {
        string hash = Hash(refreshToken); var s = Find(db,tx,hash); long now = Now();
        if (s is null || s.Account != account || s.NativeId != nativeId || s.Revoked != 0 || s.RefreshExpires <= now) throw Rejected();
        Binding(db,tx,account,nativeId);
        if (!Equal(hash,s.Refresh))
        {
            if (Scalar(db,tx,"SELECT 1 FROM native_session_tokens WHERE token_hash=$p0 AND session_id=$p1 AND kind=1;",hash,s.Id) is not null) { Revoke(db,tx,s.Id); return null; }
            throw Rejected();
        }
        if (s.Generation >= MaxRotations) throw new BackendFault(429,"native_rotation_limit");
        var m = Material(db,tx,material,now,s.Created);
        Execute(db,tx,"UPDATE native_sessions SET access_hash=$p0,refresh_hash=$p1,access_expires_at=$p2,refresh_expires_at=$p3,generation=generation+1 WHERE session_id=$p4;",m.Access,m.Refresh,m.AccessExpires,m.RefreshExpires,s.Id);
        AddHashes(db,tx,s.Id,s.Generation+1,m.Access,m.Refresh); Validate(db,tx);
        return new(s.Id,s.Account,s.NativeId,DateTimeOffset.FromUnixTimeSeconds(m.AccessExpires),DateTimeOffset.FromUnixTimeSeconds(m.RefreshExpires),s.Generation+1);
    }
    internal static void Revoke(SqliteConnection db, SqliteTransaction tx, string id) => Execute(db,tx,"UPDATE native_sessions SET revoked=1 WHERE session_id=$p0;",id);
}
