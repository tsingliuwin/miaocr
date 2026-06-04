#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use eframe::egui;
use screenshots::Screen;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use windows::{
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Globalization::Language,
    Media::Ocr::OcrEngine,
    Storage::Streams::Buffer,
    Win32::System::WinRT::IBufferByteAccess,
    core::{HSTRING, Interface},
};

fn binarize_bgra(bgra: &mut [u8]) {
    let pixel_count = bgra.len() / 4;
    if pixel_count == 0 {
        return;
    }

    // 1. 计算灰度值并生成直方图
    let mut grays = Vec::with_capacity(pixel_count);
    let mut histogram = [0u32; 256];
    for chunk in bgra.chunks_exact(4) {
        let b = chunk[0] as u32;
        let g = chunk[1] as u32;
        let r = chunk[2] as u32;
        // 使用标准心理学权重计算灰度
        let gray_val = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
        grays.push(gray_val);
        histogram[gray_val as usize] += 1;
    }

    // 2. 使用大津法 (Otsu's method) 计算最佳二值化阈值
    let mut sum = 0.0;
    for (i, &count) in histogram.iter().enumerate() {
        sum += i as f32 * count as f32;
    }

    let mut sum_b = 0.0;
    let mut w_b = 0u32;
    let mut var_max = 0.0;
    let mut threshold = 127u8;

    for t in 0..256 {
        w_b += histogram[t];
        if w_b == 0 {
            continue;
        }

        let w_f = pixel_count as u32 - w_b;
        if w_f == 0 {
            break;
        }

        sum_b += t as f32 * histogram[t] as f32;

        let m_b = sum_b / w_b as f32;
        let m_f = (sum - sum_b) / w_f as f32;

        let var_between = w_b as f32 * w_f as f32 * (m_b - m_f) * (m_b - m_f);

        if var_between > var_max {
            var_max = var_between;
            threshold = t as u8;
        }
    }

    // 3. 统计暗色像素占比，判断是否为深色背景以进行智能反色
    // (如果暗色像素占比过半，说明是深色背景，此时需要反色：原深色转白色，原亮色文本转黑色)
    let dark_pixels = grays.iter().filter(|&&g| g < threshold).count();
    let invert = dark_pixels > pixel_count / 2;

    // 4. 应用二值化
    for (i, chunk) in bgra.chunks_exact_mut(4).enumerate() {
        let g = grays[i];
        let val = if invert {
            if g < threshold { 255 } else { 0 }
        } else {
            if g < threshold { 0 } else { 255 }
        };
        chunk[0] = val; // B
        chunk[1] = val; // G
        chunk[2] = val; // R
        chunk[3] = 255; // A (不透明)
    }
}


fn save_temp_png(bgra: &[u8], width: u32, height: u32) -> Result<std::path::PathBuf> {
    let temp_dir = std::env::temp_dir();
    let temp_img_path = temp_dir.join("miaocr_temp.png");

    let rgba_bytes: Vec<u8> = bgra.chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();

    let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
        width,
        height,
        rgba_bytes,
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to convert image buffer for saving"))?;

    img_buffer.save(&temp_img_path)?;
    Ok(temp_img_path)
}

fn ocr_tesseract(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let temp_path = save_temp_png(bgra, width, height)?;

    // 显式将 TESSDATA_PREFIX 传入子进程，防止环境变量丢失
    let tessdata_prefix = std::env::var("TESSDATA_PREFIX")
        .unwrap_or_else(|_| r"C:\Program Files\Tesseract-OCR\tessdata".to_string());

    let mut cmd = std::process::Command::new("tesseract");
    cmd.arg(temp_path.to_str().unwrap())
        .arg("stdout")
        .arg("-l")
        .arg("chi_sim")
        .env("TESSDATA_PREFIX", &tessdata_prefix);

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
                Err(anyhow::anyhow!("未在系统 PATH 中找到 tesseract.exe\n请先安装 Tesseract OCR 并将其加入系统环境变量。"))
            } else {
                Err(anyhow::anyhow!("调用 Tesseract 失败: {}", e))
            }
        }
    }
}

use std::process::{Child, Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
struct JsonOcrEngine {
    child: Child,
    stdout_reader: BufReader<std::process::ChildStdout>,
    stdin_writer: std::process::ChildStdin,
}

#[allow(dead_code)]
impl JsonOcrEngine {
    fn new(exe_path: &Path) -> Result<Self> {
        let exe_dir = exe_path.parent().ok_or_else(|| anyhow::anyhow!("Invalid executable path"))?;
        
        let mut cmd = Command::new(exe_path);
        cmd.current_dir(exe_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // Windows GUI 程序：禁止子进程弹出控制台窗口 (CREATE_NO_WINDOW = 0x08000000)
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }

        let mut child = cmd.spawn()?;
            
        let stdin_writer = child.stdin.take().ok_or_else(|| anyhow::anyhow!("Failed to open stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("Failed to open stdout"))?;
        let mut stdout_reader = BufReader::new(stdout);
        
        // Wait for "OCR init completed."
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = stdout_reader.read_line(&mut line)?;
            if bytes_read == 0 {
                return Err(anyhow::anyhow!("OCR engine exited or closed stdout prematurely during initialization"));
            }
            if line.contains("OCR init completed.") {
                break;
            }
        }
        
        Ok(Self {
            child,
            stdout_reader,
            stdin_writer,
        })
    }
    
    fn ocr(&mut self, image_path: &Path) -> Result<String> {
        let path_str = image_path.to_string_lossy().replace('\\', "/");
        let cmd = serde_json::json!({
            "image_path": path_str
        });
        
        let mut cmd_str = serde_json::to_string(&cmd)?;
        cmd_str.push('\n');
        
        self.stdin_writer.write_all(cmd_str.as_bytes())?;
        self.stdin_writer.flush()?;
        
        let mut line = String::new();
        let bytes_read = self.stdout_reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Err(anyhow::anyhow!("OCR engine closed stdout connection unexpectedly"));
        }
        
        let resp: serde_json::Value = serde_json::from_str(&line)?;
        let code = resp["code"].as_i64().unwrap_or(-1);
        if code == 100 {
            if let Some(data) = resp["data"].as_array() {
                let mut lines = Vec::new();
                for item in data {
                    if let Some(text) = item["text"].as_str() {
                        lines.push(text.to_string());
                    }
                }
                Ok(lines.join("\n"))
            } else {
                Ok(String::new())
            }
        } else if code == 101 {
            Ok(String::new())
        } else {
            let err_msg = resp["msg"].as_str().unwrap_or("Unknown error").to_string();
            Err(anyhow::anyhow!("OCR engine error: {} (code {})", err_msg, code))
        }
    }
}

impl Drop for JsonOcrEngine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[allow(dead_code)]
fn find_paddle_exe() -> Option<PathBuf> {
    let base_dir = app_dir().join("PaddleOCR-json");
    let names = ["PaddleOCR-json.exe", "PaddleOCR_json.exe"];
    for name in &names {
        let p = base_dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                for name in &names {
                    let sub_p = path.join(name);
                    if sub_p.exists() {
                        return Some(sub_p);
                    }
                }
            }
        }
    }
    None
}

#[allow(dead_code)]
fn find_rapid_exe() -> Option<PathBuf> {
    let base_dir = app_dir().join("RapidOCR-json");
    let names = ["RapidOCR-json.exe", "RapidOCR_json.exe"];
    for name in &names {
        let p = base_dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                for name in &names {
                    let sub_p = path.join(name);
                    if sub_p.exists() {
                        return Some(sub_p);
                    }
                }
            }
        }
    }
    None
}

fn clear_paddle_engine() {
    PADDLE_OCR_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_some() {
            runtime_log("[ENGINE] 切换后端，释放 PaddleOCR-json 进程");
            *lock = None;
        }
    });
}

fn clear_rapid_engine() {
    RAPID_OCR_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_some() {
            runtime_log("[ENGINE] 切换后端，释放 RapidOCR-json 进程");
            *lock = None;
        }
    });
}

fn clear_ocr_rs_engine() {
    OCR_RS_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_some() {
            runtime_log("[ENGINE] 切换后端，释放 PP-OCRv5 (ocr-rs) 引擎");
            *lock = None;
        }
    });
}

fn clear_oar_ocr_engine() {
    OAR_OCR_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_some() {
            runtime_log("[ENGINE] 切换后端，释放 oar-ocr (本地) 引擎");
            *lock = None;
        }
    });
}

thread_local! {
    static PADDLE_OCR_ENGINE: std::cell::RefCell<Option<JsonOcrEngine>> = std::cell::RefCell::new(None);
    static RAPID_OCR_ENGINE: std::cell::RefCell<Option<JsonOcrEngine>> = std::cell::RefCell::new(None);
    static OCR_RS_ENGINE: std::cell::RefCell<Option<ocr_rs::OcrEngine>> = std::cell::RefCell::new(None);
    static OAR_OCR_ENGINE: std::cell::RefCell<Option<oar_ocr::pipeline::OAROCR>> = std::cell::RefCell::new(None);
}

fn ocr_paddle(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (bgra, width, height);
        return Err(anyhow::anyhow!("PaddleOCR 仅支持 Windows 系统。"));
    }
    #[cfg(target_os = "windows")]
    {
        let mut err_opt = None;
        
        let res = PADDLE_OCR_ENGINE.with(|cell| {
            let mut lock = cell.borrow_mut();
            if lock.is_none() {
                let exe_path = match find_paddle_exe() {
                    Some(p) => p,
                    None => {
                        err_opt = Some(anyhow::anyhow!("本地 PaddleOCR 引擎未安装。请在界面中点击「安装」。"));
                        return None;
                    }
                };
                
                match JsonOcrEngine::new(&exe_path) {
                    Ok(engine) => {
                        *lock = Some(engine);
                    }
                    Err(e) => {
                        err_opt = Some(anyhow::anyhow!("启动 PaddleOCR 引擎失败: {:?}", e));
                        return None;
                    }
                }
            }
            
            let engine = lock.as_mut().unwrap();
            
            let temp_path = match save_temp_png(bgra, width, height) {
                Ok(p) => p,
                Err(e) => {
                    err_opt = Some(e);
                    return None;
                }
            };
            
            let ocr_res = engine.ocr(&temp_path);
            let _ = std::fs::remove_file(temp_path);
            
            match ocr_res {
                Ok(text) => Some(text),
                Err(e) => {
                    *lock = None;
                    err_opt = Some(e);
                    None
                }
            }
        });
        
        if let Some(err) = err_opt {
            Err(err)
        } else {
            Ok(res.unwrap_or_default())
        }
    }
}

fn ocr_rapid(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (bgra, width, height);
        return Err(anyhow::anyhow!("RapidOCR 仅支持 Windows 系统。"));
    }
    #[cfg(target_os = "windows")]
    {
        let mut err_opt = None;
        
        let res = RAPID_OCR_ENGINE.with(|cell| {
            let mut lock = cell.borrow_mut();
            if lock.is_none() {
                let exe_path = match find_rapid_exe() {
                    Some(p) => p,
                    None => {
                        err_opt = Some(anyhow::anyhow!("本地 RapidOCR 引擎未安装。请在界面中点击「安装」。"));
                        return None;
                    }
                };
                
                match JsonOcrEngine::new(&exe_path) {
                    Ok(engine) => {
                        *lock = Some(engine);
                    }
                    Err(e) => {
                        err_opt = Some(anyhow::anyhow!("启动 RapidOCR 引擎失败: {:?}", e));
                        return None;
                    }
                }
            }
            
            let engine = lock.as_mut().unwrap();
            
            let temp_path = match save_temp_png(bgra, width, height) {
                Ok(p) => p,
                Err(e) => {
                    err_opt = Some(e);
                    return None;
                }
            };
            
            let ocr_res = engine.ocr(&temp_path);
            let _ = std::fs::remove_file(temp_path);
            
            match ocr_res {
                Ok(text) => Some(text),
                Err(e) => {
                    *lock = None;
                    err_opt = Some(e);
                    None
                }
            }
        });
        
        if let Some(err) = err_opt {
            Err(err)
        } else {
            Ok(res.unwrap_or_default())
        }
    }
}

fn ocr_ocr_rs(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let mut err_opt = None;
    
    let res = OCR_RS_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_none() {
            runtime_log("[OCR-RS] 初始化引擎: 检测模型文件...");
            if !detect_ocr_rs() {
                err_opt = Some(anyhow::anyhow!("本地 PP-OCRv5 引擎模型未安装。请在界面中点击「安装」。"));
                return None;
            }
            let models_dir = app_dir().join("models");
            let det_path = models_dir.join("PP-OCRv5_mobile_det.mnn");
            let rec_path = models_dir.join("PP-OCRv5_mobile_rec.mnn");
            let keys_path = models_dir.join("ppocr_keys_v5.txt");
            
            runtime_log(&format!("[OCR-RS] 加载模型: det={}, rec={}, keys={}",
                det_path.display(), rec_path.display(), keys_path.display()));
            
            // 禁用并行识别 + 单线程推理：
            // 1. rayon 并行会导致多线程同时调用 MNN FFI 产生数据竞争
            // 2. MNN 默认 4 线程，在后台 OCR 线程中可能与其他组件竞争资源
            let config = ocr_rs::OcrEngineConfig::new()
                .with_parallel(false)
                .with_threads(1);
            runtime_log("[OCR-RS] OcrEngine::new 开始...");
            match ocr_rs::OcrEngine::new(&det_path, &rec_path, &keys_path, Some(config)) {
                Ok(engine) => {
                    runtime_log("[OCR-RS] 引擎初始化成功");
                    *lock = Some(engine);
                }
                Err(e) => {
                    runtime_log(&format!("[OCR-RS] 引擎初始化失败: {:?}", e));
                    err_opt = Some(anyhow::anyhow!("初始化 PP-OCRv5 引擎失败: {:?}", e));
                    return None;
                }
            }
        }
        
        let engine = lock.as_mut().unwrap();
        
        let rgba_bytes: Vec<u8> = bgra.chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
            
        let img_buffer = match image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            width,
            height,
            rgba_bytes,
        ) {
            Some(buf) => buf,
            None => {
                err_opt = Some(anyhow::anyhow!("Failed to convert image buffer for OCR"));
                return None;
            }
        };
        let img = image::DynamicImage::ImageRgba8(img_buffer);
        
        // 使用 catch_unwind 保护 MNN C FFI 调用，防止 C++ 异常/段错误直接终止进程
        let ocr_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.recognize(&img)
        }));
        
        match ocr_res {
            Ok(Ok(results)) => {
                let mut lines = Vec::new();
                for item in results {
                    lines.push(item.text);
                }
                Some(lines.join("\n"))
            }
            Ok(Err(e)) => {
                runtime_log(&format!("[OCR-RS] 识别返回错误: {:?}", e));
                err_opt = Some(anyhow::anyhow!("PP-OCRv5 识别失败: {:?}", e));
                None
            }
            Err(panic_info) => {
                let msg = format!("PP-OCRv5 引擎 panic: {:?}", panic_info);
                runtime_log(&format!("[OCR-RS] {}", msg));
                // panic 后引擎状态可能已损坏，清除以便下次重新初始化
                *lock = None;
                err_opt = Some(anyhow::anyhow!("{}", msg));
                None
            }
        }
    });
    
    if let Some(err) = err_opt {
        Err(err)
    } else {
        Ok(res.unwrap_or_default())
    }
}

fn ocr_oar_ocr(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let mut err_opt = None;
    
    let res = OAR_OCR_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_none() {
            if !detect_oar_ocr() {
                err_opt = Some(anyhow::anyhow!("本地 oar-ocr 引擎模型未安装。请在界面中点击「安装」。"));
                return None;
            }
            let models_dir = app_dir().join("models");
            let det_path = models_dir.join("ppocrv5_mobile_det.onnx");
            let rec_path = models_dir.join("ppocrv5_mobile_rec.onnx");
            let keys_path = models_dir.join("ppocrv5_dict.txt");
            
            match oar_ocr::pipeline::OAROCRBuilder::new(
                det_path.to_string_lossy().to_string(),
                rec_path.to_string_lossy().to_string(),
                keys_path.to_string_lossy().to_string(),
            )
            .text_detection_batch_size(1)
            .text_recognition_batch_size(1)
            .text_det_threshold(0.3)
            .text_det_box_threshold(0.6)
            .text_det_unclip_ratio(1.5)
            .text_det_max_side_limit(4000)
            .text_rec_score_threshold(0.0)
            .text_rec_input_shape((3, 48, 320))
            .build() {
                Ok(engine) => {
                    *lock = Some(engine);
                }
                Err(e) => {
                    err_opt = Some(anyhow::anyhow!("初始化 oar-ocr 引擎失败: {:?}", e));
                    return None;
                }
            }
        }
        
        let engine = lock.as_mut().unwrap();
        
        let rgb_bytes: Vec<u8> = bgra.chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0]])
            .collect();
            
        let img_buffer = match image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(
            width,
            height,
            rgb_bytes,
        ) {
            Some(buf) => buf,
            None => {
                err_opt = Some(anyhow::anyhow!("Failed to convert image buffer for oar-ocr"));
                return None;
            }
        };
        
        match engine.predict(&[img_buffer]) {
            Ok(results) => {
                if let Some(res) = results.first() {
                    Some(res.concatenated_text("\n"))
                } else {
                    Some(String::new())
                }
            }
            Err(e) => {
                err_opt = Some(anyhow::anyhow!("oar-ocr 识别失败: {:?}", e));
                None
            }
        }
    });
    
    if let Some(err) = err_opt {
        Err(err)
    } else {
        Ok(res.unwrap_or_default())
    }
}


// ─── 引擎环境检测与自动安装 ────────────────────────────────

/// 引擎安装状态
#[derive(Debug, Clone, PartialEq)]
enum InstallState {
    Unchecked,           // 尚未检测
    Checking,            // 检测中
    Available,           // 已安装可用
    NotInstalled,        // 未安装
    Installing(String),  // 安装中，附带进度信息
    Failed(String),      // 安装失败，附带错误信息
}

/// 检测 Tesseract 是否可用：
/// 1. 先查 PATH（最快）
/// 2. PATH 里没有时，检查默认安装目录是否存在 exe
///    若在默认目录找到，则顺手将其注入当前进程 PATH。
/// exe 找到后调用 ensure_chi_sim() 确保中文训练数据可用。
fn detect_tesseract() -> bool {
    // 方式 1：PATH 查找
    // Windows GUI 程序：禁止子进程弹出控制台窗口 (CREATE_NO_WINDOW = 0x08000000)
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
        ensure_chi_sim();
        return true;
    }

    // 方式 2：检查默认安装目录（应对父进程 PATH 未刷新的情况）
    let default_exe = std::path::Path::new(r"C:\Program Files\Tesseract-OCR\tesseract.exe");
    if default_exe.exists() {
        // 注入到当前进程 PATH，使后续 CLI 调用直接可用，无需重启
        let tess_dir = r"C:\Program Files\Tesseract-OCR";
        let cur_path = std::env::var("PATH").unwrap_or_default();
        if !cur_path.contains(tess_dir) {
            std::env::set_var("PATH", format!("{};{}", tess_dir, cur_path));
        }
        ensure_chi_sim();
        return true;
    }

    false
}

/// 确保 chi_sim.traineddata 可用，并设置 TESSDATA_PREFIX。
/// 检查顺序：
///   1. ~/.miaocr/tessdata/  （用户目录，无需管理员权限）
///   2. C:\Program Files\Tesseract-OCR\tessdata\  （系统目录）
///   3. 以上都没有 → 自动下载到用户目录
fn ensure_chi_sim() {
    let user_tessdata = app_dir().join("tessdata");
    let user_chi = user_tessdata.join("chi_sim.traineddata");

    if user_chi.exists() {
        std::env::set_var("TESSDATA_PREFIX", &user_tessdata);
        return;
    }

    let sys_chi = std::path::Path::new(
        r"C:\Program Files\Tesseract-OCR\tessdata\chi_sim.traineddata");
    if sys_chi.exists() {
        std::env::set_var("TESSDATA_PREFIX",
            r"C:\Program Files\Tesseract-OCR\tessdata");
        return;
    }

    // 两处都没有，自动下载到用户目录（无需管理员权限）
    runtime_log("[TESSDATA] chi_sim.traineddata 缺失，正在下载到 ~/.miaocr/tessdata/");
    let chi_urls = [
        "https://mirror.ghproxy.com/https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata",
        "https://ghproxy.net/https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata",
        "https://github.com/tesseract-ocr/tessdata_fast/raw/main/chi_sim.traineddata"
    ];

    let mut download_success = false;
    for url in chi_urls {
        let mut cmd = std::process::Command::new("curl");
        cmd.args(["-k", "-L", "-o", user_chi.to_str().unwrap(), url]);
        // Windows GUI 程序：禁止子进程弹出控制台窗口 (CREATE_NO_WINDOW = 0x08000000)
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
        runtime_log("[TESSDATA] chi_sim.traineddata 下载完成");
        std::env::set_var("TESSDATA_PREFIX", &user_tessdata);
    } else {
        runtime_log("[TESSDATA] chi_sim.traineddata 下载失败，请检查网络");
    }
}

/// 检测本地 PaddleOCR 相关的 exe 文件是否存在
fn detect_paddle() -> bool {
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
    #[cfg(target_os = "windows")]
    {
        find_paddle_exe().is_some()
    }
}

/// 后台异步下载安装 PaddleOCR-json 引擎
fn start_paddle_install(state: Arc<Mutex<InstallState>>, ctx: egui::Context) {
    runtime_log("[INSTALL] PaddleOCR-json 下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            ctx.request_repaint();
        };

        let dest_dir = app_dir().join("PaddleOCR-json");
        let zip_path = app_dir().join("PaddleOCR-json.7z");
        
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
                
                runtime_log(&format!("[INSTALL] 尝试自源 {} 下载: {}", idx + 1, url));
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
                        runtime_log(&format!("[INSTALL] 源 {} 下载失败: {}", idx + 1, last_error_msg));
                    }
                    Err(e) => {
                        last_error_msg = format!("启动失败: {}", e);
                        runtime_log(&format!("[INSTALL] 无法启动 curl (源 {}): {}", idx + 1, last_error_msg));
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
                        runtime_log("[INSTALL] PaddleOCR-json 安装完成");
                        set(InstallState::Available);
                    } else {
                        let err = "解压完成，但未在目标目录中找到 PaddleOCR_json.exe 可执行文件。".to_string();
                        runtime_log(&err);
                        set(InstallState::Failed(err));
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&zip_path);
                    let err = format!("解压 PaddleOCR-json 失败: {}", e);
                    runtime_log(&err);
                    set(InstallState::Failed(err));
                }
            }
        } else {
            set(InstallState::Available);
        }
    });
}


/// 检测本地 RapidOCR 相关的 exe 文件是否存在
fn detect_rapid() -> bool {
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
fn detect_ocr_rs() -> bool {
    let models_dir = app_dir().join("models");
    let det = models_dir.join("PP-OCRv5_mobile_det.mnn");
    let rec = models_dir.join("PP-OCRv5_mobile_rec.mnn");
    let keys = models_dir.join("ppocr_keys_v5.txt");
    
    let is_valid = |path: &std::path::Path, min_size: u64| {
        path.metadata().map(|m| m.len() >= min_size).unwrap_or(false)
    };
    
    is_valid(&det, 1024 * 1024) && is_valid(&rec, 5 * 1024 * 1024) && is_valid(&keys, 10 * 1024)
}

/// 检测本地 oar-ocr 模型文件是否齐全且大小正确
fn detect_oar_ocr() -> bool {
    let models_dir = app_dir().join("models");
    let det = models_dir.join("ppocrv5_mobile_det.onnx");
    let rec = models_dir.join("ppocrv5_mobile_rec.onnx");
    let keys = models_dir.join("ppocrv5_dict.txt");
    
    let is_valid = |path: &std::path::Path, min_size: u64| {
        path.metadata().map(|m| m.len() >= min_size).unwrap_or(false)
    };
    
    is_valid(&det, 1024 * 1024) && is_valid(&rec, 5 * 1024 * 1024) && is_valid(&keys, 10 * 1024)
}

fn download_file_with_mirrors(
    dest_path: &Path,
    github_raw_path: &str,
    label: &str,
    set_state: &dyn Fn(String),
) -> Result<()> {
    let urls = [
        format!("https://mirror.ghproxy.com/https://github.com/{}", github_raw_path),
        format!("https://ghproxy.net/https://github.com/{}", github_raw_path),
        format!("https://github.com/{}", github_raw_path),
    ];
    let mut last_error_msg = String::new();
    for (idx, url) in urls.iter().enumerate() {
        set_state(format!("正在下载 {} (源 {}/{})...", label, idx + 1, urls.len()));
        runtime_log(&format!("[INSTALL] 尝试下载 {}: {}", label, url));
        let mut cmd = std::process::Command::new("curl");
        cmd.args(["-f", "-k", "-L", "--connect-timeout", "30", "--retry", "2", "-o", dest_path.to_str().unwrap(), url]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let status = cmd.status();
        match status {
            Ok(s) if s.success() => {
                runtime_log(&format!("[INSTALL] {} 下载成功", label));
                return Ok(());
            }
            Ok(s) => {
                last_error_msg = format!("退出码 {}", s);
                runtime_log(&format!("[INSTALL] 源 {} 下载失败: {}", idx + 1, last_error_msg));
            }
            Err(e) => {
                last_error_msg = format!("启动失败: {}", e);
                runtime_log(&format!("[INSTALL] 无法启动 curl (源 {}): {}", idx + 1, last_error_msg));
            }
        }
    }
    Err(anyhow::anyhow!("下载 {} 失败: {}", label, last_error_msg))
}

/// 后台异步下载安装 PP-OCRv5 模型
fn start_ocr_rs_install(state: Arc<Mutex<InstallState>>, ctx: egui::Context) {
    runtime_log("[INSTALL] PP-OCRv5 (ocr-rs) 模型下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            ctx.request_repaint();
        };
        
        let models_dir = app_dir().join("models");
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
            runtime_log("[INSTALL] PP-OCRv5 模型下载及安装完成");
            set(InstallState::Available);
        } else {
            set(InstallState::Failed("模型下载已完成，但检测失败，请重试".to_string()));
        }
    });
}

/// 后台异步下载安装 oar-ocr 模型
fn start_oar_ocr_install(state: Arc<Mutex<InstallState>>, ctx: egui::Context) {
    runtime_log("[INSTALL] oar-ocr 模型下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            ctx.request_repaint();
        };
        
        let models_dir = app_dir().join("models");
        let det_path = models_dir.join("ppocrv5_mobile_det.onnx");
        let rec_path = models_dir.join("ppocrv5_mobile_rec.onnx");
        let keys_path = models_dir.join("ppocrv5_dict.txt");
        
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
                "GreatV/oar-ocr/releases/download/v0.1.0/ppocrv5_mobile_det.onnx",
                "检测模型 (约 4.8 MB)",
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
                "GreatV/oar-ocr/releases/download/v0.1.0/ppocrv5_mobile_rec.onnx",
                "识别模型 (约 16.5 MB)",
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
                "GreatV/oar-ocr/releases/download/v0.1.0/ppocrv5_dict.txt",
                "字符集 (约 1.2 MB)",
                &set_msg,
            ) {
                let _ = std::fs::remove_file(&keys_path);
                set(InstallState::Failed(e.to_string()));
                return;
            }
        }
        
        if detect_oar_ocr() {
            runtime_log("[INSTALL] oar-ocr 模型下载及安装完成");
            set(InstallState::Available);
        } else {
            set(InstallState::Failed("模型下载已完成，但检测失败，请重试".to_string()));
        }
    });
}

/// 后台异步下载安装 RapidOCR-json 引擎
fn start_rapid_install(state: Arc<Mutex<InstallState>>, ctx: egui::Context) {
    runtime_log("[INSTALL] RapidOCR-json 下载开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            ctx.request_repaint();
        };

        let dest_dir = app_dir().join("RapidOCR-json");
        let zip_path = app_dir().join("RapidOCR-json.7z");
        
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
                
                runtime_log(&format!("[INSTALL] 尝试自源 {} 下载: {}", idx + 1, url));
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
                        runtime_log(&format!("[INSTALL] 源 {} 下载失败: {}", idx + 1, last_error_msg));
                    }
                    Err(e) => {
                        last_error_msg = format!("启动失败: {}", e);
                        runtime_log(&format!("[INSTALL] 无法启动 curl (源 {}): {}", idx + 1, last_error_msg));
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
                        runtime_log("[INSTALL] RapidOCR-json 安装完成");
                        set(InstallState::Available);
                    } else {
                        let err = "解压完成，但未在目标目录中找到 RapidOCR_json.exe 可执行文件。".to_string();
                        runtime_log(&err);
                        set(InstallState::Failed(err));
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&zip_path);
                    let err = format!("解压 RapidOCR-json 失败: {}", e);
                    runtime_log(&err);
                    set(InstallState::Failed(err));
                }
            }
        } else {
            set(InstallState::Available);
        }
    });
}


/// 后台异步安装 Tesseract（下载预编译安装包 + chi_sim，通过 UAC 一次性提权安装）
fn start_tesseract_install(state: Arc<Mutex<InstallState>>, ctx: egui::Context) {
    runtime_log("[INSTALL] Tesseract 安装开始");
    std::thread::spawn(move || {
        let set = |s: InstallState| {
            *state.lock().unwrap() = s;
            ctx.request_repaint();
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
        // 写 C:\Program Files 需要管理员权限，统一放进脚本里以管理员身份一次执行
        let installer_ps = temp_installer.to_str().unwrap().replace('\'', "''");
        let chi_ps       = temp_chi.to_str().unwrap().replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference = 'Stop'\r\nStart-Process -FilePath '{inst}' -ArgumentList '/VERYSILENT','/NORESTART','/DIR=C:\\Program Files\\Tesseract-OCR' -Wait\r\n$td = 'C:\\Program Files\\Tesseract-OCR\\tessdata'\r\nif (Test-Path '{chi}') {{ Copy-Item -Path '{chi}' -Destination \"$td\\chi_sim.traineddata\" -Force }}\r\nRemove-Item '{inst}' -Force -ErrorAction SilentlyContinue\r\nRemove-Item '{chi}'  -Force -ErrorAction SilentlyContinue\r\n",
            inst = installer_ps,
            chi  = chi_ps,
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
                runtime_log(&msg);
                set(InstallState::Failed(format!(
                    "安装脚本退出异常（exit {}）。若取消了 UAC 请重试。", s
                )));
                return;
            }
            Err(e) => {
                runtime_log(&format!("[INSTALL] 无法启动安装脚本: {}", e));
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




fn ocr_baidu_aistudio(
    bgra: &[u8],
    width: u32,
    height: u32,
    token: &str,
    model: &str,
    use_orientation: bool,
    use_unwarping: bool,
    use_chart: bool,
) -> Result<String> {
    let temp_path = save_temp_png(bgra, width, height)?;
    let temp_str = temp_path.to_str().unwrap();

    let payload = serde_json::json!({
        "useDocOrientationClassify": use_orientation,
        "useDocUnwarping": use_unwarping,
        "useChartRecognition": use_chart,
    });
    let payload_str = serde_json::to_string(&payload)?;

    // 1. Submit Job
    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg("-k")
        .arg("-H")
        .arg(format!("Authorization: bearer {}", token))
        .arg("-F")
        .arg(format!("model={}", model))
        .arg("-F")
        .arg(format!("optionalPayload={}", payload_str))
        .arg("-F")
        .arg(format!("file=@{}", temp_str))
        .arg("https://paddleocr.aistudio-app.com/api/v2/ocr/jobs")
        .output();

    let _ = std::fs::remove_file(&temp_path);

    let out = match output {
        Ok(out) => out,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err(anyhow::anyhow!("系统未找到 curl.exe，请确保环境正常。"));
            } else {
                return Err(anyhow::anyhow!("发送请求失败: {}", e));
            }
        }
    };

    if !out.status.success() {
        let err_str = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(anyhow::anyhow!("提交任务请求失败 (curl 错误): {}", err_str));
    }

    let resp_str = String::from_utf8_lossy(&out.stdout);
    let resp: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| anyhow::anyhow!("解析提交任务响应失败: {}, 响应为: {}", e, resp_str))?;

    if resp["code"] != 0 {
        return Err(anyhow::anyhow!("提交任务 API 错误: {}", resp["msg"].as_str().unwrap_or("未知错误")));
    }

    let job_id = resp["data"]["jobId"].as_str()
        .ok_or_else(|| anyhow::anyhow!("未能在响应中找到 jobId: {}", resp_str))?;

    // 2. Poll for results (Incremental Backoff Polling)
    let json_url = {
        let poll_start = std::time::Instant::now();
        let mut sleep_ms = 200;
        loop {
            if poll_start.elapsed().as_secs() > 30 {
                return Err(anyhow::anyhow!("轮询超时 (30秒)"));
            }

            let poll_out = std::process::Command::new("curl")
                .arg("-s")
                .arg("-k")
                .arg("-H")
                .arg(format!("Authorization: bearer {}", token))
                .arg(format!("https://paddleocr.aistudio-app.com/api/v2/ocr/jobs/{}", job_id))
                .output()?;

            if poll_out.status.success() {
                let poll_resp_str = String::from_utf8_lossy(&poll_out.stdout);
                if let Ok(poll_resp) = serde_json::from_str::<serde_json::Value>(&poll_resp_str) {
                    if poll_resp["code"] != 0 {
                        return Err(anyhow::anyhow!("轮询 API 错误: {}", poll_resp["msg"].as_str().unwrap_or("未知错误")));
                    }

                    let state = poll_resp["data"]["state"].as_str().unwrap_or("");
                    if state == "done" {
                        if let Some(url) = poll_resp["data"]["resultUrl"]["jsonUrl"].as_str() {
                            break url.to_string();
                        } else {
                            return Err(anyhow::anyhow!("任务已完成，但未找到 jsonUrl"));
                        }
                    } else if state == "failed" {
                        let err_msg = poll_resp["data"]["errorMsg"].as_str().unwrap_or("未知错误原因");
                        return Err(anyhow::anyhow!("任务识别失败: {}", err_msg));
                    }
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
            if sleep_ms < 500 {
                sleep_ms += 100;
            }
        }
    };

    // 3. Download and parse results
    let result_out = std::process::Command::new("curl")
        .arg("-s")
        .arg("-k")
        .arg(&json_url)
        .output()?;

    if !result_out.status.success() {
        return Err(anyhow::anyhow!("下载识别结果失败"));
    }

    let result_str = String::from_utf8_lossy(&result_out.stdout);
    let mut ocr_text = Vec::new();

    for line in result_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(layouts) = val["result"]["layoutParsingResults"].as_array() {
                for layout in layouts {
                    if let Some(txt) = layout["markdown"]["text"].as_str() {
                        ocr_text.push(txt.to_string());
                    }
                }
            }
        }
    }

    if ocr_text.is_empty() {
        Ok(String::from("未识别到文字"))
    } else {
        Ok(ocr_text.join("\n"))
    }
}

#[cfg(target_os = "macos")]
fn ensure_macocr_binary() -> Result<std::path::PathBuf> {
    let bin_path = app_dir().join("macocr_bin_v2");
    if bin_path.exists() {
        return Ok(bin_path);
    }
    
    let swift_code = include_str!("mac_ocr.swift");
    let swift_path = app_dir().join("mac_ocr.swift");
    std::fs::write(&swift_path, swift_code)?;
    
    runtime_log("[SWIFT] 正在首次编译 macOS Vision OCR 辅助程序...");
    let status = std::process::Command::new("swiftc")
        .arg("-O")
        .arg(swift_path.to_str().unwrap())
        .arg("-o")
        .arg(bin_path.to_str().unwrap())
        .status();
        
    let _ = std::fs::remove_file(swift_path);
    
    match status {
        Ok(s) if s.success() => {
            runtime_log("[SWIFT] 辅助程序编译成功");
            Ok(bin_path)
        }
        Ok(s) => Err(anyhow::anyhow!("Swift 编译器退出异常: {}", s)),
        Err(e) => Err(anyhow::anyhow!("启动 Swift 编译器失败: {}", e)),
    }
}

#[cfg(target_os = "macos")]
fn ocr_mac_native(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let bin_path = ensure_macocr_binary()?;
    
    use std::io::Write;
    let mut child = std::process::Command::new(bin_path)
        .arg(width.to_string())
        .arg(height.to_string())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
        
    {
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("无法打开子进程 stdin"))?;
        stdin.write_all(bgra)?;
        stdin.flush()?;
    }
    
    let output = child.wait_with_output()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        let out = String::from_utf8_lossy(&output.stdout).to_string();
        let combined = if err.is_empty() { out } else if out.is_empty() { err } else { format!("{}\n{}", err, out) };
        Err(anyhow::anyhow!("Vision OCR 运行出错: {}", combined.trim()))
    }
}

fn ocr_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    backend: BackendType,
    baidu_token: &str,
    baidu_model: &str,
    baidu_use_orientation: bool,
    baidu_use_unwarping: bool,
    baidu_use_chart: bool,
) -> Result<String> {
    let screens = Screen::all()?;
    if screens.is_empty() {
        return Err(anyhow::anyhow!("未检测到任何活动屏幕"));
    }
    let center_x = x + (width as i32) / 2;
    let center_y = y + (height as i32) / 2;

    // Find the screen containing this center point
    let screen = screens
        .iter()
        .find(|s| {
            let info = s.display_info;
            center_x >= info.x
                && center_x < info.x + info.width as i32
                && center_y >= info.y
                && center_y < info.y + info.height as i32
        })
        .unwrap_or(&screens[0]);

    let info = screen.display_info;
    let local_x = (x - info.x).max(0);
    let local_y = (y - info.y).max(0);

    // Clamp width and height to fit inside the screen boundaries
    let capture_width = width.min((info.width as i32 - local_x).max(0) as u32);
    let capture_height = height.min((info.height as i32 - local_y).max(0) as u32);

    if capture_width == 0 || capture_height == 0 {
        return Ok(String::new());
    }

    let image = screen.capture_area(local_x, local_y, capture_width, capture_height)?;
    let raw: &[u8] = image.as_raw();

    // 动态缩放：对于小于 500x400 的中偏小选区，放大 2 倍以提升 OCR 解析精度
    let (mut bgra, final_width, final_height) = if capture_width < 500 && capture_height < 400 {
        // Reconstruct ImageBuffer from raw RGBA bytes
        let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            image.width(),
            image.height(),
            raw.to_vec(),
        )
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;

        // Scale up 2x using CatmullRom sharpening filter to improve OCR accuracy
        let scaled_width = image.width() * 2;
        let scaled_height = image.height() * 2;
        let resized_img = image::imageops::resize(
            &img_buffer,
            scaled_width,
            scaled_height,
            image::imageops::FilterType::CatmullRom,
        );

        // Convert RGBA to BGRA
        let bgra_bytes: Vec<u8> = resized_img
            .as_raw()
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();

        (bgra_bytes, scaled_width, scaled_height)
    } else {
        // Convert RGBA to BGRA directly without resizing
        let bgra_bytes: Vec<u8> = raw
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();

        (bgra_bytes, image.width(), image.height())
    };

    // 仅对 WindowsNative 和 Tesseract 应用二值化预处理（PaddleOCR 等深度学习模型直接基于原图推理效果更好）
    if backend == BackendType::WindowsNative || backend == BackendType::Tesseract {
        binarize_bgra(&mut bgra);
    }

    if backend != BackendType::PaddleOcr {
        clear_paddle_engine();
    }
    if backend != BackendType::RapidOcr {
        clear_rapid_engine();
    }
    if backend != BackendType::OcrRs {
        clear_ocr_rs_engine();
    }
    if backend != BackendType::OarOcr {
        clear_oar_ocr_engine();
    }

    match backend {
        BackendType::WindowsNative => {
            #[cfg(target_os = "windows")]
            {
                let bitmap = SoftwareBitmap::CreateWithAlpha(
                    BitmapPixelFormat::Bgra8,
                    final_width as i32,
                    final_height as i32,
                    BitmapAlphaMode::Premultiplied,
                )?;

                let buf = Buffer::Create(bgra.len() as u32)?;
                buf.SetLength(bgra.len() as u32)?;
                unsafe {
                    let byte_access: IBufferByteAccess = buf.cast()?;
                    let data = std::slice::from_raw_parts_mut(byte_access.Buffer()?, bgra.len());
                    data.copy_from_slice(&bgra);
                }
                bitmap.CopyFromBuffer(&buf)?;

                let lang = Language::CreateLanguage(&HSTRING::from("zh-CN"))?;

                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| {
                    if let Ok(supported) = OcrEngine::IsLanguageSupported(&lang) {
                        println!("Windows OCR 'zh-CN' 支持情况: {}", supported);
                    }
                    if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                        if let Ok(rec_lang) = engine.RecognizerLanguage() {
                            if let Ok(tag) = rec_lang.LanguageTag() {
                                println!("当前实际使用的 OCR 语言: {}", tag);
                            }
                        }
                    }
                });

                let engine = OcrEngine::TryCreateFromLanguage(&lang)?;
                let result = engine.RecognizeAsync(&bitmap)?.get()?;
                let text = result.Text()?.to_string();

                let clean: String = text
                    .lines()
                    .map(|l: &str| l.replace(' ', ""))
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(clean)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(anyhow::anyhow!("Windows 原生 OCR 仅支持 Windows 系统"))
            }
        }
        BackendType::MacNative => {
            #[cfg(target_os = "macos")]
            {
                let text = ocr_mac_native(&bgra, final_width, final_height)?;
                let clean: String = text
                    .lines()
                    .map(|l: &str| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(clean)
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err(anyhow::anyhow!("macOS 原生 OCR 仅支持 macOS 系统"))
            }
        }
        BackendType::Tesseract => {
            let text = ocr_tesseract(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
        BackendType::PaddleOcr => {
            let text = ocr_paddle(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
        BackendType::RapidOcr => {
            let text = ocr_rapid(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
        BackendType::BaiduAiStudio => {
            let text = ocr_baidu_aistudio(
                &bgra,
                final_width,
                final_height,
                baidu_token,
                baidu_model,
                baidu_use_orientation,
                baidu_use_unwarping,
                baidu_use_chart,
            )?;
            Ok(text)
        }
        BackendType::OcrRs => {
            let text = ocr_ocr_rs(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
        BackendType::OarOcr => {
            let text = ocr_oar_ocr(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
    }
}

fn get_cursor_pos() -> Option<(i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        use windows::Win32::Foundation::POINT;
        unsafe {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_ok() {
                Some((pt.x, pt.y))
            } else {
                None
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        use mouse_position::mouse_position::Mouse;
        match Mouse::get_mouse_position() {
            Mouse::Position { x, y } => Some((x, y)),
            Mouse::Error => None,
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

// ─── 悬浮窗应用 ──────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum BackendType {
    WindowsNative,
    MacNative,
    Tesseract,
    PaddleOcr,
    RapidOcr,
    BaiduAiStudio,
    OcrRs,
    OarOcr,
}

impl BackendType {
    fn display_name(&self) -> &'static str {
        match self {
            BackendType::WindowsNative => "Windows 原生",
            BackendType::MacNative     => "macOS 原生",
            BackendType::Tesseract    => "Tesseract",
            BackendType::PaddleOcr   => "PaddleOCR-json",
            BackendType::RapidOcr    => "RapidOCR-json",
            BackendType::BaiduAiStudio => "百度 AI Studio",
            BackendType::OcrRs        => "PP-OCRv5 (MNN)",
            BackendType::OarOcr       => "OarOCR (ONNX)",
        }
    }

    fn log_name(&self) -> &'static str {
        match self {
            BackendType::WindowsNative => "WindowsNative",
            BackendType::MacNative     => "MacNative",
            BackendType::Tesseract     => "Tesseract",
            BackendType::PaddleOcr     => "PaddleOCR",
            BackendType::RapidOcr      => "RapidOCR",
            BackendType::BaiduAiStudio => "BaiduAiStudio",
            BackendType::OcrRs         => "OcrRs",
            BackendType::OarOcr        => "OarOcr",
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct AppSettings {
    selected_backend: BackendType,
    baidu_token: String,
    baidu_model: String,
    baidu_use_orientation: bool,
    baidu_use_unwarping: bool,
    baidu_use_chart: bool,
}

impl AppSettings {
    fn load() -> Self {
        let path = app_dir().join("settings.json");
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                return settings;
            }
        }
        // 默认配置
        AppSettings {
            selected_backend: {
                #[cfg(target_os = "windows")]
                {
                    BackendType::WindowsNative
                }
                #[cfg(target_os = "macos")]
                {
                    BackendType::MacNative
                }
                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                {
                    BackendType::Tesseract
                }
            },
            baidu_token: String::new(),
            baidu_model: String::from("PaddleOCR-VL-1.6"),
            baidu_use_orientation: false,
            baidu_use_unwarping: false,
            baidu_use_chart: false,
        }
    }

    fn save(&self) {
        let path = app_dir().join("settings.json");
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, content);
        }
    }
}

impl FloatApp {
    fn save_current_settings(&self) {
        let settings = AppSettings {
            selected_backend: *self.selected_backend.lock().unwrap(),
            baidu_token: self.baidu_token.lock().unwrap().clone(),
            baidu_model: self.baidu_model.lock().unwrap().clone(),
            baidu_use_orientation: *self.baidu_use_orientation.lock().unwrap(),
            baidu_use_unwarping: *self.baidu_use_unwarping.lock().unwrap(),
            baidu_use_chart: *self.baidu_use_chart.lock().unwrap(),
        };
        settings.save();
    }

    fn draw_settings_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("⬅ 返回").clicked() {
                self.show_settings = false;
            }
            ui.heading(egui::RichText::new("云端配置").strong().color(egui::Color32::from_rgb(17, 24, 39)).size(14.0));
        });
        
        ui.add_space(6.0);
        
        // 极细分割线
        let stroke_color = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 20);
        let cursor_y = ui.cursor().min.y;
        let width = ui.available_width();
        ui.painter().hline(
            ui.cursor().min.x..=(ui.cursor().min.x + width),
            cursor_y,
            egui::Stroke::new(1.0, stroke_color),
        );
        ui.add_space(8.0);
        
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("百度 AI Studio (云端 API)").strong().size(13.0).color(egui::Color32::from_rgb(17, 24, 39)));
                        ui.add_space(6.0);
                        
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Token:").size(12.0).color(egui::Color32::from_rgb(75, 85, 99)));
                            let mut token = self.baidu_token.lock().unwrap().clone();
                            let token_edit = egui::TextEdit::singleline(&mut token)
                                .password(true)
                                .desired_width(180.0);
                            if ui.add(token_edit).changed() {
                                *self.baidu_token.lock().unwrap() = token;
                                self.save_current_settings();
                            }
                        });
                        
                        ui.add_space(6.0);
                        
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("模型:").size(12.0).color(egui::Color32::from_rgb(75, 85, 99)));
                            let mut model = self.baidu_model.lock().unwrap().clone();
                            let prev_model = model.clone();
                            egui::ComboBox::from_id_source("settings_baidu_model_select")
                                .selected_text(model.as_str())
                                .width(150.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut model, String::from("PaddleOCR-VL-1.6"), "PaddleOCR-VL-1.6");
                                });
                            if model != prev_model {
                                *self.baidu_model.lock().unwrap() = model;
                                self.save_current_settings();
                            }
                        });
                        
                        if *self.baidu_model.lock().unwrap() == "PaddleOCR-VL-1.6" {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let mut use_ori = *self.baidu_use_orientation.lock().unwrap();
                                let mut use_unw = *self.baidu_use_unwarping.lock().unwrap();
                                let mut use_crt = *self.baidu_use_chart.lock().unwrap();

                                let mut changed = false;
                                if ui.checkbox(&mut use_ori, "方向").changed() {
                                    *self.baidu_use_orientation.lock().unwrap() = use_ori;
                                    changed = true;
                                }
                                if ui.checkbox(&mut use_unw, "去畸变").changed() {
                                    *self.baidu_use_unwarping.lock().unwrap() = use_unw;
                                    changed = true;
                                }
                                if ui.checkbox(&mut use_crt, "图表").changed() {
                                    *self.baidu_use_chart.lock().unwrap() = use_crt;
                                    changed = true;
                                }
                                if changed {
                                    self.save_current_settings();
                                }
                            });
                        }
                    });
                });
                
                ui.add_space(12.0);
                ui.label(egui::RichText::new("云端配置说明：").strong().size(12.0).color(egui::Color32::from_rgb(55, 65, 81)));
                ui.label(egui::RichText::new("• 百度 AI Studio 为云端识别服务，使用时请确保您的网络正常连通。")
                    .size(11.0).color(egui::Color32::from_rgb(107, 114, 128)));
                ui.label(egui::RichText::new("• Token 为您在百度智能云控制台获取的 API 密钥或 Access Token，在本地加密保存。")
                    .size(11.0).color(egui::Color32::from_rgb(107, 114, 128)));
            });
    }

    fn draw_main_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // ── 引擎选择及状态行 (移至顶部) ──
        ui.horizontal(|ui| {
            ui.set_min_height(ui.spacing().interact_size.y);
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("引擎:").size(12.0).color(egui::Color32::from_rgb(55, 65, 81)));
                ui.label(egui::RichText::new("ℹ").size(12.0).color(egui::Color32::from_rgb(156, 163, 175)))
                    .on_hover_text("【本地引擎运行模式说明】\n\n• 内置 MNN/ONNX 引擎 (PP-OCRv5 / OarOCR)：\n  - 直接在程序内部加载推理，冷启动和识别可在毫秒级完成，最节省系统资源。\n  - 若底层算法库遭遇致命崩溃，会连带导致主程序闪退。\n\n• 外置子进程引擎 (PaddleOCR-json / RapidOCR-json)：\n  - 独立运行于后台进程中，通过 stdio 管道通信，需首次下载额外引擎包。\n  - 具备崩溃隔离保护，子进程异常崩溃不会影响主程序，但有轻微的跨进程开销。");
            });
            let mut current_backend = *self.selected_backend.lock().unwrap();
            let prev_backend = current_backend;
            
            egui::ComboBox::from_id_source("backend_select")
                .selected_text(current_backend.display_name())
                .width(120.0)
                .show_ui(ui, |ui| {
                    ui.label(egui::RichText::new("── 本地 ──").size(10.0).color(egui::Color32::from_rgb(120, 120, 120)));
                    #[cfg(target_os = "windows")]
                    ui.selectable_value(&mut current_backend, BackendType::WindowsNative, "Windows 原生 (系统自带)")
                        .on_hover_text("Windows 原生 OCR：\n• 调用 Windows 系统内置 OCR 引擎\n• 零额外开销，启动极快，支持中英文\n• 推荐在无需高精度排版识别的日常场景下使用");
                    #[cfg(target_os = "macos")]
                    ui.selectable_value(&mut current_backend, BackendType::MacNative, "macOS 原生 (系统自带)")
                        .on_hover_text("macOS 原生 OCR：\n• 调用 macOS 系统 Vision 框架 API\n• 速度极快，识别率高，无需联网及额外下载");
                    ui.selectable_value(&mut current_backend, BackendType::Tesseract,    "Tesseract (传统多语言)")
                        .on_hover_text("Tesseract OCR：\n• 经典开源 OCR 引擎\n• 支持极其丰富的多国语言包，适合小语种识别");
                    ui.selectable_value(&mut current_backend, BackendType::OcrRs,        "PP-OCRv5 (MNN 内置, 极速)")
                        .on_hover_text("PP-OCRv5 (内置 MNN 引擎)：\n• 采用最新 PP-OCRv5 算法，性能和体积平衡极佳\n• 在程序内部加载运行，零跨进程通信延迟，推荐作为离线 OCR 首选");
                    ui.selectable_value(&mut current_backend, BackendType::OarOcr,       "OarOCR (ONNX 内置, 兼容好)")
                        .on_hover_text("OarOCR (内置 ONNX 引擎)：\n• 使用主程序内置的 ONNX Runtime 进行推理，兼容性与稳定性强\n• 直接在当前进程内加载，冷启动与识别迅速，最省系统资源");
                    #[cfg(target_os = "windows")]
                    ui.selectable_value(&mut current_backend, BackendType::PaddleOcr,    "PaddleOCR-json (Paddle 外置, 高精)")
                        .on_hover_text("PaddleOCR-json (外置 Paddle 引擎)：\n• 基于官方 C++ 预测库，独立进程运行，首次切换需下载约 98MB 引擎包\n• 识别精度非常优秀，对段落合并与复杂文本排版支持极佳");
                    #[cfg(target_os = "windows")]
                    ui.selectable_value(&mut current_backend, BackendType::RapidOcr,     "RapidOCR-json (ONNX 外置, 稳定)")
                        .on_hover_text("RapidOCR-json (外置 ONNX 引擎)：\n• 独立子进程运行的 ONNX 推理引擎（RapidOCR-json.exe）\n• 具备“崩溃隔离”优势，子进程异常崩溃绝不影响主程序，稳定性高");
                    ui.separator();
                    ui.label(egui::RichText::new("── 云端 ──").size(10.0).color(egui::Color32::from_rgb(120, 120, 120)));
                    ui.selectable_value(&mut current_backend, BackendType::BaiduAiStudio, "百度 AI Studio (高精云端)")
                        .on_hover_text("百度 AI Studio (云端 API)：\n• 联网请求百度智能云 OCR 服务\n• 识别精确度最高，对倾斜、手写、复杂表格等效果极佳，需联网和配置 Key");
                });
                
            if current_backend != prev_backend {
                *self.selected_backend.lock().unwrap() = current_backend;
                self.save_current_settings();
                // 切换引擎时触发检测（若尚未检测）
                let need_check = match current_backend {
                    BackendType::Tesseract => {
                        *self.tess_state.lock().unwrap() == InstallState::Unchecked
                    }
                    BackendType::PaddleOcr => {
                        *self.paddle_state.lock().unwrap() == InstallState::Unchecked
                    }
                    BackendType::RapidOcr => {
                        *self.rapid_state.lock().unwrap() == InstallState::Unchecked
                    }
                    BackendType::OcrRs => {
                        *self.ocr_rs_state.lock().unwrap() == InstallState::Unchecked
                    }
                    BackendType::OarOcr => {
                        *self.oar_ocr_state.lock().unwrap() == InstallState::Unchecked
                    }
                    _ => false,
                };
                if need_check {
                    let (state_arc, ctx_clone) = match current_backend {
                        BackendType::Tesseract => (self.tess_state.clone(), ctx.clone()),
                        BackendType::PaddleOcr => (self.paddle_state.clone(), ctx.clone()),
                        BackendType::RapidOcr => (self.rapid_state.clone(), ctx.clone()),
                        BackendType::OcrRs => (self.ocr_rs_state.clone(), ctx.clone()),
                        BackendType::OarOcr => (self.oar_ocr_state.clone(), ctx.clone()),
                        _ => (self.ocr_rs_state.clone(), ctx.clone()),
                    };
                    *state_arc.lock().unwrap() = InstallState::Checking;
                    let b_type = current_backend;
                    std::thread::spawn(move || {
                        let available = match b_type {
                            BackendType::Tesseract => detect_tesseract(),
                            BackendType::PaddleOcr => detect_paddle(),
                            BackendType::RapidOcr => detect_rapid(),
                            BackendType::OcrRs => detect_ocr_rs(),
                            BackendType::OarOcr => detect_oar_ocr(),
                            _ => false,
                        };
                        *state_arc.lock().unwrap() = if available {
                            InstallState::Available
                         } else {
                            InstallState::NotInstalled
                         };
                         ctx_clone.request_repaint();
                    });
                }
            }
            
            // 引擎状态指示灯与文字
            let engine_state = match current_backend {
                BackendType::Tesseract => Some(self.tess_state.lock().unwrap().clone()),
                BackendType::PaddleOcr => Some(self.paddle_state.lock().unwrap().clone()),
                BackendType::RapidOcr  => Some(self.rapid_state.lock().unwrap().clone()),
                BackendType::OcrRs     => Some(self.ocr_rs_state.lock().unwrap().clone()),
                BackendType::OarOcr    => Some(self.oar_ocr_state.lock().unwrap().clone()),
                _ => None,
            };
            
            if let Some(state) = engine_state {
                match &state {
                    InstallState::Unchecked => {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(156, 163, 175));
                            ui.label(egui::RichText::new("未检测").size(12.0).color(egui::Color32::from_rgb(107, 114, 128)));
                        });
                    }
                    InstallState::Checking => {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(250, 204, 21));
                            ui.label(egui::RichText::new("检测中").size(12.0).color(egui::Color32::from_rgb(180, 83, 9)));
                        });
                    }
                    InstallState::Available => {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(34, 197, 94));
                            ui.label(egui::RichText::new("已安装").size(12.0).color(egui::Color32::from_rgb(21, 128, 61)));
                        });
                    }
                    InstallState::NotInstalled => {
                        ui.horizontal_centered(|ui| {
                            if ui.small_button("安装").clicked() {
                                match current_backend {
                                    BackendType::Tesseract => start_tesseract_install(self.tess_state.clone(), ctx.clone()),
                                    BackendType::PaddleOcr => start_paddle_install(self.paddle_state.clone(), ctx.clone()),
                                    BackendType::RapidOcr => start_rapid_install(self.rapid_state.clone(), ctx.clone()),
                                    BackendType::OcrRs => start_ocr_rs_install(self.ocr_rs_state.clone(), ctx.clone()),
                                    BackendType::OarOcr => start_oar_ocr_install(self.oar_ocr_state.clone(), ctx.clone()),
                                    _ => {}
                                }
                            }
                        });
                    }
                    InstallState::Installing(msg) => {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(250, 204, 21));
                            
                            let display_msg = if msg.contains("正在下载") {
                                if msg.contains("Tesseract") {
                                    "下载中 (Tesseract)...".to_string()
                                } else if msg.contains("中文语言包") {
                                    "下载中 (语言包)...".to_string()
                                } else if msg.contains("PaddleOCR") {
                                    "下载中 (PaddleOCR)...".to_string()
                                } else if msg.contains("RapidOCR") {
                                    "下载中 (RapidOCR)...".to_string()
                                } else if msg.contains("检测模型") {
                                    "下载中 (检测模型)...".to_string()
                                } else if msg.contains("识别模型") {
                                    "下载中 (识别模型)...".to_string()
                                } else if msg.contains("字符集") {
                                    "下载中 (字符集)...".to_string()
                                } else {
                                    "下载中...".to_string()
                                }
                            } else if msg.contains("正在解压") {
                                "解压中...".to_string()
                            } else if msg.contains("等待 UAC") {
                                "等待授权...".to_string()
                            } else {
                                msg.clone()
                            };
                            
                            ui.label(egui::RichText::new(format!("⏳ {}", display_msg)).size(12.0).color(egui::Color32::from_rgb(180, 83, 9)))
                                .on_hover_text(msg.as_str());
                        });
                    }
                    InstallState::Failed(err) => {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(220, 38, 38));
                            let short = if err.chars().count() > 10 {
                                format!("{}…", &err.chars().take(10).collect::<String>())
                            } else {
                                err.clone()
                            };
                            ui.label(egui::RichText::new(format!("安装失败 ({})", short)).size(12.0).color(egui::Color32::from_rgb(220, 38, 38)))
                                .on_hover_text(err.as_str());
                            if ui.small_button("重试").clicked() {
                                match current_backend {
                                    BackendType::Tesseract => start_tesseract_install(self.tess_state.clone(), ctx.clone()),
                                    BackendType::PaddleOcr => start_paddle_install(self.paddle_state.clone(), ctx.clone()),
                                    BackendType::RapidOcr => start_rapid_install(self.rapid_state.clone(), ctx.clone()),
                                    BackendType::OcrRs => start_ocr_rs_install(self.ocr_rs_state.clone(), ctx.clone()),
                                    BackendType::OarOcr => start_oar_ocr_install(self.oar_ocr_state.clone(), ctx.clone()),
                                    _ => {}
                                }
                            }
                        });
                    }
                }
            }

            // 云端配置按钮
            ui.add_space(4.0);
            let cloud_btn = ui.button("⚙ 云配置");
            if cloud_btn.clicked() {
                self.show_settings = true;
            }
            cloud_btn.on_hover_text("配置云端 OCR 引擎的 API 密钥及参数");
        });

        ui.add_space(6.0);
        let stroke_color = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 20);
        let cursor_y = ui.cursor().min.y;
        let width = ui.available_width();
        ui.painter().hline(
            ui.cursor().min.x..=(ui.cursor().min.x + width),
            cursor_y,
            egui::Stroke::new(1.0, stroke_color),
        );
        ui.add_space(8.0);

        // ── 操作栏 (选区、暂停、复制、云配置、耗时) ──
        ui.horizontal(|ui| {
            let sel_btn = ui.button("⛶ 选区");
            if sel_btn.clicked() {
                if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                    if rect.min.x > -9000.0 && rect.min.y > -9000.0 {
                        self.float_pos = rect.min;
                    }
                }
                *self.ocr_region.lock().unwrap() = None;
                *self.paused.lock().unwrap() = true;
                self.select_step = 1;
            }
            sel_btn.on_hover_text("选择屏幕区域开始持续 OCR 识别");

            let has_region = self.ocr_region.lock().unwrap().is_some();
            if has_region {
                let is_paused = *self.paused.lock().unwrap();
                let play_pause_btn = if is_paused { "▶ 继续" } else { "▶I 暂停" };
                let btn_res = ui.button(play_pause_btn);
                if btn_res.clicked() {
                    let mut p = self.paused.lock().unwrap();
                    *p = !*p;
                }
                btn_res.on_hover_text(if is_paused { "继续识别" } else { "暂停识别" });
            }

            let text = self.text.lock().unwrap().clone();
            let copy_btn = ui.button("📋 复制");
            if copy_btn.clicked() {
                ui.output_mut(|o| o.copied_text = text);
            }
            copy_btn.on_hover_text("复制识别结果到剪贴板");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ms = *self.elapsed.lock().unwrap();
                let interval = *self.interval.lock().unwrap();
                if ms > 0 {
                    ui.label(
                        egui::RichText::new(format!("🕒 {}ms / {}ms", ms, interval))
                            .color(egui::Color32::from_rgb(156, 163, 175))
                            .size(11.0),
                    );
                }
            });
        });

        ui.add_space(6.0);
        let cursor_y = ui.cursor().min.y;
        ui.painter().hline(
            ui.cursor().min.x..=(ui.cursor().min.x + width),
            cursor_y,
            egui::Stroke::new(1.0, stroke_color),
        );
        ui.add_space(8.0);

        // ── 内容展示区 ──
        let mut text_lock = self.text.lock().unwrap();
        let has_text = !text_lock.is_empty() && *text_lock != "等待选择区域...";

        if !has_text {
            ui.vertical_centered(|ui| {
                ui.add_space(15.0);
                ui.label(egui::RichText::new("🔍").size(32.0));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("暂无识别内容")
                        .strong()
                        .color(egui::Color32::from_rgb(75, 85, 99))
                        .size(13.0)
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("点击左上角「🎯选区」开始框选识别")
                        .color(egui::Color32::from_rgb(107, 114, 128))
                        .size(11.0)
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("提示: 选区时按 Esc 可退出")
                        .color(egui::Color32::from_rgb(156, 163, 175))
                        .size(10.0)
                );
                ui.add_space(15.0);
            });
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let available = ui.available_size();
                    ui.add_sized(
                        available,
                        egui::TextEdit::multiline(&mut *text_lock)
                            .font(egui::FontId::proportional(13.0))
                            .text_color(egui::Color32::from_rgb(17, 24, 39))
                            .frame(false),
                    );
                });
        }
    }
}

#[derive(PartialEq)]
enum AppMode {
    Float,      // 悬浮面板状态
    Selecting,  // 屏幕选区状态
}

#[derive(Clone, Copy)]
struct ScreenInfo {
    _id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    #[allow(dead_code)]
    scale_factor: f32,
}

struct FloatApp {
    mode: AppMode,
    select_step: u8,
    frame_delay: u8,
    show_settings: bool,

    // OCR 共享状态
    ocr_region: Arc<Mutex<Option<(i32, i32, u32, u32)>>>,
    paused: Arc<Mutex<bool>>,
    text: Arc<Mutex<String>>,
    elapsed: Arc<Mutex<u128>>,
    interval: Arc<Mutex<u128>>,
    selected_backend: Arc<Mutex<BackendType>>,
    baidu_token: Arc<Mutex<String>>,
    baidu_model: Arc<Mutex<String>>,
    baidu_use_orientation: Arc<Mutex<bool>>,
    baidu_use_unwarping: Arc<Mutex<bool>>,
    baidu_use_chart: Arc<Mutex<bool>>,

    // 引擎安装状态（后台检测 / 安装线程写入）
    tess_state:   Arc<Mutex<InstallState>>,
    paddle_state: Arc<Mutex<InstallState>>,
    rapid_state:  Arc<Mutex<InstallState>>,
    ocr_rs_state: Arc<Mutex<InstallState>>,
    oar_ocr_state: Arc<Mutex<InstallState>>,

    // 记忆悬浮窗的窗口大小（用户可以拖动调整）
    float_size: egui::Vec2,

    // 选区相关的临时截图数据
    screenshot_texture: Option<egui::TextureHandle>,
    screenshot_raw: Vec<u8>,
    screenshot_width: u32,
    screenshot_height: u32,
    active_screen_info: Option<ScreenInfo>,
    drag_start: Option<egui::Pos2>,
    drag_end: Option<egui::Pos2>,

    // 记忆悬浮窗位置，以便选区完后复位
    float_pos: egui::Pos2,
}

impl eframe::App for FloatApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 截图隐藏逻辑的状态机处理
        if self.select_step == 1 {
            // 先隐藏窗口的边框和标题栏，并移至屏幕外，使其被排除在截图之外。
            // 这里不能使用 Visible(false)，因为隐藏窗口后 Windows 不再调用 egui update()，会导致状态机卡死。
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(-10000.0, -10000.0)));
            self.select_step = 2;
            self.frame_delay = 5;
        } else if self.select_step == 2 {
            if self.frame_delay > 0 {
                self.frame_delay -= 1;
                ctx.request_repaint(); // 强制刷新以将隐藏事件发往 Windows 窗口管理器
            } else {
                // 截图鼠标所在的屏幕
                if let Some(screens) = Screen::all().ok() {
                    let (mx, my) = get_cursor_pos().unwrap_or((0, 0));
                    runtime_log(&format!("[SELECT] cursor pos: ({}, {})", mx, my));
                    for (idx, scr) in screens.iter().enumerate() {
                        let info = scr.display_info;
                        runtime_log(&format!("[SELECT] screen {}: id={}, x={}, y={}, w={}, h={}, scale={}",
                            idx, info.id, info.x, info.y, info.width, info.height, info.scale_factor));
                    }
                    let active = screens
                        .iter()
                        .find(|s| {
                            let info = s.display_info;
                            mx >= info.x
                                && mx < info.x + info.width as i32
                                && my >= info.y
                                && my < info.y + info.height as i32
                        })
                        .unwrap_or(&screens[0]);
                    runtime_log(&format!("[SELECT] selected active screen: id={}", active.display_info.id));

                    if let Ok(image) = active.capture() {
                        self.screenshot_raw = image.as_raw().to_vec();
                        self.screenshot_width = image.width();
                        self.screenshot_height = image.height();
                        let info = active.display_info;
                        self.active_screen_info = Some(ScreenInfo {
                            _id: info.id,
                            x: info.x,
                            y: info.y,
                            width: info.width,
                            height: info.height,
                            scale_factor: info.scale_factor,
                        });
                    }
                }
                self.select_step = 3;
            }
        } else if self.select_step == 3 {
            if let Some(info) = &self.active_screen_info {
                #[cfg(target_os = "windows")]
                let (logical_width, logical_height, logical_x, logical_y) = {
                    let scale = ctx.pixels_per_point(); // 使用当前窗口的缩放比例进行精准坐标转换，防止跨屏 DPI 差异导致坐标偏移
                    (
                        info.width as f32 / scale,
                        info.height as f32 / scale,
                        info.x as f32 / scale,
                        info.y as f32 / scale,
                    )
                };
                #[cfg(not(target_os = "windows"))]
                let (logical_width, logical_height, logical_x, logical_y) = (
                    info.width as f32,
                    info.height as f32,
                    info.x as f32,
                    info.y as f32,
                );

                runtime_log(&format!("[SELECT] Moving window to logical pos: ({}, {}), logical size: ({}, {})",
                    logical_x, logical_y, logical_width, logical_height));

                // 移除窗口标题栏和边框，使其以完全无边框的覆盖层形式显示在目标屏幕上
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(logical_x, logical_y)));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(logical_width, logical_height)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);

                self.mode = AppMode::Selecting;
                self.drag_start = None;
                self.drag_end = None;
                self.screenshot_texture = None;
            } else {
                // 如果截图失败，恢复原来的窗口大小、位置和标题栏
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.float_size));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.float_pos));
                self.mode = AppMode::Float;
            }
            self.select_step = 0;
        }

        // 当处于悬浮窗模式且没有进行隐藏截图时，持续记录当前窗口坐标与大小，用于选区完成后复位
        if self.mode == AppMode::Float && self.select_step == 0 {
            if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                if rect.min.x > -9000.0 && rect.min.y > -9000.0 {
                    self.float_pos = rect.min;
                }
            }
            if let Some(inner_rect) = ctx.input(|i| i.viewport().inner_rect) {
                self.float_size = inner_rect.size();
            }
        }

        // 2. 选择区域模式渲染
        if self.mode == AppMode::Selecting {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                .show(ctx, |ui| {
                    let rect = ui.available_rect_before_wrap();

                    if self.screenshot_texture.is_none() && !self.screenshot_raw.is_empty() {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [self.screenshot_width as usize, self.screenshot_height as usize],
                            &self.screenshot_raw,
                        );
                        self.screenshot_texture = Some(ctx.load_texture(
                            "screenshot",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }

                    if let Some(tex) = &self.screenshot_texture {
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }

                    // 黑色半透明覆盖遮罩
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 80),
                    );

                    // 绘制顶部操作说明
                    ui.painter().text(
                        egui::pos2(rect.width() / 2.0, 40.0),
                        egui::Align2::CENTER_TOP,
                        "拖拽选择持续识别区域  |  ESC 取消",
                        egui::FontId::proportional(18.0),
                        egui::Color32::WHITE,
                    );

                    let response = ui.interact(rect, ui.id(), egui::Sense::drag());

                    if response.drag_started() {
                        self.drag_start = ctx.input(|i| i.pointer.press_origin());
                    }
                    if response.dragged() {
                        self.drag_end = ctx.input(|i| i.pointer.hover_pos());
                    }

                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                        let sel = egui::Rect::from_two_pos(start, end);

                        if let Some(tex) = &self.screenshot_texture {
                            let uv_min = egui::pos2(sel.min.x / rect.width(), sel.min.y / rect.height());
                            let uv_max = egui::pos2(sel.max.x / rect.width(), sel.max.y / rect.height());
                            ui.painter().image(
                                tex.id(),
                                sel,
                                egui::Rect::from_min_max(uv_min, uv_max),
                                egui::Color32::WHITE,
                            );
                        }

                        ui.painter().rect_stroke(
                            sel,
                            0.0,
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)),
                        );

                        ui.painter().text(
                            sel.min + egui::vec2(4.0, -18.0),
                            egui::Align2::LEFT_TOP,
                            format!("{:.0} × {:.0}", sel.width(), sel.height()),
                            egui::FontId::proportional(13.0),
                            egui::Color32::WHITE,
                        );
                    }

                    if response.drag_stopped() {
                        if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                            let sel = egui::Rect::from_two_pos(start, end);
                            if sel.width() > 10.0 && sel.height() > 10.0 {
                                if let Some(info) = &self.active_screen_info {
                                    #[cfg(target_os = "windows")]
                                    let scale = info.scale_factor;
                                    #[cfg(target_os = "windows")]
                                    let (logical_x, logical_y) = (info.x as f32 / scale, info.y as f32 / scale);
                                    #[cfg(not(target_os = "windows"))]
                                    let (logical_x, logical_y) = (info.x as f32, info.y as f32);

                                    #[cfg(target_os = "windows")]
                                    let (x, y, w, h) = (
                                        ((sel.min.x + logical_x) * scale) as i32,
                                        ((sel.min.y + logical_y) * scale) as i32,
                                        (sel.width() * scale) as u32,
                                        (sel.height() * scale) as u32,
                                    );
                                    #[cfg(not(target_os = "windows"))]
                                    let (x, y, w, h) = (
                                        (sel.min.x + logical_x) as i32,
                                        (sel.min.y + logical_y) as i32,
                                        sel.width() as u32,
                                        sel.height() as u32,
                                    );

                                    *self.ocr_region.lock().unwrap() = Some((x, y, w, h));
                                    *self.paused.lock().unwrap() = false;
                                }
                            }
                        }
                        // 还原到悬浮窗尺寸并恢复窗口边框
                        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.float_size));
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.float_pos));
                        self.mode = AppMode::Float;
                    }

                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.float_size));
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.float_pos));
                        self.mode = AppMode::Float;
                    }
                });
        }

        // 3. 悬浮面板模式渲染
        if self.mode == AppMode::Float {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::viewport::WindowLevel::AlwaysOnTop));

            // 半透明亮色卡片风格（内容区域使用，窗口本身使用系统标题栏）
            let card_frame = egui::Frame::none()
                .fill(egui::Color32::from_rgb(243, 243, 243))
                .inner_margin(egui::Margin::symmetric(12.0, 10.0));

            egui::CentralPanel::default()
                .frame(card_frame)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        if self.show_settings {
                            self.draw_settings_page(ui);
                        } else {
                            self.draw_main_page(ui, ctx);
                        }
                    });
                });
        }
    }
}

// ─── 应用数据目录 ───────────────────────────────────────────

/// 返回 ~/.miaocr 路径，并确保目录结构存在：
///   ~/.miaocr/            ← 根目录（运行日志、配置）
///   ~/.miaocr/models/     ← 模型文件（未来 PaddleOCR ONNX 等）
fn app_dir() -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dir = home.join(".miaocr");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::create_dir_all(dir.join("models"));
    let _ = std::fs::create_dir_all(dir.join("tessdata")); // chi_sim 等训练数据，无需管理员权限
    dir
}

/// 追加一条运行日志到 ~/.miaocr/miaocr.log
/// 用于记录程序错误、引擎安装事件等，方便排查 bug，与识别结果文件分开
fn runtime_log(msg: &str) {
    use std::io::Write;
    let log_path = app_dir().join("miaocr.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(f, "[{}] {}", now, msg);
    }
}

// ─── 主入口 ──────────────────────────────────────────────

struct LogManager {
    file: Option<std::fs::File>,
    last_text: Option<String>,
    last_header_pos: u64,
    start_time: Option<chrono::DateTime<chrono::Local>>,
    last_backend: Option<BackendType>,
}

impl LogManager {
    fn new() -> Self {
        Self {
            file: None,
            last_text: None,
            last_header_pos: 0,
            start_time: None,
            last_backend: None,
        }
    }

    fn start_new_file(&mut self) {
        use std::fs::OpenOptions;
        let _ = std::fs::create_dir_all(".log");
        let now = chrono::Local::now();
        let file_name = format!(".log/ocr_log_{}.txt", now.format("%Y%m%d_%H%M%S"));
        self.file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&file_name)
            .ok();
        self.reset();
    }

    fn reset(&mut self) {
        self.last_text = None;
        self.last_header_pos = 0;
        self.start_time = None;
        self.last_backend = None;
    }

    fn log(&mut self, text: &str, backend: BackendType) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        if self.file.is_none() {
            self.start_new_file();
        }

        let file = match &mut self.file {
            Some(f) => f,
            None => return,
        };

        use std::io::{Write, Seek, SeekFrom};
        let now = chrono::Local::now();
        let time_str = now.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        if let Some(ref last) = self.last_text {
            if last == text && self.last_backend == Some(backend) {
                // 如果结果与之前一样，并且引擎也一样，则定位到上一条记录的头部，覆写结束时间
                if file.seek(SeekFrom::Start(self.last_header_pos)).is_ok() {
                    let start_str = self.start_time.unwrap().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                    // 覆写时间头部，格式和长度必须保持完全一致以避免文本发生位移
                    let header = format!("=== [{}] [{} ~ {}] ===\r\n", backend.log_name(), start_str, time_str);
                    let _ = file.write_all(header.as_bytes());
                    let _ = file.flush();
                }
                return;
            }
        }

        // 如果结果不同、引擎不同或为新会话，则在文件末尾写入新片段
        let _ = file.seek(SeekFrom::End(0));
        if let Ok(pos) = file.stream_position() {
            self.last_header_pos = pos;
        } else {
            self.last_header_pos = 0;
        }

        self.last_text = Some(text.to_string());
        self.start_time = Some(now);
        self.last_backend = Some(backend);

        let header = format!("=== [{}] [{} ~ {}] ===\r\n", backend.log_name(), time_str, time_str);
        let _ = file.write_all(header.as_bytes());
        let _ = file.write_all(text.as_bytes());
        let _ = file.write_all(b"\r\n\r\n");
        let _ = file.flush();
    }
}

fn main() -> Result<()> {
    // 1. 初始化跨线程共享的状态
    let shared_text = Arc::new(Mutex::new(String::from("等待选择区域...")));
    let shared_elapsed = Arc::new(Mutex::new(0u128));
    let shared_interval = Arc::new(Mutex::new(1000u128));
    let shared_paused = Arc::new(Mutex::new(true));
    let shared_region = Arc::new(Mutex::new(None));
    let settings = AppSettings::load();
    let shared_backend = Arc::new(Mutex::new(settings.selected_backend));
    let shared_baidu_token = Arc::new(Mutex::new(settings.baidu_token));
    let shared_baidu_model = Arc::new(Mutex::new(settings.baidu_model));
    let shared_baidu_use_orientation = Arc::new(Mutex::new(settings.baidu_use_orientation));
    let shared_baidu_use_unwarping = Arc::new(Mutex::new(settings.baidu_use_unwarping));
    let shared_baidu_use_chart = Arc::new(Mutex::new(settings.baidu_use_chart));

    // 引擎安装状态（默认 Unchecked，启动后台线程完成初始检测）
    let shared_tess_state   = Arc::new(Mutex::new(InstallState::Unchecked));
    let shared_paddle_state = Arc::new(Mutex::new(InstallState::Unchecked));
    let shared_rapid_state  = Arc::new(Mutex::new(InstallState::Unchecked));
    let shared_ocr_rs_state = Arc::new(Mutex::new(InstallState::Unchecked));
    let shared_oar_ocr_state = Arc::new(Mutex::new(InstallState::Unchecked));

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([480.0, 300.0])
        .with_position([100.0, 100.0])
        .with_always_on_top()
        .with_resizable(true)  // 允许拖动调整窗口大小
        .with_min_inner_size([450.0, 200.0]);  // 设置最小尺寸

    if let Ok(image) = image::load_from_memory(include_bytes!("../assets/logo.png")) {
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        viewport = viewport.with_icon(std::sync::Arc::new(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let shared_text_clone = shared_text.clone();
    let shared_elapsed_clone = shared_elapsed.clone();
    let shared_interval_clone = shared_interval.clone();
    let shared_paused_clone = shared_paused.clone();
    let shared_region_clone = shared_region.clone();
    let shared_backend_clone = shared_backend.clone();
    let shared_baidu_token_clone = shared_baidu_token.clone();
    let shared_baidu_model_clone = shared_baidu_model.clone();
    let shared_baidu_use_orientation_clone = shared_baidu_use_orientation.clone();
    let shared_baidu_use_unwarping_clone = shared_baidu_use_unwarping.clone();
    let shared_baidu_use_chart_clone = shared_baidu_use_chart.clone();

    eframe::run_native(
        "喵OCR",
        options,
        Box::new(move |cc| {
            // 动态加载系统内置中文字体
            let mut fonts = egui::FontDefinitions::default();
            let font_bytes = {
                #[cfg(target_os = "windows")]
                {
                    std::fs::read("C:\\Windows\\Fonts\\msyh.ttc").ok()
                        .or_else(|| std::fs::read("C:\\Windows\\Fonts\\msyhbd.ttc").ok())
                }
                #[cfg(target_os = "macos")]
                {
                    std::fs::read("/System/Library/Fonts/PingFang.ttc").ok()
                        .or_else(|| std::fs::read("/System/Library/Fonts/STHeiti Light.ttc").ok())
                        .or_else(|| std::fs::read("/System/Library/Fonts/Supplemental/Arial Unicode.ttf").ok())
                }
                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                {
                    None
                }
            };

            if let Some(bytes) = font_bytes {
                fonts.font_data.insert(
                    "chinese".to_owned(),
                    egui::FontData::from_owned(bytes),
                );
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "chinese".to_owned());
                cc.egui_ctx.set_fonts(fonts);
            }
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            cc.egui_ctx.style_mut(|style| {
                style.visuals.window_rounding = 8.0.into();
                
                let border_color_inactive = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 20);
                let border_color_hovered = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 40);
                let border_color_active = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 60);
                
                let bg_color_inactive = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 8);
                let bg_color_hovered = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 16);
                let bg_color_active = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 24);
                
                style.visuals.widgets.inactive.bg_fill = bg_color_inactive;
                style.visuals.widgets.inactive.weak_bg_fill = bg_color_inactive;
                style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border_color_inactive);
                style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
                style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(55, 65, 81));
                
                style.visuals.widgets.hovered.bg_fill = bg_color_hovered;
                style.visuals.widgets.hovered.weak_bg_fill = bg_color_hovered;
                style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, border_color_hovered);
                style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
                style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(17, 24, 39));
                
                style.visuals.widgets.active.bg_fill = bg_color_active;
                style.visuals.widgets.active.weak_bg_fill = bg_color_active;
                style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, border_color_active);
                style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);
                style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(17, 24, 39));

                style.visuals.widgets.open.rounding = egui::Rounding::same(6.0);
                style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(255, 255, 255);
                style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, border_color_inactive);

                style.visuals.selection.bg_fill = egui::Color32::from_rgb(219, 234, 254); // Soft pale blue
                style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 78, 216)); // Dark blue text/accent
                style.visuals.hyperlink_color = egui::Color32::from_rgb(37, 99, 235);
                
                style.spacing.button_padding = egui::vec2(10.0, 5.0);
                style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            });
            runtime_log("=== 喵OCR 启动 ===");

            // 3. 启动引擎环境初始检测线程（启动时静默检测 Tesseract / PaddleOCR / RapidOCR / PP-OCRv5 / oar-ocr）
            {
                let tess_state   = shared_tess_state.clone();
                let paddle_state = shared_paddle_state.clone();
                let rapid_state  = shared_rapid_state.clone();
                let ocr_rs_state = shared_ocr_rs_state.clone();
                let oar_ocr_state = shared_oar_ocr_state.clone();
                let ctx_detect = cc.egui_ctx.clone();
                std::thread::spawn(move || {
                    // 并发检测五个引擎
                    let t1 = {
                        let ts = tess_state.clone();
                        let c  = ctx_detect.clone();
                        std::thread::spawn(move || {
                            *ts.lock().unwrap() = InstallState::Checking;
                            c.request_repaint();
                            let avail = detect_tesseract();
                            runtime_log(&format!("[DETECT] Tesseract: {}",
                                if avail { "已安装" } else { "未安装" }));
                            *ts.lock().unwrap() = if avail { InstallState::Available } else { InstallState::NotInstalled };
                            c.request_repaint();
                        })
                    };
                    let t2 = {
                        let ps = paddle_state.clone();
                        let c  = ctx_detect.clone();
                        std::thread::spawn(move || {
                            *ps.lock().unwrap() = InstallState::Checking;
                            c.request_repaint();
                            let avail = detect_paddle();
                            runtime_log(&format!("[DETECT] PaddleOCR: {}",
                                if avail { "已安装" } else { "未安装" }));
                            *ps.lock().unwrap() = if avail { InstallState::Available } else { InstallState::NotInstalled };
                            c.request_repaint();
                        })
                    };
                    let t3 = {
                        let rs = rapid_state.clone();
                        let c  = ctx_detect.clone();
                        std::thread::spawn(move || {
                            *rs.lock().unwrap() = InstallState::Checking;
                            c.request_repaint();
                            let avail = detect_rapid();
                            runtime_log(&format!("[DETECT] RapidOCR: {}",
                                if avail { "已安装" } else { "未安装" }));
                            *rs.lock().unwrap() = if avail { InstallState::Available } else { InstallState::NotInstalled };
                            c.request_repaint();
                        })
                    };
                    let t4 = {
                        let os = ocr_rs_state.clone();
                        let c  = ctx_detect.clone();
                        std::thread::spawn(move || {
                            *os.lock().unwrap() = InstallState::Checking;
                            c.request_repaint();
                            let avail = detect_ocr_rs();
                            runtime_log(&format!("[DETECT] PP-OCRv5 (ocr-rs): {}",
                                if avail { "已安装" } else { "未安装" }));
                            *os.lock().unwrap() = if avail { InstallState::Available } else { InstallState::NotInstalled };
                            c.request_repaint();
                        })
                    };
                    let t5 = {
                        let os = oar_ocr_state.clone();
                        let c  = ctx_detect.clone();
                        std::thread::spawn(move || {
                            *os.lock().unwrap() = InstallState::Checking;
                            c.request_repaint();
                            let avail = detect_oar_ocr();
                            runtime_log(&format!("[DETECT] oar-ocr: {}",
                                if avail { "已安装" } else { "未安装" }));
                            *os.lock().unwrap() = if avail { InstallState::Available } else { InstallState::NotInstalled };
                            c.request_repaint();
                        })
                    };
                    let _ = t1.join();
                    let _ = t2.join();
                    let _ = t3.join();
                    let _ = t4.join();
                    let _ = t5.join();
                });
            }

            // 4. 启动常驻 OCR 后台轮询线程，并传入 egui_ctx 用于 UI 自动重绘
            let ctx_clone = cc.egui_ctx.clone();
            let text_for_thread = shared_text_clone.clone();
            let elapsed_for_thread = shared_elapsed_clone.clone();
            let interval_for_thread = shared_interval_clone.clone();
            let paused_for_thread = shared_paused_clone.clone();
            let region_for_thread = shared_region_clone.clone();
            let backend_for_thread = shared_backend_clone.clone();
            let baidu_token_for_thread = shared_baidu_token_clone.clone();
            let baidu_model_for_thread = shared_baidu_model_clone.clone();
            let baidu_use_orientation_for_thread = shared_baidu_use_orientation_clone.clone();
            let baidu_use_unwarping_for_thread = shared_baidu_use_unwarping_clone.clone();
            let baidu_use_chart_for_thread = shared_baidu_use_chart_clone.clone();

            std::thread::spawn(move || {
                let mut history = std::collections::VecDeque::with_capacity(10);
                let mut log_mgr = LogManager::new();
                let mut last_error: Option<String> = None; // 去重，避免同一错误反复写入运行日志
                let mut last_region: Option<(i32, i32, u32, u32)> = None;
                let mut last_active_backend: Option<BackendType> = None;
                let mut just_switched = true;
                loop {
                    let is_paused = *paused_for_thread.lock().unwrap();
                    let opt_region = *region_for_thread.lock().unwrap();
                    if opt_region.is_none() {
                        last_region = None;
                    }
                    if is_paused {
                        history.clear();
                        log_mgr.reset();
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        continue;
                    }

                    if let Some((x, y, w, h)) = opt_region {
                        if Some((x, y, w, h)) != last_region {
                            last_region = Some((x, y, w, h));
                            log_mgr.start_new_file();
                        }
                        let backend = *backend_for_thread.lock().unwrap();
                        if Some(backend) != last_active_backend {
                            last_active_backend = Some(backend);
                            history.clear(); // 切换后端引擎，重置耗时历史以立即使用新引擎的速度
                            just_switched = true;
                        }
                        let token = baidu_token_for_thread.lock().unwrap().clone();
                        let model = baidu_model_for_thread.lock().unwrap().clone();
                        let use_orientation = *baidu_use_orientation_for_thread.lock().unwrap();
                        let use_unwarping = *baidu_use_unwarping_for_thread.lock().unwrap();
                        let use_chart = *baidu_use_chart_for_thread.lock().unwrap();
                        let start = std::time::Instant::now();
                        match ocr_region(
                            x,
                            y,
                            w,
                            h,
                            backend,
                            &token,
                            &model,
                            use_orientation,
                            use_unwarping,
                            use_chart,
                        ) {
                            Ok(text) => {
                                let ms = start.elapsed().as_millis();
                                log_mgr.log(&text, backend);
                                *text_for_thread.lock().unwrap() = text;
                                *elapsed_for_thread.lock().unwrap() = ms;
                                last_error = None; // 成功后重置，下次出错时重新记录

                                if just_switched {
                                    just_switched = false;
                                } else {
                                    if history.len() >= 10 {
                                        history.pop_front();
                                    }
                                    history.push_back(ms);
                                }
                            }
                            Err(e) => {
                                let msg = format!("识别失败: {}", e);
                                let err_str = format!("[OCR ERROR] backend={:?} {}", backend, e);
                                // 仅当错误内容变化时才写入运行日志，避免高频刷写
                                if last_error.as_deref() != Some(&err_str) {
                                    runtime_log(&err_str);
                                    last_error = Some(err_str);
                                }
                                *text_for_thread.lock().unwrap() = msg;
                            }
                        }
                    }

                    // 动态决定下一次识别的休眠时间（取最近 10 次的最大耗时，辅以 50ms 下限保护，取消上限限制）
                    let sleep_ms = if history.is_empty() {
                        1000
                    } else {
                        let max_elapsed = *history.iter().max().unwrap_or(&0);
                        max_elapsed.max(50)
                    };

                    *interval_for_thread.lock().unwrap() = sleep_ms as u128;

                    // 请求 UI 重绘以更新最新结果、耗时和间隔
                    ctx_clone.request_repaint();

                    std::thread::sleep(std::time::Duration::from_millis(sleep_ms as u64));
                }
            });

            Box::new(FloatApp {
                mode: AppMode::Float,
                select_step: 0,
                frame_delay: 0,
                show_settings: false,
                ocr_region: shared_region,
                paused: shared_paused,
                text: shared_text,
                elapsed: shared_elapsed,
                interval: shared_interval,
                selected_backend: shared_backend,
                baidu_token: shared_baidu_token,
                baidu_model: shared_baidu_model,
                baidu_use_orientation: shared_baidu_use_orientation,
                baidu_use_unwarping: shared_baidu_use_unwarping,
                baidu_use_chart: shared_baidu_use_chart,
                tess_state:   shared_tess_state,
                paddle_state: shared_paddle_state,
                rapid_state:  shared_rapid_state,
                ocr_rs_state: shared_ocr_rs_state,
                oar_ocr_state: shared_oar_ocr_state,
                float_size: egui::vec2(480.0, 300.0),
                screenshot_texture: None,
                screenshot_raw: Vec::new(),
                screenshot_width: 0,
                screenshot_height: 0,
                active_screen_info: None,
                drag_start: None,
                drag_end: None,
                float_pos: egui::pos2(100.0, 100.0),
            }) as Box<dyn eframe::App>
        }),
    )
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}