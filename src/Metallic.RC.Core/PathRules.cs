namespace Metallic.RC.Core;

internal static class PathRules
{
    internal static readonly StringComparer PathComparer = OperatingSystem.IsWindows()
        ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal;
    internal static readonly StringComparison PathComparison = OperatingSystem.IsWindows()
        ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;

    internal static string Absolute(string path)
    {
        if (string.IsNullOrWhiteSpace(path) || path.Length > 4096 || path.Any(char.IsControl) || !Path.IsPathFullyQualified(path))
            throw new CatalogException("invalid_path", "Supply a fully qualified local path without control characters.");
        if (path.StartsWith("\\\\", StringComparison.Ordinal) || path.StartsWith("//", StringComparison.Ordinal))
            throw new CatalogException("invalid_path", "Network and device paths are not accepted as local installations.");
        if (path.Split(['/', '\\']).Any(part => part is "." or ".."))
            throw new CatalogException("invalid_path", "Paths containing dot or parent traversal segments are not accepted.");
        try { return Path.TrimEndingDirectorySeparator(Path.GetFullPath(path)); }
        catch (Exception error) when (error is ArgumentException or NotSupportedException or PathTooLongException)
        { throw new CatalogException("invalid_path", "The path is not a valid local filesystem path.", error); }
    }

    internal static bool IsWithin(string parent, string path) => PathComparer.Equals(parent, path)
        || path.StartsWith(parent + Path.DirectorySeparatorChar, PathComparison);

    internal static string Relative(string relative)
    {
        if (string.IsNullOrWhiteSpace(relative) || relative.Length > 4096 || relative.Any(char.IsControl)
            || Path.IsPathRooted(relative) || relative.Contains(':') || relative.Contains('\\'))
            throw new CatalogException("invalid_relative_path", "A manifest file path must be a relative forward-slash path.");
        var parts = relative.Split('/');
        if (parts.Any(part => part is "" or "." or ".."))
            throw new CatalogException("invalid_relative_path", "A manifest file path contains an invalid segment.");
        return relative;
    }

    internal static string Combine(string root, string relative)
    {
        Relative(relative);
        var path = Path.GetFullPath(Path.Combine(root, relative.Replace('/', Path.DirectorySeparatorChar)));
        if (!IsWithin(root, path) || PathComparer.Equals(root, path))
            throw new CatalogException("path_escape", "A manifest file path escapes the installation.");
        return path;
    }

    internal static void RejectLinks(string path, bool allowMissingLeaf = false)
    {
        // Check every existing ancestor, not only the final filename. Do not resolve or traverse links.
        var current = Absolute(path);
        while (!string.IsNullOrEmpty(current))
        {
            try
            {
                var attributes = File.GetAttributes(current);
                if ((attributes & FileAttributes.ReparsePoint) != 0)
                    throw new CatalogException("link_not_allowed", "Symbolic links and reparse points are not accepted: " + current);
            }
            catch (Exception error) when (allowMissingLeaf && error is FileNotFoundException or DirectoryNotFoundException) { }
            current = Path.GetDirectoryName(current);
        }
    }

    internal static string DisplayName(string name)
    {
        if (string.IsNullOrWhiteSpace(name) || name.Length > 100 || name.Any(char.IsControl))
            throw new CatalogException("invalid_name", "Use a display name containing 1–100 visible characters.");
        return name.Trim();
    }

    internal static void Id(string id)
    {
        if (id is null || id.Length != 32 || !Guid.TryParseExact(id, "N", out _))
            throw new CatalogException("invalid_id", "An installation ID must be a 32-character catalog identifier.");
    }

    internal static bool IsSha256(string value) => value is { Length: 64 } && value.All(Uri.IsHexDigit);
}
