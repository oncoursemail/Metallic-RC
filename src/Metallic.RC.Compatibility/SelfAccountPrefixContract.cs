using System.Collections.ObjectModel;
using System.Text;
using System.Text.Json;

namespace Metallic.RC.Compatibility;

public sealed class SelfAccountPrefixResult
{
    internal SelfAccountPrefixResult(string status, Dictionary<string,object?>? fields = null)
    {
        Status = status;
        Fields = fields is null ? null : new ReadOnlyDictionary<string,object?>(fields);
    }

    public string Status { get; }
    public IReadOnlyDictionary<string,object?>? Fields { get; }
    public override string ToString() => $"SelfAccountPrefixResult {{ status: {Status}; values redacted }}";
}

// Captured build 23191908's property dispatch and primitive local stores only.
// This stops before account construction, defaults, setters and authentication.
public static class SelfAccountPrefixContract
{
    public const int MaxJsonBytes = 65_536;
    public const int MaxDepth = 32;
    public const int MaxProperties = 512;
    private static readonly UTF8Encoding StrictUtf8 = new(false,true);

    private static readonly string[] Names =
    [
        "Email", "Phone", "Birthday", "JuniorState", "ParentAccountId", "IsFakeJuniorBirthday",
        "AvailableUsernameChanges", "AccountId", "UserName", "DisplayName", "DisplayEmoji",
        "ProfileImage", "BannerImage", "TreatAsJunior", "HasBirthday", "PersonalPronouns",
        "IdentityFlags", "createdAt", "IsMetaPlatformBlocked", "IsJunior"
    ];
    public static IReadOnlyList<string> CanonicalNames { get; } = Array.AsReadOnly(Names);

    // Exact constructor observations; createdAt's writer spelling is intentionally lowercase.
    private static readonly string[][] AliasRows =
    [
        ["Email","email"], ["Phone","phone"], ["Birthday","birthday"],
        ["JuniorState","juniorState","juniorstate"],
        ["ParentAccountId","parentAccountId","parentaccountid"],
        ["IsFakeJuniorBirthday","isFakeJuniorBirthday","isfakejuniorbirthday"],
        ["AvailableUsernameChanges","availableUsernameChanges","availableusernamechanges"],
        ["AccountId","accountId","accountid"], ["UserName","userName","username"],
        ["DisplayName","displayName","displayname"], ["DisplayEmoji","displayEmoji","displayemoji"],
        ["ProfileImage","profileImage","profileimage"], ["BannerImage","bannerImage","bannerimage"],
        ["TreatAsJunior","treatAsJunior","treatasjunior"], ["HasBirthday","hasBirthday","hasbirthday"],
        ["PersonalPronouns","personalPronouns","personalpronouns"],
        ["IdentityFlags","identityFlags","identityflags"], ["CreatedAt","createdAt","createdat"],
        ["IsMetaPlatformBlocked","isMetaPlatformBlocked","ismetaplatformblocked"],
        ["IsJunior","isJunior","isjunior"]
    ];
    private static readonly IReadOnlyDictionary<string,int> Aliases = BuildAliases();

    private static IReadOnlyDictionary<string,int> BuildAliases()
    {
        var aliases = new Dictionary<string,int>(StringComparer.Ordinal);
        for (int index = 0; index < AliasRows.Length; index++)
            foreach (string alias in AliasRows[index]) aliases.Add(alias,index);
        return new ReadOnlyDictionary<string,int>(aliases);
    }

    public static SelfAccountPrefixResult Read(ReadOnlySpan<byte> utf8)
    {
        if (utf8.Length > MaxJsonBytes) throw new SelfAccountPrefixException("response_too_large");
        if (utf8.IsEmpty) throw new SelfAccountPrefixException("missing_json_value");
        try { StrictUtf8.GetCharCount(utf8); }
        catch (DecoderFallbackException) { throw new SelfAccountPrefixException("invalid_utf8"); }
        var reader = new Utf8JsonReader(utf8,new JsonReaderOptions
        {
            MaxDepth = MaxDepth,
            CommentHandling = JsonCommentHandling.Disallow,
            AllowTrailingCommas = false
        });
        if (!reader.Read()) throw new SelfAccountPrefixException("missing_json_value");
        if (reader.TokenType == JsonTokenType.Null)
        {
            RequireEnd(ref reader);
            return new("root_null");
        }
        if (reader.TokenType != JsonTokenType.StartObject)
            throw new SelfAccountPrefixException("object_or_null_required");
        if (!reader.Read()) throw new SelfAccountPrefixException("unterminated_object");
        if (reader.TokenType == JsonTokenType.EndObject)
        {
            RequireEnd(ref reader);
            return new("root_empty_object_null");
        }

        var fields = new Dictionary<string,object?>(StringComparer.Ordinal);
        int properties = 0;
        while (reader.TokenType != JsonTokenType.EndObject)
        {
            if (++properties > MaxProperties) throw new SelfAccountPrefixException("property_budget_exceeded");
            if (reader.TokenType != JsonTokenType.PropertyName)
                throw new SelfAccountPrefixException("property_required");
            string name = Encoding.UTF8.GetString(reader.ValueSpan);
            int index = -1;
            bool recognized = !reader.ValueIsEscaped && Aliases.TryGetValue(name,out index);
            if (!reader.Read()) throw new SelfAccountPrefixException("property_value_required");
            if (!recognized) reader.Skip();
            else
            {
                object? value;
                switch (index)
                {
                    case 0 or 1 or 8 or 9 or 10 or 11 or 12:
                        if (reader.TokenType is not (JsonTokenType.String or JsonTokenType.Null))
                            return new("known_value_type_error");
                        value = reader.TokenType == JsonTokenType.Null ? null : ReadKnownString(ref reader);
                        break;
                    case 6 or 7:
                        if (reader.TokenType != JsonTokenType.Number || !reader.TryGetInt32(out int number))
                            return new("known_value_type_error");
                        value = number;
                        break;
                    case 13 or 14 or 18:
                        if (reader.TokenType is not (JsonTokenType.True or JsonTokenType.False))
                            return new("known_value_type_error");
                        value = reader.TokenType == JsonTokenType.True;
                        break;
                    default:
                        // No date, enum or nullable nested formatter behavior is invented.
                        return new("nested_value_outside_profile");
                }
                fields[Names[index]] = value;
            }
            if (!reader.Read()) throw new SelfAccountPrefixException("unterminated_object");
        }
        RequireEnd(ref reader);
        return new("prefix_complete",fields);
    }

    private static void RequireEnd(ref Utf8JsonReader reader)
    {
        if (reader.Read()) throw new SelfAccountPrefixException("trailing_json");
    }

    private static string? ReadKnownString(ref Utf8JsonReader reader)
    {
        try { return reader.GetString(); }
        catch (InvalidOperationException) { throw new SelfAccountPrefixException("invalid_string"); }
    }
}

public sealed class SelfAccountPrefixException(string code) : JsonException(code)
{
    public string Code { get; } = code;
}
