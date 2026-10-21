// LOCAL ANALYSIS RECONSTRUCTION; NOT ORIGINAL DEVELOPER SOURCE.
// Build 23191908: methods 148123 and 367496, validated typed call/field mapping.
// Runtime initialization, original exception message and original BCL behavior
// have not been reconstructed here. This is not a backend or game integration.
namespace Metallic.Reconstruction;

public struct CachedResponseContext
{
    public bool CachedNameServerResponseLoaded;
    public bool CachedNameServerResponseChanged;
}

public static class ObservedServiceAddressUpdate
{
    public static string EnsureTrailingCharacter(string value, char suffix)
    {
        if (value.Length > 0 && value[value.Length - 1] == suffix) return value;
        return string.Concat(value, suffix.ToString());
    }

    public static void Apply(Dictionary<int, Uri> services, int serviceId,
                             string? address, ref CachedResponseContext context)
    {
        // The recovered exception constructor is System.Exception(string).
        // The exact original message has not been recovered; do not claim it.
        if (address is null)
            throw new Exception("Missing service address (reconstructed diagnostic; not the original message).");
        var next = new Uri(EnsureTrailingCharacter(address, '/'));
        if (context.CachedNameServerResponseLoaded &&
            services[serviceId].OriginalString != next.OriginalString)
            context.CachedNameServerResponseChanged = true;
        services[serviceId] = next;
    }
}
