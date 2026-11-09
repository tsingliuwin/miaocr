//! Agent 循环：组装系统提示词 → 调 LLM →（可选）执行工具 → 返回回复。

use crate::{render, SystemPrompt, ToolRegistry};
use llm::{ChatMessage, ChatRequest, LlmClient};

/// 一个 Agent：持有系统提示词组装器与 LLM 客户端。
pub struct Agent<'a> {
    prompt: SystemPrompt,
    client: &'a dyn LlmClient,
}

impl<'a> Agent<'a> {
    pub fn new(prompt: SystemPrompt, client: &'a dyn LlmClient) -> Self {
        Self { prompt, client }
    }

    fn render_system(&self) -> Result<String, String> {
        let assembly = self.prompt.assemble();
        render(&assembly).map_err(|e| e.to_string())
    }

    /// 单轮：组装 system prompt + 用户消息，调 LLM，返回回复文本。
    pub fn run(&self, user_msg: &str, model: &str) -> Result<String, String> {
        let system = self.render_system()?;
        let messages = vec![
            ChatMessage::new("system", system),
            ChatMessage::new("user", user_msg),
        ];
        let req = ChatRequest {
            model: model.to_string(),
            messages,
            tools: Vec::new(),
        };
        let resp = self.client.chat(&req)?;
        Ok(resp.content)
    }

    /// 多轮工具调用：LLM 可请求工具，执行结果回传，直到给出最终回复。
    pub fn run_with_tools(
        &self,
        user_msg: &str,
        model: &str,
        registry: &ToolRegistry,
    ) -> Result<String, String> {
        self.run_with_tools_ctx(user_msg, model, registry, None)
    }

    /// 同 [`Self::run_with_tools`]，但把运行时上下文快照（user-role）前置到首条用户消息。
    pub fn run_with_tools_ctx(
        &self,
        user_msg: &str,
        model: &str,
        registry: &ToolRegistry,
        runtime_context: Option<&str>,
    ) -> Result<String, String> {
        let system = self.render_system()?;
        let user_content = match runtime_context {
            Some(ctx) if !ctx.is_empty() => format!("{ctx}\n\n{user_msg}"),
            _ => user_msg.to_string(),
        };
        let mut messages = vec![
            ChatMessage::new("system", system),
            ChatMessage::new("user", user_content),
        ];
        let tools = registry.to_openai_tools();

        for _ in 0..8 {
            let req = ChatRequest {
                model: model.to_string(),
                messages: messages.clone(),
                tools: tools.clone(),
            };
            let resp = self.client.chat(&req)?;
            if !resp.has_tool_calls() {
                return Ok(resp.content);
            }

            let tool_calls = resp.tool_calls;
            messages.push(ChatMessage::assistant_with_tool_calls(tool_calls.clone()));
            for tc in &tool_calls {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.arguments).unwrap_or_else(|_| serde_json::json!({}));
                let result = registry
                    .execute(&tc.name, args)
                    .unwrap_or_else(|e| format!("error: {e}"));
                messages.push(ChatMessage::tool_result(tc.id.clone(), result));
            }
        }
        Err("tool loop exceeded max iterations".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{order, PromptSection};
    use llm::{CaptureClient, ChatResponse, ScriptedClient, ToolCall};
    use serde_json::Value;

    #[test]
    fn agent_assembles_system_prompt_then_calls_client() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new(
            "agent:identity",
            order::IDENTITY,
            "You are Mia Agent.",
        ))
        .unwrap();

        let client = CaptureClient::new("hello from llm");
        let agent = Agent::new(sp, &client);

        let answer = agent.run("what's on screen?", "test-model").unwrap();
        assert_eq!(answer, "hello from llm");

        let req = client.last_request().unwrap();
        assert_eq!(req.model, "test-model");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].content, "what's on screen?");
    }

    struct EchoTool;

    impl crate::Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo back the input"
        }
        fn run(&self, args: Value) -> Result<String, String> {
            Ok(args["text"].as_str().unwrap_or("").to_string())
        }
    }

    #[test]
    fn run_with_tools_executes_tool_then_returns_final() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("agent:identity", -100, "You are Mia."))
            .unwrap();

        // 第一轮返回 tool_calls，第二轮返回纯文本
        let client = ScriptedClient::new(vec![
            ChatResponse {
                content: String::new(),
                tool_calls: vec![ToolCall {
                    id: "call-1".into(),
                    name: "echo".into(),
                    arguments: r#"{"text":"hi"}"#.into(),
                }],
            },
            ChatResponse {
                content: "final answer".into(),
                tool_calls: Vec::new(),
            },
        ]);

        let mut registry = ToolRegistry::new();
        registry.register(std::sync::Arc::new(EchoTool));

        let agent = Agent::new(sp, &client);
        let answer = agent
            .run_with_tools("use echo", "test-model", &registry)
            .unwrap();
        assert_eq!(answer, "final answer");

        // 第二轮请求应包含工具结果消息
        let requests = client.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let second = &requests[1];
        assert!(second
            .messages
            .iter()
            .any(|m| m.role == "tool" && m.content == "hi"));
    }
}