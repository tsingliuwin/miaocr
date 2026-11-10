//! Agent 桥接：组装 miaocr 系统提示词并调用 LLM，把「眼睛」与「大脑」接起来。
//!
//! 「问 AI」UI 已暂时下线（2026-11），本模块整体保留待恢复：
//! 数据结构与 ~/.miaocr/agent_config.json 的读写不变，重新接线时
//! 只需在 ocr_worker 恢复 Agent 任务并在 UI 加回输入框即可。
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use agent::{order, render_context_snapshot, PromptContext, PromptSection, SystemPrompt};
use llm::{ChatMessage, OpenAiCompatibleClient};
use serde::{Deserialize, Serialize};

/// 应用数据目录（~/.miaocr）。
pub fn data_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let dir = home.join(".miaocr");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Agent 配置（DeepSeek/OpenAI 兼容），持久化到 ~/.miaocr/agent_config.json。
#[derive(Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "deepseek-chat".to_string(),
            base_url: "https://api.deepseek.com".to_string(),
        }
    }
}

impl AgentConfig {
    pub fn load() -> Self {
        let path = data_dir().join("agent_config.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn save(&self) {
        let path = data_dir().join("agent_config.json");
        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    eprintln!("[config] 写入 {} 失败: {e}", path.display());
                }
            }
            Err(e) => eprintln!("[config] 序列化 agent_config 失败: {e}"),
        }
    }
}

/// 引擎与界面设置，持久化到 ~/.miaocr/settings.json。
/// 字段与旧 egui 版保持一致，两个前端可以互相读取。
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub selected_backend: Option<String>,
    #[serde(default)]
    pub baidu_token: String,
    #[serde(default)]
    pub baidu_model: String,
    #[serde(default)]
    pub baidu_use_orientation: bool,
    #[serde(default)]
    pub baidu_use_unwarping: bool,
    #[serde(default)]
    pub baidu_use_chart: bool,
}

impl AppSettings {
    pub fn load() -> Self {
        let path = data_dir().join("settings.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = data_dir().join("settings.json");
        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    // 不再静默失败：写不进去时至少要能在日志里看到
                    eprintln!("[config] 写入 {} 失败: {e}", path.display());
                }
            }
            Err(e) => eprintln!("[config] 序列化 settings 失败: {e}"),
        }
    }

    /// 百度云端模型：空值回落到默认模型。
    pub fn baidu_model(&self) -> &str {
        if self.baidu_model.trim().is_empty() {
            "PaddleOCR-VL-1.6"
        } else {
            &self.baidu_model
        }
    }
}

/// 交给 Agent 的运行时事实（进入 user-role 动态上下文快照）。
#[derive(Clone, Default)]
pub struct AgentContext {
    /// 当前选区 (x, y, w, h)，None 表示尚未选区。
    pub region: Option<(i32, i32, u32, u32)>,
    /// 当前 OCR 引擎的展示名。
    pub backend: String,
    /// 最近一次识别结果。
    pub ocr_text: String,
}

/// 构建 miaocr 的系统提示词（借鉴 deepseek-harness 的 order 分带约定）。
pub fn build_system_prompt(model: &str, backend: &str) -> SystemPrompt {
    let mut sp = SystemPrompt::new();
    sp.add_section(PromptSection::new(
        "agent:identity",
        order::IDENTITY,
        "You are Mia Agent, a screen-intelligence assistant powered by {{model}}.",
    ))
    .unwrap();
    sp.add_section(PromptSection::new(
        "agent:persona",
        order::PERSONA,
        "Your working directory is {{cwd}}. You run on {{os}}, using the {{active_engine}} OCR engine.\n\
         You can SEE the user's screen through OCR and screenshots. Ground every statement \
         in what you actually observed; never invent OCR text, pixel positions, or screen content. \
         If you are unsure what is on screen, capture it or say so. Reply concisely in the same \
         language the user used.",
    ))
    .unwrap();
    sp.add_section(PromptSection::new(
        "mode:observe",
        order::MODE_BEGIN,
        "Default mode: observe and explain. Do not click, type, scroll, or write files unless the \
         user explicitly asks you to act; when you do act, state your intent in one sentence first.",
    ))
    .unwrap();

    let model = model.to_string();
    sp.add_variable("model", Arc::new(move |_| Some(model.clone())))
        .unwrap();
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());
    sp.add_variable("cwd", Arc::new(move |_| Some(cwd.clone())))
        .unwrap();
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    sp.add_variable("os", Arc::new(move |_| Some(os.to_string())))
        .unwrap();
    let backend = backend.to_string();
    sp.add_variable("active_engine", Arc::new(move |_| Some(backend.clone())))
        .unwrap();
    sp
}

/// 运行时上下文快照正文（不含 DSH 开头句，由 render_context_snapshot 添加）。
fn snapshot_body(ctx: &AgentContext) -> String {
    let mut lines = Vec::new();
    match ctx.region {
        Some((x, y, w, h)) => lines.push(format!("Selected screen region: x={x}, y={y}, w={w}, h={h}")),
        None => lines.push("Selected screen region: none (ask the user to select one)".to_string()),
    }
    lines.push(format!("Active OCR engine: {}", ctx.backend));
    let text = ctx.ocr_text.trim();
    if !text.is_empty() {
        let mut excerpt: String = text.chars().take(600).collect();
        if text.chars().count() > 600 {
            excerpt.push('…');
        }
        lines.push(format!("Latest OCR text:\n{excerpt}"));
    }
    lines.join("\n")
}

/// 把 OCR 文本 + 运行时快照 + 用户问题交给 Agent（多轮 + 工具循环）。
///
/// `history` 是上一轮返回的会话历史；返回 `(本轮回答, 更新后的历史)`。
/// 调用方（UI / worker）负责保存历史，并在重新选区或清空时丢弃它。
pub fn run_agent(
    ctx: &AgentContext,
    config: &AgentConfig,
    question: &str,
    mut history: Vec<ChatMessage>,
) -> Result<(String, Vec<ChatMessage>), String> {
    if config.api_key.trim().is_empty() {
        return Err("未配置 API Key：请在设置页填入，或编辑 ~/.miaocr/agent_config.json".into());
    }
    let mut sp = build_system_prompt(&config.model, &ctx.backend);
    let registry = tools::build_default_tools();
    registry.add_to_prompt(&mut sp);

    let body = snapshot_body(ctx);
    sp.add_context(PromptContext::new("runtime", 0, body));
    let assembly = sp.assemble();
    let snapshot = render_context_snapshot(&assembly).map_err(|e| e.to_string())?;

    let client = OpenAiCompatibleClient::new(config.base_url.clone(), config.api_key.clone());
    let agent = agent::Agent::new(sp, &client);

    let ocr = if ctx.ocr_text.trim().is_empty() {
        "（当前没有识别到文字，可调用 screenshot 或 ocr_read 获取）"
    } else {
        ctx.ocr_text.trim()
    };
    let question = question.trim();
    let prompt = if question.is_empty() {
        format!(
            "以下是我从屏幕上识别到的文字：\n{ocr}\n\n请简要说明这段内容。\
             若需要识别屏幕上其它区域，可调用 ocr_read 工具（不要臆测未见内容）。"
        )
    } else {
        format!(
            "以下是我从屏幕上识别到的文字：\n{ocr}\n\n我的问题：{question}\n\n\
             如果现有文字不足以回答，可调用 ocr_read / screenshot 工具补充观察；不要臆测未见内容。"
        )
    };

    let answer = agent.run_turn(&mut history, &prompt, &config.model, &registry, Some(&snapshot))?;
    Ok((answer, history))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_has_identity_persona_and_mode() {
        let sp = build_system_prompt("test-model", "Windows 原生");
        let rendered = agent::render(&sp.assemble()).unwrap();
        assert!(rendered.contains("powered by test-model"));
        assert!(rendered.contains("Windows 原生"));
        assert!(rendered.contains("Default mode: observe"));
        assert!(!rendered.contains("{{"));
    }

    #[test]
    fn snapshot_lists_region_and_engine() {
        let ctx = AgentContext {
            region: Some((10, 20, 300, 200)),
            backend: "Tesseract".into(),
            ocr_text: "hello".into(),
        };
        let body = snapshot_body(&ctx);
        assert!(body.contains("x=10, y=20, w=300, h=200"));
        assert!(body.contains("Active OCR engine: Tesseract"));
        assert!(body.contains("hello"));
    }

    #[test]
    fn empty_region_is_stated_explicitly() {
        let body = snapshot_body(&AgentContext::default());
        assert!(body.contains("region: none"));
        assert!(!body.contains("Latest OCR text"));
    }

    #[test]
    fn baidu_model_falls_back_when_empty() {
        let s = AppSettings::default();
        assert_eq!(s.baidu_model(), "PaddleOCR-VL-1.6");
        let s2 = AppSettings {
            baidu_model: "custom".into(),
            ..Default::default()
        };
        assert_eq!(s2.baidu_model(), "custom");
    }
}
