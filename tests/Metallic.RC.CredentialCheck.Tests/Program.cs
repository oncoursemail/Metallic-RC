using System.Globalization;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using Metallic.RC.Backend;
using Microsoft.AspNetCore.Identity;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.Options;

if (args.Length != 2 || args[0] != "--self-test") throw new ArgumentException("Use --self-test NEW-result.json.");
if (File.Exists(args[1])) throw new IOException("Evidence output already exists.");
var checks = new List<string>();
void Check(bool value, string label)
{
    if (!value) throw new InvalidOperationException(label);
    checks.Add(label);
}
void Fault(Action action, int status, string code)
{
    bool rejected = false;
    try { action(); }
    catch (BackendFault error)
    {
        rejected = error.Status == status && error.Code == code && error.Message == code;
    }
    Check(rejected, "unchanged credential/session failure: " + code);
}
string rootName = "metallic-credential-check-" + Guid.NewGuid().ToString("N");
string root = Path.Combine(Path.GetTempPath(), rootName);
Directory.CreateDirectory(root);
string NewDirectory() { string path = Path.Combine(root, Guid.NewGuid().ToString("N")); Directory.CreateDirectory(path); return path; }
string GeneratedValue() => "synthetic-" + Guid.NewGuid().ToString("N");
SqliteConnection Open(string directory)
{
    if (!Path.GetFullPath(directory).StartsWith(Path.GetFullPath(root) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
        throw new InvalidOperationException("Only this run's generated temporary databases are allowed.");
    var db = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = Path.Combine(directory, "backend.sqlite"), Pooling = false }.ToString());
    db.Open(); return db;
}
object? Sql(string directory, string text, params (string Name, object Value)[] values)
{
    using var db = Open(directory); using var command = db.CreateCommand(); command.CommandText = text;
    foreach (var value in values) command.Parameters.AddWithValue(value.Name, value.Value);
    return command.ExecuteScalar();
}
long Count(string directory, string table) => (long)Sql(directory, "SELECT COUNT(*) FROM " + table + ";")!;
string StoredHash(string directory, string accountId) => (string)Sql(directory, "SELECT password_hash FROM accounts WHERE id=$id;", ("$id", accountId))!;
BackendStore Store(string directory, bool mapping = false) => new(new BackendOptions(0, directory) { EnableNativeIdentityMapping = mapping });
AccountView Register(BackendStore store, string password) => store.Register(new("owned_" + Guid.NewGuid().ToString("N")[..12], password, "Synthetic account"));
Func<LoginRequest, AccountView> Checker(BackendStore store)
{
    // Exercise the actual internal method without publishing it or adding a friend assembly.
    var method = typeof(BackendStore).GetMethod("VerifyCredentials", BindingFlags.Instance | BindingFlags.NonPublic)
        ?? throw new InvalidOperationException("Credential helper is missing.");
    Check(method.IsAssembly && method.ReturnType == typeof(AccountView) &&
        method.GetParameters().Select(parameter => parameter.ParameterType).SequenceEqual(new[] { typeof(LoginRequest) }),
        "credential-only helper remains internal and returns only the existing AccountView");
    return method.CreateDelegate<Func<LoginRequest, AccountView>>(store);
}
AccountView QuietCheck(Func<LoginRequest, AccountView> checker, LoginRequest request)
{
    var previousOut = Console.Out; var previousError = Console.Error;
    using var output = new StringWriter(CultureInfo.InvariantCulture);
    using var error = new StringWriter(CultureInfo.InvariantCulture);
    Console.SetOut(output); Console.SetError(error);
    try { return checker(request); }
    finally
    {
        Console.SetOut(previousOut); Console.SetError(previousError);
        Check(output.ToString().Length == 0 && error.ToString().Length == 0, "credential helper emits no console text on success or failure");
    }
}

try
{
    foreach (bool mapping in new[] { false, true })
    {
        string directory = NewDirectory(), password = GeneratedValue(), incorrect = GeneratedValue();
        using var store = Store(directory, mapping);
        var account = Register(store, password); var checker = Checker(store);
        string hashBefore = StoredHash(directory, account.Id);
        string expiredTokenHash = Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(32));
        _ = Sql(directory, "INSERT INTO sessions(token_hash,account_id,expires_at) VALUES($hash,$id,0);", ("$hash", expiredTokenHash), ("$id", account.Id));
        Check(Count(directory, "sessions") == 1, "fixture starts with one expired owned session");
        var result = QuietCheck(checker, new(account.Username.ToUpperInvariant(), password));
        Check(result == account, "correct owned credentials return the account with existing username normalization");
        Check(Count(directory, "sessions") == 1, "successful helper does not mint a session or prune expired sessions");
        Fault(() => QuietCheck(checker, new(account.Username, incorrect)), 401, "invalid_credentials");
        Fault(() => QuietCheck(checker, new("unknown_" + Guid.NewGuid().ToString("N")[..12], password)), 401, "invalid_credentials");
        Fault(() => QuietCheck(checker, new("invalid?", password)), 400, "invalid_username");
        Fault(() => QuietCheck(checker, new(null!, password)), 400, "invalid_username");
        foreach (string? badPassword in new string?[] { null, "", new string('x', 11), new string('x', 257) })
            Fault(() => QuietCheck(checker, new(account.Username, badPassword!)), 400, "password_length_12_to_256_required");
        Check(Count(directory, "sessions") == 1 && StoredHash(directory, account.Id) == hashBefore && store.Profile(account.Id) == account,
            "helper successes and failures leave sessions, password hashes and profile unchanged");
        Check(mapping ? Count(directory, "native_account_ids") == 0 &&
            (long)Sql(directory, "SELECT COUNT(*) FROM sqlite_sequence WHERE name='native_account_ids';")! == 0 :
            (long)Sql(directory, "SELECT COUNT(*) FROM sqlite_schema WHERE name='native_account_ids';")! == 0,
            "helper neither enables identity mapping nor allocates a native identity");
        string[] formatted = [result.ToString(), JsonSerializer.Serialize(result), JsonSerializer.Serialize<object>(result)];
        Check(formatted.All(text => !text.Contains(password, StringComparison.Ordinal) && !text.Contains(incorrect, StringComparison.Ordinal) && !text.Contains(hashBefore, StringComparison.Ordinal)),
            "returned account formatting and serialization contain no password or password hash");
        Check(typeof(AccountView).GetProperties().Select(property => property.Name).Order().SequenceEqual(new[] { "DisplayName", "Id", "Revision", "Username" }),
            "helper returns existing profile metadata rather than a credential/session envelope");
        Check((long)Sql(directory, "PRAGMA user_version;")! == (mapping ? 2 : 1), "helper leaves the selected identity schema version unchanged");
    }

    string limitDirectory = NewDirectory(), limitPassword = GeneratedValue();
    using (var store = Store(limitDirectory))
    {
        var account = Register(store, limitPassword); var other = Register(store, GeneratedValue()); var checker = Checker(store);
        var sessions = new List<SessionView>();
        for (int index = 0; index < 16; index++)
        {
            var before = DateTimeOffset.UtcNow;
            var session = store.Login(new(account.Username, limitPassword));
            var after = DateTimeOffset.UtcNow;
            sessions.Add(session);
            Check(session.Account == account && Regex.IsMatch(session.AccessToken, "\\A[0-9a-f]{64}\\z", RegexOptions.CultureInvariant), "Login still returns the existing owned 64-hex session format");
            Check(session.ExpiresAt >= before.AddHours(1) && session.ExpiresAt <= after.AddHours(1), "Login keeps the existing one-hour expiry");
            Check(store.Authenticate("Bearer " + session.AccessToken).AccountId == account.Id, "issued owned bearer still authenticates the same account");
            Check((long)Sql(limitDirectory, "SELECT COUNT(*) FROM sessions WHERE token_hash=$raw;", ("$raw", session.AccessToken))! == 0 &&
                (long)Sql(limitDirectory, "SELECT COUNT(*) FROM sessions WHERE token_hash=$hash;", ("$hash", Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(session.AccessToken)))))! == 1,
                "Login still stores only the SHA-256 bearer hash");
        }
        Check(sessions.Select(session => session.AccessToken).Distinct(StringComparer.Ordinal).Count() == 16 && Count(limitDirectory, "sessions") == 16,
            "sixteen distinct owned sessions are retained without an extra helper session");
        Check(QuietCheck(checker, new(account.Username, limitPassword)) == account && Count(limitDirectory, "sessions") == 16,
            "credential check succeeds at the session cap without allocating or evicting a session");
        Fault(() => store.Login(new(account.Username, limitPassword)), 429, "session_limit");
        Fault(() => store.Login(new(account.Username, GeneratedValue())), 401, "invalid_credentials");
        Check(Count(limitDirectory, "sessions") == 16 && Count(limitDirectory, "accounts") == 2, "failed Login leaves existing sessions and accounts intact");
        var revoked = store.Authenticate("Bearer " + sessions[0].AccessToken); store.Logout(revoked);
        Fault(() => store.Authenticate("Bearer " + sessions[0].AccessToken), 401, "session_expired_or_revoked");
        var replacement = store.Login(new(account.Username, limitPassword));
        Check(Count(limitDirectory, "sessions") == 16 && replacement.AccessToken != sessions[0].AccessToken, "logout frees exactly one slot for the unchanged owned Login path");
        // Expire an existing row, then verify cleanup remains part of Login rather than the helper.
        _ = Sql(limitDirectory, "UPDATE sessions SET expires_at=0 WHERE token_hash=$hash;", ("$hash", Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(replacement.AccessToken)))));
        Check(QuietCheck(checker, new(account.Username, limitPassword)) == account && Count(limitDirectory, "sessions") == 16,
            "helper preserves expired rows even at capacity");
        _ = store.Login(new(account.Username, limitPassword));
        Check(Count(limitDirectory, "sessions") == 16 && (long)Sql(limitDirectory, "SELECT COUNT(*) FROM sessions WHERE expires_at=0;")! == 0,
            "Login retains expired-session cleanup before applying the sixteen-session cap");
        Check(store.Profile(other.Id) == other, "unrelated owned account remains unchanged");
    }

    string legacyDirectory = NewDirectory(), legacyPassword = GeneratedValue();
    using (var store = Store(legacyDirectory))
    {
        var account = Register(store, legacyPassword); var checker = Checker(store);
        var legacyHasher = new PasswordHasher<string>(Options.Create(new PasswordHasherOptions { IterationCount = 1000 }));
        string legacyHash = legacyHasher.HashPassword(account.Id, legacyPassword);
        _ = Sql(legacyDirectory, "UPDATE accounts SET password_hash=$hash WHERE id=$id;", ("$hash", legacyHash), ("$id", account.Id));
        var currentHasher = new PasswordHasher<string>(Options.Create(new PasswordHasherOptions { IterationCount = 600000 }));
        Check(currentHasher.VerifyHashedPassword(account.Id, legacyHash, legacyPassword) == PasswordVerificationResult.SuccessRehashNeeded,
            "fixture genuinely reaches the existing SuccessRehashNeeded result");
        Check(QuietCheck(checker, new(account.Username, legacyPassword)) == account && Count(legacyDirectory, "sessions") == 0 && StoredHash(legacyDirectory, account.Id) == legacyHash,
            "helper preserves existing legacy-hash acceptance without rehash or session writes");
        var session = store.Login(new(account.Username, legacyPassword));
        Check(session.Account == account && Count(legacyDirectory, "sessions") == 1 && StoredHash(legacyDirectory, account.Id) == legacyHash,
            "Login preserves prior SuccessRehashNeeded acceptance without silently changing stored hash semantics");
    }

    var report = new { status = "credential_check_self_tests_passed", checks = checks.Count, tests = checks,
        databases = "new synthetic temporary databases only", installedDatabaseAccessed = false, nativeCredentialsIssued = false,
        nativeEndpointsEnabled = false, nativeClientExecuted = false };
    using (var output = new FileStream(args[1], FileMode.CreateNew)) JsonSerializer.Serialize(output, report, new JsonSerializerOptions { WriteIndented = true });
    Console.WriteLine(JsonSerializer.Serialize(new { report.status, report.checks }));
}
finally
{
    string full = Path.GetFullPath(root), temp = Path.GetFullPath(Path.GetTempPath());
    if (!string.Equals(full, Path.GetFullPath(Path.Combine(temp, rootName)), StringComparison.OrdinalIgnoreCase) ||
        !rootName.StartsWith("metallic-credential-check-", StringComparison.Ordinal) ||
        !Guid.TryParseExact(rootName["metallic-credential-check-".Length..], "N", out _) ||
        !full.StartsWith(temp.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) ||
        (File.GetAttributes(full) & FileAttributes.ReparsePoint) != 0)
        throw new InvalidOperationException("Temporary cleanup target escaped its generated root.");
    Directory.Delete(full, true);
}
