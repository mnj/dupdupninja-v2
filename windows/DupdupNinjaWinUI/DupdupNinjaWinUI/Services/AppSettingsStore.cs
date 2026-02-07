using System;
using System.IO;
using System.Text.Json;
using System.Threading.Tasks;

namespace DupdupNinjaWinUI.Services;

public sealed class AppSettingsStore
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    private readonly string _settingsPath;

    public AppSettingsStore()
    {
        _settingsPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "dupdupninja",
            "settings.json");
    }

    public async Task<AppSettings> LoadAsync()
    {
        if (!File.Exists(_settingsPath))
        {
            var defaults = AppSettings.CreateDefault();
            defaults.Normalize();
            return defaults;
        }

        try
        {
            var json = await File.ReadAllTextAsync(_settingsPath).ConfigureAwait(false);
            var loaded = JsonSerializer.Deserialize<AppSettings>(json, JsonOptions) ?? AppSettings.CreateDefault();
            loaded.Normalize();
            return loaded;
        }
        catch
        {
            var fallback = AppSettings.CreateDefault();
            fallback.Normalize();
            return fallback;
        }
    }

    public async Task SaveAsync(AppSettings settings)
    {
        settings.Normalize();
        Directory.CreateDirectory(Path.GetDirectoryName(_settingsPath)!);
        var json = JsonSerializer.Serialize(settings, JsonOptions);
        await File.WriteAllTextAsync(_settingsPath, json).ConfigureAwait(false);
    }
}
