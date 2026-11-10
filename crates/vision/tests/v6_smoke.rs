//! PP-OCRv6 ONNX 模型与现有 oar-ocr 0.2.2 管线的兼容性冒烟测试。
//!
//! 模型文件需先存在于 ~/.miaocr/models/（ppocrv6_small_{det,rec}.onnx +
//! ppocrv6_dict.txt）。缺文件时跳过，不算失败。

use std::path::PathBuf;

fn models_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap();
    PathBuf::from(home).join(".miaocr").join("models")
}

#[test]
fn oar_ocr_v6_smoke() {
    let dir = models_dir();
    let det = dir.join("ppocrv6_small_det.onnx");
    let rec = dir.join("ppocrv6_small_rec.onnx");
    let keys = dir.join("ppocrv6_dict.txt");
    let test_img = dir.join("v6_test.png");
    if !(det.exists() && rec.exists() && keys.exists() && test_img.exists()) {
        eprintln!("跳过：v6 模型或测试图不存在");
        return;
    }

    let img = image::open(&test_img).unwrap().to_rgb8();
    let engine = oar_ocr::pipeline::OAROCRBuilder::new(
        det.to_string_lossy().to_string(),
        rec.to_string_lossy().to_string(),
        keys.to_string_lossy().to_string(),
    )
    .text_detection_batch_size(1)
    .text_recognition_batch_size(1)
    // PP-OCRv6 官方 DB 后处理参数（oar-ocr 0.10 预设同款）
    .text_det_threshold(0.2)
    .text_det_box_threshold(0.45)
    .text_det_unclip_ratio(1.4)
    .text_det_max_side_limit(4000)
    .text_rec_score_threshold(0.0)
    .text_rec_input_shape((3, 48, 320))
    .build()
    .expect("构建 v6 引擎失败（模型图不兼容？）");

    let results = engine.predict(&[img]).expect("v6 推理失败");
    let text = results
        .first()
        .map(|r| r.concatenated_text("\n"))
        .unwrap_or_default();
    eprintln!("v6 识别结果: {text:?}");
    assert!(
        text.contains("识别测试") && text.contains("123"),
        "识别文本不符: {text:?}"
    );
}
