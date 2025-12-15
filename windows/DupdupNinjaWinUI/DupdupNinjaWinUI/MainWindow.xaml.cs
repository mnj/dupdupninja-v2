using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace DupdupNinjaWinUI;

public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        TryMaximize();
    }

    private void Exit_Click(object sender, RoutedEventArgs e)
    {
        Close();
    }

    private async void ScanFolder_Click(object sender, RoutedEventArgs e)
    {
        var folder = await PickFolderAsync(PickerLocationId.DocumentsLibrary);
        if (folder is null)
        {
            return;
        }
        StatusText.Text = $"Folder scan path:\n{folder.Path}";
    }

    private async void ScanDisk_Click(object sender, RoutedEventArgs e)
    {
        var folder = await PickFolderAsync(PickerLocationId.ComputerFolder);
        if (folder is null)
        {
            return;
        }

        var root = Path.GetPathRoot(folder.Path) ?? folder.Path;
        var driveInfo = new DriveInfo(root);
        var volumeGuid = GetVolumeGuidForRoot(root);

        StatusText.Text =
            $"Disk scan path:\n{folder.Path}\n\n" +
            $"Drive root: {root}\n" +
            $"Volume GUID: {volumeGuid ?? "(unknown)"}\n" +
            $"FS type: {Safe(() => driveInfo.DriveFormat) ?? "(unknown)"}\n" +
            $"Label: {Safe(() => driveInfo.VolumeLabel) ?? "(unknown)"}";
    }

    private void TryMaximize()
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        var windowId = Microsoft.UI.Win32Interop.GetWindowIdFromWindow(hwnd);
        var appWindow = AppWindow.GetFromWindowId(windowId);
        if (appWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.Maximize();
        }
    }

    private async System.Threading.Tasks.Task<Windows.Storage.StorageFolder?> PickFolderAsync(PickerLocationId startLocation)
    {
        var picker = new FolderPicker
        {
            SuggestedStartLocation = startLocation
        };
        picker.FileTypeFilter.Add("*");

        var hwnd = WindowNative.GetWindowHandle(this);
        InitializeWithWindow.Initialize(picker, hwnd);

        return await picker.PickSingleFolderAsync();
    }

    private static string? GetVolumeGuidForRoot(string rootPath)
    {
        var root = rootPath;
        if (!root.EndsWith("\\", StringComparison.Ordinal))
        {
            root += "\\";
        }

        var sb = new StringBuilder(64);
        return GetVolumeNameForVolumeMountPoint(root, sb, (uint)sb.Capacity) ? sb.ToString() : null;
    }

    private static string? Safe(Func<string> fn)
    {
        try
        {
            return fn();
        }
        catch
        {
            return null;
        }
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool GetVolumeNameForVolumeMountPoint(
        string lpszVolumeMountPoint,
        StringBuilder lpszVolumeName,
        uint cchBufferLength
    );
}
