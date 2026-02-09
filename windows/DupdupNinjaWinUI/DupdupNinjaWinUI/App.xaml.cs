using System;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace DupdupNinjaWinUI
{
    /// <summary>
    /// Provides application-specific behavior to supplement the default Application class.
    /// </summary>
    public partial class App : Application
    {
        private Window? _window;

        /// <summary>
        /// Initializes the singleton application object.  This is the first line of authored code
        /// executed, and as such is the logical equivalent of main() or WinMain().
        /// </summary>
        public App()
        {
            DiagnosticLog.Info("App ctor start");
            AppDomain.CurrentDomain.UnhandledException += CurrentDomain_UnhandledException;
            TaskScheduler.UnobservedTaskException += TaskScheduler_UnobservedTaskException;
            UnhandledException += App_UnhandledException;
            InitializeComponent();
            DiagnosticLog.Info("App ctor complete");
        }

        /// <summary>
        /// Invoked when the application is launched.
        /// </summary>
        /// <param name="args">Details about the launch request and process.</param>
        protected override void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
        {
            DiagnosticLog.Info("OnLaunched start");
            try
            {
                _window = new MainWindow();
                _window.Activate();
                DiagnosticLog.Info("MainWindow activated");
            }
            catch (Exception ex)
            {
                DiagnosticLog.Error("Fatal error while launching MainWindow", ex);
                DiagnosticLog.ShowFatalDialog(
                    "DupdupNinja startup error",
                    "The app crashed during startup.\n\n" +
                    $"See log:\n{DiagnosticLog.PathOnDisk}\n\n" +
                    ex.Message);
                throw;
            }
        }

        private static void CurrentDomain_UnhandledException(object sender, UnhandledExceptionEventArgs e)
        {
            var ex = e.ExceptionObject as Exception;
            DiagnosticLog.Error($"AppDomain unhandled exception (terminating={e.IsTerminating})", ex);
        }

        private static void TaskScheduler_UnobservedTaskException(object? sender, UnobservedTaskExceptionEventArgs e)
        {
            DiagnosticLog.Error("Unobserved task exception", e.Exception);
        }

        private static void App_UnhandledException(object sender, Microsoft.UI.Xaml.UnhandledExceptionEventArgs e)
        {
            DiagnosticLog.Error("XAML unhandled exception", e.Exception);
        }
    }
}
