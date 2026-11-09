fn main() {
    // Windows 可执行文件图标 + 版本信息。
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=写入 Windows 资源失败: {e}");
        }
    }
}