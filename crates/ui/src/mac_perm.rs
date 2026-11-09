//! macOS 屏幕录制权限。
//!
//! 从旧版 egui 入口迁移过来：只检查 (`CGPreflightScreenCaptureAccess`) 不会弹窗，
//! 必须主动调用 `CGRequestScreenCaptureAccess` 系统才会询问用户。
//! 详见 `PERMISSION_FIX.md` 的历史背景。

#[cfg(target_os = "macos")]
mod imp {
    const FRAMEWORK: &str =
        "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics";

    type PreflightFn = unsafe extern "C" fn() -> bool;
    type RequestFn = unsafe extern "C" fn() -> bool;

    /// 返回 (是否已授权, 系统是否支持该 API)。
    fn preflight() -> (bool, bool) {
        unsafe {
            match libloading::Library::new(FRAMEWORK) {
                Ok(lib) => match lib.get::<PreflightFn>(b"CGPreflightScreenCaptureAccess\0") {
                    Ok(f) => (f(), true),
                    Err(_) => (true, false), // 旧系统没有该符号，视为已授权
                },
                Err(_) => (true, false),
            }
        }
    }

    /// 主动请求权限（会触发系统弹窗）。
    fn request() -> bool {
        unsafe {
            match libloading::Library::new(FRAMEWORK) {
                Ok(lib) => match lib.get::<RequestFn>(b"CGRequestScreenCaptureAccess\0") {
                    Ok(f) => f(),
                    Err(_) => false,
                },
                Err(_) => false,
            }
        }
    }

    pub fn ensure() -> bool {
        let (granted, supported) = preflight();
        if !supported || granted {
            return true;
        }
        crate::tlog("[PERM] 未授权屏幕录制, 正在请求");
        let _ = request();
        // 请求返回只代表「弹过窗」，用户可能点了拒绝，必须复查
        let (granted_after, _) = preflight();
        if granted_after {
            crate::tlog("[PERM] 屏幕录制权限已授予");
        } else {
            crate::tlog("[PERM] 仍未授权，请到 系统设置 → 隐私与安全性 → 屏幕录制 中开启");
        }
        granted_after
    }
}

/// 确保拥有屏幕录制权限；非 macOS 平台恒为 true。
pub fn ensure() -> bool {
    #[cfg(target_os = "macos")]
    {
        imp::ensure()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}