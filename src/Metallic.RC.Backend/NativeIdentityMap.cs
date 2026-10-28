using Microsoft.Data.Sqlite;

namespace Metallic.RC.Backend;

// Owned-database identity allocation only. These IDs do not identify production
// accounts and do not authenticate a user or establish a native account profile.
internal static class NativeIdentityMap
{
    private const string TableSql = """
        CREATE TABLE native_account_ids(
            native_id INTEGER PRIMARY KEY AUTOINCREMENT CHECK(native_id BETWEEN 1 AND 2147483647),
            account_id TEXT NOT NULL REFERENCES accounts(id)
                CHECK(length(account_id)=32 AND account_id NOT GLOB '*[^0-9a-f]*'
                    AND account_id <> '00000000000000000000000000000000')
        ) STRICT
        """;
    private const string IndexSql = "CREATE UNIQUE INDEX native_account_ids_account ON native_account_ids(account_id)";

    internal static bool IsAccountId(string? value) => value is not null &&
        Guid.TryParseExact(value, "N", out var id) && id != Guid.Empty &&
        string.Equals(value, id.ToString("N"), StringComparison.Ordinal);

    internal static void Migrate(SqliteConnection db)
    {
        using var transaction = db.BeginTransaction();
        try
        {
            Execute(db, transaction, CanonicalSql(TableSql));
            Execute(db, transaction, IndexSql);
            Validate(db, transaction);
            Execute(db, transaction, "PRAGMA user_version=2;");
            transaction.Commit();
        }
        catch (SqliteException error)
        {
            throw new IOException("Native identity migration failed; original version preserved.", error);
        }
    }

    internal static void Validate(SqliteConnection db, SqliteTransaction? transaction)
    {
        try
        {
            RequireDefinition(db, transaction, "table", "native_account_ids", TableSql);
            RequireDefinition(db, transaction, "index", "native_account_ids_account", IndexSql);
            // This authored table has no triggers. Additional SQL could silently
            // redirect an otherwise constraint-valid allocation to another owner.
            if (Scalar(db, transaction, "SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name='native_account_ids' COLLATE NOCASE;") is not null)
                throw Invalid();
            using (var command = Command(db, transaction, "PRAGMA foreign_key_check(native_account_ids);"))
            using (var reader = command.ExecuteReader())
                if (reader.Read()) throw Invalid();
            using (var command = Command(db, transaction, "SELECT native_id,account_id FROM native_account_ids;"))
            using (var reader = command.ExecuteReader())
                while (reader.Read())
                    if (reader.GetValue(0) is not long id || id is < 1 or > int.MaxValue ||
                        reader.GetValue(1) is not string accountId || !IsAccountId(accountId)) throw Invalid();
            _ = Sequence(db, transaction);
        }
        catch (SqliteException error) { throw new IOException("Native identity schema or data is invalid.", error); }
    }

    internal static int GetOrCreate(SqliteConnection db, SqliteTransaction transaction, string accountId)
    {
        Validate(db, transaction);
        if (Scalar(db, transaction, "SELECT 1 FROM accounts WHERE id=$value;", accountId) is null)
            throw new BackendFault(404, "account_not_found");
        object? existing = Scalar(db, transaction, "SELECT native_id FROM native_account_ids WHERE account_id=$value;", accountId);
        if (existing is long found) return checked((int)found);
        if (Sequence(db, transaction) == int.MaxValue) throw new BackendFault(429, "native_identity_exhausted");
        Execute(db, transaction, "INSERT INTO native_account_ids(account_id) VALUES($value);", accountId);
        object? allocated = Scalar(db, transaction, "SELECT last_insert_rowid();");
        if (allocated is not long id || id is < 1 or > int.MaxValue) throw Invalid();
        if (!string.Equals(Scalar(db, transaction, "SELECT account_id FROM native_account_ids WHERE native_id=$value;", id) as string,
            accountId, StringComparison.Ordinal)) throw Invalid();
        return (int)id;
    }

    internal static string Owner(SqliteConnection db, SqliteTransaction transaction, int nativeId)
    {
        Validate(db, transaction);
        return Scalar(db, transaction, "SELECT account_id FROM native_account_ids WHERE native_id=$value;", nativeId) as string
            ?? throw new BackendFault(404, "native_identity_not_found");
    }

    private static long Sequence(SqliteConnection db, SqliteTransaction? transaction)
    {
        object? maximum = Scalar(db, transaction, "SELECT COALESCE(MAX(native_id),0) FROM native_account_ids;");
        if (maximum is not long max || max is < 0 or > int.MaxValue) throw Invalid();
        using var command = Command(db, transaction, "SELECT seq FROM sqlite_sequence WHERE name='native_account_ids';");
        using var reader = command.ExecuteReader();
        if (!reader.Read())
        {
            if (max != 0) throw Invalid();
            return 0;
        }
        if (reader.GetValue(0) is not long sequence || sequence < max || sequence is < 0 or > int.MaxValue || reader.Read())
            throw Invalid();
        return sequence;
    }

    private static void RequireDefinition(SqliteConnection db, SqliteTransaction? transaction, string type, string name, string expected)
    {
        using var command = Command(db, transaction, "SELECT type,sql FROM sqlite_schema WHERE name=$value;", name);
        using var reader = command.ExecuteReader();
        if (!reader.Read() || reader.GetString(0) != type || reader.GetValue(1) is not string sql ||
            !string.Equals(CanonicalSql(sql), CanonicalSql(expected), StringComparison.Ordinal) || reader.Read()) throw Invalid();
    }

    // Only source-file line endings vary across supported builds. Preserve all
    // other SQL text so altered constraints, indexes or storage modes fail closed.
    private static string CanonicalSql(string sql) => sql.Replace("\r\n", "\n", StringComparison.Ordinal);
    private static IOException Invalid() => new("Native identity schema or data is invalid.");
    private static SqliteCommand Command(SqliteConnection db, SqliteTransaction? transaction, string sql, object? value = null)
    {
        var command = db.CreateCommand(); command.Transaction = transaction; command.CommandText = sql;
        if (value is not null) command.Parameters.AddWithValue("$value", value);
        return command;
    }
    private static object? Scalar(SqliteConnection db, SqliteTransaction? transaction, string sql, object? value = null)
    { using var command = Command(db, transaction, sql, value); return command.ExecuteScalar(); }
    private static void Execute(SqliteConnection db, SqliteTransaction? transaction, string sql, object? value = null)
    { using var command = Command(db, transaction, sql, value); command.ExecuteNonQuery(); }
}
