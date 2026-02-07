using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using DupdupNinjaWinUI.Services;
using Microsoft.VisualBasic.FileIO;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace DupdupNinjaWinUI;

public sealed partial class MainWindow : Window
{
    private const int ResultsPageSize = 5000;

    private readonly AppSettingsStore _settingsStore;
    private readonly OpenFilesetsStore _openFilesetsStore;
    private readonly IWinUiDataAdapter _dataAdapter;
    private readonly ObservableCollection<OpenFilesetItem> _openFilesets;
    private readonly ObservableCollection<FilesetResultRow> _resultRows;
    private readonly List<ExactGroupState> _exactGroups;
    private AppSettings _settings;
    private int _resultsLoadVersion;

    public MainWindow()
    {
        InitializeComponent();

        _settingsStore = new AppSettingsStore();
        _openFilesetsStore = new OpenFilesetsStore();
        _dataAdapter = new NativeWinUiDataAdapter();
        _openFilesets = new ObservableCollection<OpenFilesetItem>();
        _resultRows = new ObservableCollection<FilesetResultRow>();
        _exactGroups = new List<ExactGroupState>();
        _settings = AppSettings.CreateDefault();

        FilesetList.ItemsSource = _openFilesets;
        ResultsList.ItemsSource = _resultRows;
        DuplicatesOnlyCheckBox.IsChecked = false;
        MatchingModeCombo.SelectedIndex = 0;
        UpdateActionButtonsState();
        UpdateActiveFilesetUi();

        Activated += MainWindow_Activated;
        Closed += OnClosed;
    }

    private async void MainWindow_Activated(object sender, WindowActivatedEventArgs args)
    {
        Activated -= MainWindow_Activated;
        _settings = await _settingsStore.LoadAsync();

        var persisted = await _openFilesetsStore.LoadAsync();
        foreach (var path in persisted)
        {
            AddOrSelectFileset(path, persist: false, select: false);
        }

        if (_openFilesets.Count > 0)
        {
            FilesetList.SelectedIndex = 0;
        }

        UpdateActiveFilesetUi();
        await LoadActiveFilesetRowsAsync();
    }

    private void OpenSettings_Click(object sender, RoutedEventArgs e)
    {
        var settingsWindow = new SettingsWindow(_settingsStore, _settings);
        settingsWindow.SettingsSaved += OnSettingsSaved;
        settingsWindow.Activate();
    }

    private void OnSettingsSaved(object? sender, AppSettings settings)
    {
        _settings = settings;
    }

    private void OpenAbout_Click(object sender, RoutedEventArgs e)
    {
        new AboutWindow().Activate();
    }

    private async void OpenFileset_Click(object sender, RoutedEventArgs e)
    {
        var file = await PickFilesetAsync();
        if (file is null)
        {
            return;
        }

        AddOrSelectFileset(file.Path, persist: true, select: true);
        StatusText.Text = $"Status: Opened fileset {Path.GetFileName(file.Path)}";
    }

    private void CloseFileset_Click(object sender, RoutedEventArgs e)
    {
        var selected = FilesetList.SelectedItem as OpenFilesetItem;
        if (selected is null)
        {
            return;
        }

        var index = _openFilesets.IndexOf(selected);
        if (index < 0)
        {
            return;
        }

        _openFilesets.RemoveAt(index);

        if (_openFilesets.Count > 0)
        {
            FilesetList.SelectedIndex = Math.Clamp(index, 0, _openFilesets.Count - 1);
        }

        _ = PersistOpenFilesetsAsync();
        UpdateActiveFilesetUi();
        StatusText.Text = "Status: Fileset closed";
        _ = LoadActiveFilesetRowsAsync();
    }

    private async void FilesetProperties_Click(object sender, RoutedEventArgs e)
    {
        var dbPath = GetActiveFilesetPath();
        if (dbPath is null)
        {
            return;
        }

        try
        {
            var meta = await _dataAdapter.GetFilesetMetadataAsync(dbPath);
            await ShowFilesetPropertiesDialogAsync(dbPath, meta);
        }
        catch (Exception ex)
        {
            StatusText.Text = $"Status: {ex.Message}";
        }
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
            ViewMode = PickerViewMode.List,
        };
        picker.FileTypeFilter.Add("*");

        var hWnd = WindowNative.GetWindowHandle(this);
        InitializeWithWindow.Initialize(picker, hWnd);

        return await picker.PickSingleFolderAsync();
    }

    private async Task<StorageFile?> PickFilesetAsync()
    {
        var picker = new FileOpenPicker
        {
            SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
            ViewMode = PickerViewMode.List,
            CommitButtonText = "Open Fileset",
        };
        picker.FileTypeFilter.Add(".ddn");

        var hWnd = WindowNative.GetWindowHandle(this);
        InitializeWithWindow.Initialize(picker, hWnd);

        return await picker.PickSingleFileAsync();
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
            CloseButtonText = "OK",
        };

        if (Content is FrameworkElement root)
        {
            dialog.XamlRoot = root.XamlRoot;
        }

        await dialog.ShowAsync();
    }

    private async Task StartScanAsync(string rootPath)
    {
        if (_dataAdapter.IsScanning)
        {
            await ShowInfoAsync("Scan in progress", "Please wait for the current scan to finish or cancel it.");
            return;
        }

        _settings.Normalize();
        var dbPath = BuildFilesetPath(rootPath, _settings);

        StatusText.Text = "Status: Preparing scan...";
        ScanProgress.IsIndeterminate = true;
        ScanProgress.Value = 0;
        CancelButton.IsEnabled = true;

        var result = await _dataAdapter.ScanFolderAsync(
            rootPath,
            dbPath,
            _settings,
            onPrescan: update =>
            {
                var label = BuildProgressLabel(update.CurrentPath);
                EnqueueUi(() =>
                {
                    StatusText.Text = $"Status: Preparing {label} ({update.FilesSeen} files)";
                    ScanProgress.IsIndeterminate = true;
                });
            },
            onProgress: update =>
            {
                var label = BuildProgressLabel(update.CurrentPath);
                var fraction = update.TotalFiles > 0 ? (double)update.FilesSeen / update.TotalFiles : 0;
                EnqueueUi(() =>
                {
                    StatusText.Text = $"Status: Scanning {label} ({update.FilesSeen}/{update.TotalFiles})";
                    ScanProgress.IsIndeterminate = false;
                    ScanProgress.Value = Math.Clamp(fraction, 0, 1);
                });
            });

        if (result.Success)
        {
            AddOrSelectFileset(dbPath, persist: true, select: true);
            StatusText.Text = $"Status: Scan complete ({Path.GetFileName(dbPath)})";
            ScanProgress.IsIndeterminate = false;
            ScanProgress.Value = 1;
            CancelButton.IsEnabled = false;
            _ = LoadActiveFilesetRowsAsync();
            return;
        }

        StatusText.Text = result.Cancelled ? "Status: Scan cancelled" : $"Status: {result.ErrorMessage ?? "Scan failed."}";
        ScanProgress.IsIndeterminate = false;
        ScanProgress.Value = 0;
        CancelButton.IsEnabled = false;
    }

    private void CancelScan_Click(object sender, RoutedEventArgs e)
    {
        _dataAdapter.CancelScan();
        StatusText.Text = "Status: Cancelling...";
        CancelButton.IsEnabled = false;
    }

    private void FilesetList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        UpdateActiveFilesetUi();
        _ = LoadActiveFilesetRowsAsync();
    }

    private void DuplicatesOnlyCheckBox_Changed(object sender, RoutedEventArgs e)
    {
        _ = LoadActiveFilesetRowsAsync();
    }

    private void MatchingModeCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        var similarMode = IsSimilarMode();
        if (similarMode && (DuplicatesOnlyCheckBox.IsChecked ?? false))
        {
            DuplicatesOnlyCheckBox.IsChecked = false;
        }
        DuplicatesOnlyCheckBox.IsEnabled = !similarMode;
        _ = LoadActiveFilesetRowsAsync();
    }

    private void ResultsList_ItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not FilesetResultRow row || !row.IsGroupHeader || row.GroupIndex < 0)
        {
            return;
        }

        if (row.GroupIndex >= _exactGroups.Count)
        {
            return;
        }

        _exactGroups[row.GroupIndex].Collapsed = !_exactGroups[row.GroupIndex].Collapsed;
        RebuildExactGroupRows(IsSimilarMode() ? "similar" : "exact");
    }

    private void ResultsList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        UpdateActionButtonsState();
    }

    private async void RecycleSelected_Click(object sender, RoutedEventArgs e)
    {
        await ApplyFileActionAsync("Recycle", path =>
        {
            FileSystem.DeleteFile(path, UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin);
        });
    }

    private async void DeleteSelected_Click(object sender, RoutedEventArgs e)
    {
        await ApplyFileActionAsync("Delete", path => File.Delete(path));
    }

    private async void CopySelected_Click(object sender, RoutedEventArgs e)
    {
        var destination = await PickFolderAsync(PickerLocationId.DocumentsLibrary, "Copy To");
        if (destination is null)
        {
            return;
        }
        var selectedRows = GetSelectedDataRows();
        foreach (var row in selectedRows)
        {
            var destPath = Path.Combine(destination.Path, Path.GetFileName(row.Path));
            File.Copy(row.Path, destPath, overwrite: true);
        }
        StatusText.Text = $"Status: Copied {selectedRows.Count} file(s).";
    }

    private async void MoveSelected_Click(object sender, RoutedEventArgs e)
    {
        var destination = await PickFolderAsync(PickerLocationId.DocumentsLibrary, "Move To");
        if (destination is null)
        {
            return;
        }
        var dbPath = GetActiveFilesetPath();
        if (dbPath is null)
        {
            return;
        }

        var selectedRows = GetSelectedDataRows();
        var movedOriginalPaths = new List<string>();
        foreach (var row in selectedRows)
        {
            var destPath = Path.Combine(destination.Path, Path.GetFileName(row.Path));
            File.Move(row.Path, destPath, overwrite: true);
            movedOriginalPaths.Add(row.Path);
        }

        await _dataAdapter.RemoveFilesByPathAsync(dbPath, movedOriginalPaths);
        await LoadActiveFilesetRowsAsync();
        StatusText.Text = $"Status: Moved {movedOriginalPaths.Count} file(s).";
    }

    private async void HardlinkSelected_Click(object sender, RoutedEventArgs e)
    {
        var selectedRows = GetSelectedDataRows();
        if (selectedRows.Count < 2)
        {
            await ShowInfoAsync("Hard link", "Select at least two files.");
            return;
        }

        var source = selectedRows[0];
        var sourceRoot = Path.GetPathRoot(source.Path) ?? string.Empty;
        var dbPath = GetActiveFilesetPath();
        if (dbPath is null)
        {
            return;
        }

        var replaced = new List<string>();
        foreach (var row in selectedRows.Skip(1))
        {
            if (!string.Equals(Path.GetPathRoot(row.Path) ?? string.Empty, sourceRoot, StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }

            File.Delete(row.Path);
            File.CreateHardLink(row.Path, source.Path);
            replaced.Add(row.Path);
        }

        await _dataAdapter.RemoveFilesByPathAsync(dbPath, replaced);
        await LoadActiveFilesetRowsAsync();
        StatusText.Text = $"Status: Replaced {replaced.Count} file(s) with hard links.";
    }

    private async void CompareSelected_Click(object sender, RoutedEventArgs e)
    {
        var rows = GetSelectedDataRows();
        if (rows.Count < 2)
        {
            await ShowInfoAsync("Compare", "Select at least two files to compare.");
            return;
        }

        var a = rows[0];
        var b = rows[1];
        var dbPath = GetActiveFilesetPath();
        if (dbPath is null)
        {
            return;
        }

        IReadOnlyList<SnapshotInfoModel> snapshotsA;
        IReadOnlyList<SnapshotInfoModel> snapshotsB;
        try
        {
            snapshotsA = await _dataAdapter.ListSnapshotsByPathAsync(dbPath, a.Path);
            snapshotsB = await _dataAdapter.ListSnapshotsByPathAsync(dbPath, b.Path);
        }
        catch (Exception ex)
        {
            await ShowInfoAsync("Compare", ex.Message);
            return;
        }

        var panel = new Grid { ColumnSpacing = 16, RowSpacing = 10 };
        panel.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        panel.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        panel.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        panel.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        panel.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        panel.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var titleA = new TextBlock { Text = $"A: {a.DisplayName}", FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        var titleB = new TextBlock { Text = $"B: {b.DisplayName}", FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        Grid.SetColumn(titleA, 0);
        Grid.SetColumn(titleB, 1);
        panel.Children.Add(titleA);
        panel.Children.Add(titleB);

        var metaA = new TextBlock
        {
            Text = $"{a.Path}\nSize: {a.SizeDisplay}  Type: {a.FileType}\nSnapshots: {snapshotsA.Count}",
            TextWrapping = TextWrapping.WrapWholeWords,
        };
        var metaB = new TextBlock
        {
            Text = $"{b.Path}\nSize: {b.SizeDisplay}  Type: {b.FileType}\nSnapshots: {snapshotsB.Count}",
            TextWrapping = TextWrapping.WrapWholeWords,
        };
        Grid.SetRow(metaA, 1);
        Grid.SetRow(metaB, 1);
        Grid.SetColumn(metaA, 0);
        Grid.SetColumn(metaB, 1);
        panel.Children.Add(metaA);
        panel.Children.Add(metaB);

        var listA = new ListView
        {
            MaxHeight = 220,
            ItemsSource = snapshotsA.Select(s => $"#{s.SnapshotIndex + 1} @{s.TimeDisplay}  {s.HashDisplay}").ToList(),
        };
        var listB = new ListView
        {
            MaxHeight = 220,
            ItemsSource = snapshotsB.Select(s => $"#{s.SnapshotIndex + 1} @{s.TimeDisplay}  {s.HashDisplay}").ToList(),
        };
        Grid.SetRow(listA, 2);
        Grid.SetRow(listB, 2);
        Grid.SetColumn(listA, 0);
        Grid.SetColumn(listB, 1);
        panel.Children.Add(listA);
        panel.Children.Add(listB);

        var compareSummary = BuildSnapshotCompareSummary(snapshotsA, snapshotsB);
        var summary = new TextBlock
        {
            Text = compareSummary,
            TextWrapping = TextWrapping.WrapWholeWords,
            Opacity = 0.9,
        };
        Grid.SetRow(summary, 3);
        Grid.SetColumnSpan(summary, 2);
        panel.Children.Add(summary);

        var dialog = new ContentDialog
        {
            Title = "Compare Selected (with snapshots)",
            Content = panel,
            CloseButtonText = "Close",
        };
        if (Content is FrameworkElement root)
        {
            dialog.XamlRoot = root.XamlRoot;
        }
        await dialog.ShowAsync();
    }

    private void AddOrSelectFileset(string path, bool persist, bool select)
    {
        var normalized = Path.GetFullPath(path);
        if (!File.Exists(normalized))
        {
            return;
        }

        var existing = _openFilesets.FirstOrDefault(
            item => string.Equals(item.Path, normalized, StringComparison.OrdinalIgnoreCase));

        OpenFilesetItem selectedItem;
        if (existing is not null)
        {
            selectedItem = existing;
        }
        else
        {
            selectedItem = new OpenFilesetItem { Path = normalized };
            _openFilesets.Add(selectedItem);
        }

        if (select)
        {
            FilesetList.SelectedItem = selectedItem;
        }

        UpdateActiveFilesetUi();

        if (persist)
        {
            _ = PersistOpenFilesetsAsync();
        }
    }

    private async Task PersistOpenFilesetsAsync()
    {
        try
        {
            await _openFilesetsStore.SaveAsync(_openFilesets.Select(f => f.Path));
        }
        catch
        {
            // non-fatal; keep UI responsive even if persistence fails.
        }
    }

    private void UpdateActiveFilesetUi()
    {
        var selected = FilesetList.SelectedItem as OpenFilesetItem;

        if (selected is null)
        {
            ActiveFilesetText.Text = "No fileset selected";
            ResultsCountText.Text = "0 files";
            _resultRows.Clear();
            _exactGroups.Clear();
            return;
        }

        ActiveFilesetText.Text = $"Active fileset: {selected.DisplayName}";
    }

    private async Task LoadActiveFilesetRowsAsync()
    {
        var selected = FilesetList.SelectedItem as OpenFilesetItem;
        if (selected is null)
        {
            _resultRows.Clear();
            ResultsCountText.Text = "0 files";
            return;
        }

        var version = ++_resultsLoadVersion;
        ResultsCountText.Text = "Loading...";

        try
        {
            var similarMode = IsSimilarMode();
            var duplicatesOnly = !similarMode && (DuplicatesOnlyCheckBox.IsChecked ?? false);

            if (similarMode)
            {
                var groups = await _dataAdapter.ListSimilarGroupsAsync(
                    selected.Path,
                    _settings.SimilarPhashMaxDistance,
                    _settings.SimilarDhashMaxDistance,
                    _settings.SimilarAhashMaxDistance,
                    ResultsPageSize,
                    0);

                if (version != _resultsLoadVersion)
                {
                    return;
                }

                _exactGroups.Clear();
                foreach (var group in groups)
                {
                    _exactGroups.Add(new ExactGroupState(group));
                }
                RebuildExactGroupRows("similar");
                return;
            }

            if (duplicatesOnly)
            {
                var groups = await _dataAdapter.ListExactGroupsAsync(
                    selected.Path,
                    ResultsPageSize,
                    0);

                if (version != _resultsLoadVersion)
                {
                    return;
                }

                _exactGroups.Clear();
                foreach (var group in groups)
                {
                    _exactGroups.Add(new ExactGroupState(group));
                }
                RebuildExactGroupRows("exact");
            }
            else
            {
                var rows = await _dataAdapter.ListFilesetRowsAsync(
                    selected.Path,
                    false,
                    ResultsPageSize,
                    0);

                if (version != _resultsLoadVersion)
                {
                    return;
                }

                _exactGroups.Clear();
                _resultRows.Clear();
                foreach (var row in rows)
                {
                    _resultRows.Add(row);
                }

                ResultsCountText.Text = $"{_resultRows.Count} files";
            }
        }
        catch (Exception ex)
        {
            if (version != _resultsLoadVersion)
            {
                return;
            }

            _exactGroups.Clear();
            _resultRows.Clear();
            ResultsCountText.Text = "Load failed";
            StatusText.Text = $"Status: {ex.Message}";
        }
        finally
        {
            UpdateActionButtonsState();
        }
    }

    private void RebuildExactGroupRows(string mode = "exact")
    {
        _resultRows.Clear();
        var fileCount = 0;
        for (var i = 0; i < _exactGroups.Count; i++)
        {
            var group = _exactGroups[i];
            var indicator = group.Collapsed ? "\u25B6" : "\u25BC";
            _resultRows.Add(FilesetResultRow.CreateGroupHeader($"{indicator} {group.Group.Label}", group.Group.Rows.Count, i));
            if (group.Collapsed)
            {
                continue;
            }

            foreach (var row in group.Group.Rows)
            {
                var child = row.AsGroupedChild(i);
                if (mode == "exact")
                {
                    child = child.WithMatchInfo("100.00%");
                }
                _resultRows.Add(child);
                fileCount++;
            }
        }

        var prefix = mode == "similar" ? "similar " : string.Empty;
        ResultsCountText.Text = $"{_exactGroups.Count} {prefix}groups / {fileCount} files";
    }

    private static string BuildProgressLabel(string path)
    {
        var folder = Path.GetFileName(Path.GetDirectoryName(path) ?? path);
        return string.IsNullOrEmpty(folder) ? path : folder;
    }

    private static string BuildFilesetPath(string rootPath, AppSettings settings)
    {
        var baseFolder = settings.EffectiveFilesetFolder();
        Directory.CreateDirectory(baseFolder);

        var rootName = Path.GetFileName(rootPath.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar));
        if (string.IsNullOrWhiteSpace(rootName))
        {
            rootName = "fileset";
        }

        foreach (var c in Path.GetInvalidFileNameChars())
        {
            rootName = rootName.Replace(c, '_');
        }

        var stamp = DateTimeOffset.UtcNow.ToString("yyyyMMdd-HHmmss");
        return Path.Combine(baseFolder, $"{rootName}-{stamp}.ddn");
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

    private void EnqueueUi(DispatcherQueueHandler action)
    {
        _ = DispatcherQueue.TryEnqueue(action);
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

    private async Task ShowFilesetPropertiesDialogAsync(string dbPath, FilesetMetadataModel metadata)
    {
        var nameBox = new TextBox { Text = metadata.Name, Header = "Name" };
        var descBox = new TextBox { Text = metadata.Description, Header = "Description" };
        var statusBox = new TextBox { Text = metadata.Status, Header = "Status" };
        var notesBox = new TextBox
        {
            Text = metadata.Notes,
            Header = "Notes",
            AcceptsReturn = true,
            Height = 120,
            TextWrapping = TextWrapping.Wrap,
        };

        var panel = new StackPanel { Spacing = 10 };
        panel.Children.Add(nameBox);
        panel.Children.Add(descBox);
        panel.Children.Add(statusBox);
        panel.Children.Add(notesBox);

        var dialog = new ContentDialog
        {
            Title = "Fileset Properties",
            Content = panel,
            PrimaryButtonText = "Save",
            CloseButtonText = "Cancel",
        };
        if (Content is FrameworkElement root)
        {
            dialog.XamlRoot = root.XamlRoot;
        }

        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary)
        {
            return;
        }

        metadata.Name = nameBox.Text?.Trim() ?? string.Empty;
        metadata.Description = descBox.Text?.Trim() ?? string.Empty;
        metadata.Status = statusBox.Text?.Trim() ?? string.Empty;
        metadata.Notes = notesBox.Text?.Trim() ?? string.Empty;
        await _dataAdapter.SetFilesetMetadataAsync(dbPath, metadata);
        StatusText.Text = "Status: Fileset properties saved.";
    }

    private void OnClosed(object sender, WindowEventArgs args)
    {
        try
        {
            PersistOpenFilesetsAsync().GetAwaiter().GetResult();
        }
        catch
        {
            // best effort only
        }

        _dataAdapter.Dispose();
    }

    private bool IsSimilarMode()
    {
        return MatchingModeCombo.SelectedIndex == 1;
    }

    private string? GetActiveFilesetPath()
    {
        return (FilesetList.SelectedItem as OpenFilesetItem)?.Path;
    }

    private List<FilesetResultRow> GetSelectedDataRows()
    {
        return ResultsList.SelectedItems
            .OfType<FilesetResultRow>()
            .Where(r => !r.IsGroupHeader && !string.IsNullOrWhiteSpace(r.Path))
            .ToList();
    }

    private async Task ApplyFileActionAsync(string actionName, Action<string> action)
    {
        var selectedRows = GetSelectedDataRows();
        if (selectedRows.Count == 0)
        {
            return;
        }
        var dbPath = GetActiveFilesetPath();
        if (dbPath is null)
        {
            return;
        }

        var deletedPaths = new List<string>();
        foreach (var row in selectedRows)
        {
            action(row.Path);
            deletedPaths.Add(row.Path);
        }

        await _dataAdapter.RemoveFilesByPathAsync(dbPath, deletedPaths);
        await LoadActiveFilesetRowsAsync();
        StatusText.Text = $"Status: {actionName} complete ({deletedPaths.Count} file(s)).";
    }

    private void UpdateActionButtonsState()
    {
        var selected = GetSelectedDataRows();
        var hasAny = selected.Count > 0;
        RecycleButton.IsEnabled = hasAny;
        DeleteButton.IsEnabled = hasAny;
        CopyButton.IsEnabled = hasAny;
        MoveButton.IsEnabled = hasAny;
        CompareButton.IsEnabled = selected.Count >= 2;
        HardlinkButton.IsEnabled = selected.Count >= 2;
    }

    private static string BuildSnapshotCompareSummary(
        IReadOnlyList<SnapshotInfoModel> a,
        IReadOnlyList<SnapshotInfoModel> b)
    {
        var pairCount = Math.Min(a.Count, b.Count);
        if (pairCount == 0)
        {
            return "No overlapping snapshots to compare.";
        }

        var scored = 0;
        var confidenceSum = 0.0f;
        for (var i = 0; i < pairCount; i++)
        {
            if (a[i].Phash is not ulong pa || b[i].Phash is not ulong pb)
            {
                continue;
            }
            var dist = Hamming64(pa, pb);
            var confidence = Math.Min(99.99f, ((64 - dist) / 64f) * 100f);
            confidenceSum += confidence;
            scored++;
        }

        if (scored == 0)
        {
            return $"Compared {pairCount} snapshot slot(s), but pHash values were unavailable.";
        }

        var avg = confidenceSum / scored;
        return $"Compared {pairCount} snapshot slot(s), scored {scored} pHash pair(s). Average snapshot confidence: {avg:0.00}%";
    }

    private static int Hamming64(ulong a, ulong b)
    {
        return System.Numerics.BitOperations.PopCount(a ^ b);
    }

    private sealed class ExactGroupState
    {
        public ExactGroupState(FilesetExactGroup group)
        {
            Group = group;
        }

        public FilesetExactGroup Group { get; }

        public bool Collapsed { get; set; }
    }
}
