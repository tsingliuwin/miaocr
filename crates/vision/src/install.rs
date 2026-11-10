//! 引擎一键下载安装（PaddleOCR-json / PP-OCRv5 / oar-ocr / RapidOCR-json / Tesseract）。

use std::sync::{Arc, Mutex};

use crate::detect::{
    detect_oar_ocr, detect_ocr_rs, detect_paddle, detect_rapid, detect_tesseract,
    download_file_from_urls,
    download_file_with_mirrors,
};
use crate::{data_dir, log, repaint, InstallState};

/// 后台异步下载安装 PaddleOCR-json 引擎
pub fn start_paddle_install(state: Arc<Mutex<InstallState>>) {
    log("[INSTALL] PaddleOCR-json 下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            repaint();
        };

        let dest_dir = data_dir().join("PaddleOCR-json");
        let zip_path = data_dir().join("PaddleOCR-json.7z");

        let urls = [
            "https://mirror.ghproxy.com/https://github.com/hiroi-sora/PaddleOCR-json/releases/download/v1.4.1/PaddleOCR-json_v1.4.1_windows_x64.7z",
            "https://ghproxy.net/https://github.com/hiroi-sora/PaddleOCR-json/releases/download/v1.4.1/PaddleOCR-json_v1.4.1_windows_x64.7z",
            "https://github.com/hiroi-sora/PaddleOCR-json/releases/download/v1.4.1/PaddleOCR-json_v1.4.1_windows_x64.7z"
        ];

        if !detect_paddle() {
            let mut download_success = false;
            let mut last_error_msg = String::new();

            for (idx, url) in urls.iter().enumerate() {
                let msg = format!("正在下载 PaddleOCR-json 引擎 (源 {}/{}，约 98 MB)...", idx + 1, urls.len());
                set(InstallState::Installing(msg));

                log(&format!("[INSTALL] 尝试自源 {} 下载: {}", idx + 1, url));
                let mut cmd = std::process::Command::new("curl");
                cmd.args(["-k", "-L", "--connect-timeout", "30", "--retry", "2", "-o", zip_path.to_str().unwrap(), url]);
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                let status = cmd.status();

                match status {
                    Ok(s) if s.success() => {
                        download_success = true;
                        break;
                    }
                    Ok(s) => {
                        last_error_msg = format!("退出码 {}", s);
                        log(&format!("[INSTALL] 源 {} 下载失败: {}", idx + 1, last_error_msg));
                    }
                    Err(e) => {
                        last_error_msg = format!("启动失败: {}", e);
                        log(&format!("[INSTALL] 无法启动 curl (源 {}): {}", idx + 1, last_error_msg));
                    }
                }
            }

            if !download_success {
                let err = format!(
                    "下载 PaddleOCR-json 失败 ({})。请检查网络或代理，或者手动下载并解压到 ~/.miaocr/PaddleOCR-json/ 目录，下载地址: {}",
                    last_error_msg, urls[0]
                );
                set(InstallState::Failed(err));
                return;
            }

            set(InstallState::Installing("正在解压 PaddleOCR-json 引擎...".into()));
            if dest_dir.exists() {
                let _ = std::fs::remove_dir_all(&dest_dir);
            }
            let _ = std::fs::create_dir_all(&dest_dir);

            match sevenz_rust::decompress_file(&zip_path, &dest_dir) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&zip_path);
                    if detect_paddle() {
                        log("[INSTALL] PaddleOCR-json 安装完成");
                        set(InstallState::Available);
                    } else {
                        let err = "解压完成，但未在目标目录中找到 PaddleOCR_json.exe 可执行文件。".to_string();
                        log(&err);
                        set(InstallState::Failed(err));
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&zip_path);
                    let err = format!("解压 PaddleOCR-json 失败: {}", e);
                    log(&err);
                    set(InstallState::Failed(err));
                }
            }
        } else {
            set(InstallState::Available);
        }
    });
}

/// 后台异步下载安装 PP-OCRv5 模型
pub fn start_ocr_rs_install(state: Arc<Mutex<InstallState>>) {
    log("[INSTALL] PP-OCRv5 (ocr-rs) 模型下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            repaint();
        };

        let models_dir = data_dir().join("models");
        let det_path = models_dir.join("PP-OCRv5_mobile_det.mnn");
        let rec_path = models_dir.join("PP-OCRv5_mobile_rec.mnn");
        let keys_path = models_dir.join("ppocr_keys_v5.txt");

        let set_msg = |msg: String| {
            set(InstallState::Installing(msg));
        };

        let is_valid = |path: &std::path::Path, min_size: u64| {
            path.metadata().map(|m| m.len() >= min_size).unwrap_or(false)
        };

        if !is_valid(&det_path, 1024 * 1024) {
            let _ = std::fs::remove_file(&det_path);
            if let Err(e) = download_file_with_mirrors(
                &det_path,
                "zibo-chen/rust-paddle-ocr/raw/main/models/PP-OCRv5_mobile_det.mnn",
                "检测模型 (约 4.5 MB)",
                &set_msg,
            ) {
                let _ = std::fs::remove_file(&det_path);
                set(InstallState::Failed(e.to_string()));
                return;
            }
        }

        if !is_valid(&rec_path, 5 * 1024 * 1024) {
            let _ = std::fs::remove_file(&rec_path);
            if let Err(e) = download_file_with_mirrors(
                &rec_path,
                "zibo-chen/rust-paddle-ocr/raw/main/models/PP-OCRv5_mobile_rec.mnn",
                "识别模型 (约 15.7 MB)",
                &set_msg,
            ) {
                let _ = std::fs::remove_file(&rec_path);
                set(InstallState::Failed(e.to_string()));
                return;
            }
        }

        if !is_valid(&keys_path, 10 * 1024) {
            let _ = std::fs::remove_file(&keys_path);
            if let Err(e) = download_file_with_mirrors(
                &keys_path,
                "zibo-chen/rust-paddle-ocr/raw/main/models/ppocr_keys_v5.txt",
                "字符集 (约 74 KB)",
                &set_msg,
            ) {
                let _ = std::fs::remove_file(&keys_path);
                set(InstallState::Failed(e.to_string()));
                return;
            }
        }

        if detect_ocr_rs() {
            log("[INSTALL] PP-OCRv5 模型下载及安装完成");
            set(InstallState::Available);
        } else {
            set(InstallState::Failed("模型下载已完成，但检测失败，请重试".to_string()));
        }
    });
}

/// 后台异步下载安装 oar-ocr 模型
pub fn start_oar_ocr_install(state: Arc<Mutex<InstallState>>) {
    log("[INSTALL] oar-ocr 模型下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            repaint();
        };

        let models_dir = data_dir().join("models");
        let det_path = models_dir.join("ppocrv6_small_det.onnx");
        let rec_path = models_dir.join("ppocrv6_small_rec.onnx");
        let keys_path = models_dir.join("ppocrv6_dict.txt");

        let set_msg = |msg: String| {
            set(InstallState::Installing(msg));
        };

        let is_valid = |path: &std::path::Path, min_size: u64| {
            path.metadata().map(|m| m.len() >= min_size).unwrap_or(false)
        };

        // PP-OCRv6 small 档（ONNX）来自 ModelScope 的 oar-ocr 官方模型仓库。
        // 必须钉住 commit 下载：master 分支的 LFS 引用解析会返回 HTTP 200 +
        // JSON 错误（"文件内容为空"），curl -f 拦不住，所以下载后还要按
        // 最小体积校验兜底。
        let files: [(std::path::PathBuf, u64, Vec<String>, &str); 3] = [
            (
                det_path,
                5 * 1024 * 1024,
                vec![String::from(
                    "https://www.modelscope.cn/models/greatv/oar-ocr/resolve/3f286c18c934050784bdf25dae4ccf55d197208d/pp-ocrv6_small_det.onnx",
                )],
                "PP-OCRv6 检测模型 (约 9.9 MB)",
            ),
            (
                rec_path,
                15 * 1024 * 1024,
                vec![String::from(
                    "https://www.modelscope.cn/models/greatv/oar-ocr/resolve/1684bdef2329ac08227939501a49d91c9cc0d24a/pp-ocrv6_small_rec.onnx",
                )],
                "PP-OCRv6 识别模型 (约 21 MB)",
            ),
            (
                keys_path,
                50 * 1024,
                vec![String::from(
                    "https://www.modelscope.cn/models/greatv/oar-ocr/resolve/f5adaf9b173ce1ac489cdf07fc9f001082f4371b/ppocrv6_dict.txt",
                )],
                "PP-OCRv6 字符集 (约 75 KB)",
            ),
        ];

        for (path, min_size, urls, label) in &files {
            if is_valid(path, *min_size) {
                continue;
            }
            let _ = std::fs::remove_file(path);
            if let Err(e) = download_file_from_urls(path, urls, label, &set_msg) {
                let _ = std::fs::remove_file(path);
                set(InstallState::Failed(e.to_string()));
                return;
            }
            if !is_valid(path, *min_size) {
                let _ = std::fs::remove_file(path);
                set(InstallState::Failed(format!("{label} 下载不完整（体积校验未通过）")));
                return;
            }
        }

        if detect_oar_ocr() {
            log("[INSTALL] PP-OCRv6 (oar-ocr) 模型下载及安装完成");
            set(InstallState::Available);
        } else {
            set(InstallState::Failed("模型下载已完成，但检测失败，请重试".to_string()));
        }
    });
}

/// 后台异步下载安装 RapidOCR-json 引擎
pub fn start_rapid_install(state: Arc<Mutex<InstallState>>) {
    log("[INSTALL] RapidOCR-json 下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            repaint();
        };

        let dest_dir = data_dir().join("RapidOCR-json");
        let zip_path = data_dir().join("RapidOCR-json.7z");

        let urls = [
            "https://mirror.ghproxy.com/https://github.com/hiroi-sora/RapidOCR-json/releases/download/v0.2.0/RapidOCR-json_v0.2.0.7z",
            "https://ghproxy.net/https://github.com/hiroi-sora/RapidOCR-json/releases/download/v0.2.0/RapidOCR-json_v0.2.0.7z",
            "https://github.com/hiroi-sora/RapidOCR-json/releases/download/v0.2.0/RapidOCR-json_v0.2.0.7z"
        ];

        if !detect_rapid() {
            let mut download_success = false;
            let mut last_error_msg = String::new();

            for (idx, url) in urls.iter().enumerate() {
                let msg = format!("正在下载 RapidOCR-json 引擎 (源 {}/{}，约 15 MB)...", idx + 1, urls.len());
                set(InstallState::Installing(msg));

                log(&format!("[INSTALL] 尝试自源 {} 下载: {}", idx + 1, url));
                let mut cmd = std::process::Command::new("curl");
                cmd.args(["-k", "-L", "--connect-timeout", "30", "--retry", "2", "-o", zip_path.to_str().unwrap(), url]);
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                let status = cmd.status();

                match status {
                    Ok(s) if s.success() => {
                        download_success = true;
                        break;
                    }
                    Ok(s) => {
                        last_error_msg = format!("退出码 {}", s);
                        log(&format!("[INSTALL] 源 {} 下载失败: {}", idx + 1, last_error_msg));
                    }
                    Err(e) => {
                        last_error_msg = format!("启动失败: {}", e);
                        log(&format!("[INSTALL] 无法启动 curl (源 {}): {}", idx + 1, last_error_msg));
                    }
                }
            }

            if !download_success {
                let err = format!(
                    "下载 RapidOCR-json 失败 ({})。请检查网络或代理，或者手动下载并解压到 ~/.miaocr/RapidOCR-json/ 目录，下载地址: {}",
                    last_error_msg, urls[0]
                );
                set(InstallState::Failed(err));
                return;
            }

            set(InstallState::Installing("正在解压 RapidOCR-json 引擎...".into()));
            if dest_dir.exists() {
                let _ = std::fs::remove_dir_all(&dest_dir);
            }
            let _ = std::fs::create_dir_all(&dest_dir);

            match sevenz_rust::decompress_file(&zip_path, &dest_dir) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&zip_path);
                    if detect_rapid() {
                        log("[INSTALL] RapidOCR-json 安装完成");
                        set(InstallState::Available);
                    } else {
                        let err = "解压完成，但未在目标目录中找到 RapidOCR_json.exe 可执行文件。".to_string();
                        log(&err);
                        set(InstallState::Failed(err));
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&zip_path);
                    let err = format!("解压 RapidOCR-json 失败: {}", e);
                    log(&err);
                    set(InstallState::Failed(err));
                }
            }
        } else {
            set(InstallState::Available);
        }
    });
}

/// 后台异步安装 Tesseract（下载预编译安装包 + chi_sim，通过 UAC 一次性提权安装）
#[cfg(target_os = "windows")]
pub fn start_tesseract_install(state: Arc<Mutex<InstallState>>) {
    log("[INSTALL] Tesseract 安装开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            repaint();
        };

        let temp_dir = std::env::temp_dir();

        // ── 步骤 1：获取最新版下载链接 ────────────────────────────
        set(InstallState::Installing("正在获取最新版本信息...".into()));
        let installer_url: String = {
            let api_out = std::process::Command::new("curl")
                .args(["-k", "-s", "-L",
                       "-H", "Accept: application/vnd.github+json",
                       "-H", "User-Agent: miaocr",
                       "https://api.github.com/repos/UB-Mannheim/tesseract/releases/latest"])
                .output();
            match api_out {
                Ok(ref o) if o.status.success() => {
                    let json = String::from_utf8_lossy(&o.stdout);
                    let mut found = None;
                    let mut rest = json.as_ref();
                    while let Some(pos) = rest.find("browser_download_url") {
                        rest = &rest[pos + 20..];
                        if let Some(us) = rest.find("https://") {
                            if let Some(ue) = rest[us..].find('"') {
                                let url = &rest[us..us + ue];
                                if url.contains("w64-setup") && !url.ends_with(".sig") {
                                    found = Some(url.to_string());
                                    break;
                                }
                            }
                        }
                    }
                    found.unwrap_or_else(|| {
                        "https://github.com/UB-Mannheim/tesseract/releases/download/v5.5.0.20241111/tesseract-ocr-w64-setup-5.5.0.20241111.exe".to_string()
                    })
                }
                _ => "https://github.com/UB-Mannheim/tesseract/releases/download/v5.5.0.20241111/tesseract-ocr-w64-setup-5.5.0.20241111.exe".to_string(),
            }
        };

        // ── 步骤 2：下载安装包到临时目录（普通权限即可）──────────
        let temp_installer = temp_dir.join("miaocr_tesseract_setup.exe");
        set(InstallState::Installing("正在下载 Tesseract 安装包（约 50 MB）...".into()));
        let dl1_urls = [
            installer_url.replace("https://github.com/", "https://mirror.ghproxy.com/https://github.com/"),
            installer_url.replace("https://github.com/", "https://ghproxy.net/https://github.com/"),
            installer_url.clone()
        ];
        let mut dl1_success = false;
        for url in &dl1_urls {
            let status = std::process::Command::new("curl")
                .args(["-k", "-L", "-o", temp_installer.to_str().unwrap(), url])
                .status();
            if status.map(|s| s.success()).unwrap_or(false) {
                dl1_success = true;
                break;
            }
        }
        if !dl1_success {
            set(InstallState::Failed("安装包下载失败，请检查网络".into()));
            return;
        }

        // ── 步骤 3：下载 chi_sim 中文训练数据到临时目录 ──────────
        let temp_chi = temp_dir.join("miaocr_chi_sim.traineddata");
        set(InstallState::Installing("正在下载中文语言包（chi_sim，约 20 MB）...".into()));
        let chi_url = "https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata";
        let dl2_urls = [
            chi_url.replace("https://github.com/", "https://mirror.ghproxy.com/https://github.com/"),
            chi_url.replace("https://github.com/", "https://ghproxy.net/https://github.com/"),
            chi_url.to_string()
        ];
        let mut dl2_success = false;
        for url in &dl2_urls {
            let status = std::process::Command::new("curl")
                .args(["-k", "-L", "-o", temp_chi.to_str().unwrap(), url])
                .status();
            if status.map(|s| s.success()).unwrap_or(false) {
                dl2_success = true;
                break;
            }
        }
        if !dl2_success {
            set(InstallState::Failed("中文语言包下载失败，请检查网络".into()));
            return;
        }

        // ── 步骤 4：生成 PowerShell 安装脚本 ─────────────────────
        let installer_ps = temp_installer.to_str().unwrap().replace('\'', "''");
        let chi_ps = temp_chi.to_str().unwrap().replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference = 'Stop'\r\nStart-Process -FilePath '{inst}' -ArgumentList '/VERYSILENT','/NORESTART','/DIR=C:\\Program Files\\Tesseract-OCR' -Wait\r\n$td = 'C:\\Program Files\\Tesseract-OCR\\tessdata'\r\nif (Test-Path '{chi}') {{ Copy-Item -Path '{chi}' -Destination \"$td\\chi_sim.traineddata\" -Force }}\r\nRemove-Item '{inst}' -Force -ErrorAction SilentlyContinue\r\nRemove-Item '{chi}'  -Force -ErrorAction SilentlyContinue\r\n",
            inst = installer_ps,
            chi = chi_ps,
        );

        let script_path = temp_dir.join("miaocr_tess_install.ps1");
        if std::fs::write(&script_path, script.as_bytes()).is_err() {
            set(InstallState::Failed("无法创建安装脚本，请检查临时目录权限".into()));
            return;
        }

        // ── 步骤 5：以管理员身份运行脚本（触发一次 UAC 弹窗）────
        set(InstallState::Installing("等待 UAC 授权（请在弹窗中点击「是」）...".into()));
        let ps_cmd = format!(
            "Start-Process powershell -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File \"{script}\"' -Verb RunAs -Wait",
            script = script_path.to_str().unwrap()
        );
        let run = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps_cmd])
            .status();

        let _ = std::fs::remove_file(&script_path);

        match run {
            Ok(s) if s.success() => {}
            Ok(s) => {
                let msg = format!("[INSTALL] Tesseract 安装脚本退出异常（exit {})", s);
                log(&msg);
                set(InstallState::Failed(format!(
                    "安装脚本退出异常（exit {}）。若取消了 UAC 请重试。", s
                )));
                return;
            }
            Err(e) => {
                log(&format!("[INSTALL] 无法启动安装脚本: {}", e));
                set(InstallState::Failed(format!("无法启动安装脚本: {}", e)));
                return;
            }
        }

        // ── 步骤 6：将安装目录加入当前进程 PATH，使 CLI 立即可用 ─
        let tess_dir = r"C:\Program Files\Tesseract-OCR";
        let cur_path = std::env::var("PATH").unwrap_or_default();
        if !cur_path.contains(tess_dir) {
            std::env::set_var("PATH", format!("{};{}", tess_dir, cur_path));
        }
        std::env::set_var("TESSDATA_PREFIX", r"C:\Program Files\Tesseract-OCR\tessdata");

        // ── 步骤 7：再次检测确认 ──────────────────────────────────
        if detect_tesseract() {
            set(InstallState::Available);
        } else {
            set(InstallState::Failed(
                "安装完成，重启 喵OCR 后 Tesseract 即可正常使用。".into()
            ));
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub fn start_tesseract_install(state: Arc<Mutex<InstallState>>) {
    log("[INSTALL] Tesseract macOS/Linux 提示安装");
    let set = move |s: InstallState| {
        *state.lock().unwrap() = s;
        repaint();
    };
    #[cfg(target_os = "macos")]
    set(InstallState::Failed("macOS 请在终端运行 `brew install tesseract` 安装".into()));
    #[cfg(not(target_os = "macos"))]
    set(InstallState::Failed("请先在系统上安装 Tesseract OCR".into()));
}