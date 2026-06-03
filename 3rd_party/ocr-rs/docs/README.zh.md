# Rust PaddleOCR

[English](../README.md) | [中文](README.zh.md) | [日本語](README.ja.md) | [한국어](README.ko.md)

一个基于PaddleOCR模型的轻量级高效OCR（光学字符识别）Rust库。该库利用MNN推理框架提供高性能的文本检测和识别功能。

**本项目是纯Rust库**，专注于提供OCR核心功能。如需命令行工具或其他语言绑定，请参考：
- 🖥️ **命令行工具**：[newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)
- 🔌 **C API绑定**：[paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi) - 提供C API以方便与其他编程语言集成
- 🌐 **HTTP服务**：[newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service) ⚠️ (施工中)

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

## ✨ 版本 2.0 新特性

- 🎯 **全新分层 API 设计**：提供从底层模型到高层 Pipeline 的完整分层 API
- 🔧 **灵活的模型加载**：支持从文件路径或内存字节加载模型
- ⚙️ **可配置的检测参数**：支持自定义检测阈值、分辨率、精度模式等
- 🚀 **GPU 加速支持**：支持 Metal、OpenCL、Vulkan 等多种 GPU 后端
- 📦 **批量处理优化**：支持批量文本识别以提高吞吐量
- 🔌 **独立引擎模式**：可以只创建检测引擎或识别引擎

## 特性

### 核心功能
- **文本检测**：准确定位图像中的文本区域
- **文本识别**：识别检测区域中的文本内容
- **端到端识别**：一次调用完成检测和识别全流程
- **分层 API 架构**：支持端到端、分层调用和独立模型三种使用方式

### 模型支持
- **多版本模型支持**：支持 PP-OCRv4 和 PP-OCRv5 模型，灵活选择使用
- **多语言支持**：PP-OCRv5 支持11+种专用语言模型，覆盖100+种语言
- **复杂场景识别**：增强的手写体、竖排文本、生僻字识别能力
- **灵活加载方式**：支持从文件路径或内存字节加载模型

### 性能特性
- **高性能推理**：基于 MNN 推理框架，速度快、内存占用低
- **GPU 加速**：支持 Metal、OpenCL、Vulkan 等多种 GPU 后端
- **批量处理**：支持批量文本识别，提高吞吐量

### 开发体验
- **灵活配置**：检测阈值、分辨率、精度模式等参数均可自定义
- **内存安全**：自动内存管理，防止内存泄漏
- **纯 Rust 实现**：无需外部运行时，跨平台兼容
- **最小依赖**：轻量级且易于集成

## 模型版本

该库支持三个PaddleOCR模型版本：

### PP-OCRv4
- **稳定版本**：经过充分验证，兼容性好
- **适用场景**：常规文档识别，对准确性要求较高的场景
- **模型文件**：
  - 检测模型：`ch_PP-OCRv4_det_infer.mnn`
  - 识别模型：`ch_PP-OCRv4_rec_infer.mnn`
  - 字符集：`ppocr_keys_v4.txt`

### PP-OCRv5
- **最新版本**：新一代文字识别解决方案
- **多语言支持**：默认模型（`PP-OCRv5_mobile_rec.mnn`）支持简体中文、繁体中文、英文、日文、中文拼音
- **专用语言模型**：提供11+种语言的专用模型，覆盖100+种语言，以获得最佳性能
- **共享检测模型**：所有V5语言模型使用相同的检测模型（`PP-OCRv5_mobile_det.mnn`）
- **增强场景识别**：
  - 中英复杂手写体识别能力显著提升
  - 竖排文本识别优化
  - 生僻字识别能力增强
- **性能提升**：相比PP-OCRv4端到端提升13个百分点
- **模型文件**（默认多语言）：
  - 检测模型：`PP-OCRv5_mobile_det.mnn`（所有语言共享）
  - 识别模型：`PP-OCRv5_mobile_rec.mnn`（默认，支持中文/英文/日文）
  - 字符集：`ppocr_keys_v5.txt`
- **专用语言模型文件**（可选）：
  - 识别模型：`{lang}_PP-OCRv5_mobile_rec_infer.mnn`
  - 字符集：`ppocr_keys_{lang}.txt`
  - 可用语言代码：`arabic`、`cyrillic`、`devanagari`、`el`、`en`、`eslav`、`korean`、`latin`、`ta`、`te`、`th`

#### PP-OCRv5 语言模型详细支持列表

| 模型名称 | 支持的语言 |
|---------|-----------|
| **korean_PP-OCRv5_mobile_rec** | 韩语、英语 |
| **latin_PP-OCRv5_mobile_rec** | 法语、德语、南非荷兰语、意大利语、西班牙语、波斯尼亚语、葡萄牙语、捷克语、威尔士语、丹麦语、爱沙尼亚语、爱尔兰语、克罗地亚语、乌兹别克语、匈牙利语、塞尔维亚语（拉丁字母）、印度尼西亚语、奥克语、冰岛语、立陶宛语、毛利语、马来语、荷兰语、挪威语、波兰语、斯洛伐克语、斯洛文尼亚语、阿尔巴尼亚语、瑞典语、斯瓦希里语、他加禄语、土耳其语、拉丁语、阿塞拜疆语、库尔德语、拉脱维亚语、马耳他语、巴利语、罗马尼亚语、越南语、芬兰语、巴斯克语、加利西亚语、卢森堡语、罗曼什语、加泰罗尼亚语、克丘亚语 |
| **eslav_PP-OCRv5_mobile_rec** | 俄语、白俄罗斯语、乌克兰语、英语 |
| **th_PP-OCRv5_mobile_rec** | 泰语、英语 |
| **el_PP-OCRv5_mobile_rec** | 希腊语、英语 |
| **en_PP-OCRv5_mobile_rec** | 英语 |
| **cyrillic_PP-OCRv5_mobile_rec** | 俄语、白俄罗斯语、乌克兰语、塞尔维亚语（西里尔字母）、保加利亚语、蒙古语、阿布哈兹语、阿迪格语、卡巴尔达语、阿瓦尔语、达尔金语、印古什语、车臣语、拉克语、列兹金语、塔巴萨兰语、哈萨克语、吉尔吉斯语、塔吉克语、马其顿语、鞑靼语、楚瓦什语、巴什基尔语、马里语、摩尔多瓦语、乌德穆尔特语、科米语、奥塞梯语、布里亚特语、卡尔梅克语、图瓦语、萨哈语、卡拉卡尔帕克语、英语 |
| **arabic_PP-OCRv5_mobile_rec** | 阿拉伯语、波斯语、维吾尔语、乌尔都语、普什图语、库尔德语、信德语、俾路支语、英语 |
| **devanagari_PP-OCRv5_mobile_rec** | 印地语、马拉地语、尼泊尔语、比哈尔语、迈蒂利语、昂加语、博杰普尔语、摩揭陀语、桑塔利语、尼瓦尔语、孔卡尼语、梵语、哈里亚纳语、英语 |
| **ta_PP-OCRv5_mobile_rec** | 泰米尔语、英语 |
| **te_PP-OCRv5_mobile_rec** | 泰卢固语、英语 |

### PP-OCRv5 FP16
- **高效版本**：在不牺牲准确率的情况下提供更快的推理速度和更低的内存使用
- **适用场景**：需要高性能和低内存使用的场景
- **性能提升**：
  - 推理速度提升约9% (支持FP16推理加速的设备上性能会更高)
  - 内存使用减少约8%
  - 模型大小减半
- **模型文件**：
  - 检测模型：`PP-OCRv5_mobile_det_fp16.mnn`
  - 识别模型：`PP-OCRv5_mobile_rec_fp16.mnn`
  - 字符集：`ppocr_keys_v5.txt`

### 模型性能对比

| 特性               | PP-OCRv4 | PP-OCRv5 | PP-OCRv5 FP16 |
|--------------------|----------|----------|---------------|
| 语言支持           | 中文、英文 | 多语言（默认支持中文/英文/日文，提供11+种专用语言模型） | 多语言（默认支持中文/英文/日文，提供11+种专用语言模型） |
| 文字类型支持       | 中文、英文 | 简体中文、繁体中文、英文、日文、中文拼音 | 简体中文、繁体中文、英文、日文、中文拼音 |
| 手写体识别         | 基础支持  | 显著增强  | 显著增强       |
| 竖排文本           | 基础支持  | 优化提升  | 优化提升       |
| 生僻字识别         | 有限支持  | 增强识别  | 增强识别       |
| 推理速度 (FPS)     | 1.1      | 1.2      | 1.2           |
| 内存使用 (峰值)    | 422.22MB | 388.41MB | 388.41MB      |
| 模型大小           | 标准      | 标准      | 减半           |
| 推荐场景           | 常规文档  | 复杂场景与多语言 | 高性能场景与多语言 |

## 应用场景

根据不同的使用需求，选择合适的 API 层级：

### 场景 1：快速集成 OCR 功能
**使用：端到端识别（OcrEngine）**

适合：
- 快速原型开发
- 简单的文档识别需求
- 不需要中间处理步骤
- 只关心最终文本结果

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
let results = engine.recognize(&image)?;
```

### 场景 2：需要自定义检测后处理
**使用：分层调用（OcrEngine 的 detect + recognize_batch）**

适合：
- 需要过滤或筛选检测结果
- 需要调整文本框位置
- 需要按特定顺序处理文本
- 需要对检测框进行排序或分组

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
// 1. 检测
let mut boxes = engine.detect(&image)?;
// 2. 自定义处理（如过滤小框）
boxes.retain(|b| b.rect.width() > 50);
// 3. 识别
let detections = engine.det_model().detect_and_crop(&image)?;
let results = engine.recognize_batch(&images)?;
```

### 场景 3：只需要检测功能
**使用：DetOnlyEngine**

适合：
- 文档版面分析
- 文本区域标注工具
- 预处理流程（只需要知道文本位置）
- 与其他识别引擎配合使用

```rust
let det_engine = OcrEngine::det_only("models/det_model.mnn", None)?;
let text_boxes = det_engine.detect(&image)?;
// 使用检测框做其他处理...
```

### 场景 4：只需要识别功能
**使用：RecOnlyEngine**

适合：
- 已知文本位置，只需要识别
- 处理预先裁剪好的文本行图像
- 手写体识别（输入单行文字图像）
- 批量识别固定格式的文本

```rust
let rec_engine = OcrEngine::rec_only(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;
let text = rec_engine.recognize_text(&text_line_image)?;
```

### 场景 5：完全自定义流程
**使用：独立模型（DetModel + RecModel）**

适合：
- 需要自定义预处理流程
- 需要对检测和识别使用不同配置
- 需要在检测和识别之间插入复杂处理逻辑
- 性能优化（如复用检测结果）

```rust
let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
let rec_model = RecModel::from_file(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;

// 完全自定义的处理流程...
```

### 场景 6：嵌入式或加密部署
**使用：从字节加载模型**

适合：
- 嵌入式设备（将模型编译进二进制）
- 需要模型加密
- 从网络动态下载模型
- 自定义模型存储格式

```rust
let det_bytes = include_bytes!("../models/det_model.mnn");
let rec_bytes = include_bytes!("../models/rec_model.mnn");
let charset_bytes = include_bytes!("../models/ppocr_keys.txt");

let engine = OcrEngine::from_bytes(det_bytes, rec_bytes, charset_bytes, None)?;
```


## 安装

在`Cargo.toml`中添加：

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"

```

您也可以指定特定分支或标签：

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"
branch = "main"
```

### 前提条件

该库需要：
- 转换为MNN格式的预训练PaddleOCR模型
- 用于文本识别的字符集文件

### MNN 链接方式

默认情况下，会自动从 [MNN-Prebuilds](https://github.com/zibo-chen/MNN-Prebuilds) 下载预编译的 MNN 静态库，无需安装 cmake 或 C++ 编译工具链。

支持自动下载预编译库的平台：
- Linux x86_64 / aarch64
- Windows x86_64 / i686
- macOS（通用：x86_64 + arm64）
- iOS arm64 / arm64-sim
- Android arm64-v8a / armeabi-v7a

不支持的平台会自动回退到从源码编译。

#### 强制从源码编译

如果需要自定义 MNN 编译选项（如 GPU 加速），可以强制从源码编译：

```bash
cargo build --features build-mnn-from-source
```

#### 使用预编译动态库

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-dynamic
```

#### 使用预编译静态库

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-static
```

#### 环境变量

| 变量 | 是否必需 | 说明 |
|---|---|---|
| `MNN_LIB_DIR` | 是（使用 `mnn-dynamic` / `mnn-static` 时） | 包含预编译 MNN 库文件的目录（`libMNN.so` / `libMNN.dylib` / `libMNN.a`） |
| `MNN_INCLUDE_DIR` | 否 | 包含 MNN 头文件的目录。未设置时回退到 `MNN_SOURCE_DIR/include` 或 `3rd_party/MNN/include` |
| `MNN_SOURCE_DIR` | 否 | MNN 源码目录（用于获取头文件或从源码编译） |

## API 架构

本库提供了**分层推理 API**，让您可以根据不同场景灵活选择使用方式：

```text
┌─────────────────────────────────────────────────┐
│         OcrEngine (端到端 Pipeline)              │
│          一次调用完成检测和识别                    │
├─────────────────────────────────────────────────┤
│  DetOnlyEngine  │  RecOnlyEngine   │  OcrEngine │
│  只做检测        │  只做识别          │  检测+识别  │
├─────────────────────────────────────────────────┤
│     DetModel          │        RecModel         │
│   文本检测模型          │       文本识别模型        │
├─────────────────────────────────────────────────┤
│            InferenceEngine (MNN)                │
│              底层推理引擎                         │
└─────────────────────────────────────────────────┘
```

### 三种使用方式

#### 1. 端到端识别（推荐）- 最简单

使用 `OcrEngine` 完成完整的 OCR 流程，一次调用完成检测和识别：

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 OCR 引擎（使用默认配置）
    let engine = OcrEngine::new(
        "models/PP-OCRv5_mobile_det.mnn",
        "models/PP-OCRv5_mobile_rec.mnn",
        "models/ppocr_keys_v5.txt",
        None,
    )?;
    
    // 加载图像
    let image = image::open("test.jpg")?;
    
    // 一次调用完成检测和识别
    let results = engine.recognize(&image)?;
    
    // 输出结果
    for result in results {
        println!("文本: {}", result.text);
        println!("置信度: {:.2}%", result.confidence * 100.0);
        println!("位置: ({}, {})", result.bbox.rect.left(), result.bbox.rect.top());
    }
    
    Ok(())
}
```

#### 2. 分层调用 - 更灵活

使用 `OcrEngine` 但分别调用检测和识别，适合需要在中间插入自定义处理的场景：

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
    let image = image::open("test.jpg")?;
    
    // 1. 先只做检测
    let text_boxes = engine.detect(&image)?;
    println!("检测到 {} 个文本区域", text_boxes.len());
    
    // 这里可以做一些自定义处理，比如：
    // - 过滤不需要的区域
    // - 调整检测框位置
    // - 按位置排序等
    
    // 2. 获取检测模型，手动裁剪
    let det_model = engine.det_model();
    let detections = det_model.detect_and_crop(&image)?;
    
    // 3. 批量识别裁剪后的图像
    let cropped_images: Vec<_> = detections.iter()
        .map(|(img, _)| img.clone())
        .collect();
    let rec_results = engine.recognize_batch(&cropped_images)?;
    
    for (result, (_, bbox)) in rec_results.iter().zip(detections.iter()) {
        println!("{}: {:.2}%", result.text, result.confidence * 100.0);
    }
    
    Ok(())
}
```

#### 3. 独立模型调用 - 最灵活

分别创建检测和识别引擎，或只创建单一功能引擎：

```rust
use ocr_rs::{DetModel, RecModel, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方式A: 分别创建检测和识别模型
    let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
    let rec_model = RecModel::from_file(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?.with_options(RecOptions::new().with_min_score(0.5));
    
    let image = image::open("test.jpg")?;
    
    // 检测并裁剪
    let detections = det_model.detect_and_crop(&image)?;
    
    // 批量识别
    let images: Vec<_> = detections.iter().map(|(img, _)| img.clone()).collect();
    let results = rec_model.recognize_batch(&images)?;
    
    // 处理结果...
    
    // 方式B: 只创建检测引擎
    let det_only = OcrEngine::det_only("models/det_model.mnn", None)?;
    let text_boxes = det_only.detect(&image)?;
    
    // 方式C: 只创建识别引擎
    let rec_only = OcrEngine::rec_only(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?;
    let text = rec_only.recognize_text(&cropped_image)?;
    
    Ok(())
}
```

## 使用示例

### 基本配置选项

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用快速模式配置
    let config = OcrEngineConfig::fast();
    
    let engine = OcrEngine::new(
        "models/PP-OCRv5_mobile_det.mnn",
        "models/PP-OCRv5_mobile_rec.mnn",
        "models/ppocr_keys_v5.txt",
        Some(config),
    )?;
    
    let image = image::open("test.jpg")?;
    let results = engine.recognize(&image)?;
    
    Ok(())
}
```

### GPU 加速

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, Backend};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用 GPU 加速
    let config = OcrEngineConfig::new()
        .with_backend(Backend::Metal);    // macOS: Metal
        // .with_backend(Backend::OpenCL); // 跨平台: OpenCL
        // .with_backend(Backend::Vulkan); // Windows/Linux: Vulkan
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### 自定义检测和识别参数

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 自定义配置
    let config = OcrEngineConfig::new()
        .with_threads(8)
        .with_det_options(
            DetOptions::new()
                .with_max_side_len(1920)     // 更高的检测分辨率
                .with_box_threshold(0.6)     // 更严格的边界框阈值
                .with_merge_boxes(true)      // 合并相邻文本框
        )
        .with_rec_options(
            RecOptions::new()
                .with_min_score(0.5)         // 过滤低置信度结果
                .with_batch_size(16)         // 批量识别大小
        );
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### 使用特定语言模型

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用韩语模型
    let engine = OcrEngine::new(
        "models/PP-OCRv5_mobile_det.mnn",
        "models/korean_PP-OCRv5_mobile_rec_infer.mnn",
        "models/ppocr_keys_korean.txt",
        None,
    )?;
    
    let image = image::open("korean_text.jpg")?;
    let results = engine.recognize(&image)?;
    
    for result in results {
        println!("{}: {:.2}%", result.text, result.confidence * 100.0);
    }
    
    Ok(())
}
```

### 从内存字节加载模型

适用于嵌入式部署或需要加密模型的场景：

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从文件读取模型字节
    let det_bytes = std::fs::read("models/det_model.mnn")?;
    let rec_bytes = std::fs::read("models/rec_model.mnn")?;
    let charset_bytes = std::fs::read("models/ppocr_keys.txt")?;
    
    // 从字节创建引擎
    let engine = OcrEngine::from_bytes(
        &det_bytes,
        &rec_bytes,
        &charset_bytes,
        None,
    )?;
    
    let image = image::open("test.jpg")?;
    let results = engine.recognize(&image)?;
    
    Ok(())
}
```

### 便捷函数

```rust
use ocr_rs::ocr_file;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 一行代码完成 OCR
    let results = ocr_file(
        "test.jpg",
        "models/det_model.mnn",
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
    )?;
    
    for result in results {
        println!("{}", result.text);
    }
    
    Ok(())
}
```

更多完整示例请参考 [examples](../examples) 目录。

## 相关项目

- 🖥️ **[newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)** - 基于本库的命令行工具，提供简单易用的OCR命令行接口
- 🔌 **[paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi)** - 提供C API绑定，方便其他编程语言（Python、Node.js、Go等）集成
- 🌐 **[newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service)** - 基于本库的HTTP服务，提供RESTful API接口 ⚠️ (施工中)

## 性能优化建议

### 1. 选择合适的精度模式

```rust
// 实时处理场景
let config = OcrEngineConfig::fast();
```

### 2. 使用 GPU 加速

```rust
// macOS/iOS
let config = OcrEngineConfig::gpu();  // 使用 Metal

// 其他平台
let config = OcrEngineConfig::new().with_backend(Backend::OpenCL);
```

### 3. 批量处理

```rust
// 批量识别多个文本行，比逐个识别快得多
let results = rec_model.recognize_batch(&images)?;
```


## 贡献

欢迎贡献！请随时提交Issue或Pull Request。



## 许可证

该项目采用Apache许可证2.0版 - 详情请参阅[LICENSE](LICENSE)文件。

## 致谢

- [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) - 提供原始OCR模型和研究
- [MNN](https://github.com/alibaba/MNN) - 提供高效的神经网络推理框架
