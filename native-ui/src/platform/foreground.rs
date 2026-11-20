//! Foreground raise for the single-instance hand-off.
//!
//! A second launch exits after signalling the running instance, so that
//! instance must bring itself forward. `ViewportCommand::Focus` alone is a
//! request the window manager may ignore when the caller is not the foreground
//! process (the shortcut click belongs to explorer.exe), so the running process
//! restores and raises its own window through Win32 directly.

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    type Hwnd = isize;

    const SW_RESTORE: i32 = 9;
    const FLASHW_ALL: u32 = 0x3;
    const FLASHW_TIMERNOFG: u32 = 0xC;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> Hwnd;
        fn ShowWindow(hwnd: Hwnd, command: i32) -> i32;
        fn SetForegroundWindow(hwnd: Hwnd) -> i32;
        fn IsIconic(hwnd: Hwnd) -> i32;
        fn FlashWindowEx(info: *const FlashInfo) -> i32;
    }

    #[repr(C)]
    struct FlashInfo {
        size: u32,
        hwnd: Hwnd,
        flags: u32,
        count: u32,
        timeout: u32,
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Restores and raises the window whose title starts with `title_prefix`.
    /// Returns true when a matching window was found.
    pub fn raise_window(title_prefix: &str) -> bool {
        // eframe titles the viewport with the app title; find it by prefix by
        // enumerating through FindWindowW with the exact title first.
        let exact = wide(title_prefix);
        // SAFETY: both pointers are valid NUL-terminated UTF-16 buffers that
        // outlive the call; the class filter (null) is accepted by user32.
        let mut hwnd = unsafe { FindWindowW(std::ptr::null(), exact.as_ptr()) };
        if hwnd == 0 {
            // Fall back to the class-less search used by the shell: the window
            // class is eframe's, so match on the title we set in main.rs.
            for candidate in [
                "Prompt Studio (native \u{b7} port in progress)",
                "Prompt Studio",
            ] {
                let wide_title = wide(candidate);
                // SAFETY: same contract as above.
                hwnd = unsafe { FindWindowW(std::ptr::null(), wide_title.as_ptr()) };
                if hwnd != 0 {
                    break;
                }
            }
        }
        if hwnd == 0 {
            return false;
        }
        // SAFETY: `hwnd` came from a successful FindWindowW call.
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            if SetForegroundWindow(hwnd) == 0 {
                // Foreground was refused; flash the taskbar entry so the launch
                // is still visible instead of silently doing nothing.
                let info = FlashInfo {
                    size: std::mem::size_of::<FlashInfo>() as u32,
                    hwnd,
                    flags: FLASHW_ALL | FLASHW_TIMERNOFG,
                    count: 4,
                    timeout: 0,
                };
                FlashWindowEx(&info);
            }
        }
        true
    }
}

#[cfg(windows)]
pub use imp::raise_window;

#[cfg(not(windows))]
pub fn raise_window(_title_prefix: &str) -> bool {
    false
}
