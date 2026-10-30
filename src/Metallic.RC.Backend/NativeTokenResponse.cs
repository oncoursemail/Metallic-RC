using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Metallic.RC.Compatibility;

namespace Metallic.RC.Backend;

// Deliberate host handoff only. This does not issue or authenticate a stored session.
public delegate void NativeTokenMaterialConsumer(NativeSessionMaterial material, ReadOnlySpan<byte> requestSigningKey);

public sealed class NativeTokenResponse
{
    // Authored local development policy, not recovered vendor JWT policy.
    public const string Issuer = "urn:metallic:rc:local";
    public const string Audience = "urn:metallic:rc:native-local";
    private readonly string accessToken, refreshToken;
    private readonly byte[] requestKey;
    private readonly DateTimeOffset accessExpires, refreshExpires;

    private NativeTokenResponse(string access, string refresh, byte[] key, DateTimeOffset accessExpiry, DateTimeOffset refreshExpiry)
    { accessToken = access; refreshToken = refresh; requestKey = key; accessExpires = accessExpiry; refreshExpires = refreshExpiry; }

    public override string ToString() => "NativeTokenResponse { credential values redacted }";

    /// <summary>The only response-body export; the returned bytes contain credentials.</summary>
    public byte[] WriteTransportBody() => LoginResponseContract.Write(new LoginResponseProfile(
        AccessToken: accessToken, RefreshToken: refreshToken, Key: Convert.ToBase64String(requestKey)));

    /// <summary>Explicit synchronous secret handoff. Copy the span only for reviewed storage; it expires on return.</summary>
    public void UseSessionMaterial(NativeTokenMaterialConsumer consumer)
    {
        ArgumentNullException.ThrowIfNull(consumer);
        // The host never receives this object's backing key buffer.
        byte[] copy = requestKey.ToArray();
        try { consumer(new NativeSessionMaterial(accessToken, refreshToken, accessExpires, refreshExpires), copy); }
        finally { CryptographicOperations.ZeroMemory(copy); }
    }

    /// <summary>Build local material from an explicit dedicated 32-byte CSPRNG server key and whole-second UTC times.</summary>
    public static NativeTokenResponse Create(int nativeAccountId, ReadOnlySpan<byte> serverSigningKey,
        DateTimeOffset issuedAt, DateTimeOffset accessExpiresAt, DateTimeOffset refreshExpiresAt)
    {
        if (nativeAccountId <= 0) throw Invalid("native_token_id_invalid");
        if (serverSigningKey.Length != 32) throw Invalid("native_token_server_key_invalid");
        long issued = Seconds(issuedAt), access = Seconds(accessExpiresAt), refresh = Seconds(refreshExpiresAt);
        if (issued < 0 || issued > 253402300799 - 30 * 86400 || access <= issued || access - issued > 3600 ||
            refresh < access || refresh - issued > 30 * 86400) throw Invalid("native_token_expiry_invalid");

        byte[] master = serverSigningKey.ToArray();
        byte[]? request = null, refreshBytes = null, jti = null;
        try
        {
            if (master.All(b => b == 0)) throw Invalid("native_token_server_key_invalid");
            request = RandomNumberGenerator.GetBytes(32);
            refreshBytes = RandomNumberGenerator.GetBytes(48);
            jti = RandomNumberGenerator.GetBytes(16);
            if (request.All(b => b == 0) || CryptographicOperations.FixedTimeEquals(master, request))
                throw Invalid("native_token_random_material_invalid");
            string header = Url64("{\"alg\":\"HS256\",\"typ\":\"JWT\"}"u8);
            byte[] payload;
            using (var stream = new MemoryStream())
            {
                using (var writer = new Utf8JsonWriter(stream))
                {
                    writer.WriteStartObject();
                    writer.WriteString("sub", nativeAccountId.ToString(CultureInfo.InvariantCulture));
                    writer.WriteString("iss", Issuer); writer.WriteString("aud", Audience);
                    writer.WriteNumber("iat", issued); writer.WriteNumber("exp", access);
                    writer.WriteString("jti", Url64(jti)); writer.WriteEndObject();
                }
                payload = stream.ToArray();
            }
            string signed = header + "." + Url64(payload);
            byte[] signature = HMACSHA256.HashData(master, Encoding.ASCII.GetBytes(signed));
            string token = signed + "." + Url64(signature);
            // Construction takes a private copy before temporary byte buffers are cleared.
            return new(token, Url64(refreshBytes), request.ToArray(), accessExpiresAt, refreshExpiresAt);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(master);
            if (request is not null) CryptographicOperations.ZeroMemory(request);
            if (refreshBytes is not null) CryptographicOperations.ZeroMemory(refreshBytes);
            if (jti is not null) CryptographicOperations.ZeroMemory(jti);
        }
    }

    private static long Seconds(DateTimeOffset time)
    {
        if (time.Offset != TimeSpan.Zero || time.Ticks % TimeSpan.TicksPerSecond != 0)
            throw Invalid("native_token_time_invalid");
        return time.ToUnixTimeSeconds();
    }
    private static string Url64(ReadOnlySpan<byte> bytes) => Convert.ToBase64String(bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_');
    private static NativeTokenResponseException Invalid(string code) => new(code);
}

public sealed class NativeTokenResponseException : ArgumentException
{
    internal NativeTokenResponseException(string code) : base(code) { Code = code; }
    public string Code { get; }
}
