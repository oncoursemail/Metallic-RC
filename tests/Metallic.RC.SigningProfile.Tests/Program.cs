using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;

if (args.Length > 1) throw new ArgumentException("Use [NEW-result.json].");
int checks = 0;
void Check(bool value, string label)
{
    if (!value) throw new InvalidOperationException(label);
    checks++;
}
void Reject<T>(Action action, string label) where T : Exception
{
    bool rejected = false;
    try { action(); } catch (T) { rejected = true; }
    Check(rejected, label);
}
byte[] Pattern(int size) => Enumerable.Range(0, size).Select(index => (byte)((index * 37 + 11) % 256)).ToArray();
const string path = "/api/v1/synthetic?x=1&name=unit%20test";
byte[] key = Enumerable.Range(0, 32).Select(index => (byte)index).ToArray();
// Negative controls build an ordinary byte message and use the one-shot HMAC API.
// Positive signatures come from the retained Python stdlib oracle, not this helper.
byte[] Message(byte[]? body, bool full = false, bool length = true, bool bigEndian = false, bool method = false)
{
    using var stream = new MemoryStream();
    if (method) stream.Write("POST"u8);
    stream.Write(Encoding.ASCII.GetBytes(path));
    if (body is { Length: > 0 })
    {
        if (length)
        {
            Span<byte> count = stackalloc byte[4];
            if (bigEndian) BinaryPrimitives.WriteInt32BigEndian(count, body.Length);
            else BinaryPrimitives.WriteInt32LittleEndian(count, body.Length);
            stream.Write(count);
        }
        if (full || body.Length <= 2048) stream.Write(body);
        else
            foreach (int offset in Enumerable.Range(0, 16).Select(index => index * (body.Length / 16)))
                stream.Write(body, offset, 128);
    }
    return stream.ToArray();
}
string Sign(byte[] message) => Convert.ToBase64String(HMACSHA256.HashData(key, message));
using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "vectors.json")));
int?[] expectedLengths = [null, 0, 2048, 2049, 2063, 2064, 4096, 65536];
var rows = vectors.RootElement.EnumerateArray().ToArray();
Check(rows.Length == expectedLengths.Length, "all oracle boundary cases retained");
for (int index = 0; index < rows.Length; index++)
{
    var row = rows[index];
    int? size = row.GetProperty("bodyLength").ValueKind == JsonValueKind.Null ? null : row.GetProperty("bodyLength").GetInt32();
    Check(size == expectedLengths[index], "oracle case identity");
    byte[]? body = size.HasValue ? Pattern(size.Value) : null;
    string expected = row.GetProperty("signature").GetString()!;
    Check(NativeRequestSigningProfile.Compute(key, path, body) == expected, "Python HMAC vector " + index);
    Check(Convert.ToHexStringLower(SHA256.HashData(Message(body))) == row.GetProperty("messageSha256").GetString(), "independent message fingerprint " + index);
    Check(Sign(Message(body, method: true)) != expected, "method-prefix mutant rejected " + index);
    if (size > 0)
    {
        Check(Sign(Message(body, length: false)) != expected, "missing-length mutant rejected " + index);
        Check(Sign(Message(body, bigEndian: true)) != expected, "big-endian mutant rejected " + index);
    }
    if (size > 2048) Check(Sign(Message(body, full: true)) != expected, "full-body mutant rejected " + index);
}
Check(NativeRequestSigningProfile.Compute(key, path, null) == NativeRequestSigningProfile.Compute(key, path, []), "null and empty omit length");
byte[] large = Pattern(4096), original = (byte[])large.Clone(), savedKey = (byte[])key.Clone();
string baseline = NativeRequestSigningProfile.Compute(key, path, large);
Check(large.SequenceEqual(original) && key.SequenceEqual(savedKey), "caller arrays unchanged");
large[128] ^= 1;
Check(NativeRequestSigningProfile.Compute(key, path, large) == baseline, "omitted byte is NOT integrity protected");
large[0] ^= 1;
Check(NativeRequestSigningProfile.Compute(key, path, large) != baseline, "sampled byte changes signature");
byte[] small = Pattern(2048);
string smallBaseline = NativeRequestSigningProfile.Compute(key, path, small);
small[^1] ^= 1;
Check(NativeRequestSigningProfile.Compute(key, path, small) != smallBaseline, "2048th byte is protected");
Check(NativeRequestSigningProfile.Compute([], "", null) == Convert.ToBase64String(HMACSHA256.HashData([], [])), "empty key and path have no invented native minimum");
Check(NativeRequestSigningProfile.Compute(key, path + "x", null) != NativeRequestSigningProfile.Compute(key, path, null), "path participates");
Check(NativeRequestSigningProfile.Compute([1], path, null) != NativeRequestSigningProfile.Compute(key, path, null), "key participates");
Check(Convert.FromBase64String(baseline).Length == 32, "SHA256 result is Base64");
Check(NativeRequestSigningProfile.Compute(key, new string('a', NativeRequestSigningProfile.MaxPathChars), null).Length == 44, "path boundary admitted");
Check(NativeRequestSigningProfile.Compute(new byte[NativeRequestSigningProfile.MaxKeyBytes], path, null).Length == 44, "key boundary admitted");
Check(NativeRequestSigningProfile.Compute(key, path, new byte[NativeRequestSigningProfile.MaxBodyBytes]).Length == 44, "body boundary admitted");
Reject<ArgumentNullException>(() => NativeRequestSigningProfile.Compute(null!, path, null), "null key rejected");
Reject<ArgumentNullException>(() => NativeRequestSigningProfile.Compute(key, null!, null), "null path rejected");
Reject<ArgumentException>(() => NativeRequestSigningProfile.Compute(key, new string('a', NativeRequestSigningProfile.MaxPathChars + 1), null), "path bound enforced");
Reject<ArgumentException>(() => NativeRequestSigningProfile.Compute(new byte[NativeRequestSigningProfile.MaxKeyBytes + 1], path, null), "key bound enforced");
Reject<ArgumentException>(() => NativeRequestSigningProfile.Compute(key, path, new byte[NativeRequestSigningProfile.MaxBodyBytes + 1]), "body bound enforced");
foreach (string nonAscii in new[] { "caf\u00e9", "\ud800", "\udc00", "\U0001f600" })
    Reject<ArgumentException>(() => NativeRequestSigningProfile.Compute(key, nonAscii, null), "non-ASCII outside profile");
var result = new { status = "passed", checks, syntheticOnly = true, nativeExecution = false, nativeEquivalenceEstablished = false };
string json = JsonSerializer.Serialize(result);
if (args.Length == 1)
{
    using var output = new FileStream(args[0], FileMode.CreateNew, FileAccess.Write);
    JsonSerializer.Serialize(output, result);
}
Console.WriteLine(json);
