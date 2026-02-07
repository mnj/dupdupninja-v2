using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;

namespace DupdupNinjaWinUI.Services;

public sealed class OpenFilesetsStore
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    private readonly string _statePath;

    public OpenFilesetsStore()
    {
        _statePath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "dupdupninja",
            "open-filesets.json");
    }

    public async Task<IReadOnlyList<string>> LoadAsync()
    {
        if (!File.Exists(_statePath))
        {
            return Array.Empty<string>();
        }

        try
        {
            var json = await File.ReadAllTextAsync(_statePath).ConfigureAwait(false);
            var paths = JsonSerializer.Deserialize<List<string>>(json, JsonOptions) ?? new List<string>();
            return paths
                .Where(p => !string.IsNullOrWhiteSpace(p))
                .Select(Path.GetFullPath)
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .Where(File.Exists)
                .ToArray();
        }
        catch
        {
            return Array.Empty<string>();
        }
    }

    public async Task SaveAsync(IEnumerable<string> paths)
    {
        var normalized = paths
            .Where(p => !string.IsNullOrWhiteSpace(p))
            .Select(Path.GetFullPath)
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToArray();

        Directory.CreateDirectory(Path.GetDirectoryName(_statePath)!);
        var json = JsonSerializer.Serialize(normalized, JsonOptions);
        await File.WriteAllTextAsync(_statePath, json).ConfigureAwait(false);
    }
}
