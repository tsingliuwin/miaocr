# 喵Agent 系统提示词设计规格

> 本文是 `docs/agent-refactor-plan.md` §6 的详细规格，供阶段 2 实施使用。
> 核心思想：**将 DeepSeek Harness（`dsh-system-prompt`）的「有序分片组装注册表」移植到 Rust，按 miaocr 单进程规模裁剪——去掉插件框架，保留「有序、分层、严格插值、动态上下文分离、按需技能」五个核心纪律。**

---

## 1. 设计原则

| 原则 | 含义 |
|---|---|
| 职责单一 | 每一段提示词由「拥有该事实的模块」贡献（工具指导归工具，身份归身份），模块贡献自己的 section，不互相拼文本 |
| 确定性顺序 | section 按 `order` 升序拼接，工具 schema 有显式顺序，注册顺序无关（消除插件加载顺序影响） |
| fail loud | 变量未知/无值、重复 name、`order` 非有限数 → **返回 Err**，绝不静默产出残缺提示词 |
| 分层可组合 | 全局默认 + 场景覆盖（同 name shadow），支持「默认 Agent」「选区解释模式」等切换 |
| 上下文精准 | 常驻提示词最小化；运行时事实走动态上下文快照；低频技能按需加载 |

---

## 2. 核心数据结构（Rust）

```rust
/// 一段系统提示词（DSH 的 PromptSection）
pub struct PromptSection {
    pub name: String,       // 唯一名；同层重复即错
    pub order: i32,         // 升序拼接；分带见 §3
    pub text: String,       // 可含 {{变量}}
    pub complete: bool,     // true 时本段即完整提示词（完全接管场景），抑制其余
}

/// 运行时动态上下文（DSH 的 PromptContext）——与 system section 分离，走 user-role
pub struct PromptContext {
    pub name: String,
    pub order: i32,
    pub text: String,       // 每步组装时生成
}

/// 变量提供者：每次组装求值
pub type VariableProvider = Box<dyn Fn(&AssembleContext) -> Option<String> + Send + Sync>;

/// 提示词组装器（DSH 的 SystemPrompt service）
pub struct SystemPrompt {
    sections: BTreeMap<i32, Vec<PromptSection>>,  // order -> sections（同 order 按注册序）
    contexts: Vec<PromptContext>,
    variables: BTreeMap<String, VariableProvider>,
    tool_schemas: Vec<ToolSchema>,
    suppress_context: bool,
}

/// 单次组装结果（DSH 的 PromptAssembly）
pub struct PromptAssembly {
    pub sections: Vec<String>,   // 已排序、未插值
    pub contexts: Vec<String>,   // 动态上下文
    pub tools: Vec<ToolSchema>,  // 已确定性排序
    pub variables: BTreeMap<String, Option<String>>,
}

pub fn render(asm: &PromptAssembly) -> Result<String, PromptError> { /* 严格插值 + 去空段 + 空行连接 */ }
```

**组装流程**（对齐 DSH `SystemPrompt::assemble`）：
1. 收集全局 + 场景层 section（同 name 场景层覆盖全局）；
2. section 按 `order` 升序排序，`complete:true` 段至多一个（多个即 Err）；
3. 求值所有变量 provider；
4. 若未抑制，收集动态上下文（按 `order` 排序）；
5. 工具 schema 按 `toolOrder`（或字典序）排序；
6. `render()` 做严格 `{{变量}}` 插值 → 最终 system prompt 文本。

---

## 3. Order 分带（约定）

| 区间 | section | 说明 |
|---|---|---|
| `-100` | `agent:identity` | 固定开场身份，一句话 |
| `0` | `agent:persona` | 一句话定位：模型 + 工作目录 + 职责 |
| `100–199` | `tool:*` | 每个工具一节，写死用法/边界/陷阱 |
| `200–299` | `mode:*` | 运行模式规则（observe / act / plan） |
| `300+` | 预留 | 未来扩展 |

---

## 4. 变量（严格插值）

注册名约束 `[a-z][a-z0-9_]*`。内置：

| 变量 | 来源 |
|---|---|
| `{{model}}` | 当前 LLM 模型名 |
| `{{cwd}}` | 工作目录 |
| `{{os}}` | `Windows` / `macOS` |
| `{{active_engine}}` | 当前默认 OCR 引擎名 |

插值规则（对齐 DSH `renderPrompt`）：
- 未知变量（用 `Object.hasOwn`/`HashMap::contains_key` 查，防原型名）→ Err；
- 已注册但本次求值为 `None` → Err；
- `{{}}`、未闭合且后续有 `}}` 的畸形组 → Err；
- 孤立的 `{{`（无任何 `}}`）按字面保留；替换值不再二次扫描。

---

## 5. 动态上下文快照（user-role，与 system 分离）

每步组装注入，形如（对齐 DSH `renderContextSnapshot` 的开头句）：

```
Current runtime context. This snapshot supersedes earlier runtime-context snapshots.

Screen: 2 displays; active region (x=0, y=0, w=1920, h=1080), scale=1.0
Active OCR engine: WindowsNative
Latest OCR: <最近一次识别结果，截断至 N 字>
Permission: screen-capture granted
Clipboard: <剪贴板摘要，可选>
```

- 由 `vision`/`app` 模块作为 context provider 注册，`SystemPrompt` 每步求值。
- `suppress_context` 可按场景关闭（如云配置页、纯文本问答）。

---

## 6. 系统提示词正文（英文，面向 LLM 稳定）

```
──── order -100 · agent:identity ─────────────────────────────
You are Mia Agent, a screen-intelligence assistant powered by {{model}}.

──── order 0 · agent:persona ────────────────────────────────
Your working directory is {{cwd}}. You run on {{os}}.
You can SEE the user's screen through screenshots and OCR, and ACT on it through
mouse, keyboard, clipboard and shell tools only when the user authorizes action.
Ground every statement in what you actually observed. Never invent OCR text,
pixel positions, or screen content. If you are unsure what is on screen, capture
it or ask — do not guess.

──── order 100 · tool:ocr_read ──────────────────────────────
Read text in a screen region via OCR. Use the {{active_engine}} engine by default;
pass an engine name only when the user names one. Reply with the exact recognized
text first, then a one-line interpretation. If the region is empty or unreadable,
say so plainly instead of fabricating text.

──── order 110 · tool:screenshot ────────────────────────────
Capture a screen region for YOUR OWN inspection when OCR alone is ambiguous
(layout, icons, tables, images). Always state which region you captured before
describing it. Never claim visual content you did not capture.

──── order 120 · tool:clipboard_read / tool:clipboard_write ──
Read or write the system clipboard. Read before quoting clipboard content; write
only when asked, and echo what you wrote.

──── order 130 · tool:web_search ────────────────────────────
Search the web for current information. Cite source URLs as markdown links. Do
not present search results as your own knowledge.

──── order 200 · mode:observe ───────────────────────────────
Default mode: observe and explain only. Do NOT click, type, write files, or run
shell commands. State what you saw and what you would do, then ask for approval.

──── order 210 · mode:act ───────────────────────────────────
Active only after the user explicitly authorizes action. You may use mouse,
keyboard, clipboard and shell tools. Before each destructive or irreversible
action (typing into a real app, deleting, running a command), state your intent
in one sentence and wait for the result to confirm before continuing.
```

> 实施下一页工具就补一节 `tool:*`（100–199 段），新运行模式补一节 `mode:*`（200+ 段）。**工具的 schema 与它的指导 section 同属一个 assembly**，同一来源维护，避免「schema 有、指导没有」的漂移。

---

## 7. 技能系统（按需加载，压低常驻 token）

- 目录 `~/.miaocr/skills/<name>.md`，前置 YAML front-matter：`name` + `description`（描述触发条件）。
- `skill` 工具：按 name 加载完整 SKILL.md 进入当前会话上下文，不预载。
- 内置首批技能：`bbox-click`（从 OCR 边界框精确点击）、`table-extract`（表格识别与结构化）、`form-fill`（表单填写流程）。

---

## 8. 校验与测试

| 项 | 规则 |
|---|---|
| 单元测试 | 渲染：空段剔除、变量插值、未知变量报错、`complete` 覆盖、`order` 稳定排序 |
| 快照测试 | 给定一组 section + 上下文，产出确定性 system prompt 文本，存快照防回归 |
| 约束校验 | 工具 schema 与指导 section 的 name 一一对应（缺一即装配期告警） |
| KV-cache 意识 | 身份/ persona/ 工具顺序一旦确定保持稳定，减少前缀失效 |