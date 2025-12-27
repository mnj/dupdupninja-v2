using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Windows.Graphics;
using WinRT.Interop;

namespace App4;

public sealed partial class AboutWindow : Window
{
    public AboutWindow()
    {
        InitializeComponent();
        SetDefaultSize(420, 320);
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
