namespace DupdupNinjaWinUI.Services;

public sealed class SnapshotInfoModel
{
    public int SnapshotIndex { get; init; }

    public int SnapshotCount { get; init; }

    public long AtMs { get; init; }

    public long? DurationMs { get; init; }

    public ulong? Ahash { get; init; }

    public ulong? Dhash { get; init; }

    public ulong? Phash { get; init; }

    public string TimeDisplay { get; init; } = string.Empty;

    public string HashDisplay { get; init; } = string.Empty;
}
