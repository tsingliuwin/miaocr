//! 原生 / 云端 OCR 引擎：百度 AI Studio（云端）、macOS Vision（原生）。

use anyhow::Result;

use crate::save_temp_png;
#[cfg(target_os = "macos")]
use crate::{data_dir, log};

/// 百度 AI Studio 云端 OCR（提交任务 → 增量退避轮询 → 下载解析结果）。
pub fn ocr_baidu_aistudio(
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
        return Err(anyhow::anyhow!(
            "提交任务 API 错误: {}",
            resp["msg"].as_str().unwrap_or("未知错误")
        ));
    }

    let job_id = resp["data"]["jobId"]
        .as_str()
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
                .arg(format!(
                    "https://paddleocr.aistudio-app.com/api/v2/ocr/jobs/{}",
                    job_id
                ))
                .output()?;

            if poll_out.status.success() {
                let poll_resp_str = String::from_utf8_lossy(&poll_out.stdout);
                if let Ok(poll_resp) = serde_json::from_str::<serde_json::Value>(&poll_resp_str) {
                    if poll_resp["code"] != 0 {
                        return Err(anyhow::anyhow!(
                            "轮询 API 错误: {}",
                            poll_resp["msg"].as_str().unwrap_or("未知错误")
                        ));
                    }

                    let state = poll_resp["data"]["state"].as_str().unwrap_or("");
                    if state == "done" {
                        if let Some(url) = poll_resp["data"]["resultUrl"]["jsonUrl"].as_str() {
                            break url.to_string();
                        } else {
                            return Err(anyhow::anyhow!("任务已完成，但未找到 jsonUrl"));
                        }
                    } else if state == "failed" {
                        let err_msg = poll_resp["data"]["errorMsg"]
                            .as_str()
                            .unwrap_or("未知错误原因");
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
    let bin_path = data_dir().join("macocr_bin_v2");
    if bin_path.exists() {
        return Ok(bin_path);
    }

    let swift_code = include_str!("mac_ocr.swift");
    let swift_path = data_dir().join("mac_ocr.swift");
    std::fs::write(&swift_path, swift_code)?;

    log("[SWIFT] 正在首次编译 macOS Vision OCR 辅助程序...");
    let status = std::process::Command::new("swiftc")
        .arg("-O")
        .arg(swift_path.to_str().unwrap())
        .arg("-o")
        .arg(bin_path.to_str().unwrap())
        .status();

    let _ = std::fs::remove_file(swift_path);

    match status {
        Ok(s) if s.success() => {
            log("[SWIFT] 辅助程序编译成功");
            Ok(bin_path)
        }
        Ok(s) => Err(anyhow::anyhow!("Swift 编译器退出异常: {}", s)),
        Err(e) => Err(anyhow::anyhow!("启动 Swift 编译器失败: {}", e)),
    }
}

#[cfg(target_os = "macos")]
pub fn ocr_mac_native(bgra: &[u8], width: u32, height: u32) -> Result<String> {
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
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("无法打开子进程 stdin"))?;
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
        let combined = if err.is_empty() {
            out
        } else if out.is_empty() {
            err
        } else {
            format!("{}\n{}", err, out)
        };
        Err(anyhow::anyhow!(
            "Vision OCR 运行出错: {}",
            combined.trim()
        ))
    }
}