//! 系统提示词组装注册表：借鉴 DeepSeek Harness（`dsh-system-prompt`）的
//! 「有序分片 + 严格变量插值 + 动态上下文分离」设计，按 miaocr 单进程规模裁剪。
//!
//! 核心纪律：职责单一、确定性顺序、fail loud、动态上下文与 system 分离。

pub mod agent_loop;
pub mod tool;

pub use agent_loop::Agent;
pub use tool::{Tool, ToolRegistry};

use std::collections::BTreeMap;
use std::sync::Arc;

/// Order 分带约定（与 DSH 一致）。
pub mod order {
    /// 固定身份开篇（`You are ...`）。
    pub const IDENTITY: i32 = -100;
    /// 一句话 persona（模型 + 工作目录 + 职责）。
    pub const PERSONA: i32 = 0;
    /// 工具指导起止。
    pub const TOOL_BEGIN: i32 = 100;
    /// 运行模式规则起止。
    pub const MODE_BEGIN: i32 = 200;
}

/// 变量名约束：`[a-z][a-z0-9_]*`。
fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// 一段系统提示词 section。
#[derive(Clone, Debug)]
pub struct PromptSection {
    pub name: String,
    pub order: i32,
    pub text: String,
    pub complete: bool,
}

impl PromptSection {
    pub fn new(name: impl Into<String>, order: i32, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order,
            text: text.into(),
            complete: false,
        }
    }

    /// 标记本段为完整提示词（抑制其余 section）。
    pub fn complete(mut self) -> Self {
        self.complete = true;
        self
    }
}

/// 动态运行时上下文（user-role 快照，与 system section 分离）。
#[derive(Clone, Debug)]
pub struct PromptContext {
    pub name: String,
    pub order: i32,
    pub text: String,
}

impl PromptContext {
    pub fn new(name: impl Into<String>, order: i32, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order,
            text: text.into(),
        }
    }
}

/// 组装时的动态上下文（为将来扩展预留：当前为空）。
#[derive(Default)]
pub struct AssembleContext {}

/// 变量提供者：每次组装求值。
pub type VariableProvider = Arc<dyn Fn(&AssembleContext) -> Option<String> + Send + Sync>;

/// 工具 schema（朝向模型的工具定义，与提示词同属一个 assembly）。
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
}

impl ToolSchema {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }
}

/// 系统提示词相关错误（fail loud，绝不静默产出残缺提示词）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptError {
    UnknownVariable { name: String, source: String },
    ValuelessVariable { name: String, source: String },
    MalformedReference { text: String, source: String },
    DuplicateSection { name: String },
    InvalidVariableName { name: String },
    MultipleComplete,
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::UnknownVariable { name, source } => {
                write!(f, "unknown prompt variable {{{{{}}}}} in {source}", name)
            }
            PromptError::ValuelessVariable { name, source } => {
                write!(f, "prompt variable {{{{{}}}}} has no value for this assembly ({source})", name)
            }
            PromptError::MalformedReference { text, source } => {
                write!(f, "malformed prompt variable reference at \"{text}\" in {source}")
            }
            PromptError::DuplicateSection { name } => {
                write!(f, "prompt section \"{name}\" already registered")
            }
            PromptError::InvalidVariableName { name } => {
                write!(f, "invalid prompt variable name \"{name}\" (must match [a-z][a-z0-9_]*)")
            }
            PromptError::MultipleComplete => {
                write!(f, "multiple complete prompt sections are active")
            }
        }
    }
}

impl std::error::Error for PromptError {}

/// 系统提示词组装器。
pub struct SystemPrompt {
    sections: BTreeMap<i32, Vec<PromptSection>>,
    section_names: std::collections::BTreeSet<String>,
    contexts: Vec<PromptContext>,
    variables: BTreeMap<String, VariableProvider>,
    tools: Vec<ToolSchema>,
    suppress_context: bool,
}

impl Default for SystemPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemPrompt {
    pub fn new() -> Self {
        Self {
            sections: BTreeMap::new(),
            section_names: std::collections::BTreeSet::new(),
            contexts: Vec::new(),
            variables: BTreeMap::new(),
            tools: Vec::new(),
            suppress_context: false,
        }
    }

    /// 注册一个 section（同层重复名即错）。
    pub fn add_section(&mut self, section: PromptSection) -> Result<(), PromptError> {
        if !self.section_names.insert(section.name.clone()) {
            return Err(PromptError::DuplicateSection { name: section.name });
        }
        self.sections.entry(section.order).or_default().push(section);
        Ok(())
    }

    /// 注册动态上下文。
    pub fn add_context(&mut self, context: PromptContext) {
        self.contexts.push(context);
    }

    /// 注册变量提供者。
    pub fn add_variable(
        &mut self,
        name: impl Into<String>,
        provider: VariableProvider,
    ) -> Result<(), PromptError> {
        let name = name.into();
        if !is_valid_variable_name(&name) {
            return Err(PromptError::InvalidVariableName { name });
        }
        self.variables.insert(name, provider);
        Ok(())
    }

    /// 注册工具 schema。
    pub fn add_tool(&mut self, tool: ToolSchema) {
        self.tools.push(tool);
    }

    /// 抑制所有动态上下文。
    pub fn suppress_context(&mut self) {
        self.suppress_context = true;
    }

    /// 组装一次提示词：排序 section、求值变量、决定上下文。
    pub fn assemble(&self) -> PromptAssembly {
        // sections 按 order 升序；同 order 按注册序。
        let mut sorted: Vec<&PromptSection> = Vec::new();
        for (_k, list) in &self.sections {
            sorted.extend(list.iter());
        }

        let mut complete: Option<&PromptSection> = None;
        for s in &sorted {
            if s.complete {
                match complete {
                    None => complete = Some(s),
                    Some(_) => {
                        // 多个 complete 在 render 前报错
                        return PromptAssembly {
                            sections: Vec::new(),
                            contexts: Vec::new(),
                            tools: self.tools.clone(),
                            variables: BTreeMap::new(),
                            multiple_complete: true,
                        };
                    }
                }
            }
        }

        let variables: BTreeMap<String, Option<String>> = self
            .variables
            .iter()
            .map(|(name, provider)| (name.clone(), provider(&AssembleContext::default())))
            .collect();

        let sections: Vec<String> = match complete {
            Some(c) => vec![c.text.clone()],
            None => sorted.into_iter().map(|s| s.text.clone()).collect(),
        };

        let contexts = if self.suppress_context {
            Vec::new()
        } else {
            let mut ctx: Vec<&PromptContext> = self.contexts.iter().collect();
            ctx.sort_by_key(|c| c.order);
            ctx.into_iter().map(|c| c.text.clone()).collect()
        };

        PromptAssembly {
            sections,
            contexts,
            tools: self.tools.clone(),
            variables,
            multiple_complete: false,
        }
    }
}

/// 单次组装结果。
#[derive(Debug)]
pub struct PromptAssembly {
    pub sections: Vec<String>,
    pub contexts: Vec<String>,
    pub tools: Vec<ToolSchema>,
    pub variables: BTreeMap<String, Option<String>>,
    multiple_complete: bool,
}

/// 渲染系统提示词：严格 `{{变量}}` 插值、剔除空段、空行连接。
pub fn render(assembly: &PromptAssembly) -> Result<String, PromptError> {
    if assembly.multiple_complete {
        return Err(PromptError::MultipleComplete);
    }
    let mut parts = Vec::new();
    for section in &assembly.sections {
        let rendered = interpolate(section, &assembly.variables, "section")?;
        if !rendered.is_empty() {
            parts.push(rendered);
        }
    }
    Ok(parts.join("\n\n"))
}

/// 渲染动态上下文快照，带 DSH 风格的开头句。
pub fn render_context_snapshot(assembly: &PromptAssembly) -> Result<String, PromptError> {
    let mut parts = Vec::new();
    for context in &assembly.contexts {
        let rendered = interpolate(context, &assembly.variables, "context")?;
        if !rendered.is_empty() {
            parts.push(rendered);
        }
    }
    if parts.is_empty() {
        return Ok(String::new());
    }
    let body = parts.join("\n\n");
    Ok(format!(
        "Current runtime context. This snapshot supersedes earlier runtime-context snapshots.\n\n{body}"
    ))
}

/// 严格插值：未知/无值/畸形一律报错；孤立 `{{`（后续无 `}}`）按字面保留。
fn interpolate(
    text: &str,
    variables: &BTreeMap<String, Option<String>>,
    source: &str,
) -> Result<String, PromptError> {
    let mut result = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        result.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        match after.find("}}") {
            Some(close) => {
                let name = &after[..close];
                if !is_valid_variable_name(name) {
                    return Err(PromptError::MalformedReference {
                        text: format!("{{{{{name}}}}}"),
                        source: source.to_string(),
                    });
                }
                match variables.get(name) {
                    Some(Some(value)) => result.push_str(value),
                    Some(None) => {
                        return Err(PromptError::ValuelessVariable {
                            name: name.to_string(),
                            source: source.to_string(),
                        });
                    }
                    None => {
                        return Err(PromptError::UnknownVariable {
                            name: name.to_string(),
                            source: source.to_string(),
                        });
                    }
                }
                rest = &after[close + 2..];
            }
            None => {
                // 孤立 `{{`，无闭合：按字面保留两个花括号
                result.push_str("{{");
                rest = &rest[open + 2..];
            }
        }
    }
    result.push_str(rest);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(value: &'static str) -> VariableProvider {
        Arc::new(move |_| Some(value.to_string()))
    }

    #[test]
    fn renders_sections_in_order_with_interpolation() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("agent:identity", order::IDENTITY, "You are Mia Agent powered by {{model}}.")).unwrap();
        sp.add_section(PromptSection::new("agent:persona", order::PERSONA, "Working directory is {{cwd}}.")).unwrap();
        sp.add_variable("model", provider("deepseek-v4")).unwrap();
        sp.add_variable("cwd", provider("/home/u")).unwrap();

        let assembly = sp.assemble();
        let rendered = render(&assembly).unwrap();
        assert_eq!(
            rendered,
            "You are Mia Agent powered by deepseek-v4.\n\nWorking directory is /home/u."
        );
    }

    #[test]
    fn empty_sections_are_dropped() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("a", 0, "")).unwrap();
        sp.add_section(PromptSection::new("b", 1, "hello")).unwrap();
        let rendered = render(&sp.assemble()).unwrap();
        assert_eq!(rendered, "hello");
    }

    #[test]
    fn unknown_variable_fails() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("a", 0, "hi {{nope}}")).unwrap();
        let err = render(&sp.assemble()).unwrap_err();
        assert!(matches!(err, PromptError::UnknownVariable { .. }));
    }

    #[test]
    fn valueless_variable_fails() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("a", 0, "hi {{x}}")).unwrap();
        sp.add_variable("x", Arc::new(|_| None)).unwrap();
        let err = render(&sp.assemble()).unwrap_err();
        assert!(matches!(err, PromptError::ValuelessVariable { .. }));
    }

    #[test]
    fn malformed_reference_fails() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("a", 0, "hi {{9bad}}")).unwrap();
        let err = render(&sp.assemble()).unwrap_err();
        assert!(matches!(err, PromptError::MalformedReference { .. }));
    }

    #[test]
    fn lone_open_brace_is_literal() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("a", 0, "literal {{ no close")).unwrap();
        let rendered = render(&sp.assemble()).unwrap();
        assert_eq!(rendered, "literal {{ no close");
    }

    #[test]
    fn complete_section_suppresses_others() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("a", 0, "ignored")).unwrap();
        sp.add_section(PromptSection::new("b", 5, "the whole prompt").complete()).unwrap();
        let rendered = render(&sp.assemble()).unwrap();
        assert_eq!(rendered, "the whole prompt");
    }

    #[test]
    fn duplicate_section_name_fails() {
        let mut sp = SystemPrompt::new();
        sp.add_section(PromptSection::new("a", 0, "x")).unwrap();
        let err = sp.add_section(PromptSection::new("a", 1, "y")).unwrap_err();
        assert!(matches!(err, PromptError::DuplicateSection { .. }));
    }

    #[test]
    fn context_snapshot_has_header() {
        let mut sp = SystemPrompt::new();
        sp.add_context(PromptContext::new("screen", 0, "region x=0 y=0"));
        let snapshot = render_context_snapshot(&sp.assemble()).unwrap();
        assert!(snapshot.starts_with("Current runtime context."));
        assert!(snapshot.contains("region x=0 y=0"));
    }

    #[test]
    fn suppressed_context_is_empty() {
        let mut sp = SystemPrompt::new();
        sp.add_context(PromptContext::new("screen", 0, "region"));
        sp.suppress_context();
        assert_eq!(render_context_snapshot(&sp.assemble()).unwrap(), "");
    }
}