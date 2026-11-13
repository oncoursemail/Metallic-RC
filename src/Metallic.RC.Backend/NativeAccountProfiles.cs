using System.Globalization;
using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;
using Microsoft.Data.Sqlite;

namespace Metallic.RC.Backend;

// Payload access is explicit and returns a copy. Default JSON/text output
// contains revision metadata only, not contact data or other profile fields.
public sealed class NativeAccountProfileView
{
    private readonly byte[] response;
    public long Revision { get; }
    public long OwnedRevision { get; }
    internal NativeAccountProfileView(long revision, long ownedRevision, byte[] bytes)
    { Revision = revision; OwnedRevision = ownedRevision; response = (byte[])bytes.Clone(); }
    public byte[] GetResponseBytes() => (byte[])response.Clone();
    public override string ToString() => "NativeAccountProfileView { profile values redacted }";
}

public sealed partial class BackendStore
{
    private readonly bool nativeProfilesEnabled;
    private void NativeProfilesRequired()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (!nativeProfilesEnabled) throw new BackendFault(503, "native_profiles_disabled");
    }
    private void NativeProfileAdmin(string key)
    { if (key is null || !IsAdmin(key)) throw new BackendFault(401, "local_admin_key_required"); }

    public NativeAccountProfileView WriteNativeAccountProfile(string adminKey, string accountId,
        int nativeAccountId, long expectedRevision, NativeSelfAccountResponseModel model)
    {
        NativeProfilesRequired(); NativeProfileAdmin(adminKey);
        lock (gate)
        {
            using var db = Open(); using var tx = db.BeginTransaction();
            NativeAccountProfiles.Validate(db, tx, allPayloads: false);
            var view = NativeAccountProfiles.Write(db, tx, accountId, nativeAccountId, expectedRevision, model);
            tx.Commit(); return view;
        }
    }
    public NativeAccountProfileView ReadNativeAccountProfile(string adminKey, string accountId, int nativeAccountId)
    {
        NativeProfilesRequired(); NativeProfileAdmin(adminKey);
        lock (gate)
        {
            using var db = Open(); using var tx = db.BeginTransaction();
            NativeAccountProfiles.Validate(db, tx, allPayloads: false);
            var view = NativeAccountProfiles.Read(db, tx, accountId, nativeAccountId);
            tx.Commit(); return view;
        }
    }
    public NativeAccountProfileView NativeAccountProfileForSession(string accessToken, string accountId, int nativeAccountId)
    {
        NativeProfilesRequired(); NativeSessionsRequired();
        lock (gate)
        {
            using var db = Open(); using var tx = db.BeginTransaction();
            NativeAccountProfiles.Validate(db, tx, allPayloads: false);
            _ = NativeSessions.Authenticate(db, tx, accessToken, accountId, nativeAccountId);
            var view = NativeAccountProfiles.Read(db, tx, accountId, nativeAccountId);
            tx.Commit(); return view;
        }
    }
}

internal static class NativeAccountProfiles
{
    private const int MaxRows = 1024;
    private static readonly UTF8Encoding Utf8 = new(false, true);
    private const string TableSql = """
        CREATE TABLE native_account_profiles(
            account_id TEXT PRIMARY KEY REFERENCES accounts(id),
            native_id INTEGER NOT NULL REFERENCES native_account_ids(native_id),
            revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 9223372036854775807),
            owned_revision INTEGER NOT NULL CHECK(owned_revision BETWEEN 1 AND 9223372036854775807),
            payload BLOB NOT NULL CHECK(length(payload) BETWEEN 1 AND 65536)
        ) STRICT
        """;
    private const string IndexSql = "CREATE UNIQUE INDEX native_account_profiles_native ON native_account_profiles(native_id)";
    private static IOException Invalid() => new("Native profile schema or data is invalid.");
    private static string Canonical(string sql) => sql.Replace("\r\n", "\n", StringComparison.Ordinal);
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
    internal static void Migrate(SqliteConnection db)
    {
        using var tx = db.BeginTransaction();
        try
        {
            Execute(db, tx, Canonical(TableSql)); Execute(db, tx, IndexSql);
            Validate(db, tx, allPayloads: true); Execute(db, tx, "PRAGMA user_version=4;"); tx.Commit();
        }
        catch (SqliteException) { throw new IOException("Native profile migration failed; original version preserved."); }
    }
    private sealed record Metadata(string Account, int NativeId, long Revision, long OwnedRevision)
    { public override string ToString() => "NativeProfileMetadata { values redacted }"; }
    internal static void Validate(SqliteConnection db, SqliteTransaction? tx, bool allPayloads)
    {
        NativeSessions.Validate(db, tx);
        try
        {
            foreach (var row in new[] { ("table", "native_account_profiles", TableSql), ("index", "native_account_profiles_native", IndexSql) })
                if (Scalar(db, tx, "SELECT sql FROM sqlite_schema WHERE type=$p0 AND name=$p1;", row.Item1, row.Item2) is not string sql || Canonical(sql) != Canonical(row.Item3)) throw Invalid();
            if (Scalar(db, tx, "SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name='native_account_profiles' COLLATE NOCASE;") is not null) throw Invalid();
            if (Convert.ToInt64(Scalar(db, tx, "SELECT count(*) FROM native_account_profiles;"), CultureInfo.InvariantCulture) > MaxRows) throw Invalid();
            var rows = new List<Metadata>();
            // Check sizes and metadata without materializing any profile BLOB.
            using (var c = Command(db, tx, "SELECT account_id,native_id,revision,owned_revision,length(payload),typeof(payload) FROM native_account_profiles;"))
            using (var r = c.ExecuteReader()) while (r.Read())
            {
                if (r.GetValue(0) is not string account || !NativeIdentityMap.IsAccountId(account) ||
                    r.GetValue(1) is not long native || native is < 1 or > int.MaxValue ||
                    r.GetValue(2) is not long revision || revision < 1 || r.GetValue(3) is not long owned || owned < 1 ||
                    r.GetValue(4) is not long size || size is < 1 or > NativeSelfAccountResponseProfile.MaxJsonBytes || r.GetString(5) != "blob") throw Invalid();
                rows.Add(new(account,(int)native,revision,owned));
            }
            foreach (var row in rows)
            {
                try { NativeSessions.Binding(db, tx, row.Account, row.NativeId); }
                catch (BackendFault) { throw Invalid(); }
                if (allPayloads) _ = Payload(db, tx, row.Account, row.NativeId);
            }
        }
        catch (SqliteException) { throw new IOException("Native profile schema or data is invalid."); }
    }

    private static (byte[] Bytes, NativeSelfAccountResponseModel Model) Payload(SqliteConnection db, SqliteTransaction? tx, string account, int nativeId)
    {
        // SQL length/type admission precedes GetFieldValue allocation. A held
        // transaction keeps metadata and this bounded selected row consistent.
        using var c = Command(db, tx, "SELECT payload FROM native_account_profiles WHERE account_id=$p0 AND native_id=$p1 AND typeof(payload)='blob' AND length(payload) BETWEEN 1 AND 65536;", account,nativeId);
        using var r = c.ExecuteReader(); if (!r.Read()) throw Invalid();
        byte[] bytes = r.GetFieldValue<byte[]>(0);
        try
        {
            _ = Utf8.GetCharCount(bytes);
            using var json = JsonDocument.Parse(bytes,new JsonDocumentOptions { MaxDepth = 4 });
            var value = json.RootElement;
            if (value.ValueKind != JsonValueKind.Object || value.EnumerateObject().Count() != 20) throw Invalid();
            string? S(string key) { var v=value.GetProperty(key); return v.ValueKind==JsonValueKind.Null ? null : v.GetString(); }
            int I(string key) => value.GetProperty(key).GetInt32();
            bool B(string key) => value.GetProperty(key).GetBoolean();
            int? NI(string key) { var v=value.GetProperty(key); return v.ValueKind==JsonValueKind.Null ? null : v.GetInt32(); }
            bool? NB(string key) { var v=value.GetProperty(key); return v.ValueKind==JsonValueKind.Null ? null : v.GetBoolean(); }
            var birthday=value.GetProperty("Birthday");
            var model = new NativeSelfAccountResponseModel(S("Email"),S("Phone"),birthday.ValueKind==JsonValueKind.Null?null:birthday.GetInt64(),I("JuniorState"),
                NI("ParentAccountId"),NB("IsFakeJuniorBirthday"),I("AvailableUsernameChanges"),I("AccountId"),S("UserName"),S("DisplayName"),S("DisplayEmoji"),S("ProfileImage"),S("BannerImage"),
                B("TreatAsJunior"),B("HasBirthday"),I("PersonalPronouns"),I("IdentityFlags"),value.GetProperty("createdAt").GetInt64(),B("IsMetaPlatformBlocked"),NB("IsJunior"));
            if (model.AccountId != nativeId || model.UserName is null || model.DisplayName is null ||
                !NativeSelfAccountResponseProfile.Write(model).AsSpan().SequenceEqual(bytes)) throw Invalid();
            return (bytes,model);
        }
        catch (Exception e) when (e is JsonException or KeyNotFoundException or InvalidOperationException or FormatException or OverflowException or DecoderFallbackException or NativeSelfAccountResponseException)
        { throw new IOException("Native profile payload is invalid."); }
    }

    private static (string UserName, string DisplayName, long Revision) Account(SqliteConnection db, SqliteTransaction tx, string account, int nativeId)
    {
        NativeSessions.Binding(db, tx, account,nativeId);
        using var c = Command(db,tx,"SELECT username,display_name,revision FROM accounts WHERE id=$p0;",account);
        using var r = c.ExecuteReader(); if(!r.Read() || r.GetValue(2) is not long rev || rev < 1) throw Invalid();
        return (r.GetString(0),r.GetString(1),rev);
    }
    internal static NativeAccountProfileView Write(SqliteConnection db, SqliteTransaction tx, string account, int nativeId, long expected, NativeSelfAccountResponseModel model)
    {
        var owner=Account(db,tx,account,nativeId);
        if (model is null || model.AccountId != nativeId || model.UserName != owner.UserName || model.DisplayName != owner.DisplayName)
            throw new BackendFault(400,"native_profile_identity_mismatch");
        if (expected < 0) throw new BackendFault(400,"native_profile_revision_required");
        byte[] bytes;
        try { bytes=NativeSelfAccountResponseProfile.Write(model); }
        catch (NativeSelfAccountResponseException) { throw new BackendFault(400,"native_profile_value_invalid"); }
        object? existing=Scalar(db,tx,"SELECT revision FROM native_account_profiles WHERE account_id=$p0;",account);
        long current=existing is long found ? found : 0;
        if (expected != current) throw new BackendFault(412,"native_profile_revision_conflict");
        if (current==long.MaxValue) throw new BackendFault(409,"native_profile_revision_exhausted");
        long next=current+1;
        if (current==0)
        {
            if(Convert.ToInt64(Scalar(db,tx,"SELECT count(*) FROM native_account_profiles;"),CultureInfo.InvariantCulture)>=MaxRows) throw new BackendFault(429,"native_profile_limit");
            Execute(db,tx,"INSERT INTO native_account_profiles VALUES($p0,$p1,$p2,$p3,$p4);",account,nativeId,next,owner.Revision,bytes);
        }
        else Execute(db,tx,"UPDATE native_account_profiles SET revision=$p0,owned_revision=$p1,payload=$p2 WHERE account_id=$p3 AND native_id=$p4;",next,owner.Revision,bytes,account,nativeId);
        var saved=Read(db,tx,account,nativeId);
        if (saved.Revision != next || !saved.GetResponseBytes().AsSpan().SequenceEqual(bytes)) throw Invalid();
        return saved;
    }
    internal static NativeAccountProfileView Read(SqliteConnection db, SqliteTransaction tx, string account, int nativeId)
    {
        var owner=Account(db,tx,account,nativeId);
        using var c=Command(db,tx,"SELECT revision,owned_revision FROM native_account_profiles WHERE account_id=$p0 AND native_id=$p1;",account,nativeId);
        long revision,owned;
        using(var r=c.ExecuteReader())
        {if(!r.Read())throw new BackendFault(404,"native_profile_missing");revision=r.GetInt64(0);owned=r.GetInt64(1);}
        var data=Payload(db,tx,account,nativeId);
        if(owned!=owner.Revision || data.Model.UserName!=owner.UserName || data.Model.DisplayName!=owner.DisplayName)throw new BackendFault(409,"native_profile_stale");
        return new(revision,owned,data.Bytes);
    }
}
