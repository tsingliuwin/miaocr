//! Tesseract 传统 OCR 引擎封装。

use anyhow::Result;

use crate::preprocess::save_temp_png;

/// 解析 Tesseract 可执行文件路径（PATH → 默认安装目录）。
pub fn get_tesseract_path() -> String {
    // 方式 1：PATH 查找
    let in_path = {
        let mut cmd = std::process::Command::new("tesseract");
        cmd.arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.status().map(|s| s.success()).unwrap_or(false)
    };
    if in_path {
        return "tesseract".to_string();
    }

    // 方式 2：检查默认安装目录
    #[cfg(target_os = "windows")]
    {
        let default_exe = r"C:\Program Files\Tesseract-OCR\tesseract.exe";
        if std::path::Path::new(default_exe).exists() {
            return default_exe.to_string();
        }
    }

    #[cfg(target_os = "macos")]
    {
        let paths = ["/opt/homebrew/bin/tesseract", "/usr/local/bin/tesseract"];
        for path in &paths {
            if std::path::Path::new(path).exists() {
                return path.to_string();
            }
        }
    }

    "tesseract".to_string()
}

/// 使用 Tesseract 识别 BGRA 图像，返回文本。
pub fn ocr_tesseract(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let temp_path = save_temp_png(bgra, width, height)?;

    let tesseract_exe = get_tesseract_path();
    let mut cmd = std::process::Command::new(&tesseract_exe);
    cmd.arg(temp_path.to_str().unwrap())
        .arg("stdout")
        .arg("-l")
        .arg("chi_sim");

    if let Ok(tessdata_prefix) = std::env::var("TESSDATA_PREFIX") {
        cmd.env("TESSDATA_PREFIX", &tessdata_prefix);
    } else {
        #[cfg(target_os = "windows")]
        {
            cmd.env("TESSDATA_PREFIX", r"C:\Program Files\Tesseract-OCR\tessdata");
        }
    }

    // Windows GUI 程序：禁止子进程弹出控制台窗口 (CREATE_NO_WINDOW = 0x08000000)
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    let output = cmd.output();

    let _ = std::fs::remove_file(temp_path);

    match output {
        Ok(out) => {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                Ok(text)
            } else {
                let err = String::from_utf8_lossy(&out.stderr).to_string();
                Err(anyhow::anyhow!("Tesseract 运行出错: {}", err))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                let err_msg = if cfg!(target_os = "windows") {
                    "未在系统 PATH 中找到 tesseract.exe\n请先安装 Tesseract OCR 并将其加入系统环境变量。"
                } else if cfg!(target_os = "macos") {
                    "未在系统 PATH 中找到 tesseract\n请先通过 Homebrew 安装 Tesseract OCR（在终端运行 `brew install tesseract`）。"
                } else {
                    "未在系统 PATH 中找到 tesseract\n请先安装 Tesseract OCR 并将其加入系统环境变量。"
                };
                Err(anyhow::anyhow!("{}", err_msg))
            } else {
                Err(anyhow::anyhow!("调用 Tesseract 失败: {}", e))
            }
        }
    }
}