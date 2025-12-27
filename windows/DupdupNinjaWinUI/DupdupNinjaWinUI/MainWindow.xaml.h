#pragma once

#include "MainWindow.xaml.g.h"

namespace winrt::DupdupNinjaWinUI::implementation {
struct MainWindow : MainWindowT<MainWindow> {
    MainWindow();
};
}

namespace winrt::DupdupNinjaWinUI::factory_implementation {
struct MainWindow : MainWindowT<MainWindow, implementation::MainWindow> {};
}
