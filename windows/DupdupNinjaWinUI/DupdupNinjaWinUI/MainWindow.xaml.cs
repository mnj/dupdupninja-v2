using System;
using System.IO;
using System.Threading.Tasks;
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
        public MainWindow()
        {
            InitializeComponent();
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
            _ = await PickFolderAsync(PickerLocationId.DocumentsLibrary, "Scan Folder");
        }

        private async void ScanDisk_Click(object sender, RoutedEventArgs e)
        {
            _ = await PickDriveAsync();
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
    }
}
