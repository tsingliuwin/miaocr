# 喵OCR / 喵Agent

**以 OCR 为眼睛、LLM 为大脑、工具为手的屏幕智能助手。**

框选屏幕任意区域 → 持续实时识别 → 一键交给 LLM 理解，必要时还能调用鼠标、键盘、剪贴板、文件等工具去「做事」。

前端基于 **[gpui-kit 0.9](https://gpui-kit.com)**（GPUI `gpui-pre` 0.3.11 + gpui-component 0.9），Windows / macOS 双平台。

---

## 核心特性

- **动态屏幕选区**：全屏覆盖层拖拽框选，选区内保持原图亮度、选区外压暗；实时尺寸徽标（物理像素）、四角手柄、十字参考线；右键 / Esc 取消；选区过小会红框提示并允许重选。
- **自适应轮询**：选区生效后常驻识别，按上一轮实际耗时动态调整间隔（120~1260ms）；可随时暂停 / 继续。
- **八大 OCR 引擎**：Windows 原生、macOS 原生、PP-OCRv5 (MNN 内置)、OarOCR (ONNX 内置)、PaddleOCR-json、RapidOCR-json、Tesseract、百度 AI Studio（云端）。本地引擎支持界面内一键下载安装，下载走多镜像轮询。
- **智能预处理**：小区域（<500×400）自动 2x Catmull-Rom 放大；原生 / Tesseract 引擎自动应用大津法二值化与智能反色，适配深色浅色背景。
- **问 AI**：把识别结果连同运行时快照（选区、引擎、最近文本）交给 LLM；支持自由提问与多轮对话，工具调用循环兼容 DeepSeek / OpenAI 接口。
- **设置页**：模型 / API Key / 百度 Token 等全部在界面内直接编辑（gpui-component `Input`），保存即生效。
- **置顶 + 复制 + 清空**：一键置顶（Win32 `SetWindowPos`），识别结果可复制到剪贴板。

---

## 工作区结构

```
miaocr/
├── crates/vision    「眼睛」：8 引擎 OCR + 预处理 + 检测/安装（对 UI 框架零依赖）
├── crates/tools     「手」：10 个 Agent 工具（复用 vision + 系统原语）
├── crates/agent     「脑」：系统提示词组装 + Agent 循环 + 工具注册表
├── crates/llm       模型接缝：OpenAI / DeepSeek 兼容 + Mock
├── crates/ui        界面：gpui-kit 前端（选区 / 轮询 / 设置 / 问 AI）
├── 3rd_party/       内置 MNN（ocr-rs）与 ONNX（oar-ocr）推理引擎
└── docs/            架构与设计文档
```

依赖方向（单向）：`ui → tools → agent → llm`；`tools → vision`。

---

## 构建与运行

### 环境要求

- Rust（stable，MSVC 工具链）
- **Windows**：Visual Studio Build Tools + Windows SDK（GPUI 需要）
- **macOS**：Xcode Command Line Tools

### 编译

```powershell
# 推荐：脚本会自动载入 MSVC 环境并处理临时目录
.\scripts\build.ps1

# 或手动
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cargo build -p miaocr-ui
```

> 首次编译会从源码编译 MNN，约 10~15 分钟；之后增量秒级。

```bash
cargo test -p agent -p tools -p miaocr-ui   # 单测
cargo run -p miaocr-ui                      # 运行
```

调试开关：`--probe`（截屏并打开覆盖层后存活 60 秒）、`--settings`（直接以设置页启动）。

### 配置

- 模型 / API Key：`~/.miaocr/agent_config.json`，或界面「⚙ 设置」里直接填。
- 引擎与百度 Token：`~/.miaocr/settings.json`，同样可在设置页编辑。
- 日志：`~/.miaocr/run.log`（不可写时回退到系统临时目录）。

---

## 文档

- [架构说明](docs/ARCHITECTURE.md)
- [重构方案](docs/agent-refactor-plan.md)
- [系统提示词设计](docs/system-prompt.md)

## 许可证

MIT