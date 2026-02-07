using System;
using System.IO;

namespace DupdupNinjaWinUI.Services;

public sealed class FilesetResultRow
{
    public bool IsGroupHeader { get; init; }

    public int GroupIndex { get; init; } = -1;

    public long Id { get; init; }

    public string IdDisplay => IsGroupHeader ? string.Empty : Id.ToString();

    public required string Path { get; init; }

    public required string DisplayName { get; init; }

    public required string DirectoryPath { get; init; }

    public ulong SizeBytes { get; init; }

    public required string SizeDisplay { get; init; }

    public required string FileType { get; init; }

    public required string Blake3 { get; init; }

    public required string Sha256 { get; init; }

    public required string MatchInfo { get; init; }

    public static FilesetResultRow Create(
        long id,
        string path,
        ulong sizeBytes,
        string fileType,
        string blake3,
        string sha256,
        int groupIndex = -1)
    {
        var displayName = System.IO.Path.GetFileName(path);
        if (string.IsNullOrWhiteSpace(displayName))
        {
            displayName = path;
        }

        return new FilesetResultRow
        {
            IsGroupHeader = false,
            GroupIndex = groupIndex,
            Id = id,
            Path = path,
            DisplayName = displayName,
            DirectoryPath = System.IO.Path.GetDirectoryName(path) ?? string.Empty,
            SizeBytes = sizeBytes,
            SizeDisplay = FormatBytes(sizeBytes),
            FileType = string.IsNullOrWhiteSpace(fileType) ? "(unknown)" : fileType,
            Blake3 = blake3,
            Sha256 = sha256,
            MatchInfo = string.Empty,
        };
    }

    public static FilesetResultRow CreateGroupHeader(string label, int count, int groupIndex)
    {
        return new FilesetResultRow
        {
            IsGroupHeader = true,
            GroupIndex = groupIndex,
            Id = 0,
            Path = string.Empty,
            DisplayName = $"{label} [{count}]",
            DirectoryPath = string.Empty,
            SizeBytes = 0,
            SizeDisplay = string.Empty,
            FileType = string.Empty,
            Blake3 = string.Empty,
            Sha256 = string.Empty,
            MatchInfo = string.Empty,
        };
    }

    public FilesetResultRow AsGroupedChild(int groupIndex)
    {
        if (IsGroupHeader)
        {
            return this;
        }

        return new FilesetResultRow
        {
            IsGroupHeader = false,
            GroupIndex = groupIndex,
            Id = Id,
            Path = Path,
            DisplayName = $"  - {DisplayName}",
            DirectoryPath = DirectoryPath,
            SizeBytes = SizeBytes,
            SizeDisplay = SizeDisplay,
            FileType = FileType,
            Blake3 = Blake3,
            Sha256 = Sha256,
            MatchInfo = MatchInfo,
        };
    }

    public FilesetResultRow WithMatchInfo(string matchInfo)
    {
        return new FilesetResultRow
        {
            IsGroupHeader = IsGroupHeader,
            GroupIndex = GroupIndex,
            Id = Id,
            Path = Path,
            DisplayName = DisplayName,
            DirectoryPath = DirectoryPath,
            SizeBytes = SizeBytes,
            SizeDisplay = SizeDisplay,
            FileType = FileType,
            Blake3 = Blake3,
            Sha256 = Sha256,
            MatchInfo = matchInfo,
        };
    }

    public static FilesetResultRow CreateSimilar(
        long id,
        string path,
        ulong sizeBytes,
        string fileType,
        string blake3,
        string sha256,
        float confidencePercent,
        byte phashDistance,
        byte dhashDistance,
        byte ahashDistance)
    {
        var row = Create(id, path, sizeBytes, fileType, blake3, sha256);
        return new FilesetResultRow
        {
            IsGroupHeader = row.IsGroupHeader,
            GroupIndex = row.GroupIndex,
            Id = row.Id,
            Path = row.Path,
            DisplayName = row.DisplayName,
            DirectoryPath = row.DirectoryPath,
            SizeBytes = row.SizeBytes,
            SizeDisplay = row.SizeDisplay,
            FileType = row.FileType,
            Blake3 = row.Blake3,
            Sha256 = row.Sha256,
            MatchInfo = $"{confidencePercent:0.00}% (p:{phashDistance}, d:{dhashDistance}, a:{ahashDistance})",
        };
    }

    private static string FormatBytes(ulong bytes)
    {
        string[] suffixes = ["B", "KB", "MB", "GB", "TB"];
        var value = (double)bytes;
        var idx = 0;
        while (value >= 1024 && idx < suffixes.Length - 1)
        {
            value /= 1024;
            idx++;
        }

        return $"{value:0.##} {suffixes[idx]}";
    }
}
