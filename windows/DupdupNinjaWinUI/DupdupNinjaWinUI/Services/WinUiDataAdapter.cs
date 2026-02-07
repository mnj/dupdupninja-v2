using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace DupdupNinjaWinUI.Services;

public readonly record struct PrescanProgressUpdate(ulong FilesSeen, string CurrentPath);

public readonly record struct ScanProgressUpdate(ulong FilesSeen, ulong TotalFiles, string CurrentPath);

public readonly record struct ScanRunResult(bool Success, bool Cancelled, string? ErrorMessage);

public interface IWinUiDataAdapter : IDisposable
{
    bool IsScanning { get; }

    Task<ScanRunResult> ScanFolderAsync(
        string rootPath,
        string dbPath,
        AppSettings settings,
        Action<PrescanProgressUpdate>? onPrescan,
        Action<ScanProgressUpdate>? onProgress);

    Task<IReadOnlyList<FilesetResultRow>> ListFilesetRowsAsync(
        string dbPath,
        bool duplicatesOnly,
        int limit,
        int offset);

    Task<IReadOnlyList<FilesetExactGroup>> ListExactGroupsAsync(
        string dbPath,
        int limit,
        int offset);

    Task<IReadOnlyList<FilesetExactGroup>> ListSimilarGroupsAsync(
        string dbPath,
        int phashMaxDistance,
        int dhashMaxDistance,
        int ahashMaxDistance,
        int limit,
        int offset);

    Task<FilesetMetadataModel> GetFilesetMetadataAsync(string dbPath);

    Task SetFilesetMetadataAsync(string dbPath, FilesetMetadataModel metadata);

    Task RemoveFilesByPathAsync(string dbPath, IEnumerable<string> paths);

    Task<IReadOnlyList<SnapshotInfoModel>> ListSnapshotsByPathAsync(string dbPath, string filePath);

    void CancelScan();
}

public sealed class NativeWinUiDataAdapter : IWinUiDataAdapter
{
    private readonly object _gate = new();
    private IntPtr _engine;
    private IntPtr _cancelToken;
    private bool _isScanning;
    private Action<PrescanProgressUpdate>? _prescanHandler;
    private Action<ScanProgressUpdate>? _progressHandler;
    private NativeMethods.ProgressCallback? _progressCallback;
    private NativeMethods.PrescanCallback? _prescanCallback;

    public NativeWinUiDataAdapter()
    {
        _engine = NativeMethods.dupdupninja_engine_new();
    }

    public bool IsScanning
    {
        get
        {
            lock (_gate)
            {
                return _isScanning;
            }
        }
    }

    public async Task<ScanRunResult> ScanFolderAsync(
        string rootPath,
        string dbPath,
        AppSettings settings,
        Action<PrescanProgressUpdate>? onPrescan,
        Action<ScanProgressUpdate>? onProgress)
    {
        lock (_gate)
        {
            if (_isScanning)
            {
                return new ScanRunResult(false, false, "Scan already in progress.");
            }

            _isScanning = true;
            _prescanHandler = onPrescan;
            _progressHandler = onProgress;
        }

        try
        {
            if (_engine == IntPtr.Zero)
            {
                return new ScanRunResult(false, false, "Engine is not initialized.");
            }

            settings.Normalize();
            Directory.CreateDirectory(Path.GetDirectoryName(dbPath)!);

            if (_cancelToken != IntPtr.Zero)
            {
                NativeMethods.dupdupninja_cancel_token_free(_cancelToken);
                _cancelToken = IntPtr.Zero;
            }
            _cancelToken = NativeMethods.dupdupninja_cancel_token_new();

            _prescanCallback ??= OnPrescanProgress;
            _progressCallback ??= OnScanProgress;

            return await Task.Run(() =>
            {
                var totals = new NativeMethods.PrescanTotals();
                var prescanStatus = NativeMethods.dupdupninja_prescan_folder(
                    rootPath,
                    _cancelToken,
                    _prescanCallback,
                    IntPtr.Zero,
                    ref totals);

                if (prescanStatus != NativeMethods.DupdupStatus.Ok)
                {
                    var error = GetLastError() ?? "Prescan failed.";
                    return new ScanRunResult(false, IsCancelledError(error), error);
                }

                var options = new NativeMethods.ScanOptions
                {
                    CaptureSnapshots = settings.CaptureSnapshots,
                    SnapshotsPerVideo = (uint)settings.SnapshotsPerVideo,
                    SnapshotMaxDim = (uint)settings.SnapshotMaxDim,
                };

                var scanStatus = NativeMethods.dupdupninja_scan_folder_to_sqlite_with_progress_totals_and_options(
                    _engine,
                    rootPath,
                    dbPath,
                    _cancelToken,
                    totals.TotalFiles,
                    totals.TotalBytes,
                    ref options,
                    _progressCallback,
                    IntPtr.Zero);

                if (scanStatus != NativeMethods.DupdupStatus.Ok)
                {
                    var error = GetLastError() ?? "Scan failed.";
                    return new ScanRunResult(false, IsCancelledError(error), error);
                }

                return new ScanRunResult(true, false, null);
            }).ConfigureAwait(false);
        }
        finally
        {
            lock (_gate)
            {
                _isScanning = false;
                _prescanHandler = null;
                _progressHandler = null;
            }
        }
    }

    public void CancelScan()
    {
        if (_cancelToken != IntPtr.Zero)
        {
            NativeMethods.dupdupninja_cancel_token_cancel(_cancelToken);
        }
    }

    public async Task<IReadOnlyList<FilesetResultRow>> ListFilesetRowsAsync(
        string dbPath,
        bool duplicatesOnly,
        int limit,
        int offset)
    {
        return await Task.Run(() =>
        {
            var status = NativeMethods.dupdupninja_fileset_list_rows(
                dbPath,
                duplicatesOnly,
                (ulong)Math.Clamp(limit, 1, 10_000),
                (ulong)Math.Max(0, offset),
                out var rowsPtr,
                out var rowsLen);

            if (status != NativeMethods.DupdupStatus.Ok)
            {
                var error = GetLastError() ?? "Failed to load fileset rows.";
                throw new InvalidOperationException(error);
            }

            if (rowsPtr == IntPtr.Zero || rowsLen == 0)
            {
                return (IReadOnlyList<FilesetResultRow>)Array.Empty<FilesetResultRow>();
            }

            try
            {
                return ReadRows(rowsPtr, rowsLen);
            }
            finally
            {
                NativeMethods.dupdupninja_fileset_rows_free(rowsPtr, rowsLen);
            }
        }).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<FilesetExactGroup>> ListExactGroupsAsync(
        string dbPath,
        int limit,
        int offset)
    {
        return await Task.Run(() =>
        {
            var status = NativeMethods.dupdupninja_fileset_list_exact_groups(
                dbPath,
                (ulong)Math.Clamp(limit, 1, 10_000),
                (ulong)Math.Max(0, offset),
                out var groupsPtr,
                out var groupsLen,
                out var rowsPtr,
                out var rowsLen);

            if (status != NativeMethods.DupdupStatus.Ok)
            {
                var error = GetLastError() ?? "Failed to load exact groups.";
                throw new InvalidOperationException(error);
            }

            if (groupsPtr == IntPtr.Zero || groupsLen == 0 || rowsPtr == IntPtr.Zero || rowsLen == 0)
            {
                return (IReadOnlyList<FilesetExactGroup>)Array.Empty<FilesetExactGroup>();
            }

            try
            {
                var rows = ReadRows(rowsPtr, rowsLen);
                var groups = new List<FilesetExactGroup>((int)groupsLen);
                var groupSize = Marshal.SizeOf<NativeMethods.ExactGroup>();
                for (nuint i = 0; i < groupsLen; i++)
                {
                    var ptr = IntPtr.Add(groupsPtr, checked((int)(i * (nuint)groupSize)));
                    var group = Marshal.PtrToStructure<NativeMethods.ExactGroup>(ptr);
                    var label = Marshal.PtrToStringUTF8(group.Label) ?? "Exact group";
                    var start = checked((int)group.RowsStart);
                    var len = checked((int)group.RowsLen);
                    if (start < 0 || len <= 0 || start >= rows.Count)
                    {
                        continue;
                    }
                    var take = Math.Min(len, rows.Count - start);
                    groups.Add(new FilesetExactGroup
                    {
                        Label = label,
                        Rows = rows.GetRange(start, take),
                    });
                }

                return (IReadOnlyList<FilesetExactGroup>)groups;
            }
            finally
            {
                NativeMethods.dupdupninja_exact_groups_free(groupsPtr, groupsLen);
                NativeMethods.dupdupninja_fileset_rows_free(rowsPtr, rowsLen);
            }
        }).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<FilesetExactGroup>> ListSimilarGroupsAsync(
        string dbPath,
        int phashMaxDistance,
        int dhashMaxDistance,
        int ahashMaxDistance,
        int limit,
        int offset)
    {
        return await Task.Run(() =>
        {
            var status = NativeMethods.dupdupninja_fileset_list_similar_groups(
                dbPath,
                (ulong)Math.Clamp(limit, 1, 2_000),
                (ulong)Math.Max(0, offset),
                (byte)Math.Clamp(phashMaxDistance, 1, 32),
                (byte)Math.Clamp(dhashMaxDistance, 1, 32),
                (byte)Math.Clamp(ahashMaxDistance, 1, 32),
                out var groupsPtr,
                out var groupsLen,
                out var rowsPtr,
                out var rowsLen);

            if (status != NativeMethods.DupdupStatus.Ok)
            {
                var error = GetLastError() ?? "Failed to load similar groups.";
                throw new InvalidOperationException(error);
            }

            if (groupsPtr == IntPtr.Zero || groupsLen == 0 || rowsPtr == IntPtr.Zero || rowsLen == 0)
            {
                return (IReadOnlyList<FilesetExactGroup>)Array.Empty<FilesetExactGroup>();
            }

            try
            {
                var rows = ReadSimilarRows(rowsPtr, rowsLen);
                var groups = new List<FilesetExactGroup>((int)groupsLen);
                var groupSize = Marshal.SizeOf<NativeMethods.SimilarGroup>();
                for (nuint i = 0; i < groupsLen; i++)
                {
                    var ptr = IntPtr.Add(groupsPtr, checked((int)(i * (nuint)groupSize)));
                    var group = Marshal.PtrToStructure<NativeMethods.SimilarGroup>(ptr);
                    var label = Marshal.PtrToStringUTF8(group.Label) ?? "Similar group";
                    var start = checked((int)group.RowsStart);
                    var len = checked((int)group.RowsLen);
                    if (start < 0 || len <= 0 || start >= rows.Count)
                    {
                        continue;
                    }
                    var take = Math.Min(len, rows.Count - start);
                    groups.Add(new FilesetExactGroup
                    {
                        Label = label,
                        Rows = rows.GetRange(start, take),
                    });
                }

                return (IReadOnlyList<FilesetExactGroup>)groups;
            }
            finally
            {
                NativeMethods.dupdupninja_similar_groups_free(groupsPtr, groupsLen);
                NativeMethods.dupdupninja_similar_rows_free(rowsPtr, rowsLen);
            }
        }).ConfigureAwait(false);
    }

    public async Task<FilesetMetadataModel> GetFilesetMetadataAsync(string dbPath)
    {
        return await Task.Run(() =>
        {
            var status = NativeMethods.dupdupninja_fileset_get_metadata(dbPath, out var meta);
            if (status != NativeMethods.DupdupStatus.Ok)
            {
                var error = GetLastError() ?? "Failed to load fileset metadata.";
                throw new InvalidOperationException(error);
            }

            try
            {
                return new FilesetMetadataModel
                {
                    Name = Marshal.PtrToStringUTF8(meta.Name) ?? string.Empty,
                    Description = Marshal.PtrToStringUTF8(meta.Description) ?? string.Empty,
                    Notes = Marshal.PtrToStringUTF8(meta.Notes) ?? string.Empty,
                    Status = Marshal.PtrToStringUTF8(meta.Status) ?? string.Empty,
                };
            }
            finally
            {
                NativeMethods.dupdupninja_fileset_metadata_free(ref meta);
            }
        }).ConfigureAwait(false);
    }

    public async Task SetFilesetMetadataAsync(string dbPath, FilesetMetadataModel metadata)
    {
        await Task.Run(() =>
        {
            var status = NativeMethods.dupdupninja_fileset_set_metadata(
                dbPath,
                metadata.Name ?? string.Empty,
                metadata.Description ?? string.Empty,
                metadata.Notes ?? string.Empty,
                metadata.Status ?? string.Empty);
            if (status != NativeMethods.DupdupStatus.Ok)
            {
                var error = GetLastError() ?? "Failed to save fileset metadata.";
                throw new InvalidOperationException(error);
            }
        }).ConfigureAwait(false);
    }

    public async Task RemoveFilesByPathAsync(string dbPath, IEnumerable<string> paths)
    {
        await Task.Run(() =>
        {
            foreach (var path in paths)
            {
                if (string.IsNullOrWhiteSpace(path))
                {
                    continue;
                }
                var status = NativeMethods.dupdupninja_fileset_delete_file_by_path(dbPath, path);
                if (status != NativeMethods.DupdupStatus.Ok)
                {
                    var error = GetLastError() ?? $"Failed to remove {path} from fileset database.";
                    throw new InvalidOperationException(error);
                }
            }
        }).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<SnapshotInfoModel>> ListSnapshotsByPathAsync(string dbPath, string filePath)
    {
        return await Task.Run(() =>
        {
            var status = NativeMethods.dupdupninja_fileset_list_snapshots_by_path(
                dbPath,
                filePath,
                out var rowsPtr,
                out var rowsLen);
            if (status != NativeMethods.DupdupStatus.Ok)
            {
                var error = GetLastError() ?? "Failed to load snapshots.";
                throw new InvalidOperationException(error);
            }

            if (rowsPtr == IntPtr.Zero || rowsLen == 0)
            {
                return (IReadOnlyList<SnapshotInfoModel>)Array.Empty<SnapshotInfoModel>();
            }

            try
            {
                var result = new List<SnapshotInfoModel>((int)rowsLen);
                var rowSize = Marshal.SizeOf<NativeMethods.SnapshotInfo>();
                for (nuint i = 0; i < rowsLen; i++)
                {
                    var ptr = IntPtr.Add(rowsPtr, checked((int)(i * (nuint)rowSize)));
                    var row = Marshal.PtrToStructure<NativeMethods.SnapshotInfo>(ptr);
                    var at = TimeSpan.FromMilliseconds(Math.Max(0, row.AtMs));
                    var timeDisplay = $"{at:hh\\:mm\\:ss}";
                    var hashDisplay = $"p:{(row.HasPhash ? row.Phash.ToString("X16") : "-")} d:{(row.HasDhash ? row.Dhash.ToString("X16") : "-")} a:{(row.HasAhash ? row.Ahash.ToString("X16") : "-")}";
                    result.Add(new SnapshotInfoModel
                    {
                        SnapshotIndex = checked((int)row.SnapshotIndex),
                        SnapshotCount = checked((int)row.SnapshotCount),
                        AtMs = row.AtMs,
                        DurationMs = row.HasDuration ? row.DurationMs : null,
                        Ahash = row.HasAhash ? row.Ahash : null,
                        Dhash = row.HasDhash ? row.Dhash : null,
                        Phash = row.HasPhash ? row.Phash : null,
                        TimeDisplay = timeDisplay,
                        HashDisplay = hashDisplay,
                    });
                }
                return (IReadOnlyList<SnapshotInfoModel>)result;
            }
            finally
            {
                NativeMethods.dupdupninja_snapshots_info_free(rowsPtr, rowsLen);
            }
        }).ConfigureAwait(false);
    }

    public void Dispose()
    {
        if (_cancelToken != IntPtr.Zero)
        {
            NativeMethods.dupdupninja_cancel_token_free(_cancelToken);
            _cancelToken = IntPtr.Zero;
        }

        if (_engine != IntPtr.Zero)
        {
            NativeMethods.dupdupninja_engine_free(_engine);
            _engine = IntPtr.Zero;
        }
    }

    private void OnPrescanProgress(IntPtr progressPtr, IntPtr userData)
    {
        if (progressPtr == IntPtr.Zero)
        {
            return;
        }

        var progress = Marshal.PtrToStructure<NativeMethods.PrescanProgress>(progressPtr);
        var path = Marshal.PtrToStringUTF8(progress.CurrentPath) ?? string.Empty;
        _prescanHandler?.Invoke(new PrescanProgressUpdate(progress.FilesSeen, path));
    }

    private void OnScanProgress(IntPtr progressPtr, IntPtr userData)
    {
        if (progressPtr == IntPtr.Zero)
        {
            return;
        }

        var progress = Marshal.PtrToStructure<NativeMethods.ScanProgress>(progressPtr);
        var path = Marshal.PtrToStringUTF8(progress.CurrentPath) ?? string.Empty;
        _progressHandler?.Invoke(new ScanProgressUpdate(progress.FilesSeen, progress.TotalFiles, path));
    }

    private static string? GetLastError()
    {
        var ptr = NativeMethods.dupdupninja_last_error_message();
        return ptr == IntPtr.Zero ? null : Marshal.PtrToStringUTF8(ptr);
    }

    private static bool IsCancelledError(string message)
    {
        return message.Contains("cancel", StringComparison.OrdinalIgnoreCase);
    }

    private static List<FilesetResultRow> ReadRows(IntPtr rowsPtr, nuint rowsLen)
    {
        var result = new List<FilesetResultRow>((int)rowsLen);
        var rowSize = Marshal.SizeOf<NativeMethods.FilesetRow>();
        for (nuint i = 0; i < rowsLen; i++)
        {
            var ptr = IntPtr.Add(rowsPtr, checked((int)(i * (nuint)rowSize)));
            var row = Marshal.PtrToStructure<NativeMethods.FilesetRow>(ptr);

            var path = Marshal.PtrToStringUTF8(row.Path) ?? string.Empty;
            var fileType = Marshal.PtrToStringUTF8(row.FileType) ?? string.Empty;
            var blake3 = Marshal.PtrToStringUTF8(row.Blake3Hex) ?? string.Empty;
            var sha256 = Marshal.PtrToStringUTF8(row.Sha256Hex) ?? string.Empty;
            result.Add(FilesetResultRow.Create(row.Id, path, row.SizeBytes, fileType, blake3, sha256));
        }

        return result;
    }

    private static List<FilesetResultRow> ReadSimilarRows(IntPtr rowsPtr, nuint rowsLen)
    {
        var result = new List<FilesetResultRow>((int)rowsLen);
        var rowSize = Marshal.SizeOf<NativeMethods.SimilarRow>();
        for (nuint i = 0; i < rowsLen; i++)
        {
            var ptr = IntPtr.Add(rowsPtr, checked((int)(i * (nuint)rowSize)));
            var row = Marshal.PtrToStructure<NativeMethods.SimilarRow>(ptr);
            var path = Marshal.PtrToStringUTF8(row.Path) ?? string.Empty;
            var fileType = Marshal.PtrToStringUTF8(row.FileType) ?? string.Empty;
            var blake3 = Marshal.PtrToStringUTF8(row.Blake3Hex) ?? string.Empty;
            var sha256 = Marshal.PtrToStringUTF8(row.Sha256Hex) ?? string.Empty;
            result.Add(FilesetResultRow.CreateSimilar(
                row.Id,
                path,
                row.SizeBytes,
                fileType,
                blake3,
                sha256,
                row.ConfidencePercent,
                row.PhashDistance,
                row.DhashDistance,
                row.AhashDistance));
        }
        return result;
    }
}
