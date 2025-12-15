#[cfg(all(target_os = "windows", feature = "winui"))]
fn main() -> windows::core::Result<()> {
    app::run()
}

#[cfg(all(target_os = "windows", feature = "winui"))]
mod app {
    #![allow(unsafe_code)]

    use windows::core::w;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetMessageW, LoadCursorW, PostQuitMessage, RegisterClassW, SetMenu, ShowWindow,
        TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, MB_OK,
        MF_POPUP, MF_STRING, MSG, SW_MAXIMIZE, SW_SHOW, WM_COMMAND, WM_DESTROY, WM_NCCREATE,
        WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    const IDM_EXIT: usize = 1001;

    pub fn run() -> windows::core::Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            register_window_class(instance);

            let hwnd = CreateWindowExW(
                Default::default(),
                w!("DupdupMainWindow"),
                w!("dupdup"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1100,
                720,
                None,
                None,
                instance,
                None,
            );

            if hwnd.0 == 0 {
                windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    None,
                    w!("Failed to create window."),
                    w!("dupdup"),
                    MB_OK,
                );
                return Err(windows::core::Error::from_win32());
            }

            set_menu(hwnd);

            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_MAXIMIZE);
            SetFocus(hwnd);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND(0), 0, 0).into() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    unsafe fn register_window_class(instance: HINSTANCE) {
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW).ok(),
            lpszClassName: w!("DupdupMainWindow"),
            ..Default::default()
        };

        let _ = RegisterClassW(&wc);
    }

    unsafe fn set_menu(hwnd: HWND) {
        let menubar = CreateMenu();
        let file_menu = CreateMenu();
        let _ = AppendMenuW(file_menu, MF_STRING, IDM_EXIT, w!("E&xit"));
        let _ = AppendMenuW(menubar, MF_POPUP, file_menu.0 as usize, w!("&File"));
        SetMenu(hwnd, menubar);
    }

    extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_NCCREATE => DefWindowProcW(hwnd, msg, wparam, lparam),
                WM_COMMAND => {
                    let cmd = (wparam.0 & 0xffff) as usize;
                    if cmd == IDM_EXIT {
                        DestroyWindow(hwnd);
                        return LRESULT(0);
                    }
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }
}

#[cfg(not(all(target_os = "windows", feature = "winui")))]
fn main() {
    println!("dupdup-ui-windows stub. On Windows: build with `cargo run -p dupdup-ui-windows --features winui`.");
}
