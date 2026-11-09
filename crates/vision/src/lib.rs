//! miaocr 的「眼睛」层：OCR 后端引擎、图像预处理、统一分发与引擎检测/下载。
//!
//! 本 crate 对 UI 框架（egui / gpui）零依赖：通过 [`init`] 注入数据目录、日志与重绘回调，
//! 以便被任意前端复用。

pub mod preprocess;
pub mod tesseract;
pub mod json_engine;
pub mod engines;
pub mod native;
pub mod region;
pub mod detect;
pub mod install;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub use preprocess::{binarize_bgra, save_temp_png};
pub use tesseract::{get_tesseract_path, ocr_tesseract};
pub use json_engine::JsonOcrEngine;
pub use engines::{ocr_oar_ocr, ocr_ocr_rs, ocr_paddle, ocr_rapid};
pub use native::ocr_baidu_aistudio;
#[cfg(target_os = "macos")]
pub use native::ocr_mac_native;
pub use region::ocr_region;
pub use detect::{
    detect_oar_ocr, detect_ocr_rs, detect_paddle, detect_rapid, detect_tesseract,
    download_file_with_mirrors, ensure_chi_sim,
};
pub use install::{
    start_oar_ocr_install, start_ocr_rs_install, start_paddle_install, start_rapid_install,
    start_tesseract_install,
};

/// OCR 后端引擎枚举
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BackendType {
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
    pub fn display_name(&self) -> &'static str {
        match self {
            BackendType::WindowsNative => "Windows 原生",
            BackendType::MacNative => "macOS 原生",
            BackendType::Tesseract => "Tesseract",
            BackendType::PaddleOcr => "PaddleOCR-json",
            BackendType::RapidOcr => "RapidOCR-json",
            BackendType::BaiduAiStudio => "百度 AI Studio",
            BackendType::OcrRs => "PP-OCRv5 (MNN)",
            BackendType::OarOcr => "OarOCR (ONNX)",
        }
    }

    pub fn log_name(&self) -> &'static str {
        match self {
            BackendType::WindowsNative => "WindowsNative",
            BackendType::MacNative => "MacNative",
            BackendType::Tesseract => "Tesseract",
            BackendType::PaddleOcr => "PaddleOCR",
            BackendType::RapidOcr => "RapidOCR",
            BackendType::BaiduAiStudio => "BaiduAiStudio",
            BackendType::OcrRs => "OcrRs",
            BackendType::OarOcr => "OarOcr",
        }
    }
}

/// 引擎安装状态
#[derive(Debug, Clone, PartialEq)]
pub enum InstallState {
    Unchecked,
    Checking,
    Available,
    NotInstalled,
    Installing(String),
    Failed(String),
}

pub type LogFn = Arc<dyn Fn(&str) + Send + Sync>;
pub type RepaintFn = Arc<dyn Fn() + Send + Sync>;

struct Ctx {
    data_dir: PathBuf,
    log: LogFn,
    repaint: RepaintFn,
}

static CTX: OnceLock<Ctx> = OnceLock::new();

/// 初始化 vision 运行时上下文（应用启动时调用一次）。
pub fn init(data_dir: PathBuf, log: LogFn, repaint: RepaintFn) {
    let _ = CTX.set(Ctx {
        data_dir,
        log,
        repaint,
    });
}

pub(crate) fn data_dir() -> PathBuf {
    CTX.get()
        .map(|c| c.data_dir.clone())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn log(msg: &str) {
    if let Some(c) = CTX.get() {
        (c.log)(msg);
    }
}

pub(crate) fn repaint() {
    if let Some(c) = CTX.get() {
        (c.repaint)();
    }
}