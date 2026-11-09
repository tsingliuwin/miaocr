# AGENTS.md

喵OCR / 喵Agent — Rust 实时屏幕选区 OCR 工具，正在重构为「OCR 为眼睛、LLM 为大脑、工具为手」的屏幕智能助手（gpui-kit 0.9 = gpui-pre 0.3.11 + gpui-component 0.9）。

## 必读文档

- 架构总览：`docs/ARCHITECTURE.md`（重构后的分层与系统提示词组装机制）
- 整体重构方案：`docs/agent-refactor-plan.md`
- 系统提示词设计规格：`docs/system-prompt.md`

## 工作区结构

Cargo **虚拟 workspace**（无根包），成员：`crates/{vision, ui, agent, llm, tools}`。

- `crates/vision` — 「眼睛」：OCR 引擎、预处理、检测/安装（对 UI 框架零依赖）
- `crates/agent` — 系统提示词组装注册表 + Agent 循环 + 工具接缝
- `crates/tools` — Agent 工具箱（vision 能力包装）
- `crates/llm` — LLM 客户端接缝（OpenAI 兼容 + Mock）
- `crates/ui` — 唯一可执行入口，crate 名 `miaocr-ui`，产物 `miaocr.exe` / `miaocr`（gpui-kit 0.9 前端）

> 旧的 egui 单文件入口、`poc/` 原型、`PERMISSION_FIX.md` 已移除；
> macOS 屏幕录制权限逻辑已迁入 `crates/ui/src/mac_perm.rs`。

## 常用命令

- 编译：`.\scripts\build.ps1`（自动载入 MSVC 环境 + 处理临时目录）
- 等价手动命令：
  ```powershell
  call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
  cargo build -p miaocr-ui
  ```
- 单测：`cargo test -p agent -p tools -p miaocr-ui`
- `3rd_party/` 下的 ocr-rs / oar-ocr 是第三方源码依赖，谨慎改动

### 构建前置（Windows / MSVC）

1. bindgen 需要 MSVC 环境，普通 shell 直接 `cargo` 会报 `stddef.h not found`，
   必须先 call `vcvars64.bat`。
2. **`cc` crate 会传 `-Z7`**，本机 MSVC 14.44 下触发
   `error D8050: 无法执行 c1xx.dll / 未能将命令行放入调试记录中`。
   已在根 `Cargo.toml` 用 `[profile.dev.package."*"] debug = false` 关掉，
   依赖（ring / ocr-rs / oar-ocr 等）不再带调试信息。
3. **受限进程环境**（如 agent 沙箱）里，cargo 派生的 build-script 子进程
   对系统 TEMP 无写权限，会导致 `cl: D8050` 与 `nmake: U1054 无法创建内联文件`。
   `scripts/build.ps1` 会把 `TMP`/`TEMP` 指到工作区内 `.tmp` 规避。
4. `3rd_party/ocr-rs/build.rs` 原来监听不存在的 `MNN/CMakeLists.txt`，
   导致 ocr-rs 每次构建都被判 dirty 重编；已改为监听真实路径。
