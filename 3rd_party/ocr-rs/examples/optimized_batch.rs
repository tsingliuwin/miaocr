//! 优化的批量识别示例
//!
//! 展示新的优化功能：
//! 1. 真正的批量推理
//! 2. 减少内存克隆
//! 3. 并行处理支持

use ocr_rs::{OcrEngine, OcrEngineConfig};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::init();

    println!("=== OCR 批量识别性能优化示例 ===\n");

    // 模型路径
    let det_model = "models/PP-OCRv5_mobile_det_fp16.mnn";
    let rec_model = "models/PP-OCRv5_mobile_rec_fp16.mnn";
    let charset = "models/ppocr_keys_v5.txt";

    // 测试图像
    let test_image = "res/Paste_1221144147238.png";

    if !std::path::Path::new(test_image).exists() {
        eprintln!("测试图像不存在: {}", test_image);
        return Ok(());
    }

    // ============ 1. 默认配置（序列批量推理）============
    println!("1️⃣  默认配置 - 序列批量推理");
    let config_default = OcrEngineConfig::fast();

    let engine_default =
        OcrEngine::new(det_model, rec_model, charset, Some(config_default))?;
    let image = image::open(test_image)?;

    let start = Instant::now();
    let results_default = engine_default.recognize(&image)?;
    let duration_default = start.elapsed();

    println!("   检测到 {} 个文本区域", results_default.len());
    println!("   耗时: {:.2}ms", duration_default.as_secs_f64() * 1000.0);
    println!();

    // ============ 2. 启用并行处理 ============
    println!("2️⃣  启用并行处理 - Rayon 并行识别");
    let config_parallel = OcrEngineConfig::fast().with_parallel(true);

    let engine_parallel =
        OcrEngine::new(det_model, rec_model, charset, Some(config_parallel))?;

    let start = Instant::now();
    let results_parallel = engine_parallel.recognize(&image)?;
    let duration_parallel = start.elapsed();

    println!("   检测到 {} 个文本区域", results_parallel.len());
    println!("   耗时: {:.2}ms", duration_parallel.as_secs_f64() * 1000.0);

    let speedup = duration_default.as_secs_f64() / duration_parallel.as_secs_f64();
    println!("   加速比: {:.2}x", speedup);
    println!();

    // ============ 3. 显示识别结果 ============
    println!("3️⃣  识别结果：");
    for (i, result) in results_parallel.iter().enumerate().take(5) {
        println!(
            "   [{}] 文本: {}, 置信度: {:.2}%",
            i + 1,
            result.text,
            result.confidence * 100.0
        );
    }

    if results_parallel.len() > 5 {
        println!("   ... 还有 {} 个结果", results_parallel.len() - 5);
    }
    println!();

    // ============ 4. 性能对比总结 ============
    println!("📊 性能对比总结：");
    println!(
        "   序列批量推理: {:.2}ms",
        duration_default.as_secs_f64() * 1000.0
    );
    println!(
        "   并行处理:     {:.2}ms ({})",
        duration_parallel.as_secs_f64() * 1000.0,
        if duration_parallel < duration_default {
            "✅ 更快"
        } else {
            "⚠️  更慢"
        }
    );
    Ok(())
}
