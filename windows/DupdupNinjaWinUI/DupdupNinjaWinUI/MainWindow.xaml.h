#pragma once

#include "MainWindow.xaml.g.h"

namespace winrt::DupdupNinjaWinUI::implementation {
struct MainWindow : MainWindowT<MainWindow> {
    MainWindow();

    void Settings_Click(IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
    void About_Click(IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
    void Exit_Click(IInspectable const&, Microsoft::UI::Xaml::RoutedEventArgs const&);
};
}

namespace winrt::DupdupNinjaWinUI::factory_implementation {
struct MainWindow : MainWindowT<MainWindow, implementation::MainWindow> {};
}
