//! Windows 置顶窗口辅助。
//!
//! gpui-kit 0.9 的 `Window` 实现了 raw-window-handle 的 `HasWindowHandle`，
//! 可以直接拿到真实 HWND，不再需要按窗口标题 `FindWindowW` 去猜
//! （标题一变就失效，且可能命中别的进程同名窗口）。

/// 切换当前窗口的置顶状态，返回是否成功。
#[cfg(target_os = "windows")]
pub fn apply_topmost(window: &gpui_kit::Window, on: bool) -> bool {
    use raw_window_handle::RawWindowHandle;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    // 注意：`Window` 自己也有一个 `window_handle()`（返回 GPUI 的 AnyWindowHandle），
    // 因此这里必须用 UFCS 明确调用 raw-window-handle 的 trait 方法。
    let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
        return false;
    };
    let RawWindowHandle::Win32(raw) = handle.as_raw() else {
        return false;
    };
    // raw-window-handle 0.6 的 hwnd 是非零 isize
    let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
    let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
    unsafe {
        SetWindowPos(
            hwnd,
            after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .is_ok()
    }
}

/// 非 Windows 平台暂不支持。
#[cfg(not(target_os = "windows"))]
pub fn apply_topmost(_window: &gpui_kit::Window, _on: bool) -> bool {
    false
}
