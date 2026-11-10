//! 本地 OCR 引擎封装：PaddleOCR-json / RapidOCR-json（外置子进程）、
//! PP-OCRv5（MNN，内置）、oar-ocr（ONNX，内置）。

use std::cell::RefCell;
use std::path::PathBuf;

use anyhow::Result;

use crate::detect::{detect_oar_ocr, detect_ocr_rs};
use crate::{data_dir, log, save_temp_png, JsonOcrEngine};

#[allow(dead_code)]
pub(crate) fn find_paddle_exe() -> Option<PathBuf> {
    let base_dir = data_dir().join("PaddleOCR-json");
    let names = ["PaddleOCR-json.exe", "PaddleOCR_json.exe"];
    for name in &names {
        let p = base_dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
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
pub(crate) fn find_rapid_exe() -> Option<PathBuf> {
    let base_dir = data_dir().join("RapidOCR-json");
    let names = ["RapidOCR-json.exe", "RapidOCR_json.exe"];
    for name in &names {
        let p = base_dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
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
            log("[ENGINE] 切换后端，释放 PaddleOCR-json 进程");
            *lock = None;
        }
    });
}

fn clear_rapid_engine() {
    RAPID_OCR_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_some() {
            log("[ENGINE] 切换后端，释放 RapidOCR-json 进程");
            *lock = None;
        }
    });
}

fn clear_ocr_rs_engine() {
    OCR_RS_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_some() {
            log("[ENGINE] 切换后端，释放 PP-OCRv5 (ocr-rs) 引擎");
            *lock = None;
        }
    });
}

fn clear_oar_ocr_engine() {
    OAR_OCR_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_some() {
            log("[ENGINE] 切换后端，释放 oar-ocr (本地) 引擎");
            *lock = None;
        }
    });
}

/// 切换后端前释放其它引擎，仅保留当前选中的引擎（若有）。
pub(crate) fn clear_engines_except(backend: crate::BackendType) {
    if backend != crate::BackendType::PaddleOcr {
        clear_paddle_engine();
    }
    if backend != crate::BackendType::RapidOcr {
        clear_rapid_engine();
    }
    if backend != crate::BackendType::OcrRs {
        clear_ocr_rs_engine();
    }
    if backend != crate::BackendType::OarOcr {
        clear_oar_ocr_engine();
    }
}

thread_local! {
    static PADDLE_OCR_ENGINE: RefCell<Option<JsonOcrEngine>> = RefCell::new(None);
    static RAPID_OCR_ENGINE: RefCell<Option<JsonOcrEngine>> = RefCell::new(None);
    static OCR_RS_ENGINE: RefCell<Option<ocr_rs::OcrEngine>> = RefCell::new(None);
    static OAR_OCR_ENGINE: RefCell<Option<oar_ocr::pipeline::OAROCR>> = RefCell::new(None);
}

pub fn ocr_paddle(bgra: &[u8], width: u32, height: u32) -> Result<String> {
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

pub fn ocr_rapid(bgra: &[u8], width: u32, height: u32) -> Result<String> {
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

pub fn ocr_ocr_rs(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let mut err_opt = None;

    let res = OCR_RS_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_none() {
            log("[OCR-RS] 初始化引擎: 检测模型文件...");
            if !detect_ocr_rs() {
                err_opt = Some(anyhow::anyhow!("本地 PP-OCRv5 引擎模型未安装。请在界面中点击「安装」。"));
                return None;
            }
            let models_dir = data_dir().join("models");
            let det_path = models_dir.join("PP-OCRv5_mobile_det.mnn");
            let rec_path = models_dir.join("PP-OCRv5_mobile_rec.mnn");
            let keys_path = models_dir.join("ppocr_keys_v5.txt");

            log(&format!(
                "[OCR-RS] 加载模型: det={}, rec={}, keys={}",
                det_path.display(),
                rec_path.display(),
                keys_path.display()
            ));

            // 禁用并行识别 + 单线程推理：
            // 1. rayon 并行会导致多线程同时调用 MNN FFI 产生数据竞争
            // 2. MNN 默认 4 线程，在后台 OCR 线程中可能与其他组件竞争资源
            let config = ocr_rs::OcrEngineConfig::new()
                .with_parallel(false)
                .with_threads(1);
            log("[OCR-RS] OcrEngine::new 开始...");
            match ocr_rs::OcrEngine::new(&det_path, &rec_path, &keys_path, Some(config)) {
                Ok(engine) => {
                    log("[OCR-RS] 引擎初始化成功");
                    *lock = Some(engine);
                }
                Err(e) => {
                    log(&format!("[OCR-RS] 引擎初始化失败: {:?}", e));
                    err_opt = Some(anyhow::anyhow!("初始化 PP-OCRv5 引擎失败: {:?}", e));
                    return None;
                }
            }
        }

        let engine = lock.as_mut().unwrap();

        let rgba_bytes: Vec<u8> = bgra
            .chunks_exact(4)
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
        let ocr_res =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.recognize(&img)));

        match ocr_res {
            Ok(Ok(results)) => {
                let mut lines = Vec::new();
                for item in results {
                    lines.push(item.text);
                }
                Some(lines.join("\n"))
            }
            Ok(Err(e)) => {
                log(&format!("[OCR-RS] 识别返回错误: {:?}", e));
                err_opt = Some(anyhow::anyhow!("PP-OCRv5 识别失败: {:?}", e));
                None
            }
            Err(panic_info) => {
                let msg = format!("PP-OCRv5 引擎 panic: {:?}", panic_info);
                log(&format!("[OCR-RS] {}", msg));
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

pub fn ocr_oar_ocr(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let mut err_opt = None;

    let res = OAR_OCR_ENGINE.with(|cell| {
        let mut lock = cell.borrow_mut();
        if lock.is_none() {
            if !detect_oar_ocr() {
                err_opt = Some(anyhow::anyhow!("本地 oar-ocr 引擎模型未安装。请在界面中点击「安装」。"));
                return None;
            }
            let models_dir = data_dir().join("models");
            // PP-OCRv6（ONNX，small 档）优先；老安装只有 v5 文件时回落 v5。
            // 检测后处理参数随模型版本切换：v6 官方 DB 默认
            //（score 0.2 / box 0.45 / unclip 1.4，oar-ocr 0.10 预设同款），
            // v5 沿用旧值（0.3 / 0.6 / 1.5）。
            let v6 = (
                models_dir.join("ppocrv6_small_det.onnx"),
                models_dir.join("ppocrv6_small_rec.onnx"),
                models_dir.join("ppocrv6_dict.txt"),
            );
            let use_v6 = v6.0.exists() && v6.1.exists() && v6.2.exists();
            let (det_path, rec_path, keys_path, (th, box_th, unclip)) = if use_v6 {
                log("[OAR-OCR] 使用 PP-OCRv6 small (ONNX)");
                (v6.0, v6.1, v6.2, (0.2, 0.45, 1.4))
            } else {
                log("[OAR-OCR] 使用 PP-OCRv5 mobile (ONNX)");
                (
                    models_dir.join("ppocrv5_mobile_det.onnx"),
                    models_dir.join("ppocrv5_mobile_rec.onnx"),
                    models_dir.join("ppocrv5_dict.txt"),
                    (0.3, 0.6, 1.5),
                )
            };

            match oar_ocr::pipeline::OAROCRBuilder::new(
                det_path.to_string_lossy().to_string(),
                rec_path.to_string_lossy().to_string(),
                keys_path.to_string_lossy().to_string(),
            )
            .text_detection_batch_size(1)
            .text_recognition_batch_size(1)
            .text_det_threshold(th)
            .text_det_box_threshold(box_th)
            .text_det_unclip_ratio(unclip)
            .text_det_max_side_limit(4000)
            .text_rec_score_threshold(0.0)
            .text_rec_input_shape((3, 48, 320))
            .build()
            {
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

        let rgb_bytes: Vec<u8> = bgra
            .chunks_exact(4)
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