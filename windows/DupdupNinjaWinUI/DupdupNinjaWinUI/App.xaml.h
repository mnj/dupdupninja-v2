#pragma once

#include "App.xaml.g.h"

namespace winrt::DupdupNinjaWinUI::implementation {
struct App : AppT<App> {
    App();

    void OnLaunched(Microsoft::UI::Xaml::LaunchActivatedEventArgs const& args);
};
}

namespace winrt::DupdupNinjaWinUI::factory_implementation {
struct App : AppT<App, implementation::App> {};
}
