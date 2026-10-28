using System.Collections.ObjectModel;
using System.Security.Cryptography;
using System.Text;

namespace Metallic.RC.Compatibility;

public sealed class NativePasswordFormResult
{
    private readonly Dictionary<string, string> manualFields;

    internal NativePasswordFormResult(string status, string? reasonCode = null,
        IEnumerable<string>? names = null, Dictionary<string, string>? manual = null)
    {
        Status = status;
        ReasonCode = reasonCode;
        FieldNames = new ReadOnlyCollection<string>(names?.ToArray() ?? []);
        manualFields = manual is null ? new(StringComparer.Ordinal) : new(manual, StringComparer.Ordinal);
    }

    public string Status { get; }
    public string? ReasonCode { get; }
    public IReadOnlyList<string> FieldNames { get; }
    public int FieldCount => FieldNames.Count;

    // Values are available only through an explicit call, never a serializable property or enumerator.
    public bool TryGetManualField(string? name, out string? value)
    {
        value = null;
        return name is not null && manualFields.TryGetValue(name, out value);
    }

    public override string ToString() =>
        $"NativePasswordFormResult {{ status: {Status}; fields: {FieldCount}; values redacted }}";
}

// A bounded decoder for the captured string-only builder branch. It does not authenticate or issue tokens.
public static class NativePasswordFormProfile
{
    public const string ContentType = "application/x-www-form-urlencoded";
    public const int MaxBodyBytes = 65_536;
    public const int MaxFields = 32;
    public const int MaxKeyBytes = 64;
    public const int MaxValueBytes = 8_192;
    private static readonly UTF8Encoding StrictUtf8 = new(false, true);
    private static readonly HashSet<string> ManualNames = new(StringComparer.Ordinal)
    {
        "grant_type", "username", "password", "security_code", "2fa_code", "login_context", "require_password"
    };
    private static readonly HashSet<string> ContextNames = new(StringComparer.Ordinal)
    {
        "client_id", "client_secret", "platform", "platform_id", "device_id", "device_class", "time", "ver",
        "cid", "asid", "locale", "isInitialLogin", "build_key"
    };

    public static NativePasswordFormResult Read(ReadOnlySpan<byte> body, string? contentType)
    {
        if (!string.Equals(contentType, ContentType, StringComparison.Ordinal)) return Outside("content_type_outside_profile");
        if (body.Length > MaxBodyBytes) return Outside("body_budget_exceeded");
        if (body.IsEmpty) return Outside("missing_body");
        int fieldCount = 1;
        foreach (byte octet in body)
        {
            if (octet == '+') return Outside("literal_plus_outside_profile");
            if (octet > 0x7f) return Outside("non_ascii_body");
            if (octet == '&' && ++fieldCount > MaxFields) return Outside("field_budget_exceeded");
        }

        Span<byte> keyScratch = stackalloc byte[MaxKeyBytes];
        Span<byte> valueScratch = stackalloc byte[MaxValueBytes];
        var seen = new HashSet<string>(StringComparer.Ordinal);
        var names = new List<string>();
        var manual = new Dictionary<string, string>(StringComparer.Ordinal);
        try
        {
            int position = 0;
            while (position < body.Length)
            {
                int next = body[position..].IndexOf((byte)'&');
                int length = next < 0 ? body.Length - position : next;
                ReadOnlySpan<byte> field = body.Slice(position, length);
                int equals = field.IndexOf((byte)'=');
                if (equals <= 0 || field[(equals + 1)..].IndexOf((byte)'=') >= 0) return Outside("malformed_field");
                if (!Decode(field[..equals], keyScratch, out int keyLength, out string? error))
                    return Outside(error == "decoded_budget_exceeded" ? "key_budget_exceeded" : error!);
                string name;
                try { name = StrictUtf8.GetString(keyScratch[..keyLength]); }
                catch (DecoderFallbackException) { return Outside("invalid_utf8"); }
                bool isManual = ManualNames.Contains(name);
                if (!isManual && !ContextNames.Contains(name)) return Outside("unknown_field");
                if (!seen.Add(name)) return Outside("duplicate_field");
                if (!Decode(field[(equals + 1)..], valueScratch, out int valueLength, out error))
                    return Outside(error == "decoded_budget_exceeded" ? "value_budget_exceeded" : error!);
                try
                {
                    // Context values are validated without creating strings or retaining their bytes in the result.
                    if (isManual) manual.Add(name, StrictUtf8.GetString(valueScratch[..valueLength]));
                    else StrictUtf8.GetCharCount(valueScratch[..valueLength]);
                }
                catch (DecoderFallbackException) { return Outside("invalid_utf8"); }
                CryptographicOperations.ZeroMemory(valueScratch);
                names.Add(name);
                if (next < 0) break;
                position += length + 1;
                if (position == body.Length) return Outside("malformed_field");
            }
            if (!manual.TryGetValue("grant_type", out string? grant) ||
                !manual.TryGetValue("username", out string? username) ||
                !manual.TryGetValue("password", out string? password)) return Outside("required_field_missing");
            if (!string.Equals(grant, "password", StringComparison.Ordinal)) return Outside("grant_outside_profile");
            if (username.Length == 0 || password.Length == 0) return Outside("empty_credentials");
            return new("within_profile", names: names, manual: manual);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(keyScratch);
            CryptographicOperations.ZeroMemory(valueScratch);
        }
    }

    private static NativePasswordFormResult Outside(string code) => new("outside_profile", code);

    private static bool Decode(ReadOnlySpan<byte> encoded, Span<byte> output, out int count, out string? error)
    {
        count = 0;
        error = null;
        for (int index = 0; index < encoded.Length; index++)
        {
            byte value = encoded[index];
            if (value == '%')
            {
                if (index + 2 >= encoded.Length || Hex(encoded[index + 1]) < 0 || Hex(encoded[index + 2]) < 0)
                { error = "malformed_percent_encoding"; return false; }
                value = (byte)((Hex(encoded[index + 1]) << 4) | Hex(encoded[index + 2]));
                index += 2;
            }
            else if (!Unreserved(value)) { error = "unescaped_character_outside_profile"; return false; }
            if (count == output.Length) { error = "decoded_budget_exceeded"; return false; }
            output[count++] = value;
        }
        return true;
    }

    private static bool Unreserved(byte value) => value is >= (byte)'a' and <= (byte)'z' or
        >= (byte)'A' and <= (byte)'Z' or >= (byte)'0' and <= (byte)'9' or (byte)'-' or (byte)'_' or (byte)'.' or (byte)'~';

    private static int Hex(byte value) => value switch
    {
        >= (byte)'0' and <= (byte)'9' => value - '0',
        >= (byte)'a' and <= (byte)'f' => value - 'a' + 10,
        >= (byte)'A' and <= (byte)'F' => value - 'A' + 10,
        _ => -1
    };
}
