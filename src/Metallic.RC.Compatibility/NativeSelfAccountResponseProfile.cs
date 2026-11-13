using System.Text;
using System.Text.Json;

namespace Metallic.RC.Compatibility;

// Every value is caller-supplied. This model has no account defaults or store mapping.
public sealed class NativeSelfAccountResponseModel(
    string? email, string? phone, long? birthdayUtcTicks, int juniorState,
    int? parentAccountId, bool? isFakeJuniorBirthday, int availableUsernameChanges, int accountId,
    string? userName, string? displayName, string? displayEmoji, string? profileImage, string? bannerImage,
    bool treatAsJunior, bool hasBirthday, int personalPronouns, int identityFlags, long createdAtUtcTicks,
    bool isMetaPlatformBlocked, bool? isJunior)
{
    public string? Email { get; } = email;
    public string? Phone { get; } = phone;
    public long? BirthdayUtcTicks { get; } = birthdayUtcTicks;
    public int JuniorState { get; } = juniorState;
    public int? ParentAccountId { get; } = parentAccountId;
    public bool? IsFakeJuniorBirthday { get; } = isFakeJuniorBirthday;
    public int AvailableUsernameChanges { get; } = availableUsernameChanges;
    public int AccountId { get; } = accountId;
    public string? UserName { get; } = userName;
    public string? DisplayName { get; } = displayName;
    public string? DisplayEmoji { get; } = displayEmoji;
    public string? ProfileImage { get; } = profileImage;
    public string? BannerImage { get; } = bannerImage;
    public bool TreatAsJunior { get; } = treatAsJunior;
    public bool HasBirthday { get; } = hasBirthday;
    public int PersonalPronouns { get; } = personalPronouns;
    public int IdentityFlags { get; } = identityFlags;
    public long CreatedAtUtcTicks { get; } = createdAtUtcTicks;
    public bool IsMetaPlatformBlocked { get; } = isMetaPlatformBlocked;
    public bool? IsJunior { get; } = isJunior;
    public override string ToString() => "NativeSelfAccountResponseModel { values redacted }";
}

// Authored backend response format accepted by the bounded default-numeric prefix profile.
// This does not reconstruct the original client's serializer or account construction.
public static class NativeSelfAccountResponseProfile
{
    public const int MaxJsonBytes = 65_536;
    public const int MaxStringUtf8Bytes = 16_384;
    private const long MaxDateTimeTicks = 3_155_378_975_999_999_999;
    private static readonly UTF8Encoding StrictUtf8 = new(false,true);

    public static byte[] Write(NativeSelfAccountResponseModel model)
    {
        ArgumentNullException.ThrowIfNull(model);
        ValidateString(model.Email); ValidateString(model.Phone); ValidateString(model.UserName);
        ValidateString(model.DisplayName); ValidateString(model.DisplayEmoji);
        ValidateString(model.ProfileImage); ValidateString(model.BannerImage);
        if (model.BirthdayUtcTicks is long birthday) ValidateTicks(birthday);
        ValidateTicks(model.CreatedAtUtcTicks);

        using var output = new BoundedOutput();
        using (var writer = new Utf8JsonWriter(output))
        {
            writer.WriteStartObject();
            writer.WriteString("Email",model.Email);
            writer.WriteString("Phone",model.Phone);
            if (model.BirthdayUtcTicks is long ticks) writer.WriteNumber("Birthday",ticks); else writer.WriteNull("Birthday");
            writer.WriteNumber("JuniorState",model.JuniorState);
            if (model.ParentAccountId is int parentId) writer.WriteNumber("ParentAccountId",parentId); else writer.WriteNull("ParentAccountId");
            if (model.IsFakeJuniorBirthday is bool fake) writer.WriteBoolean("IsFakeJuniorBirthday",fake); else writer.WriteNull("IsFakeJuniorBirthday");
            writer.WriteNumber("AvailableUsernameChanges",model.AvailableUsernameChanges);
            writer.WriteNumber("AccountId",model.AccountId);
            writer.WriteString("UserName",model.UserName);
            writer.WriteString("DisplayName",model.DisplayName);
            writer.WriteString("DisplayEmoji",model.DisplayEmoji);
            writer.WriteString("ProfileImage",model.ProfileImage);
            writer.WriteString("BannerImage",model.BannerImage);
            writer.WriteBoolean("TreatAsJunior",model.TreatAsJunior);
            writer.WriteBoolean("HasBirthday",model.HasBirthday);
            writer.WriteNumber("PersonalPronouns",model.PersonalPronouns);
            writer.WriteNumber("IdentityFlags",model.IdentityFlags);
            writer.WriteNumber("createdAt",model.CreatedAtUtcTicks);
            writer.WriteBoolean("IsMetaPlatformBlocked",model.IsMetaPlatformBlocked);
            if (model.IsJunior is bool junior) writer.WriteBoolean("IsJunior",junior); else writer.WriteNull("IsJunior");
            writer.WriteEndObject();
            writer.Flush();
        }
        return output.ToArray();
    }

    private static void ValidateString(string? value)
    {
        if (value is null) return;
        int bytes;
        try { bytes = StrictUtf8.GetByteCount(value); }
        catch (EncoderFallbackException) { throw new NativeSelfAccountResponseException("invalid_unicode"); }
        if (bytes > MaxStringUtf8Bytes) throw new NativeSelfAccountResponseException("string_too_large");
    }

    private static void ValidateTicks(long ticks)
    {
        if (ticks < 0 || ticks > MaxDateTimeTicks) throw new NativeSelfAccountResponseException("ticks_out_of_range");
    }

    private sealed class BoundedOutput : MemoryStream
    {
        private void Admit(int length)
        {
            if (length > MaxJsonBytes-Length) throw new NativeSelfAccountResponseException("response_too_large");
        }
        public override void Write(ReadOnlySpan<byte> buffer) { Admit(buffer.Length); base.Write(buffer); }
        public override void Write(byte[] buffer,int offset,int count) { Admit(count); base.Write(buffer,offset,count); }
        public override void WriteByte(byte value) { Admit(1); base.WriteByte(value); }
    }
}

public sealed class NativeSelfAccountResponseException(string code) : ArgumentException(code)
{
    public string Code { get; } = code;
}
