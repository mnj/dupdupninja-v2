using System.IO;

namespace DupdupNinjaWinUI.Services;

public sealed class OpenFilesetItem
{
    public required string Path { get; init; }

    public string DisplayName
    {
        get
        {
            var name = System.IO.Path.GetFileName(Path);
            return string.IsNullOrWhiteSpace(name) ? Path : name;
        }
    }

    public string DirectoryName => Directory.GetParent(Path)?.FullName ?? string.Empty;
}
