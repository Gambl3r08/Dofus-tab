//! Win32 window backend.
//!
//! # Identifying the game
//!
//! Unlike X11, the Win32 window class is useless here: every Unity game reports
//! `UnityWndClass`. The executable name is the discriminator instead, so
//! [`GameWindow::app_class`] is filled with the process image name (`Dofus.exe`)
//! rather than the window class. That matches what X11 exposes through
//! `WM_CLASS`, which is itself derived from the executable, so the shared
//! filtering in `dofus-core` keeps working unchanged.

use std::ffi::c_void;

use dofus_core::{Error, GameWindow, Result, WindowId, WindowManager};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, TRUE};
use windows::Win32::System::Threading::{
    AttachThreadInput, OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, HWND_TOPMOST, IsIconic, IsWindowVisible, SW_RESTORE,
    SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowPos, ShowWindow, WS_EX_TOOLWINDOW,
};

/// Largest window title we bother reading.
const MAX_TITLE: usize = 512;

/// Window manager backend for Windows.
pub struct WindowsBackend;

impl WindowsBackend {
    /// Nothing to connect to on Windows; the API is always available.
    pub fn connect() -> Result<Self> {
        Ok(Self)
    }
}

/// Collects every top-level window handle. `lparam` is a `&mut Vec<HWND>`.
///
/// # Safety
///
/// Called by `EnumWindows` with the `lparam` we passed to it, which is always a
/// live `Vec<HWND>` pointer for the duration of the call.
unsafe extern "system" fn collect_window(handle: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let handles = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    handles.push(handle);
    // Keep enumerating.
    TRUE
}

/// Every top-level window, visible or not.
fn top_level_windows() -> Result<Vec<HWND>> {
    let mut handles: Vec<HWND> = Vec::new();
    let lparam = LPARAM(&mut handles as *mut Vec<HWND> as isize);

    unsafe { EnumWindows(Some(collect_window), lparam) }
        .map_err(|error| Error::Backend(format!("EnumWindows failed: {error}")))?;

    Ok(handles)
}

/// Reads a window title, returning an empty string when it has none.
fn window_title(handle: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(handle) };
    if length <= 0 {
        return String::new();
    }

    // +1 for the terminator GetWindowTextW writes.
    let mut buffer = vec![0u16; (length as usize + 1).min(MAX_TITLE)];
    let written = unsafe { GetWindowTextW(handle, &mut buffer) };
    if written <= 0 {
        return String::new();
    }

    String::from_utf16_lossy(&buffer[..written as usize])
}

/// Reads the executable file name of a process, e.g. `Dofus.exe`.
///
/// Uses the limited-information access right so it works without elevation.
fn process_image_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buffer = vec![0u16; MAX_PATH as usize];
    let mut size = buffer.len() as u32;
    let query = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    // The handle must be released whether or not the query succeeded.
    let _ = unsafe { CloseHandle(process) };
    query.ok()?;

    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    // Keep only the file name; the full path adds nothing for filtering.
    path.rsplit(['\\', '/']).next().map(str::to_owned)
}

/// Owning process of a window.
fn window_pid(handle: HWND) -> Option<u32> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(handle, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

/// Thread that owns a window, used for the focus handover.
fn window_thread(handle: HWND) -> u32 {
    unsafe { GetWindowThreadProcessId(handle, None) }
}

fn to_hwnd(id: WindowId) -> HWND {
    HWND(id.0 as *mut c_void)
}

fn to_window_id(handle: HWND) -> WindowId {
    WindowId(handle.0 as u64)
}

impl WindowManager for WindowsBackend {
    fn backend_name(&self) -> &'static str {
        "win32"
    }

    fn list_windows(&mut self) -> Result<Vec<GameWindow>> {
        Ok(top_level_windows()?
            .into_iter()
            // Hidden helper windows outnumber real ones by a wide margin.
            .filter(|handle| unsafe { IsWindowVisible(*handle) }.as_bool())
            .map(|handle| {
                let pid = window_pid(handle);
                GameWindow {
                    id: to_window_id(handle),
                    pid,
                    title: window_title(handle),
                    app_class: pid.and_then(process_image_name).unwrap_or_default(),
                }
            })
            .collect())
    }

    fn active_window(&mut self) -> Result<Option<WindowId>> {
        let handle = unsafe { GetForegroundWindow() };
        Ok((!handle.is_invalid()).then(|| to_window_id(handle)))
    }

    fn make_overlay(&mut self, id: WindowId) -> Result<()> {
        let handle = to_hwnd(id);

        unsafe {
            SetWindowPos(
                handle,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            )
        }
        .map_err(|error| Error::Backend(format!("SetWindowPos failed: {error}")))?;

        // A tool window is kept out of the taskbar and the Alt+Tab list, which
        // is the Win32 equivalent of _NET_WM_STATE_SKIP_TASKBAR.
        let style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) };
        if style != 0 {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                    handle,
                    GWL_EXSTYLE,
                    style | WS_EX_TOOLWINDOW.0 as isize,
                )
            };
        }

        Ok(())
    }

    fn focus(&mut self, id: WindowId) -> Result<()> {
        let target = to_hwnd(id);

        // A minimised window cannot take focus until it is restored.
        if unsafe { IsIconic(target) }.as_bool() {
            let _ = unsafe { ShowWindow(target, SW_RESTORE) };
        }

        // Windows only lets the foreground process hand focus over. Attaching
        // our input queue to the current foreground thread makes us count as
        // that process for the duration of the call, which is the long-standing
        // way to raise another application's window.
        let foreground = unsafe { GetForegroundWindow() };
        let foreground_thread = (!foreground.is_invalid()).then(|| window_thread(foreground));
        let target_thread = window_thread(target);

        if let Some(source) = foreground_thread
            && source != target_thread
        {
            let _ = unsafe { AttachThreadInput(target_thread, source, true) };
            let _ = unsafe { SetForegroundWindow(target) };
            let _ = unsafe { SetFocus(Some(target)) };
            let _ = unsafe { AttachThreadInput(target_thread, source, false) };
        } else {
            let _ = unsafe { SetForegroundWindow(target) };
        }

        Ok(())
    }
}
