using System.Security.Cryptography;
using System.Text.Json;
using Metallic.RC.Backend;
using Microsoft.Data.Sqlite;

if (args.Length != 2 || args[0] != "--self-test") throw new ArgumentException("Use --self-test NEW-result.json.");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");
var checks = new List<string>();
void Check(bool value, string name)
{
    if (!value) throw new InvalidOperationException(name);
    checks.Add(name);
}
void Fault(Action action, int status, string code)
{
    bool failed = false;
    try { action(); } catch (BackendFault error) { failed = error.Status == status && error.Code == code; }
    Check(failed, "explicit mapping failure: " + code);
}
string rootName = "metallic-native-identity-" + Guid.NewGuid().ToString("N");
string root = Path.Combine(Path.GetTempPath(), rootName);
Directory.CreateDirectory(root);
string NewDirectory() { string path = Path.Combine(root, Guid.NewGuid().ToString("N")); Directory.CreateDirectory(path); return path; }
string Database(string directory) => Path.Combine(directory, "backend.sqlite");
void RequireFixture(string directory)
{
    if (!Path.GetFullPath(directory).StartsWith(Path.GetFullPath(root) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
        throw new InvalidOperationException("Only this run's generated databases are allowed.");
}
SqliteConnection Open(string directory)
{
    RequireFixture(directory);
    var db = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = Database(directory), Pooling = false }.ToString());
    db.Open(); return db;
}
object? Sql(string directory, string sql)
{
    using var db = Open(directory); using var command = db.CreateCommand(); command.CommandText = sql; return command.ExecuteScalar();
}
long Number(string directory, string sql) => (long)(Sql(directory, sql) ?? throw new InvalidOperationException("Expected integer fixture result."));
BackendStore Store(string directory, bool enabled = false) => new(new BackendOptions(0, directory) { EnableNativeIdentityMapping = enabled });
string Clone(string directory)
{
    // Only generated, quiescent fixtures may be copied. Hold the same owner lock
    // as BackendStore and explicitly checkpoint WAL before copying the main DB.
    RequireFixture(directory);
    using var owner = new FileStream(Path.Combine(directory, "backend.lock"), FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
    using (var db = Open(directory))
    using (var command = db.CreateCommand())
    {
        command.CommandText = "PRAGMA wal_checkpoint(TRUNCATE);"; command.CommandTimeout = 1;
        using var result = command.ExecuteReader();
        if (!result.Read() || result.GetInt64(0) != 0) throw new IOException("Fixture WAL checkpoint was busy.");
    }
    string wal = Database(directory) + "-wal";
    if (File.Exists(wal) && new FileInfo(wal).Length != 0) throw new IOException("Fixture WAL was not drained.");
    string copy = NewDirectory();
    File.Copy(Database(directory), Database(copy));
    File.Copy(Path.Combine(directory, ".metallic-backend"), Path.Combine(copy, ".metallic-backend"));
    return copy;
}
string Password() => "synthetic-" + Guid.NewGuid().ToString("N");
AccountView Register(BackendStore store, string password) => store.Register(new("user_" + Guid.NewGuid().ToString("N")[..12], password, "Synthetic profile"));
void Constraint(string directory, string sql, string label)
{
    bool failed = false;
    try { _ = Sql(directory, "PRAGMA foreign_keys=ON;" + sql); }
    catch (SqliteException error) { failed = error.SqliteErrorCode == 19; }
    Check(failed, label);
}
void InvalidDatabase(string source, string mutation, string label)
{
    string copy = Clone(source); _ = Sql(copy, mutation);
    byte[] before = SHA256.HashData(File.ReadAllBytes(Database(copy)));
    foreach (bool enabled in new[] { false, true })
    {
        bool failed = false;
        try { using var unused = Store(copy, enabled); } catch (IOException) { failed = true; }
        Check(failed, "fail closed with mapping " + (enabled ? "on: " : "off: ") + label);
        Check(SHA256.HashData(File.ReadAllBytes(Database(copy))).AsSpan().SequenceEqual(before), "failed validation preserves database bytes: " + label);
    }
}

try
{
    string directory = NewDirectory(), password = Password();
    AccountView first, second, third;
    SessionView ownedSession;
    using (var store = Store(directory))
    {
        Check(!new BackendOptions(0, directory).EnableNativeIdentityMapping, "embedding option is false by default");
        Check(!BackendOptions.Parse(["--data-dir", directory]).EnableNativeIdentityMapping, "CLI cannot enable the mapping");
        bool cliRejected = false;
        try { _ = BackendOptions.Parse(["--enable-native-identity-mapping", "true"]); } catch (ArgumentException) { cliRejected = true; }
        Check(cliRejected, "no native identity CLI activation was added");
        first = Register(store, password); second = Register(store, password); third = Register(store, password);
        ownedSession = store.Login(new(first.Username, password));
        Fault(() => store.GetOrCreateNativeAccountId(first.Id), 409, "native_identity_disabled");
        Fault(() => store.AccountIdForNative(1), 409, "native_identity_disabled");
    }
    Check(Number(directory, "PRAGMA user_version;") == 1, "default-off creation stays schema v1");
    Check(Number(directory, "SELECT COUNT(*) FROM sqlite_schema WHERE name LIKE 'native_account_ids%';") == 0, "default-off creates no identity objects");
    string v1 = Clone(directory);
    string walSource = Clone(v1);
    using (var writer = Open(walSource))
    {
        using var command = writer.CreateCommand();
        command.CommandText = "PRAGMA journal_mode=WAL; UPDATE accounts SET display_name='Checkpointed synthetic';";
        command.ExecuteNonQuery();
        Check(File.Exists(Database(walSource) + "-wal") && new FileInfo(Database(walSource) + "-wal").Length > 0,
            "fixture contains committed WAL state before cloning");
        string walCopy = Clone(walSource);
        using var copied = Store(walCopy);
        Check(copied.Profile(first.Id).DisplayName == "Checkpointed synthetic", "explicit checkpoint preserves committed WAL data in the quiescent fixture copy");
    }
    using (var off = Store(directory)) Check(off.Profile(first.Id) == first, "default-off reopening preserves existing profile");
    Check(Number(directory, "PRAGMA user_version;") == 1, "default-off reopening does not migrate v1");
    int firstId, secondId, thirdId;
    using (var store = Store(directory, true))
    {
        bool activeCopyRejected = false;
        try { _ = Clone(directory); } catch (IOException) { activeCopyRejected = true; }
        Check(activeCopyRejected, "clone refuses a fixture with an active BackendStore owner lock");
        Check(Number(directory, "PRAGMA user_version;") == 2, "explicit opt-in commits schema v2");
        Check(Number(directory, "SELECT COUNT(*) FROM native_account_ids;") == 0, "migration does not implicitly allocate identities");
        Check(store.Profile(first.Id) == first && store.Authenticate("Bearer " + ownedSession.AccessToken).AccountId == first.Id,
            "migration preserves owned accounts and sessions");
        firstId = store.GetOrCreateNativeAccountId(first.Id);
        Check(firstId > 0 && store.GetOrCreateNativeAccountId(first.Id) == firstId, "positive mapping allocation is idempotent");
        Check(store.AccountIdForNative(firstId) == first.Id, "reverse mapping binds the exact owned GUID");
        int[] same = await Task.WhenAll(Enumerable.Range(0, 40).Select(_ => Task.Run(() => store.GetOrCreateNativeAccountId(second.Id))));
        secondId = same[0];
        Check(same.All(id => id == secondId) && secondId != firstId, "concurrent same-account calls allocate exactly one distinct identity");
        int[] different = await Task.WhenAll(Enumerable.Range(0, 40).Select(index => Task.Run(() => store.GetOrCreateNativeAccountId(index % 2 == 0 ? first.Id : third.Id))));
        thirdId = different[1];
        Check(different.Where((_, index) => index % 2 == 0).All(id => id == firstId) &&
            different.Where((_, index) => index % 2 != 0).All(id => id == thirdId) && thirdId != firstId && thirdId != secondId,
            "concurrent mixed-account calls preserve both directions without duplicates");
        Check(Number(directory, "SELECT COUNT(*) FROM native_account_ids;") == 3, "concurrency allocated exactly three rows");
        foreach (string? invalid in new string?[] { null, "", "not-a-guid", Guid.Empty.ToString("N"), Guid.Parse(first.Id).ToString("D"), new string('A', 32) })
            Fault(() => store.GetOrCreateNativeAccountId(invalid!), 400, "invalid_account_id");
        Fault(() => store.GetOrCreateNativeAccountId(Guid.NewGuid().ToString("N")), 404, "account_not_found");
        Fault(() => store.AccountIdForNative(0), 400, "invalid_native_account_id");
        Fault(() => store.AccountIdForNative(-1), 400, "invalid_native_account_id");
        Fault(() => store.AccountIdForNative(int.MaxValue), 404, "native_identity_not_found");
        Check(Number(directory, "SELECT COUNT(*) FROM native_account_ids;") == 3, "invalid requests do not allocate or replace mappings");
    }
    using (var off = Store(directory))
    {
        Check(off.Profile(first.Id) == first && off.Authenticate("Bearer " + ownedSession.AccessToken).AccountId == first.Id, "v2 remains usable by default-off owned APIs");
        var session = off.Login(new(first.Username, password));
        Check(off.Authenticate("Bearer " + session.AccessToken).AccountId == first.Id, "default-off owned login remains functional on v2");
        Fault(() => off.GetOrCreateNativeAccountId(first.Id), 409, "native_identity_disabled");
        Fault(() => off.AccountIdForNative(firstId), 409, "native_identity_disabled");
    }
    using (var reopened = Store(directory, true))
        Check(reopened.GetOrCreateNativeAccountId(first.Id) == firstId && reopened.GetOrCreateNativeAccountId(second.Id) == secondId &&
            reopened.GetOrCreateNativeAccountId(third.Id) == thirdId && reopened.AccountIdForNative(secondId) == second.Id,
            "both mapping directions survive store disposal and reopening");
    string newline = Clone(directory);
    _ = Sql(newline, "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,char(10),char(13)||char(10)) WHERE name='native_account_ids'; PRAGMA schema_version=123;");
    Check(((string)Sql(newline, "SELECT sql FROM sqlite_schema WHERE name='native_account_ids';")!).Contains("\r\n", StringComparison.Ordinal), "fixture stores CRLF schema SQL explicitly");
    using (var store = Store(newline, true)) Check(store.GetOrCreateNativeAccountId(first.Id) == firstId, "CRLF stored schema retains exact mapping under canonical line-ending comparison");
    _ = Sql(newline, "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,char(13)||char(10),char(10)) WHERE name='native_account_ids'; PRAGMA schema_version=124;");
    using (var store = Store(newline, true)) Check(store.AccountIdForNative(secondId) == second.Id, "LF stored schema retains the same reverse mapping");
    Constraint(directory, "INSERT INTO native_account_ids(account_id) VALUES('" + Guid.NewGuid().ToString("N") + "');", "foreign key refuses a nonexistent account");
    Constraint(directory, "INSERT INTO native_account_ids(account_id) VALUES('" + first.Id + "');", "unique index refuses a second native ID for the same GUID");
    foreach (long invalid in new long[] { 0, -1, (long)int.MaxValue + 1 })
        Constraint(directory, "INSERT INTO native_account_ids(native_id,account_id) VALUES(" + invalid + ",'" + first.Id + "');", "database enforces positive Int32 native ID bounds");
    Constraint(directory, "DELETE FROM accounts WHERE id='" + first.Id + "';", "foreign key preserves a mapped account's identity binding");

    string reuse = Clone(directory);
    _ = Sql(reuse, "DELETE FROM native_account_ids WHERE native_id=" + thirdId + ";");
    using (var store = Store(reuse, true))
    {
        var account = Register(store, Password()); int next = store.GetOrCreateNativeAccountId(account.Id);
        Check(next > Math.Max(thirdId, Math.Max(firstId, secondId)), "AUTOINCREMENT does not reuse an externally deleted highest ID");
        Fault(() => store.AccountIdForNative(thirdId), 404, "native_identity_not_found");
    }
    string capacity = Clone(directory);
    _ = Sql(capacity, "UPDATE sqlite_sequence SET seq=2147483646 WHERE name='native_account_ids';");
    using (var store = Store(capacity, true))
    {
        var last = Register(store, Password()); var beyond = Register(store, Password());
        Check(store.GetOrCreateNativeAccountId(last.Id) == int.MaxValue, "last available positive Int32 can be allocated exactly");
        Fault(() => store.GetOrCreateNativeAccountId(beyond.Id), 429, "native_identity_exhausted");
        Check(store.GetOrCreateNativeAccountId(last.Id) == int.MaxValue && store.GetOrCreateNativeAccountId(first.Id) == firstId,
            "exhaustion preserves existing identities instead of wrapping or reallocating");
    }

    string collision = Clone(v1);
    _ = Sql(collision, "CREATE INDEX native_account_ids_account ON accounts(username);");
    string beforeCollision = (string)Sql(collision, "SELECT sql FROM sqlite_schema WHERE name='native_account_ids_account';")!;
    bool migrationFailed = false;
    try { using var unused = Store(collision, true); } catch (IOException) { migrationFailed = true; }
    Check(migrationFailed, "second migration DDL fails on an existing index-name collision");
    Check(Number(collision, "PRAGMA user_version;") == 1 && Number(collision, "SELECT COUNT(*) FROM sqlite_schema WHERE name='native_account_ids';") == 0,
        "failed second DDL rolls back the newly created mapping table and user_version");
    Check((string)Sql(collision, "SELECT sql FROM sqlite_schema WHERE name='native_account_ids_account';")! == beforeCollision,
        "migration failure preserves the preexisting conflicting index");
    using (var off = Store(collision)) Check(off.Profile(first.Id) == first && off.Authenticate("Bearer " + ownedSession.AccessToken).AccountId == first.Id,
        "failed migration preserves original owned account and session behavior");

    InvalidDatabase(directory, "PRAGMA user_version=3;", "unknown schema version");
    InvalidDatabase(directory, "DROP INDEX native_account_ids_account; CREATE INDEX native_account_ids_account ON native_account_ids(account_id);", "nonunique replacement index");
    InvalidDatabase(directory, "DROP TABLE native_account_ids;", "missing version-2 mapping table");
    InvalidDatabase(directory, "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,'AUTOINCREMENT','') WHERE name='native_account_ids'; PRAGMA schema_version=125;", "removed allocation constraint");
    InvalidDatabase(directory, "CREATE TRIGGER unexpected_identity AFTER INSERT ON native_account_ids BEGIN DELETE FROM native_account_ids WHERE native_id=new.native_id; END;", "unexpected mapping trigger");
    InvalidDatabase(directory, "PRAGMA foreign_keys=OFF; UPDATE native_account_ids SET account_id='" + Guid.NewGuid().ToString("N") + "' WHERE native_id=" + firstId + ";", "orphaned GUID binding");
    InvalidDatabase(directory, "PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON; UPDATE native_account_ids SET account_id='not-a-guid' WHERE native_id=" + firstId + ";", "invalid stored GUID");
    InvalidDatabase(directory, "UPDATE sqlite_sequence SET seq='invalid' WHERE name='native_account_ids';", "nonnumeric sequence");
    InvalidDatabase(directory, "UPDATE sqlite_sequence SET seq=-1 WHERE name='native_account_ids';", "negative sequence");
    InvalidDatabase(directory, "UPDATE sqlite_sequence SET seq=0 WHERE name='native_account_ids';", "sequence behind allocated identities");
    InvalidDatabase(directory, "DELETE FROM sqlite_sequence WHERE name='native_account_ids';", "missing sequence with allocated identities");
    InvalidDatabase(directory, "INSERT INTO sqlite_sequence(name,seq) VALUES('native_account_ids',3);", "ambiguous duplicate sequence row");
    InvalidDatabase(directory, "UPDATE sqlite_sequence SET seq=2147483648 WHERE name='native_account_ids';", "sequence beyond Int32 capacity");
    string liveCorruption = Clone(directory);
    using (var store = Store(liveCorruption, true))
    {
        _ = Sql(liveCorruption, "UPDATE sqlite_sequence SET seq=0 WHERE name='native_account_ids';");
        bool failedForward = false, failedReverse = false;
        try { _ = store.GetOrCreateNativeAccountId(first.Id); } catch (IOException) { failedForward = true; }
        try { _ = store.AccountIdForNative(firstId); } catch (IOException) { failedReverse = true; }
        Check(failedForward && failedReverse, "full per-operation validation refuses corruption introduced after startup");
    }
    string triggerCase = Clone(v1);
    string redirectTrigger = "CREATE TRIGGER redirect_identity AFTER INSERT ON native_account_ids BEGIN UPDATE native_account_ids SET account_id='" + second.Id + "' WHERE native_id=new.native_id; END;";
    using (var store = Store(triggerCase, true))
    {
        _ = Sql(triggerCase, redirectTrigger);
        bool failed = false;
        try { _ = store.GetOrCreateNativeAccountId(first.Id); } catch (IOException) { failed = true; }
        Check(failed && Number(triggerCase, "SELECT COUNT(*) FROM native_account_ids;") == 0,
            "unexpected trigger cannot redirect an allocation to another existing account after startup");
    }
    string triggerBeforeOpen = Clone(v1);
    using (var migrated = Store(triggerBeforeOpen, true)) { }
    _ = Sql(triggerBeforeOpen, redirectTrigger);
    foreach (bool enabled in new[] { false, true })
    {
        bool failed = false;
        try { using var unused = Store(triggerBeforeOpen, enabled); } catch (IOException) { failed = true; }
        Check(failed && Number(triggerBeforeOpen, "SELECT COUNT(*) FROM native_account_ids;") == 0,
            "existing v2 A-to-B redirect trigger is rejected before open without a mapping or misbinding");
    }

    string restore = NewDirectory();
    using (var store = Store(directory, true))
    {
        using var metadata = JsonDocument.Parse(JsonSerializer.Serialize(store.Backup()));
        File.Copy(Path.Combine(directory, "backups", metadata.RootElement.GetProperty("file").GetString()!), Database(restore));
    }
    File.Copy(Path.Combine(directory, ".metallic-backend"), Path.Combine(restore, ".metallic-backend"));
    using (var store = Store(restore, true))
        Check(store.GetOrCreateNativeAccountId(first.Id) == firstId && store.AccountIdForNative(secondId) == second.Id,
            "consistent owned backup restores the same durable native mapping");

    var report = new { status = "native_identity_self_tests_passed", checks = checks.Count, tests = checks,
        databases = "new synthetic temporary databases only", nativeClientExecuted = false, endpointsEnabled = false,
        installedDatabaseAccessed = false, nativeCredentialsIssued = false };
    using (var output = new FileStream(args[1], FileMode.CreateNew)) JsonSerializer.Serialize(output, report, new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks }));
}
finally
{
    string full = Path.GetFullPath(root), temp = Path.GetFullPath(Path.GetTempPath());
    string expected = Path.GetFullPath(Path.Combine(temp, rootName));
    if (!string.Equals(full, expected, StringComparison.OrdinalIgnoreCase) ||
        !rootName.StartsWith("metallic-native-identity-", StringComparison.Ordinal) ||
        !Guid.TryParseExact(rootName["metallic-native-identity-".Length..], "N", out _) ||
        !full.StartsWith(temp.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) ||
        (File.GetAttributes(full) & FileAttributes.ReparsePoint) != 0)
        throw new InvalidOperationException("Temporary cleanup target escaped its generated root.");
    Directory.Delete(full, true);
}
