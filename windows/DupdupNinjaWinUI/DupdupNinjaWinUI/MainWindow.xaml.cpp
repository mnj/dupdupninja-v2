#include "pch.h"
#include "MainWindow.xaml.h"

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;

namespace winrt::DupdupNinjaWinUI::implementation {
MainWindow::MainWindow() {
    InitializeComponent();
}

void MainWindow::Settings_Click(IInspectable const&, RoutedEventArgs const&) {
    ContentDialog dialog;
    dialog.Title(box_value(L"Settings"));
    dialog.Content(box_value(L"Settings are not implemented yet."));
    dialog.CloseButtonText(L"Close");
    dialog.XamlRoot(this->Content().XamlRoot());
    dialog.ShowAsync();
}

void MainWindow::About_Click(IInspectable const&, RoutedEventArgs const&) {
    ContentDialog dialog;
    dialog.Title(box_value(L"About dupdupninja"));
    dialog.Content(box_value(L"Cross-platform duplicate/near-duplicate media finder."));
    dialog.CloseButtonText(L"Close");
    dialog.XamlRoot(this->Content().XamlRoot());
    dialog.ShowAsync();
}

void MainWindow::Exit_Click(IInspectable const&, RoutedEventArgs const&) {
    Close();
}
}
