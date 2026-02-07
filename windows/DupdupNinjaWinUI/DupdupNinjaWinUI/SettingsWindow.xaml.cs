using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using DupdupNinjaWinUI.Services;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace DupdupNinjaWinUI;

public sealed partial class SettingsWindow : Window
{
    private readonly AppSettingsStore _settingsStore;
    private readonly AppSettings _working;

    public event EventHandler<AppSettings>? SettingsSaved;

    public SettingsWindow(AppSettingsStore settingsStore, AppSettings currentSettings)
    {
        InitializeComponent();
        SetDefaultSize(560, 640);

        _settingsStore = settingsStore;
        _working = currentSettings.Clone();
        _working.Normalize();

        LoadForm();
    }

    private void LoadForm()
    {
        CaptureSnapshotsToggle.IsOn = _working.CaptureSnapshots;
        ConcurrentScanToggle.IsOn = _working.ConcurrentScanProcessing;
        SnapshotsPerVideoBox.Value = _working.SnapshotsPerVideo;
        SelectSnapshotSize(_working.SnapshotMaxDim);
        SimilarPhashDistanceBox.Value = _working.SimilarPhashMaxDistance;
        SimilarDhashDistanceBox.Value = _working.SimilarDhashMaxDistance;
        SimilarAhashDistanceBox.Value = _working.SimilarAhashMaxDistance;

        DefaultFilesetFolderText.Text = _working.DefaultFilesetFolder ?? string.Empty;
        UpdateSnapshotInputsEnabled();
        UpdateEffectiveFolderText();
    }

    private void SelectSnapshotSize(int dim)
    {
        var item = SnapshotMaxDimCombo.Items
            .OfType<ComboBoxItem>()
            .FirstOrDefault(i => ParseDim(i.Tag as string) == dim);
        SnapshotMaxDimCombo.SelectedItem = item ?? SnapshotMaxDimCombo.Items[4];
    }

    private void CaptureSnapshotsToggle_Toggled(object sender, RoutedEventArgs e)
    {
        UpdateSnapshotInputsEnabled();
    }

    private void UpdateSnapshotInputsEnabled()
    {
        var enabled = CaptureSnapshotsToggle.IsOn;
        SnapshotsPerVideoBox.IsEnabled = enabled;
        SnapshotMaxDimCombo.IsEnabled = enabled;
    }

    private async void ChangeFilesetFolder_Click(object sender, RoutedEventArgs e)
    {
        var folder = await PickFolderAsync();
        if (folder is null)
        {
            return;
        }

        _working.DefaultFilesetFolder = folder.Path;
        _working.Normalize();
        DefaultFilesetFolderText.Text = _working.DefaultFilesetFolder ?? string.Empty;
        UpdateEffectiveFolderText();
    }

    private void ResetFilesetFolder_Click(object sender, RoutedEventArgs e)
    {
        _working.DefaultFilesetFolder = null;
        DefaultFilesetFolderText.Text = string.Empty;
        UpdateEffectiveFolderText();
    }

    private void Cancel_Click(object sender, RoutedEventArgs e)
    {
        Close();
    }

    private async void Save_Click(object sender, RoutedEventArgs e)
    {
        _working.CaptureSnapshots = CaptureSnapshotsToggle.IsOn;
        _working.ConcurrentScanProcessing = ConcurrentScanToggle.IsOn;
        _working.SnapshotsPerVideo = Math.Clamp((int)Math.Round(SnapshotsPerVideoBox.Value), 1, 10);

        var selectedItem = SnapshotMaxDimCombo.SelectedItem as ComboBoxItem;
        _working.SnapshotMaxDim = ParseDim(selectedItem?.Tag as string);
        _working.SimilarPhashMaxDistance = Math.Clamp((int)Math.Round(SimilarPhashDistanceBox.Value), 1, 32);
        _working.SimilarDhashMaxDistance = Math.Clamp((int)Math.Round(SimilarDhashDistanceBox.Value), 1, 32);
        _working.SimilarAhashMaxDistance = Math.Clamp((int)Math.Round(SimilarAhashDistanceBox.Value), 1, 32);

        var customFolder = DefaultFilesetFolderText.Text?.Trim();
        _working.DefaultFilesetFolder = string.IsNullOrWhiteSpace(customFolder) ? null : customFolder;
        _working.Normalize();

        try
        {
            await _settingsStore.SaveAsync(_working);
            SettingsSaved?.Invoke(this, _working.Clone());
            Close();
        }
        catch (Exception ex)
        {
            await ShowInfoAsync("Settings save failed", ex.Message);
        }
    }

    private static int ParseDim(string? value)
    {
        return int.TryParse(value, out var parsed) ? parsed : 1024;
    }

    private void UpdateEffectiveFolderText()
    {
        EffectiveFolderText.Text = $"Effective folder: {_working.EffectiveFilesetFolder()}";
    }

    private async Task<StorageFolder?> PickFolderAsync()
    {
        var picker = new FolderPicker
        {
            SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
            CommitButtonText = "Select Folder",
            ViewMode = PickerViewMode.List,
        };
        picker.FileTypeFilter.Add("*");

        var hWnd = WindowNative.GetWindowHandle(this);
        InitializeWithWindow.Initialize(picker, hWnd);

        return await picker.PickSingleFolderAsync();
    }

    private async Task ShowInfoAsync(string title, string message)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            Content = message,
            CloseButtonText = "OK",
        };

        if (Content is FrameworkElement root)
        {
            dialog.XamlRoot = root.XamlRoot;
        }

        await dialog.ShowAsync();
    }

    private void SetDefaultSize(int width, int height)
    {
        var hWnd = WindowNative.GetWindowHandle(this);
        var windowId = Win32Interop.GetWindowIdFromWindow(hWnd);
        if (AppWindow.GetFromWindowId(windowId) is AppWindow appWindow)
        {
            appWindow.Resize(new SizeInt32(width, height));
        }
    }
}
