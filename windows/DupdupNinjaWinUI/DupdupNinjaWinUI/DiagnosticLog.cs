using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace DupdupNinjaWinUI;

internal static class DiagnosticLog
{
    private static readonly object Gate = new();
    private static readonly string LogPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "dupdupninja",
        "winui-startup.log");

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int MessageBoxW(IntPtr hWnd, string text, string caption, uint type);

    internal static string PathOnDisk => LogPath;

    internal static void Info(string message) => Write("INFO", message, null);

    internal static void Error(string message, Exception? ex = null) => Write("ERROR", message, ex);

    internal static void ShowFatalDialog(string title, string message)
    {
        try
        {
            MessageBoxW(IntPtr.Zero, message, title, 0x00000010);
        }
        catch
        {
            // Best effort only.
        }
    }

    private static void Write(string level, string message, Exception? ex)
    {
        try
        {
            lock (Gate)
            {
                Directory.CreateDirectory(System.IO.Path.GetDirectoryName(LogPath)!);
                using var writer = new StreamWriter(LogPath, append: true, Encoding.UTF8);
                writer.WriteLine($"[{DateTimeOffset.Now:yyyy-MM-dd HH:mm:ss.fff zzz}] {level} {message}");
                if (ex is not null)
                {
                    writer.WriteLine(ex.ToString());
                }
            }
        }
        catch
        {
            // Never throw from diagnostics.
        }
    }
}

