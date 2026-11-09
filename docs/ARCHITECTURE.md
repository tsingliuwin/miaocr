# miaocr 架构说明（GPUI 重构版）

> 本文描述重构后的架构。整体方案见 `docs/agent-refactor-plan.md`，系统提示词规格见 `docs/system-prompt.md`。

## 1. 概览

miaocr 已从「egui 单文件悬浮 OCR 工具」（原 `src/main.rs` 3409 行）重构为基于 **gpui-kit 0.9（GPUI `gpui-pre` 0.3.11 + gpui-component 0.9 + gpui-base）** 的「屏幕智能助手」，核心思想：**OCR 为眼睛、LLM 为大脑、工具为手**。

系统提示词组装机制借鉴 **DeepSeek Harness（`dsh-system-prompt`）**：有序分片（order 分带）、严格变量插值（fail loud）、动态上下文分离、工具 schema 与指导同源。

## 2. Workspace 结构

```
miaocr/                    (workspace root，虚拟 manifest，无根包)
├── crates/vision          眼睛：8 引擎 OCR + 检测 + 安装（对 UI 框架零依赖）
│   └── src/{lib,preprocess,tesseract,json_engine,engines,native,region,detect,install}.rs
├── crates/tools           手：10 个 Agent 工具（复用 vision + 系统原语）
├── crates/agent           脑：系统提示词组装 + Agent 循环 + 工具注册表/守卫
│   └── src/{lib,agent_loop,tool}.rs
├── crates/llm             模型接缝：OpenAI/DeepSeek 兼容 + function calling + Mock
├── crates/ui              界面：gpui-kit 悬浮窗 + 选区 + 引擎 + 设置 + 问 AI（全异步）
│   └── src/{main,agent_bridge,ocr_worker,win_top,mac_perm}.rs
├── 3rd_party/{ocr-rs,oar-ocr}   内置 MNN / ONNX 推理引擎
├── docs/                  方案 + 系统提示词 + 本文件
└── scripts/               构建 / 打包脚本
```

依赖方向（单向）：`ui → tools → agent → llm`；`tools → vision`；`ui → vision/agent/llm/tools`。

### UI 层用的 gpui-kit 组件

| 用途 | 组件 |
|---|---|
| 按钮 / 图标按钮 | `Button` + `IconName`（内置 Lucide 图标，需 `with_assets(AllAssets)`） |
| 卡片分组 | `GroupBox` |
| 输入框 | `Input` / `InputState`（API Key、Token 掩码输入） |
| 勾选项 | `Checkbox` |
| 状态标签 | `Tag`（success / warning / info / danger / secondary） |
| 空状态 | `Empty` + `EmptyHeader/Media/Title/Description` |
| 长文本滚动 | `ScrollableElement::overflow_y_scrollbar()` |
| 配色 | `ActiveTheme::theme().colors.*`，不再硬编码 rgb |

## 3. 数据流

```
选区拖拽（crates/ui）
  → 截图（worker 线程）→ 在截屏那块屏上开全屏覆盖层
  → 拖拽：四块遮罩（选区内保持原图亮度、外部压暗）+ 实时尺寸徽标 + 四角手柄 + 十字参考线
  → 松开 → 坐标映射 → vision::ocr_region(区域, 引擎)
  → OCR 文本回显（异步 window.spawn + update_entity）

「🤖 问 AI」
  → agent::SystemPrompt 组装（identity/persona + 10 工具的指导 section + schema）
  → Agent::run_with_tools → LLM（OpenAI/DeepSeek function calling）
  → 若 LLM 请求工具 → ToolRegistry::execute（含 catch_unwind 守卫）→ 结果回传
  → 多轮直到最终解答 → 回显
```

选区交互细节：
- **取消**：右键或 Esc（覆盖层自持 `FocusHandle` 并 `track_focus`，否则键盘事件无处投递）；
  取消信号走 `SelectShared.cancelled`，主窗口等待任务轮询后退出。
- **过小选区**（≤10px）：边框与徽标转红显示「太小」，松开后原地重置，可继续重选。
- **坐标换算**：视口逻辑像素 → 屏幕物理像素按 `screen_size / viewport_size` 缩放，
  再叠加截屏返回的屏幕原点；徽标里显示的是换算后的物理像素。

关键点：
- **模型可见 ⟺ 可追溯**：工具 schema 与系统提示词「工具指导 section」由 `ToolRegistry::add_to_prompt` 同源生成，杜绝漂移。
- **fail loud**：变量插值未知/无值/畸形一律报错，绝不静默产出残缺提示词。
- **异步**：OCR/LLM 通过 `window.spawn + AsyncWindowContext::update_entity` 在后台执行，不阻塞 UI。

## 4. 系统提示词设计（借鉴 DeepSeek Harness）

- **Order 分带**：`-100` 身份 / `0` persona / `100+` 工具指导 / `200+` 模式。
- **严格变量插值**：`{{model}}`/`{{cwd}}` 等，`VARIABLE_NAME` 约束 `[a-z][a-z0-9_]*`。
- **动态上下文快照**：`render_context_snapshot` 输出 `Current runtime context...` 开头句，作为 user-role 快照与 system 分离，可抑制。
- **complete section**：标记 `complete` 的 section 成为完整提示词，抑制其余。

## 5. 工具集（crates/tools，10 个）

`ocr_read`、`clipboard_read`、`clipboard_write`、`screenshot`、`file_read`、`file_write`、`mouse_click`、`keyboard_type`、`scroll`、`web_fetch`。

- 屏幕操作（`mouse_click`/`keyboard_type`/`scroll`）用 `enigo 0.5`。
- 剪贴板用 `arboard 3`。
- 截图/OCR/下载复用 `vision` 与系统 `curl`。

## 6. 构建与运行

```powershell
.\scripts\build.ps1              # 推荐：自动载入 MSVC 环境 + 处理临时目录
.\scripts\build.ps1 -Check       # 仅类型检查
.\scripts\build.ps1 -Test        # 单测

# 等价手动命令
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cargo build -p miaocr-ui
cargo run -p miaocr-ui

# 调试开关：
#   --probe     截屏 + 打开覆盖层后存活 60 秒，用于验证窗口链路不崩
#   --settings  直接以设置页启动，方便检查布局
```

> 首次编译会从源码编译 MNN（`ocr-rs`）与 ONNX（`oar-ocr`），加上 gpui-kit 全量依赖约 15 分钟；之后增量秒级。

### 构建相关的三处硬性约束

1. **必须 call `vcvars64.bat`**：bindgen 找不到 `stddef.h`。
2. **依赖关掉调试信息**：根 `Cargo.toml` 里 `[profile.dev.package."*"] debug = false`。
   `cc` crate 传 `-Z7` 会在本机 MSVC 14.44 触发
   `error D8050: 未能将命令行放入调试记录中`。
3. **`TMP`/`TEMP` 指向工作区**：受限进程环境里 build-script 子进程对系统 TEMP
   无写权限，会表现为 `D8050` / `nmake: U1054`。`scripts/build.ps1` 已处理。

「问 AI」需先在 `~/.miaocr/agent_config.json` 填 DeepSeek API Key（`api_key`/`model`/`base_url`），或直接在界面「⚙ 设置」里填入并点「保存」。
百度 AI Studio 的 token/模型/三个识别开关存在 `~/.miaocr/settings.json`，同样可在「⚙ 设置」页直接编辑。

## 7. 状态与剩余

| 项 | 状态 |
|---|---|
| vision（8 引擎 + 检测 + 安装）| ✅ |
| agent（提示词组装 + 循环 + 工具调用 + 真实参数 schema）| ✅ |
| ui（选区 + 持续轮询 + 暂停/复制/清空 + 置顶 + 引擎 + 设置 + 问 AI）| ✅ |
| 设置页（component 0.9 `Input` 输入框 + 内联保存/重载）| ✅ |
| gpui-kit 0.9 迁移（gpui-pre 0.3.11 + component 0.9）| ✅ |
| 单元测试（agent 18 + tools 4 + ui 4，0 失败）| ✅ |
| 真实 DeepSeek 端到端验证 | ⏳ 需 API Key |
| 百度云端 OCR 真机调用 | ⏳ 需消耗额度，仅验证参数透传 |
| gpui-component 现成组件（GroupBox/Checkbox/Tag/Empty/Input/Icon/Scroll）| ✅ 已替换手写 div |
| egui 旧入口 / poc / 旧权限文档清理 | ✅ 已移除 |
| shell 工具 | ✅ 已实现，默认不注册（`build_default_tools_with_shell` 显式 opt-in）|