//! LLM 能力接缝：模型适配器（`LlmClient` trait + OpenAI/DeepSeek 兼容实现 + Mock）。
//!
//! 支持 function calling：请求携带工具定义，响应携带 `tool_calls`。

use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::json;

/// 一次工具调用（LLM 发出）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 一条对话消息。
#[derive(Clone, Debug, Default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// 助手消息 + 工具调用。
    pub fn assistant_with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: String::new(),
            tool_calls,
            tool_call_id: None,
        }
    }

    /// 工具结果消息。
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// 一次 chat 请求。
#[derive(Clone, Debug, Default)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// OpenAI 格式的工具定义（JSON 数组）。
    pub tools: Vec<serde_json::Value>,
}

/// 一次 chat 响应。
#[derive(Clone, Debug, Default)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

impl ChatResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// LLM 模型适配器接缝（可替换：真实 API / Mock / 其它提供商）。
pub trait LlmClient: Send + Sync {
    fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, String>;
}

/// OpenAI / DeepSeek 兼容的 Chat Completions 客户端（同步 ureq）。
pub struct OpenAiCompatibleClient {
    base_url: String,
    api_key: String,
}

impl OpenAiCompatibleClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

impl OpenAiCompatibleClient {
    fn message_to_json(m: &ChatMessage) -> serde_json::Value {
        let mut obj = json!({ "role": m.role, "content": m.content });
        if !m.tool_calls.is_empty() {
            let tcs: Vec<_> = m
                .tool_calls
                .iter()
                .map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": { "name": tc.name, "arguments": tc.arguments }
                    })
                })
                .collect();
            obj["tool_calls"] = json!(tcs);
        }
        if let Some(id) = &m.tool_call_id {
            obj["tool_call_id"] = json!(id);
        }
        obj
    }
}

impl LlmClient for OpenAiCompatibleClient {
    fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let messages: Vec<_> = req.messages.iter().map(Self::message_to_json).collect();
        let mut body = json!({ "model": req.model, "messages": messages, "stream": false });
        if !req.tools.is_empty() {
            body["tools"] = json!(req.tools);
        }

        let resp = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_string(&body.to_string())
            .map_err(|e| e.to_string())?;

        let text = resp.into_string().map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let message = &json["choices"][0]["message"];
        let content = message["content"].as_str().unwrap_or("").to_string();
        let mut tool_calls = Vec::new();
        if let Some(tcs) = message["tool_calls"].as_array() {
            for tc in tcs {
                tool_calls.push(ToolCall {
                    id: tc["id"].as_str().unwrap_or("").to_string(),
                    name: tc["function"]["name"].as_str().unwrap_or("").to_string(),
                    arguments: tc["function"]["arguments"]
                        .as_str()
                        .unwrap_or("{}")
                        .to_string(),
                });
            }
        }
        Ok(ChatResponse {
            content,
            tool_calls,
        })
    }
}

/// 测试用 Mock：返回固定回复，并捕获最近一次请求。
pub struct CaptureClient {
    pub reply: String,
    pub last_request: Mutex<Option<ChatRequest>>,
}

impl CaptureClient {
    pub fn new(reply: impl Into<String>) -> Self {
        Self {
            reply: reply.into(),
            last_request: Mutex::new(None),
        }
    }

    pub fn last_request(&self) -> Option<ChatRequest> {
        self.last_request.lock().unwrap().clone()
    }
}

impl LlmClient for CaptureClient {
    fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        *self.last_request.lock().unwrap() = Some(req.clone());
        Ok(ChatResponse {
            content: self.reply.clone(),
            tool_calls: Vec::new(),
        })
    }
}

/// 测试用 Mock：按队列返回预定响应序列，并捕获所有请求。
pub struct ScriptedClient {
    responses: Mutex<VecDeque<ChatResponse>>,
    pub requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedClient {
    pub fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl LlmClient for ScriptedClient {
    fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        self.requests.lock().unwrap().push(req.clone());
        Ok(self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_default())
    }
}