//! 图像预处理：大津法二值化、临时 PNG 落盘。

use anyhow::Result;

/// 对大津法（Otsu）二值化 + 智能反色。
///
/// 仅对 Windows 原生 / Tesseract 这类传统引擎使用；PaddleOCR 等深度学习模型
/// 直接基于原图推理效果更好。
pub fn binarize_bgra(bgra: &mut [u8]) {
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

/// 将 BGRA 字节保存为临时 PNG，返回落盘路径。
pub fn save_temp_png(bgra: &[u8], width: u32, height: u32) -> Result<std::path::PathBuf> {
    let temp_dir = std::env::temp_dir();
    let temp_img_path = temp_dir.join("miaocr_temp.png");

    let rgba_bytes: Vec<u8> = bgra
        .chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();

    let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width, height, rgba_bytes)
        .ok_or_else(|| anyhow::anyhow!("Failed to convert image buffer for saving"))?;

    img_buffer.save(&temp_img_path)?;
    Ok(temp_img_path)
}