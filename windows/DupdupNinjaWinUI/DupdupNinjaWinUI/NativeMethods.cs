using System;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;

namespace DupdupNinjaWinUI;

internal static class NativeMethods
{
    private const string NativeLibraryName = "dupdupninja_ffi";

    static NativeMethods()
    {
        NativeLibrary.SetDllImportResolver(typeof(NativeMethods).Assembly, ResolveNativeLibrary);
    }

    private static IntPtr ResolveNativeLibrary(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (!string.Equals(libraryName, NativeLibraryName, StringComparison.Ordinal))
        {
            return IntPtr.Zero;
        }

        var runtimeFolder = RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.X64 => "win-x64",
            Architecture.X86 => "win-x86",
            Architecture.Arm64 => "win-arm64",
            _ => null
        };

        if (runtimeFolder is null)
        {
            return IntPtr.Zero;
        }

        string?[] candidatePaths =
        {
            Path.Combine(AppContext.BaseDirectory, "runtimes", runtimeFolder, "native", $"{NativeLibraryName}.dll"),
            Path.Combine(AppContext.BaseDirectory, $"{NativeLibraryName}.dll"),
            FindRuntimeLibraryInParents(runtimeFolder)
        };

        foreach (var candidate in candidatePaths)
        {
            if (candidate is null)
            {
                continue;
            }

            if (File.Exists(candidate))
            {
                return NativeLibrary.Load(candidate);
            }
        }

        return IntPtr.Zero;
    }

    private static string? FindRuntimeLibraryInParents(string runtimeFolder)
    {
        var currentDirectory = Directory.GetParent(AppContext.BaseDirectory);

        while (currentDirectory is not null)
        {
            var candidate = Path.Combine(currentDirectory.FullName, "runtimes", runtimeFolder, "native", $"{NativeLibraryName}.dll");
            if (File.Exists(candidate))
            {
                return candidate;
            }

            currentDirectory = currentDirectory.Parent;
        }

        return null;
    }

    internal enum DupdupStatus
    {
        Ok = 0,
        Error = 1,
        InvalidArgument = 2,
        NullPointer = 3,
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ScanProgress
    {
        public ulong FilesSeen;
        public ulong FilesHashed;
        public ulong FilesSkipped;
        public ulong BytesSeen;
        public ulong TotalFiles;
        public ulong TotalBytes;
        public IntPtr CurrentPath;
        public IntPtr CurrentStep;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct PrescanTotals
    {
        public ulong TotalFiles;
        public ulong TotalBytes;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct PrescanProgress
    {
        public ulong FilesSeen;
        public ulong BytesSeen;
        public ulong DirsSeen;
        public IntPtr CurrentPath;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ScanOptions
    {
        [MarshalAs(UnmanagedType.I1)]
        public bool CaptureSnapshots;

        public uint SnapshotsPerVideo;

        public uint SnapshotMaxDim;

        [MarshalAs(UnmanagedType.I1)]
        public bool ConcurrentProcessing;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct FilesetRow
    {
        public long Id;
        public IntPtr Path;
        public ulong SizeBytes;
        public IntPtr FileType;
        public IntPtr Blake3Hex;
        public IntPtr Sha256Hex;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExactGroup
    {
        public IntPtr Label;
        public nuint RowsStart;
        public nuint RowsLen;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SimilarGroup
    {
        public IntPtr Label;
        public nuint RowsStart;
        public nuint RowsLen;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SimilarRow
    {
        public long Id;
        public IntPtr Path;
        public ulong SizeBytes;
        public IntPtr FileType;
        public IntPtr Blake3Hex;
        public IntPtr Sha256Hex;
        public byte PhashDistance;
        public byte DhashDistance;
        public byte AhashDistance;
        public float ConfidencePercent;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct FilesetMetadataView
    {
        public IntPtr Name;
        public IntPtr Description;
        public IntPtr Notes;
        public IntPtr Status;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SnapshotInfo
    {
        public uint SnapshotIndex;
        public uint SnapshotCount;
        public long AtMs;
        [MarshalAs(UnmanagedType.I1)]
        public bool HasDuration;
        public long DurationMs;
        [MarshalAs(UnmanagedType.I1)]
        public bool HasAhash;
        public ulong Ahash;
        [MarshalAs(UnmanagedType.I1)]
        public bool HasDhash;
        public ulong Dhash;
        [MarshalAs(UnmanagedType.I1)]
        public bool HasPhash;
        public ulong Phash;
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void ProgressCallback(IntPtr progress, IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void PrescanCallback(IntPtr progress, IntPtr userData);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr dupdupninja_engine_new();

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_engine_free(IntPtr engine);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr dupdupninja_cancel_token_new();

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_cancel_token_free(IntPtr token);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_cancel_token_cancel(IntPtr token);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr dupdupninja_last_error_message();

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_prescan_folder(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootPath,
        IntPtr cancelToken,
        PrescanCallback progressCb,
        IntPtr userData,
        ref PrescanTotals totals);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_scan_folder_to_sqlite_with_progress_totals_and_options(
        IntPtr engine,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rootPath,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        IntPtr cancelToken,
        ulong totalFiles,
        ulong totalBytes,
        ref ScanOptions options,
        ProgressCallback progressCb,
        IntPtr userData);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_fileset_list_rows(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        [MarshalAs(UnmanagedType.I1)] bool duplicatesOnly,
        ulong limit,
        ulong offset,
        out IntPtr outRows,
        out nuint outLen);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_fileset_list_similar_groups(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        ulong limit,
        ulong offset,
        byte phashMaxDistance,
        byte dhashMaxDistance,
        byte ahashMaxDistance,
        out IntPtr outGroups,
        out nuint outGroupsLen,
        out IntPtr outRows,
        out nuint outRowsLen);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_fileset_rows_free(IntPtr rows, nuint len);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_fileset_list_exact_groups(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        ulong limit,
        ulong offset,
        out IntPtr outGroups,
        out nuint outGroupsLen,
        out IntPtr outRows,
        out nuint outRowsLen);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_exact_groups_free(IntPtr groups, nuint len);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_similar_rows_free(IntPtr rows, nuint len);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_similar_groups_free(IntPtr groups, nuint len);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_fileset_get_metadata(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        out FilesetMetadataView outMeta);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_fileset_set_metadata(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string name,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string description,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string notes,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string status);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_fileset_metadata_free(ref FilesetMetadataView meta);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_fileset_delete_file_by_path(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string filePath);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern DupdupStatus dupdupninja_fileset_list_snapshots_by_path(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string filePath,
        out IntPtr outRows,
        out nuint outLen);

    [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void dupdupninja_snapshots_info_free(IntPtr rows, nuint len);
}
