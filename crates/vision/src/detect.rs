//! 引擎环境检测与依赖下载（Tesseract / PaddleOCR / RapidOCR / PP-OCRv5 / oar-ocr）。

use std::path::Path;

use anyhow::Result;

use crate::engines::{find_paddle_exe, find_rapid_exe};
use crate::{data_dir, get_tesseract_path, log};

/// 检测 Tesseract 是否可用：先查 PATH，再查默认安装目录；找到后确保中文训练数据。
pub fn detect_tesseract() -> bool {
    let tesseract_exe = get_tesseract_path();

    // Check if the resolved path works
    let works = {
        let mut cmd = std::process::Command::new(&tesseract_exe);
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

    if works {
        #[cfg(target_os = "windows")]
        {
            if tesseract_exe.contains("C:\\Program Files\\Tesseract-OCR") {
                let tess_dir = r"C:\Program Files\Tesseract-OCR";
                let cur_path = std::env::var("PATH").unwrap_or_default();
                if !cur_path.contains(tess_dir) {
                    std::env::set_var("PATH", format!("{};{}", tess_dir, cur_path));
                }
            }
        }
        ensure_chi_sim();
        return true;
    }

    false
}

/// 确保 chi_sim.traineddata 可用，并设置 TESSDATA_PREFIX。
pub fn ensure_chi_sim() {
    let user_tessdata = data_dir().join("tessdata");
    let user_chi = user_tessdata.join("chi_sim.traineddata");

    if user_chi.exists() {
        std::env::set_var("TESSDATA_PREFIX", &user_tessdata);
        return;
    }

    #[cfg(target_os = "windows")]
    {
        let sys_chi =
            std::path::Path::new(r"C:\Program Files\Tesseract-OCR\tessdata\chi_sim.traineddata");
        if sys_chi.exists() {
            std::env::set_var("TESSDATA_PREFIX", r"C:\Program Files\Tesseract-OCR\tessdata");
            return;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let sys_paths = ["/opt/homebrew/share/tessdata", "/usr/local/share/tessdata"];
        for path in &sys_paths {
            let chi_path = std::path::Path::new(path).join("chi_sim.traineddata");
            if chi_path.exists() {
                std::env::set_var("TESSDATA_PREFIX", path);
                return;
            }
        }
    }

    log("[TESSDATA] chi_sim.traineddata 缺失，正在下载到 ~/.miaocr/tessdata/");
    let chi_urls = [
        "https://mirror.ghproxy.com/https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata",
        "https://ghproxy.net/https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata",
        "https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata",
    ];

    let mut download_success = false;
    for url in chi_urls {
        let mut cmd = std::process::Command::new("curl");
        cmd.args(["-k", "-L", "-o", user_chi.to_str().unwrap(), url]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let status = cmd.status();
        if status.map(|s| s.success()).unwrap_or(false) {
            download_success = true;
            break;
        }
    }

    if download_success {
        log("[TESSDATA] chi_sim.traineddata 下载完成");
        std::env::set_var("TESSDATA_PREFIX", &user_tessdata);
    } else {
        log("[TESSDATA] chi_sim.traineddata 下载失败，请检查网络");
    }
}

/// 检测本地 PaddleOCR 相关的 exe 文件是否存在
pub fn detect_paddle() -> bool {
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
    #[cfg(target_os = "windows")]
    {
        find_paddle_exe().is_some()
    }
}

/// 检测本地 RapidOCR 相关的 exe 文件是否存在
pub fn detect_rapid() -> bool {
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
    #[cfg(target_os = "windows")]
    {
        find_rapid_exe().is_some()
    }
}

/// 检测本地 PP-OCRv5 (ocr-rs) 模型文件是否齐全且大小正确
pub fn detect_ocr_rs() -> bool {
    let models_dir = data_dir().join("models");
    let det = models_dir.join("PP-OCRv5_mobile_det.mnn");
    let rec = models_dir.join("PP-OCRv5_mobile_rec.mnn");
    let keys = models_dir.join("ppocr_keys_v5.txt");

    let is_valid = |path: &Path, min_size: u64| {
        path.metadata().map(|m| m.len() >= min_size).unwrap_or(false)
    };

    is_valid(&det, 1024 * 1024) && is_valid(&rec, 5 * 1024 * 1024) && is_valid(&keys, 10 * 1024)
}

/// 检测本地 oar-ocr 模型文件是否齐全且大小正确
pub fn detect_oar_ocr() -> bool {
    let models_dir = data_dir().join("models");
    let det = models_dir.join("ppocrv5_mobile_det.onnx");
    let rec = models_dir.join("ppocrv5_mobile_rec.onnx");
    let keys = models_dir.join("ppocrv5_dict.txt");

    let is_valid = |path: &Path, min_size: u64| {
        path.metadata().map(|m| m.len() >= min_size).unwrap_or(false)
    };

    is_valid(&det, 1024 * 1024) && is_valid(&rec, 5 * 1024 * 1024) && is_valid(&keys, 10 * 1024)
}

/// 多镜像源轮询下载文件（Gitee/ghproxy/GitHub 直连）。
pub fn download_file_with_mirrors(
    dest_path: &Path,
    github_raw_path: &str,
    label: &str,
    set_state: &dyn Fn(String),
) -> Result<()> {
    let urls = [
        format!(
            "https://mirror.ghproxy.com/https://github.com/{}",
            github_raw_path
        ),
        format!("https://ghproxy.net/https://github.com/{}", github_raw_path),
        format!("https://github.com/{}", github_raw_path),
    ];
    let mut last_error_msg = String::new();
    for (idx, url) in urls.iter().enumerate() {
        set_state(format!("正在下载 {} (源 {}/{})...", label, idx + 1, urls.len()));
        log(&format!("[INSTALL] 尝试下载 {}: {}", label, url));
        let mut cmd = std::process::Command::new("curl");
        cmd.args([
            "-f",
            "-k",
            "-L",
            "--connect-timeout",
            "30",
            "--retry",
            "2",
            "-o",
            dest_path.to_str().unwrap(),
            url,
        ]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let status = cmd.status();
        match status {
            Ok(s) if s.success() => {
                log(&format!("[INSTALL] {} 下载成功", label));
                return Ok(());
            }
            Ok(s) => {
                last_error_msg = format!("退出码 {}", s);
                log(&format!(
                    "[INSTALL] 源 {} 下载失败: {}",
                    idx + 1,
                    last_error_msg
                ));
            }
            Err(e) => {
                last_error_msg = format!("启动失败: {}", e);
                log(&format!(
                    "[INSTALL] 无法启动 curl (源 {}): {}",
                    idx + 1,
                    last_error_msg
                ));
            }
        }
    }
    Err(anyhow::anyhow!(
        "下载 {} 失败: {}",
        label,
        last_error_msg
    ))
}