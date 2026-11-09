//! 工具系统：可扩展的「能做事」接缝 + 执行守卫。
//!
//! 借鉴 deepseek-harness 的 capability seam 思想（工具 = 名字 + 描述 + 执行），
//! 工具 schema 与系统提示词的「工具指导 section」同源生成，避免漂移。

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::ToolSchema;

/// 一个模型可调用的工具。
pub trait Tool: Send + Sync {
    /// 工具名（模型侧调用名）。
    fn name(&self) -> &str;
    /// 一句话描述（进入系统提示词的工具指导）。
    fn description(&self) -> &str;
    /// 参数的 JSON Schema（OpenAI function calling 的 `parameters`）。
    /// 默认无参数；带参工具必须覆盖，否则模型只能从描述里猜参数名。
    fn parameters(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }
    /// 执行工具，返回文本结果。
    fn run(&self, args: Value) -> Result<String, String>;
}

/// 工具注册表：按名索引，确定性顺序（BTreeMap）。
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// 生成工具 schema（供系统提示词组装）。
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools
            .values()
            .map(|t| ToolSchema::new(t.name(), t.description()))
            .collect()
    }

    /// 执行工具（守卫：未知工具报错 + 捕获 panic）。
    pub fn execute(&self, name: &str, args: Value) -> Result<String, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("unknown tool \"{name}\""))?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tool.run(args)));
        match result {
            Ok(r) => r,
            Err(_) => Err(format!("tool \"{name}\" panicked")),
        }
    }

    /// 生成 OpenAI function-calling 格式的工具定义。
    pub fn to_openai_tools(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.parameters()
                    }
                })
            })
            .collect()
    }

    /// 把注册的工具贡献进系统提示词：
    /// 每个工具的「指导 section」（order 100+ 工具分带）与其 schema 同源生成。
    pub fn add_to_prompt(&self, sp: &mut crate::SystemPrompt) {
        for (i, tool) in self.tools.values().enumerate() {
            let name = tool.name().to_string();
            let order = crate::order::TOOL_BEGIN + i as i32;
            sp.add_section(crate::PromptSection::new(
                format!("tool:{name}"),
                order,
                format!("[tool:{name}] {}", tool.description()),
            ))
            .unwrap();
            sp.add_tool(ToolSchema::new(&name, tool.description()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    impl Tool for EchoTool {
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

    struct PanicTool;

    impl Tool for PanicTool {
        fn name(&self) -> &str {
            "panic"
        }
        fn description(&self) -> &str {
            "A tool that panics"
        }
        fn run(&self, _args: Value) -> Result<String, String> {
            panic!("boom")
        }
    }

    #[test]
    fn register_and_execute() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        let args = serde_json::json!({ "text": "hello" });
        assert_eq!(reg.execute("echo", args).unwrap(), "hello");
    }

    #[test]
    fn schema_generation_is_deterministic() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        let schemas = reg.schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "echo");
        assert_eq!(schemas[0].description, "Echo back the input");
    }

    #[test]
    fn unknown_tool_errors() {
        let reg = ToolRegistry::new();
        assert!(reg.execute("nope", serde_json::json!({})).is_err());
    }

    #[test]
    fn panic_is_caught() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(PanicTool));
        let err = reg.execute("panic", serde_json::json!({})).unwrap_err();
        assert!(err.contains("panicked"));
    }

    struct TypedTool;

    impl Tool for TypedTool {
        fn name(&self) -> &str {
            "typed"
        }
        fn description(&self) -> &str {
            "A tool with a typed parameter"
        }
        fn parameters(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            })
        }
        fn run(&self, _args: Value) -> Result<String, String> {
            Ok(String::new())
        }
    }

    #[test]
    fn openai_tools_carry_real_parameter_schema() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(TypedTool));
        reg.register(Arc::new(EchoTool));
        let tools = reg.to_openai_tools();
        let typed = tools
            .iter()
            .find(|t| t["function"]["name"] == "typed")
            .expect("typed tool missing");
        assert_eq!(typed["function"]["parameters"]["required"][0], "text");
        // 没覆盖 parameters 的工具保持空 schema，而不是缺字段
        let echo = tools
            .iter()
            .find(|t| t["function"]["name"] == "echo")
            .unwrap();
        assert_eq!(echo["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn tools_contribute_sections_and_schemas() {
        use crate::{order, render, PromptSection, SystemPrompt};

        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new(
            "agent:identity",
            order::IDENTITY,
            "You are Mia.",
        ))
        .unwrap();
        reg.add_to_prompt(&mut sp);

        let assembly = sp.assemble();
        let rendered = render(&assembly).unwrap();
        assert!(rendered.contains("You are Mia."));
        assert!(rendered.contains("[tool:echo] Echo back the input"));
        assert_eq!(assembly.tools.len(), 1);
        assert_eq!(assembly.tools[0].name, "echo");
    }
}