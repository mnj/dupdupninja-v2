using System;
using System.IO;

namespace DupdupNinjaWinUI.Services;

public sealed class AppSettings
{
    public bool CaptureSnapshots { get; set; } = true;

    public int SnapshotsPerVideo { get; set; } = 3;

    public int SnapshotMaxDim { get; set; } = 1024;

    public int SimilarPhashMaxDistance { get; set; } = 8;
    public int SimilarDhashMaxDistance { get; set; } = 12;
    public int SimilarAhashMaxDistance { get; set; } = 12;
    public bool ConcurrentScanProcessing { get; set; } = true;

    public string? DefaultFilesetFolder { get; set; }

    public static AppSettings CreateDefault() => new();

    public AppSettings Clone()
    {
        return new AppSettings
        {
            CaptureSnapshots = CaptureSnapshots,
            SnapshotsPerVideo = SnapshotsPerVideo,
            SnapshotMaxDim = SnapshotMaxDim,
            SimilarPhashMaxDistance = SimilarPhashMaxDistance,
            SimilarDhashMaxDistance = SimilarDhashMaxDistance,
            SimilarAhashMaxDistance = SimilarAhashMaxDistance,
            ConcurrentScanProcessing = ConcurrentScanProcessing,
            DefaultFilesetFolder = DefaultFilesetFolder,
        };
    }

    public void Normalize()
    {
        SnapshotsPerVideo = Math.Clamp(SnapshotsPerVideo, 1, 10);
        SimilarPhashMaxDistance = Math.Clamp(SimilarPhashMaxDistance, 1, 32);
        SimilarDhashMaxDistance = Math.Clamp(SimilarDhashMaxDistance, 1, 32);
        SimilarAhashMaxDistance = Math.Clamp(SimilarAhashMaxDistance, 1, 32);
        SnapshotMaxDim = SnapshotMaxDim switch
        {
            <= 128 => 128,
            <= 256 => 256,
            <= 512 => 512,
            <= 768 => 768,
            <= 1024 => 1024,
            <= 1536 => 1536,
            _ => 2048,
        };

        if (string.IsNullOrWhiteSpace(DefaultFilesetFolder))
        {
            DefaultFilesetFolder = null;
        }
        else
        {
            DefaultFilesetFolder = Path.GetFullPath(DefaultFilesetFolder.Trim());
        }
    }

    public string EffectiveFilesetFolder()
    {
        if (!string.IsNullOrWhiteSpace(DefaultFilesetFolder))
        {
            return DefaultFilesetFolder!;
        }

        return Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "dupdupninja",
            "filesets");
    }
}
