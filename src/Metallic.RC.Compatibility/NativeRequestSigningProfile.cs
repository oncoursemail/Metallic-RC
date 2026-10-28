using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;

namespace Metallic.RC.Compatibility;

/// <summary>
/// Bounded ASCII-only profile of the retained build 23191908 request signer.
/// Produces a signature value only; it does not authorize or send a request.
/// Bodies larger than 2048 bytes are sampled, not fully integrity protected.
/// </summary>
public static class NativeRequestSigningProfile
{
    // Authored resource limits, not recovered native restrictions.
    public const int MaxPathChars = 8192;
    public const int MaxKeyBytes = 4096;
    public const int MaxBodyBytes = 16 * 1024 * 1024;

    /// <summary>
    /// The key is already decoded. The path/query must already have its intended
    /// wire spelling: this method performs no URI normalization or ASCII fallback.
    /// Callers must not mutate either input array during the call.
    /// </summary>
    public static string Compute(byte[] key, string pathAndQuery, byte[]? body)
    {
        ArgumentNullException.ThrowIfNull(key);
        ArgumentNullException.ThrowIfNull(pathAndQuery);
        if (key.Length > MaxKeyBytes) throw new ArgumentException("Signing key exceeds profile limit.", nameof(key));
        if (pathAndQuery.Length > MaxPathChars) throw new ArgumentException("Path/query exceeds profile limit.", nameof(pathAndQuery));
        if (body is { Length: > MaxBodyBytes }) throw new ArgumentException("Body exceeds profile limit.", nameof(body));
        foreach (char value in pathAndQuery)
            if (value > 127) throw new ArgumentException("Path/query must be ASCII within this profile.", nameof(pathAndQuery));

        using var hash = IncrementalHash.CreateHMAC(HashAlgorithmName.SHA256, key);
        hash.AppendData(Encoding.ASCII.GetBytes(pathAndQuery));
        if (body is { Length: > 0 })
        {
            Span<byte> length = stackalloc byte[4];
            BinaryPrimitives.WriteInt32LittleEndian(length, body.Length);
            hash.AppendData(length);
            if (body.Length <= 2048) hash.AppendData(body);
            else
            {
                int stride = body.Length / 16;
                for (int index = 0; index < 16; index++)
                    hash.AppendData(body.AsSpan(stride * index, 128));
            }
        }
        return Convert.ToBase64String(hash.GetHashAndReset());
    }
}
