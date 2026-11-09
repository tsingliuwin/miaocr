//! 外置 JSON 协议 OCR 子进程引擎（PaddleOCR-json / RapidOCR-json 共用）。

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::Result;

/// 通过 stdin/stdout 的 JSON 协议与外置 OCR 子进程通信的引擎句柄。
#[allow(dead_code)]
pub struct JsonOcrEngine {
    child: Child,
    stdout_reader: BufReader<std::process::ChildStdout>,
    stdin_writer: std::process::ChildStdin,
}

#[allow(dead_code)]
impl JsonOcrEngine {
    /// 启动子进程并等待其输出 `OCR init completed.`。
    pub fn new(exe_path: &Path) -> Result<Self> {
        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid executable path"))?;

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

        let stdin_writer = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open stdout"))?;
        let mut stdout_reader = BufReader::new(stdout);

        // Wait for "OCR init completed."
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = stdout_reader.read_line(&mut line)?;
            if bytes_read == 0 {
                return Err(anyhow::anyhow!(
                    "OCR engine exited or closed stdout prematurely during initialization"
                ));
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

    /// 对指定图片运行 OCR，返回文本。
    pub fn ocr(&mut self, image_path: &Path) -> Result<String> {
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
            return Err(anyhow::anyhow!(
                "OCR engine closed stdout connection unexpectedly"
            ));
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
            Err(anyhow::anyhow!(
                "OCR engine error: {} (code {})",
                err_msg,
                code
            ))
        }
    }
}

impl Drop for JsonOcrEngine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}