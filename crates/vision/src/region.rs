//! 屏幕截图 + OCR 统一分发入口。

use anyhow::Result;
use screenshots::Screen;

use crate::engines::{clear_engines_except, ocr_oar_ocr, ocr_ocr_rs, ocr_paddle, ocr_rapid};
use crate::native::ocr_baidu_aistudio;
use crate::{binarize_bgra, ocr_tesseract, BackendType};

#[cfg(target_os = "windows")]
use windows::{
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Globalization::Language,
    Media::Ocr::OcrEngine,
    Storage::Streams::Buffer,
    Win32::System::WinRT::IBufferByteAccess,
    core::{HSTRING, Interface},
};

/// 截取指定屏幕区域并调用所选 OCR 引擎识别，返回文本。
pub fn ocr_region(
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

    let image = screen
        .capture_area(local_x, local_y, capture_width, capture_height)
        .map_err(|e| anyhow::anyhow!("SCREEN_CAPTURE_FAILED: {}", e))?;
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

    clear_engines_except(backend);

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
                        crate::log(&format!("Windows OCR 'zh-CN' 支持情况: {}", supported));
                    }
                    if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                        if let Ok(rec_lang) = engine.RecognizerLanguage() {
                            if let Ok(tag) = rec_lang.LanguageTag() {
                                crate::log(&format!("当前实际使用的 OCR 语言: {}", tag));
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
                let text = crate::native::ocr_mac_native(&bgra, final_width, final_height)?;
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