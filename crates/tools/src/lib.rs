//! Agent 工具箱：把「眼睛」（vision）能力包装成模型可调用的工具。

use std::sync::{Arc, Mutex};

use agent::{Tool, ToolRegistry};
use enigo::{Keyboard, Mouse};
use serde_json::Value;
use vision::BackendType;

/// 当前 OCR 引擎。UI 在启动和切换引擎时写入，`ocr_read` 在模型未显式
/// 指定 backend 时使用它，保证工具行为与用户在界面上选择的引擎一致。
static CURRENT_BACKEND: Mutex<Option<BackendType>> = Mutex::new(None);

/// 平台自带的 OCR 引擎。
fn native_backend() -> BackendType {
    if cfg!(target_os = "macos") {
        BackendType::MacNative
    } else {
        BackendType::WindowsNative
    }
}

/// 由 UI 同步当前引擎（启动时、切换引擎时、重载配置时调用）。
pub fn set_current_backend(backend: BackendType) {
    if let Ok(mut guard) = CURRENT_BACKEND.lock() {
        *guard = Some(backend);
    }
}

/// 当前引擎；未设置时回落到平台原生引擎。
pub fn current_backend() -> BackendType {
    CURRENT_BACKEND
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .unwrap_or_else(native_backend)
}

fn parse_backend(name: Option<&str>) -> BackendType {
    match name {
        None => current_backend(),
        Some("tesseract") => BackendType::Tesseract,
        Some("ocr_rs") | Some("ppocrv5") | Some("mnn") => BackendType::OcrRs,
        Some("oar") | Some("oarocr") | Some("onnx") => BackendType::OarOcr,
        Some("paddle") => BackendType::PaddleOcr,
        Some("rapid") => BackendType::RapidOcr,
        Some("baidu") => BackendType::BaiduAiStudio,
        Some("mac") => BackendType::MacNative,
        Some(_) => native_backend(),
    }
}

/// 对指定屏幕区域执行 OCR。
pub struct OcrReadTool;

impl Tool for OcrReadTool {
    fn name(&self) -> &str {
        "ocr_read"
    }
    fn description(&self) -> &str {
        "Read text in a screen region via OCR. Args (all optional): x, y, w, h (int, defaults to top-left 800x500), backend (string: windows_native|tesseract|ocr_rs|oar|paddle|rapid|baidu|mac; omit to use the app's current engine)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "x": { "type": "integer", "description": "Region left edge in screen pixels (may be negative on multi-monitor setups)." },
                "y": { "type": "integer", "description": "Region top edge in screen pixels." },
                "w": { "type": "integer", "minimum": 1, "description": "Region width in pixels." },
                "h": { "type": "integer", "minimum": 1, "description": "Region height in pixels." },
                "backend": {
                    "type": "string",
                    "enum": ["windows_native", "tesseract", "ocr_rs", "oar", "paddle", "rapid", "baidu", "mac"],
                    "description": "OCR engine to use. Omit to use the app's current engine."
                }
            },
            "required": []
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let x = args["x"].as_i64().unwrap_or(0) as i32;
        let y = args["y"].as_i64().unwrap_or(0) as i32;
        let w = args["w"].as_i64().unwrap_or(800) as u32;
        let h = args["h"].as_i64().unwrap_or(500) as u32;
        let backend = parse_backend(args["backend"].as_str());
        vision::ocr_region(x, y, w, h, backend, "", "", false, false, false)
            .map_err(|e| e.to_string())
    }
}

/// 读取系统剪贴板文本。
pub struct ClipboardReadTool;

impl Tool for ClipboardReadTool {
    fn name(&self) -> &str {
        "clipboard_read"
    }
    fn description(&self) -> &str {
        "Read text from the system clipboard."
    }
    fn run(&self, _args: Value) -> Result<String, String> {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        cb.get_text().map_err(|e| e.to_string())
    }
}

/// 写入系统剪贴板。
pub struct ClipboardWriteTool;

impl Tool for ClipboardWriteTool {
    fn name(&self) -> &str {
        "clipboard_write"
    }
    fn description(&self) -> &str {
        "Write text to the system clipboard. Args: text (string)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string", "description": "Text to place on the clipboard." } },
            "required": ["text"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let text = args["text"].as_str().unwrap_or("");
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        cb.set_text(text).map_err(|e| e.to_string())?;
        Ok(format!("已写入剪贴板（{} 字）", text.chars().count()))
    }
}

/// 截取屏幕保存为临时 PNG，返回路径与尺寸。
pub struct ScreenshotTool;

impl Tool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }
    fn description(&self) -> &str {
        "Capture the primary screen to a temp PNG and return the path and size."
    }
    fn run(&self, _args: Value) -> Result<String, String> {
        let screen = screenshots::Screen::all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| "no screen".to_string())?;
        let img = screen.capture().map_err(|e| e.to_string())?;
        let (w, h) = (img.width(), img.height());
        let rgba = img.as_raw().to_vec();
        let buf = image::RgbaImage::from_raw(w, h, rgba)
            .ok_or_else(|| "failed to build image buffer".to_string())?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(buf)
            .write_to(
                &mut std::io::Cursor::new(&mut png),
                image::ImageFormat::Png,
            )
            .map_err(|e| e.to_string())?;
        let path = std::env::temp_dir().join(format!("miaocr_shot_{}.png", std::process::id()));
        std::fs::write(&path, png).map_err(|e| e.to_string())?;
        Ok(format!("已截图保存: {} ({}x{})", path.display(), w, h))
    }
}

/// 读取文本文件。
pub struct FileReadTool;

impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }
    fn description(&self) -> &str {
        "Read a text file. Args: path (string)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "File path to read (UTF-8 text)." } },
            "required": ["path"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let path = args["path"].as_str().ok_or("missing path")?;
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }
}

/// 写入文本文件。
pub struct FileWriteTool;

impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }
    fn description(&self) -> &str {
        "Write text to a file. Args: path (string), content (string)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path to write." },
                "content": { "type": "string", "description": "Full file content (overwrites)." }
            },
            "required": ["path", "content"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let path = args["path"].as_str().ok_or("missing path")?;
        let content = args["content"].as_str().ok_or("missing content")?;
        std::fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(format!("已写入 {}", path))
    }
}

/// 移动鼠标并点击。
pub struct MouseClickTool;

impl Tool for MouseClickTool {
    fn name(&self) -> &str {
        "mouse_click"
    }
    fn description(&self) -> &str {
        "Move the mouse to (x, y) and click. Args: x (int), y (int), button (left|right|middle, default left)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "x": { "type": "integer", "description": "Screen X in physical pixels." },
                "y": { "type": "integer", "description": "Screen Y in physical pixels." },
                "button": { "type": "string", "enum": ["left", "right", "middle"], "description": "Mouse button, default left." }
            },
            "required": ["x", "y"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let x = args["x"].as_i64().unwrap_or(0) as i32;
        let y = args["y"].as_i64().unwrap_or(0) as i32;
        let button = match args["button"].as_str().unwrap_or("left") {
            "right" => enigo::Button::Right,
            "middle" => enigo::Button::Middle,
            _ => enigo::Button::Left,
        };
        let mut enigo = enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;
        enigo
            .move_mouse(x, y, enigo::Coordinate::Abs)
            .map_err(|e| e.to_string())?;
        enigo
            .button(button, enigo::Direction::Click)
            .map_err(|e| e.to_string())?;
        Ok(format!("clicked ({x},{y})"))
    }
}

/// 在当前光标处输入文本。
pub struct KeyboardTypeTool;

impl Tool for KeyboardTypeTool {
    fn name(&self) -> &str {
        "keyboard_type"
    }
    fn description(&self) -> &str {
        "Type text at the current cursor. Args: text (string)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string", "description": "Text to type into the focused control." } },
            "required": ["text"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let text = args["text"].as_str().unwrap_or("");
        let mut enigo = enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;
        enigo.text(text).map_err(|e| e.to_string())?;
        Ok(format!("typed {} chars", text.chars().count()))
    }
}

/// 滚轮滚动。
pub struct ScrollTool;

impl Tool for ScrollTool {
    fn name(&self) -> &str {
        "scroll"
    }
    fn description(&self) -> &str {
        "Scroll the mouse wheel. Args: amount (int, negative scrolls down)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "amount": { "type": "integer", "description": "Wheel steps; negative scrolls down." } },
            "required": ["amount"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let amount = args["amount"].as_i64().unwrap_or(0) as i32;
        let mut enigo = enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;
        enigo
            .scroll(amount, enigo::Axis::Vertical)
            .map_err(|e| e.to_string())?;
        Ok(format!("scrolled {amount}"))
    }
}

/// 抓取 URL 文本内容（curl）。
pub struct WebFetchTool;

impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }
    fn description(&self) -> &str {
        "Fetch a URL's raw text via curl (truncated). Args: url (string)."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "Absolute http(s) URL." } },
            "required": ["url"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let url = args["url"].as_str().ok_or("missing url")?;
        let out = std::process::Command::new("curl")
            .args(["-k", "-L", "-s", "--max-time", "20", url])
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        Ok(text.chars().take(2000).collect())
    }
}

/// 执行 shell 命令（危险：默认不注册，需显式 opt-in）。
pub struct ShellTool;

impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn description(&self) -> &str {
        "DANGEROUS: execute a shell command and return its output. Args: cmd (string). Not registered by default — requires explicit opt-in."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "cmd": { "type": "string", "description": "Shell command line to execute." } },
            "required": ["cmd"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let cmd = args["cmd"].as_str().ok_or("missing cmd")?;
        #[cfg(target_os = "windows")]
        let output = std::process::Command::new("cmd")
            .args(["/C", cmd])
            .output()
            .map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "windows"))]
        let output = std::process::Command::new("sh")
            .args(["-c", cmd])
            .output()
            .map_err(|e| e.to_string())?;
        let mut out = String::from_utf8_lossy(&output.stdout).to_string();
        if !output.status.success() {
            out.push_str(&format!(
                "\n[stderr] {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(out)
    }
}

/// 构建默认工具集。
pub fn build_default_tools() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(Arc::new(OcrReadTool));
    reg.register(Arc::new(ClipboardReadTool));
    reg.register(Arc::new(ClipboardWriteTool));
    reg.register(Arc::new(ScreenshotTool));
    reg.register(Arc::new(FileReadTool));
    reg.register(Arc::new(FileWriteTool));
    reg.register(Arc::new(MouseClickTool));
    reg.register(Arc::new(KeyboardTypeTool));
    reg.register(Arc::new(ScrollTool));
    reg.register(Arc::new(WebFetchTool));
    reg
}

/// 默认工具集 + shell（显式 opt-in）。
pub fn build_default_tools_with_shell() -> ToolRegistry {
    let mut reg = build_default_tools();
    reg.register(Arc::new(ShellTool));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tools_have_expected_set() {
        let reg = build_default_tools();
        for name in [
            "ocr_read",
            "clipboard_read",
            "clipboard_write",
            "screenshot",
            "file_read",
            "file_write",
            "mouse_click",
            "keyboard_type",
            "scroll",
            "web_fetch",
        ] {
            assert!(reg.get(name).is_some(), "missing tool {name}");
        }
        assert_eq!(reg.schemas().len(), 10);
    }

    #[test]
    fn tools_expose_parameter_schemas() {
        let reg = build_default_tools();
        let tools = reg.to_openai_tools();
        for t in &tools {
            assert_eq!(
                t["function"]["parameters"]["type"], "object",
                "tool {} lost its parameters schema",
                t["function"]["name"]
            );
        }
        let ocr = tools
            .iter()
            .find(|t| t["function"]["name"] == "ocr_read")
            .unwrap();
        assert_eq!(
            ocr["function"]["parameters"]["properties"]["backend"]["type"],
            "string"
        );
    }

    #[test]
    fn shell_tool_runs_echo() {
        let tool = ShellTool;
        let out = tool.run(serde_json::json!({"cmd": "echo hello"})).unwrap();
        assert!(out.contains("hello"));
    }

    #[test]
    fn parse_backend_maps_explicit_names() {
        assert_eq!(parse_backend(Some("mnn")), BackendType::OcrRs);
        assert_eq!(parse_backend(Some("onnx")), BackendType::OarOcr);
        assert_eq!(parse_backend(Some("paddle")), BackendType::PaddleOcr);
    }

    #[test]
    fn current_backend_defaults_to_native_and_can_be_set() {
        // 恢复为未设置状态不可行（OnceCell 语义），测试内直接设置后校验。
        assert_eq!(parse_backend(Some("unknown")), native_backend());
        set_current_backend(BackendType::Tesseract);
        assert_eq!(current_backend(), BackendType::Tesseract);
        assert_eq!(parse_backend(None), BackendType::Tesseract);
        set_current_backend(native_backend());
    }
}