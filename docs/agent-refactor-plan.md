# 喵OCR → 喵Agent：屏幕智能助手整体重构方案

> 状态：**方案待评审**。本文件是设计文档，评审通过后再动代码。
> 目标：把 `miaocr` 从一个「纯 OCR 悬浮工具」升级为「以 OCR 为眼睛、LLM 为大脑的屏幕智能助手（Agent）」，UI 框架从 `egui` 迁移到 `gpui` + `gpui-component`，并借鉴 DeepSeek Harness 的系统提示词设计，为 Agent 新建一套系统提示词与提示词组装机制。

---

## 1. 目标与定位

| 维度 | 现状 | 目标 |
|---|---|---|
| 定位 | 纯 OCR 选区工具 | 屏幕智能助手：感知屏幕 → OCR 理解 → LLM 推理 → 应答/自动操作 |
| 眼睛 | 8 大 OCR 引擎 + 屏幕截图 | 保留并模块化（这是成熟资产，不可丢） |
| 大脑 | 无 | LLM Agent（系统提示词 + 工具 + 循环） |
| UI | egui 0.27（单文件 3409 行） | gpui 0.2.2 + gpui-component（多模块） |
| 平台 | Windows 7+ / macOS | macOS + Windows（见 §3 风险） |

**一句话：让「看到什么」直接衔接「该做什么」。**

---

## 2. 现状分析与痛点

### 2.1 现有资产（必须保留/复用）

- **8 个 OCR 引擎**（`src/main.rs:1804` 的 `match backend` 分发）：
  - Windows 原生（WinRT `Media_Ocr`）、macOS 原生（Vision via `mac_ocr.swift`）
  - PP-OCRv5（MNN，内置）、OarOCR（ONNX，内置）
  - PaddleOCR-json / RapidOCR-json（外置子进程 `JsonOcrEngine`）
  - Tesseract、百度 AI Studio（云端）
- **统一识别入口 `ocr_region()`**：截图 → 小区域 2x Catmull-Rom 放大 → Otsu 二值化 → 分发引擎。
- **选区状态机**（`select_step` 0→1→2→3）：隐藏窗口 → 截图鼠标所在屏 → 全屏覆盖层拖拽 → 计算物理坐标（含 DPI scale）。
- **后台轮询线程**：自适应间隔（最近 10 次耗时）、去重归档（`LogManager`）。
- **多镜像下载安装器**、**云配置（百度 Token）**、**日志**。

### 2.2 架构痛点（重构动因）

1. **单文件 3409 行**（`main.rs`）：UI、OCR、线程、安装、云服务全部耦合在一起，难以扩展 Agent。
2. **egui 框架上限**：egui 是即时模式 GUI，不适合做复杂聊天流/异步 Agent 交互；项目作者已明确要换 GPUI。
3. **无 Agent 层**：OCR 结果只「显示 + 复制 + 归档」，没有「理解 + 行动」链路。
4. **无系统提示词机制**：LLM 能力为零，谈不上提示词设计。

---

## 3. 技术选型与风险

### 3.1 GUI 框架

- **gpui 0.2.2**（Zed 的 GUI 框架）+ **gpui-component**（`longbridge/gpui-component`，最活跃的组件库）。
- **⚠️ 关键风险（已核实）**：GPUI 的 **Windows 支持 2025 年才起步**（Zed 才刚正式登陆 Windows，仍存在跨平台难题），**不支持 Windows 7**，Windows 上的稳定性未经验证。macOS 支持成熟。
- **决策（已确认）**：坚持 macOS + Windows 双平台，**接受 Windows 上 GPUI 不稳/缺失 Win7 的风险**。因此必须：
  - 阶段 0 先做 **Windows 可行性 PoC**；
  - OCR 层与 UI 层彻底解耦，保证 Windows 侧即使 UI 不成熟，OCR 能力仍可复用。

### 3.2 LLM 客户端

| 方案 | 结论 |
|---|---|
| 自研 `reqwest` 直连 DeepSeek/OpenAI 兼容 API | **推荐**：与现有百度 OCR（`reqwest`）一致，提示词组装完全可控（这正是本文档系统的核心） |
| `rig` / `async-openai` | 备选：生态成熟，但其提示词约定会束缚我们的组装设计 |

### 3.3 其它依赖

| 能力 | 建议 crate | 说明 |
|---|---|---|
| 截图 | 保留 `screenshots`（可选评估 `xcap`） | 已验证多显示器 + DPI |
| 屏幕操作（鼠标/键盘/滚动） | `enigo` | Agent「自动操作」能力（阶段 3） |
| 剪贴板 | `arboard` | 替换现有剪贴板逻辑 |
| 异步运行时 | `tokio` | GPUI 本身即异步 actor 模型 |
| 序列化 | `serde` + `serde_json`（已用） | 会话、工具参数、配置 |

---

## 4. 目标架构（分层）

借鉴 DSH「**一切皆可替换的能力接缝（capability seam）**」思想，但按 Rust 规模裁剪（不引入插件框架，用 trait + 注册表实现可替换性）。

```
miaocr/
├── crates/
│   ├── vision/        「眼睛」：截图、选区、8 OCR 引擎封装（模块化自 main.rs）
│   │     src/screen.rs    多显示器/DPI 感知
│   │     src/screenshot.rs 截图 + 选区坐标换算
│   │     src/engines/     windows_native / mac_native / mnn / onnx /
│   │                      paddle_json / rapid_json / tesseract / baidu
│   │     src/lib.rs       OcrEngine trait + 注册表 + ocr_region() 分发
│   ├── llm/            LLM 适配器：DeepSeek/OpenAI 兼容 API（reqwest）
│   │     src/adapters/   trait LlmAdapter + deepseek/openai 实现
│   ├── agent/          「大脑」：会话、循环、系统提示词组装、工具执行
│   │     src/system_prompt.rs   核心：有序 section 组装注册表（本文档重点）
│   │     src/session.rs         会话历史（内存 + 落盘）
│   │     src/loop.rs            agent 循环（step/turn）
│   │     src/tool.rs            Tool trait + 注册表 + 守卫执行
│   │     src/skills.rs          按需加载技能（~/.miaocr/skills/*.md）
│   ├── tools/          Agent 工具箱
│   │     src/{ocr_region, screenshot, clipboard, screen_action, file, shell, web}
│   ├── app/            应用装配：状态、配置、安装器（自 main.rs 拆出）
│   └── ui/             GPUI 界面（悬浮窗 / 选区覆盖层 / 聊天面板 / 设置）
│   │     src/{window, selection_overlay, chat_panel, settings}
├── src/main.rs         薄入口：装配 app + 启动 GPUI
└── Cargo.toml          workspace 化
```

**依赖方向（单向）**：`ui → app → agent → (llm, tools, vision)`；`tools → vision`（OCR 定向调用）；`vision`、`llm` 为最底层叶子。

---

## 5. Agent 设计（大脑）

### 5.1 核心循环（对齐 DSH 的 turn/step）

```text
用户消息（选区变化 / 提问 / 指令）
  → 组装系统提示词 + 工具 schema（每次 step 一次，见 §7）
  → 注入动态上下文快照（当前选区、OCR 结果、屏幕信息、权限）
  → llm/stream → assistant 回复（可能含工具调用 tool_calls）
  → 工具执行（经守卫：超时 / 权限 / 重试）→ tool_result
  → 循环直到无更多工具调用（turn 结束）
```

### 5.2 工具箱（按阶段落地）

| 工具 | 能力 | 阶段 |
|---|---|---|
| `screenshot` | 截取屏幕（区域/全屏/多屏） | 1 |
| `ocr_read` | 对指定坐标区域跑 OCR（定向调用，可指定引擎） | 1 |
| `clipboard_read` / `clipboard_write` | 读写剪贴板 | 2 |
| `answer`（内置结束通道） | 无工具、纯文本时应答 | 2 |
| `mouse_click` / `keyboard_type` / `scroll`（enigo） | 自动操作 | 3 |
| `file_read` / `file_write` | 文件 | 3 |
| `shell` | 命令执行（需沙箱/确认） | 3 |
| `web_search` | 联网检索 | 3 |

### 5.3 记忆与上下文

- **会话历史**：内存 + 落盘（`~/.miaocr/sessions/`），支持恢复。
- **动态上下文快照**（借鉴 DSH）：当前选区坐标、最近 OCR 文本、屏幕列表/DPI、权限状态、剪贴板摘要 —— **作为 user-role 快照注入**，与系统提示词 section 分离，可开关。
- **技能（Skills）**：`~/.miaocr/skills/<name>.md`，按需 `skill` 加载，降低常驻 token。

---

## 6. 系统提示词设计（借鉴 DeepSeek Harness）

> 这是本方案的核心。DSH 的系统提示词不是一段写死的文本，而是**插件化、有序、可组合、严格校验的「组装注册表」**。我们在 Rust 侧复刻其核心机制（按需裁剪，不引插件框架）。

### 6.1 机制：有序 section 组装注册表

```rust
/// 一个系统提示词片段（DSH 的 PromptSection）
struct PromptSection {
    name: String,          // 唯一名，如 "agent:identity"、"tool:ocr_read"
    order: i32,            // 升序拼接；约定分带见下
    text: String,          // 可含 {{变量}}
    // complete: Option<bool> // 可选：本 section 即完整提示词（完全接管场景）
}

/// 提示词组装器（DSH 的 SystemPrompt service）
struct SystemPrompt {
    sections: BTreeMap<i32, Vec<PromptSection>>,  // order 升序
    variables: HashMap<String, VariableProvider>, // {{name}} -> 提供者
    contexts: Vec<ContextSection>,                // 动态上下文（user 快照，分离）
    tools: Vec<ToolSchema>,                        // 工具 schema 属于同一 assembly
}

/// 组装结果（DSH 的 PromptAssembly）
struct PromptAssembly {
    sections: Vec<String>,      // 已按 order 排序的文本（未插值）
    contexts: Vec<String>,      // 动态上下文
    tools: Vec<ToolSchema>,     // 已排序
    variables: HashMap<String, String>,
}

fn render(assembly: &PromptAssembly) -> Result<String> { /* 严格插值 + 剔除空 section */ }
```

**Order 分带（约定，与 DSH 一致）**：

| order | 内容 | 说明 |
|---|---|---|
| `-100` | Agent 身份 opener | 固定：`You are Mia Agent, a screen-intelligence assistant powered by {{model}}.` |
| `0` | persona 一句话定位 | 身份 + 工作目录 + 一句话职责 |
| `100–199` | 工具指导 | **每个工具一节**，写死用法/边界/陷阱 |
| `200+` | 模式/策略 | 如「选区解释模式」「自动操作模式」「plan 模式」 |

### 6.2 严格变量插值（fail loud）

`{{model}}`、`{{cwd}}`、`{{screen_region}}`、`{{active_engine}}`、`{{os}}` 等。
- 未知变量 / 已注册但本次无值 / `{{}}` 空名 / 未闭合 `{{` —— **一律返回 Err**，绝不静默产出残缺提示词（DSH 的核心纪律）。
- 变量名约束 `[a-z][a-z0-9_]*`。

### 6.3 动态上下文快照（与 system 分离）

每 step 组装时注入 user-role 快照，形如（对齐 DSH 的 `renderContextSnapshot`）：

```
Current runtime context. This snapshot supersedes earlier runtime-context snapshots.

Screen: 2 displays; active region (x,y,w,h) = (...); scale_factor = 1.0
Latest OCR: <识别结果摘录>
Permission: screen-capture granted
Clipboard: <剪贴板摘要>
```

可全局或按场景抑制（`suppressRuntimeContext`）。

### 6.4 按需技能 + Agent Notes

- **Skills**（`~/.miaocr/skills/`）：如 `bbox-click.md`（如何精确点击 OCR 命中的坐标）、`table-extract.md`（表格提取流程）。按需加载，不进常驻系统提示词。
- **Agent Notes**（可选，阶段 4）：决策历史分类归档（implemented/proposed/rejected），让 Agent 有可追溯记忆。

### 6.5 系统提示词正文草案（英文，面向 LLM 稳定）

```
---- order -100 (agent:identity) ----
You are Mia Agent, a screen-intelligence assistant powered by {{model}}.

---- order 0 (deployment:persona) ----
Your working directory is {{cwd}}. You run on {{os}}.
You can SEE the user's screen through OCR and screenshots, and you can ACT on it
through mouse/keyboard/clipboard tools when the user asks. Ground every claim in
what you actually observed; never invent OCR text or screen content.

---- order 100-199 (tool guidance, 每个工具一节) ----
[tool:ocr_read]
Read text at a screen region via OCR. Prefer the native engine for speed;
switch to {{active_engine}} only when the user names one. Report the exact text,
then a one-line summary. If the region is empty, say so instead of guessing.

[tool:screenshot]
Capture a region for yourself when OCR alone is ambiguous (layout, icons, tables).
Always state the region you captured.

[tool:clipboard_write]/[tool:clipboard_read]
... (确定性规则，含边界与陷阱)

[tool:mouse_click]/[tool:keyboard_type]/[tool:scroll]
Coordinate from OCR bounding boxes... (阶段 3)

---- order 200+ (mode) ----
[mode:observe] 默认：只观察、只解释，不操作。
[mode:act]     用户明确授权后：允许屏幕操作，每次重大动作前简述意图。
```

> 设计原则（对齐 DSH AGENTS.md 的核心纪律）：**职责单一（每个事实由其 owner 贡献）、确定性顺序、fail loud 优先于含糊、分层可组合、KV-cache 前缀稳定**。

---

## 7. 分阶段实施计划

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| **0. PoC 验证** | Windows 上编译运行 GPUI 0.2.2 + gpui-component 最小窗口；验证截图 + 一次 OCR 闭环 | GPU 窗口正常渲染；`screenshots` 截图成功；任一引擎返回文本 |
| **1. 骨架迁移** | workspace 化；`vision`/`app` 模块从 `main.rs` 拆出；GPUI 悬浮窗 + 选区覆盖层复刻现有 UI 全功能 | 选区/暂停/复制/引擎切换/云配置 全功能等价，双平台可跑 |
| **2. Agent 核心** | `llm` 适配器 + `agent` 会话/循环 + `system_prompt` 组装注册表 + 基础工具（`ocr_read`/`screenshot`/`clipboard_*`/`answer`） | 能对话、能按选区 OCR 回答、提示词组装单测通过 |
| **3. 工具扩展** | 屏幕操作（enigo）、文件、shell（沙箱/确认）、web；工具执行守卫（超时/权限/重试） | Agent 能「看到→理解→点击/输入」完成一个端到端任务 |
| **4. 体验完善** | 技能系统、Agent Notes、安装器迁移、双平台打磨、安全加固 | 打包发布，README/架构文档同步 |

---

## 8. 风险与待决事项

1. **GPUI Windows 成熟度（最高风险）**：可能导致 Windows 侧 UI 降级或延后。缓解：阶段 0 PoC 先行；OCR 层与 UI 完全解耦，最坏情况 Windows 保留 egui 前端、共享 agent/vision 内核。
2. **单文件拆分回归风险**：3409 行拆模块易引入回归。缓解：阶段 1 以「行为等价」为唯一验收，逐功能迁移。
3. **AGENT 安全边界**：屏幕操作 / shell 工具需默认禁用 + 明确授权 + 操作前确认。系统提示词与工具守卫双重约束。
4. **LLM 密钥与成本**：沿用现有「云配置」页，扩展为「Agent 配置」（模型/密钥/端点）。
5. **gpui-component 维护状况与 API 变动**：0.x 大版本，锁定版本，紧跟其 prompt 兼容性。

---

## 9. 交付物索引（本目录后续产出）

- `docs/agent-refactor-plan.md` —— 本方案。
- `docs/system-prompt.md` —— 系统提示词组装机制的详细规格（阶段 2 实施时细化）。
- `docs/architecture.md` —— 模块与数据流（阶段 1 实施时细化）。
- `docs/tool-catalog.md` —— Agent 工具清单与 schema（阶段 2–3）。