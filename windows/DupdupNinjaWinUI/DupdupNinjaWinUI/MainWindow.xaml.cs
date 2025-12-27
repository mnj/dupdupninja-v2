using System;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace DupdupNinjaWinUI
{
    /// <summary>
    /// An empty window that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class MainWindow : Window
    {
        private IntPtr _engine = IntPtr.Zero;
        private IntPtr _cancelToken = IntPtr.Zero;
        private bool _isScanning;
        private NativeMethods.ProgressCallback? _progressCallback;
        private NativeMethods.PrescanCallback? _prescanCallback;

        public MainWindow()
        {
            InitializeComponent();
            _engine = NativeMethods.dupdupninja_engine_new();
            Closed += OnClosed;
        }

        private void OpenSettings_Click(object sender, RoutedEventArgs e)
        {
            new SettingsWindow().Activate();
        }

        private void OpenAbout_Click(object sender, RoutedEventArgs e)
        {
            new AboutWindow().Activate();
        }

        private async void ScanFolder_Click(object sender, RoutedEventArgs e)
        {
            var folder = await PickFolderAsync(PickerLocationId.DocumentsLibrary, "Scan Folder");
            if (folder is not null)
            {
                await StartScanAsync(folder.Path);
            }
        }

        private async void ScanDisk_Click(object sender, RoutedEventArgs e)
        {
            var folder = await PickDriveAsync();
            if (folder is not null)
            {
                await StartScanAsync(folder.Path);
            }
        }

        private async Task<StorageFolder?> PickFolderAsync(PickerLocationId startLocation, string commitText)
        {
            var picker = new FolderPicker
            {
                SuggestedStartLocation = startLocation,
                CommitButtonText = commitText,
                ViewMode = PickerViewMode.List
            };
            picker.FileTypeFilter.Add("*");

            var hWnd = WindowNative.GetWindowHandle(this);
            InitializeWithWindow.Initialize(picker, hWnd);

            return await picker.PickSingleFolderAsync();
        }

        private async Task<StorageFolder?> PickDriveAsync()
        {
            while (true)
            {
                var folder = await PickFolderAsync(PickerLocationId.ComputerFolder, "Select Drive");
                if (folder is null)
                {
                    return null;
                }

                if (IsDriveRoot(folder.Path))
                {
                    return folder;
                }

                await ShowDriveSelectionWarningAsync(folder.Path);
            }
        }

        private async Task ShowDriveSelectionWarningAsync(string invalidPath)
        {
            var dialog = new ContentDialog
            {
                Title = "Select a drive",
                Content = $"{invalidPath} is not a drive or mount point. Please pick a drive root or network share.",
                CloseButtonText = "OK"
            };

            if (Content is FrameworkElement root)
            {
                dialog.XamlRoot = root.XamlRoot;
            }

            await dialog.ShowAsync();
        }

        private static bool IsDriveRoot(string? path)
        {
            if (string.IsNullOrWhiteSpace(path))
            {
                return false;
            }

            var normalizedPath = NormalizePath(path);
            var normalizedRoot = NormalizePath(Path.GetPathRoot(path));
            return string.Equals(normalizedPath, normalizedRoot, StringComparison.OrdinalIgnoreCase);
        }

        private static string NormalizePath(string? path)
        {
            if (string.IsNullOrEmpty(path))
            {
                return string.Empty;
            }

            return path.EndsWith(Path.DirectorySeparatorChar)
                ? path
                : path + Path.DirectorySeparatorChar;
        }

        private async Task StartScanAsync(string rootPath)
        {
            if (_isScanning)
            {
                await ShowInfoAsync("Scan in progress", "Please wait for the current scan to finish or cancel it.");
                return;
            }

            if (_engine == IntPtr.Zero)
            {
                await ShowInfoAsync("Scan error", "Engine is not initialized.");
                return;
            }

            _isScanning = true;
            StatusText.Text = "Status: Preparing scan...";
            ScanProgress.IsIndeterminate = true;
            ScanProgress.Value = 0;
            CancelButton.IsEnabled = true;

            if (_cancelToken != IntPtr.Zero)
            {
                NativeMethods.dupdupninja_cancel_token_free(_cancelToken);
                _cancelToken = IntPtr.Zero;
            }
            _cancelToken = NativeMethods.dupdupninja_cancel_token_new();

            _prescanCallback ??= OnPrescanProgress;
            _progressCallback ??= OnScanProgress;

            var dbPath = Path.Combine(Path.GetTempPath(), $"dupdupninja-scan-{DateTimeOffset.UtcNow.ToUnixTimeSeconds()}.sqlite3");

            await Task.Run(() =>
            {
                var totals = new NativeMethods.PrescanTotals();
                var prescanStatus = NativeMethods.dupdupninja_prescan_folder(
                    rootPath,
                    _cancelToken,
                    _prescanCallback,
                    IntPtr.Zero,
                    ref totals
                );

                if (prescanStatus != NativeMethods.DupdupStatus.Ok)
                {
                    var error = GetLastError() ?? "Prescan failed.";
                    EnqueueUi(() =>
                    {
                        StatusText.Text = $"Status: {error}";
                        ScanProgress.IsIndeterminate = false;
                        ScanProgress.Value = 0;
                        CancelButton.IsEnabled = false;
                    });
                    _isScanning = false;
                    return;
                }

                var scanStatus = NativeMethods.dupdupninja_scan_folder_to_sqlite_with_progress_and_totals(
                    _engine,
                    rootPath,
                    dbPath,
                    _cancelToken,
                    totals.TotalFiles,
                    totals.TotalBytes,
                    _progressCallback,
                    IntPtr.Zero
                );

                if (scanStatus != NativeMethods.DupdupStatus.Ok)
                {
                    var error = GetLastError() ?? "Scan failed.";
                    EnqueueUi(() =>
                    {
                        StatusText.Text = $"Status: {error}";
                        ScanProgress.IsIndeterminate = false;
                        ScanProgress.Value = 0;
                        CancelButton.IsEnabled = false;
                    });
                }
                else
                {
                    EnqueueUi(() =>
                    {
                        StatusText.Text = "Status: Scan complete";
                        ScanProgress.IsIndeterminate = false;
                        ScanProgress.Value = 1;
                        CancelButton.IsEnabled = false;
                    });
                }

                _isScanning = false;
            });
        }

        private void CancelScan_Click(object sender, RoutedEventArgs e)
        {
            if (_cancelToken != IntPtr.Zero)
            {
                NativeMethods.dupdupninja_cancel_token_cancel(_cancelToken);
                StatusText.Text = "Status: Cancelling...";
                CancelButton.IsEnabled = false;
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
            var folder = Path.GetFileName(path);
            var label = string.IsNullOrEmpty(folder) ? path : folder;

            EnqueueUi(() =>
            {
                StatusText.Text = $"Status: Preparing {label} ({progress.FilesSeen} files)";
                ScanProgress.IsIndeterminate = true;
            });
        }

        private void OnScanProgress(IntPtr progressPtr, IntPtr userData)
        {
            if (progressPtr == IntPtr.Zero)
            {
                return;
            }

            var progress = Marshal.PtrToStructure<NativeMethods.ScanProgress>(progressPtr);
            var path = Marshal.PtrToStringUTF8(progress.CurrentPath) ?? string.Empty;
            var folder = Path.GetFileName(Path.GetDirectoryName(path) ?? path);
            var label = string.IsNullOrEmpty(folder) ? path : folder;

            var fraction = progress.TotalFiles > 0 ? (double)progress.FilesSeen / progress.TotalFiles : 0;
            EnqueueUi(() =>
            {
                StatusText.Text = $"Status: Scanning {label} ({progress.FilesSeen}/{progress.TotalFiles})";
                ScanProgress.IsIndeterminate = false;
                ScanProgress.Value = Math.Clamp(fraction, 0, 1);
            });
        }

        private void EnqueueUi(DispatcherQueueHandler action)
        {
            _ = DispatcherQueue.TryEnqueue(action);
        }

        private static string? GetLastError()
        {
            var ptr = NativeMethods.dupdupninja_last_error_message();
            return ptr == IntPtr.Zero ? null : Marshal.PtrToStringUTF8(ptr);
        }

        private async Task ShowInfoAsync(string title, string message)
        {
            var dialog = new ContentDialog
            {
                Title = title,
                Content = message,
                CloseButtonText = "OK"
            };

            if (Content is FrameworkElement root)
            {
                dialog.XamlRoot = root.XamlRoot;
            }

            await dialog.ShowAsync();
        }

        private void OnClosed(object sender, WindowEventArgs args)
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

        private static class NativeMethods
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
                NullPointer = 3
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
            internal static extern DupdupStatus dupdupninja_scan_folder_to_sqlite_with_progress_and_totals(
                IntPtr engine,
                [MarshalAs(UnmanagedType.LPUTF8Str)] string rootPath,
                [MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath,
                IntPtr cancelToken,
                ulong totalFiles,
                ulong totalBytes,
                ProgressCallback progressCb,
                IntPtr userData);
        }
    }
}
