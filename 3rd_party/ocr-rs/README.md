# Rust PaddleOCR

[English](README.md) | [中文](./docs/README.zh.md) | [日本語](./docs/README.ja.md) | [한국어](./docs/README.ko.md)

A lightweight and efficient OCR (Optical Character Recognition) Rust library based on PaddleOCR models. This library utilizes the MNN inference framework to provide high-performance text detection and recognition capabilities.

**This project is a pure Rust library**, focused on providing core OCR functionality. For command-line tools or other language bindings, please refer to:
- 🖥️ **Command Line Tool**: [newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)
- 🔌 **C API Bindings**: [paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi) - Provides C API for easy integration with other programming languages
- 🌐 **HTTP Service**: [newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service) ⚠️ (Under construction)

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

## ✨ Version 2.0 New Features

- 🎯 **New Layered API Design**: Provides a complete layered API ranging from low-level models to high-level Pipelines.
- 🔧 **Flexible Model Loading**: Supports loading models from file paths or memory bytes.
- ⚙️ **Configurable Detection Parameters**: Supports custom detection thresholds, resolution, precision modes, and more.
- 🚀 **GPU Acceleration Support**: Supports multiple GPU backends including Metal, OpenCL, and Vulkan.
- 📦 **Batch Processing Optimization**: Supports batch text recognition to improve throughput.
- 🔌 **Independent Engine Mode**: Ability to create standalone detection engines or recognition engines.

## Features

### Core Capabilities
- **Text Detection**: Accurately locates text regions within images.
- **Text Recognition**: Recognizes text content within detected regions.
- **End-to-End Recognition**: completes the full flow of detection and recognition in a single call.
- **Layered API Architecture**: Supports three usage patterns: end-to-end, layered calls, and independent models.

### Model Support
- **Multi-Version Support**: Supports both PP-OCRv4 and PP-OCRv5 models for flexible selection.
- **Multi-Language Support**: PP-OCRv5 supports 11+ specific language models, covering over 100 languages.
- **Complex Scenario Recognition**: Enhanced capabilities for handwritten text, vertical text, and rare characters.
- **Flexible Loading**: Models can be loaded via file paths or directly from memory bytes.

### Performance
- **High-Performance Inference**: Based on the MNN inference framework for fast speed and low memory usage.
- **GPU Acceleration**: Supports Metal, OpenCL, Vulkan, and other GPU backends.
- **Batch Processing**: Supports batch text recognition to boost throughput.

### Developer Experience
- **Flexible Configuration**: Parameters such as detection thresholds, resolution, and precision modes are customizable.
- **Memory Safety**: Automatic memory management to prevent leaks.
- **Pure Rust Implementation**: No external runtime required, cross-platform compatible.
- **Minimal Dependencies**: Lightweight and easy to integrate.

## Model Versions

This library supports three versions of PaddleOCR models:

### PP-OCRv4
- **Stable Version**: thoroughly verified with excellent compatibility.
- **Use Case**: Standard document recognition where high accuracy is required.
- **Model Files**:
  - Detection: `ch_PP-OCRv4_det_infer.mnn`
  - Recognition: `ch_PP-OCRv4_rec_infer.mnn`
  - Charset: `ppocr_keys_v4.txt`

### PP-OCRv5
- **Latest Version**: The next-generation text recognition solution.
- **Multi-Language Support**: The default model (`PP-OCRv5_mobile_rec.mnn`) supports Simplified Chinese, Traditional Chinese, English, Japanese, and Chinese Pinyin.
- **Specific Language Models**: Provides dedicated models for 11+ languages covering 100+ languages for optimal performance.
- **Shared Detection Model**: All V5 language models share the same detection model (`PP-OCRv5_mobile_det.mnn`).
- **Enhanced Scenario Recognition**:
  - Significantly improved recognition for complex handwritten Chinese/English.
  - Optimized vertical text recognition.
  - Enhanced recognition of rare characters.
- **Performance**: 13% improvement in end-to-end performance compared to PP-OCRv4.
- **Model Files** (Default Multi-language):
  - Detection: `PP-OCRv5_mobile_det.mnn` (Shared by all languages)
  - Recognition: `PP-OCRv5_mobile_rec.mnn` (Default, supports CN/EN/JP)
  - Charset: `ppocr_keys_v5.txt`
- **Specific Language Model Files** (Optional):
  - Recognition: `{lang}_PP-OCRv5_mobile_rec_infer.mnn`
  - Charset: `ppocr_keys_{lang}.txt`
  - Available Language Codes: `arabic`, `cyrillic`, `devanagari`, `el`, `en`, `eslav`, `korean`, `latin`, `ta`, `te`, `th`

#### PP-OCRv5 Language Model Support List

| Model Name | Supported Languages |
|---------|-----------|
| **korean_PP-OCRv5_mobile_rec** | Korean, English |
| **latin_PP-OCRv5_mobile_rec** | French, German, Afrikaans, Italian, Spanish, Bosnian, Portuguese, Czech, Welsh, Danish, Estonian, Irish, Croatian, Uzbek, Hungarian, Serbian (Latin), Indonesian, Occitan, Icelandic, Lithuanian, Maori, Malay, Dutch, Norwegian, Polish, Slovak, Slovenian, Albanian, Swedish, Swahili, Tagalog, Turkish, Latin, Azerbaijani, Kurdish, Latvian, Maltese, Pali, Romanian, Vietnamese, Finnish, Basque, Galician, Luxembourgish, Romansh, Catalan, Quechua |
| **eslav_PP-OCRv5_mobile_rec** | Russian, Belarusian, Ukrainian, English |
| **th_PP-OCRv5_mobile_rec** | Thai, English |
| **el_PP-OCRv5_mobile_rec** | Greek, English |
| **en_PP-OCRv5_mobile_rec** | English |
| **cyrillic_PP-OCRv5_mobile_rec** | Russian, Belarusian, Ukrainian, Serbian (Cyrillic), Bulgarian, Mongolian, Abkhaz, Adyghe, Kabardian, Avar, Dargwa, Ingush, Chechen, Lak, Lezgian, Tabasaran, Kazakh, Kyrgyz, Tajik, Macedonian, Tatar, Chuvash, Bashkir, Mari, Moldovan, Udmurt, Komi, Ossetian, Buryat, Kalmyk, Tuvan, Yakut, Karakalpak, English |
| **arabic_PP-OCRv5_mobile_rec** | Arabic, Persian, Uyghur, Urdu, Pashto, Kurdish, Sindhi, Balochi, English |
| **devanagari_PP-OCRv5_mobile_rec** | Hindi, Marathi, Nepali, Bihari, Maithili, Angika, Bhojpuri, Magahi, Santali, Newari, Konkani, Sanskrit, Haryanvi, English |
| **ta_PP-OCRv5_mobile_rec** | Tamil, English |
| **te_PP-OCRv5_mobile_rec** | Telugu, English |

### PP-OCRv5 FP16
- **High Efficiency Version**: Provides faster inference speeds and lower memory usage without sacrificing accuracy.
- **Use Case**: Scenarios requiring high performance and low memory footprint.
- **Performance Gains**:
  - Inference speed increased by ~9% (higher on devices supporting FP16 acceleration).
  - Memory usage reduced by ~8%.
  - Model size halved.
- **Model Files**:
  - Detection: `PP-OCRv5_mobile_det_fp16.mnn`
  - Recognition: `PP-OCRv5_mobile_rec_fp16.mnn`
  - Charset: `ppocr_keys_v5.txt`

### Model Performance Comparison

| Feature | PP-OCRv4 | PP-OCRv5 | PP-OCRv5 FP16 |
|---|---|---|---|
| Language Support | Chinese, English | Multi-language (Default CN/EN/JP, 11+ specific models) | Multi-language (Default CN/EN/JP, 11+ specific models) |
| Text Types | Chinese, English | Simplified/Traditional CN, EN, JP, Pinyin | Simplified/Traditional CN, EN, JP, Pinyin |
| Handwriting | Basic | Significantly Enhanced | Significantly Enhanced |
| Vertical Text | Basic | Optimized | Optimized |
| Rare Characters | Limited | Enhanced | Enhanced |
| Speed (FPS) | 1.1 | 1.2 | 1.2 |
| Memory (Peak) | 422.22MB | 388.41MB | 388.41MB |
| Model Size | Standard | Standard | Halved |
| Recommended | Standard Docs | Complex Scenes & Multi-lang | High Performance & Multi-lang |

## Application Scenarios

Choose the appropriate API level based on your requirements:

### Scenario 1: Quick OCR Integration
**Use: End-to-End Recognition (`OcrEngine`)**

Suitable for:
- Rapid prototyping.
- Simple document recognition needs.
- No need for intermediate processing.
- Only care about the final text result.

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
let results = engine.recognize(&image)?;
```

### Scenario 2: Custom Post-Processing for Detection
**Use: Layered Calls (`OcrEngine` detect + recognize_batch)**

Suitable for:
- Filtering or selecting detection results.
- Adjusting text box positions.
- Processing text in a specific order.
- Sorting or grouping detection boxes.

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
// 1. Detect
let mut boxes = engine.detect(&image)?;
// 2. Custom processing (e.g., filter small boxes)
boxes.retain(|b| b.rect.width() > 50);
// 3. Recognize
let detections = engine.det_model().detect_and_crop(&image)?;
let results = engine.recognize_batch(&images)?;
```

### Scenario 3: Detection Only
**Use: `DetOnlyEngine`**

Suitable for:
- Document layout analysis.
- Text region annotation tools.
- Pre-processing workflows (only need text locations).
- Using with other recognition engines.

```rust
let det_engine = OcrEngine::det_only("models/det_model.mnn", None)?;
let text_boxes = det_engine.detect(&image)?;
// Use detection boxes for other processing...
```

### Scenario 4: Recognition Only
**Use: `RecOnlyEngine`**

Suitable for:
- Text location is already known, only recognition is needed.
- Processing pre-cropped text line images.
- Handwriting recognition (input is a single line image).
- Batch recognition of fixed-format text.

```rust
let rec_engine = OcrEngine::rec_only(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;
let text = rec_engine.recognize_text(&text_line_image)?;
```

### Scenario 5: Fully Custom Workflow
**Use: Independent Models (`DetModel` + `RecModel`)**

Suitable for:
- Custom pre-processing workflows.
- Different configurations for detection and recognition.
- Inserting complex logic between detection and recognition.
- Performance optimization (e.g., reusing detection results).

```rust
let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
let rec_model = RecModel::from_file(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;

// Fully custom processing flow...
```

### Scenario 6: Embedded or Encrypted Deployment
**Use: Load from Bytes**

Suitable for:
- Embedded devices (compiling models into the binary).
- Model encryption requirements.
- Downloading models dynamically from the network.
- Custom model storage formats.

```rust
let det_bytes = include_bytes!("../models/det_model.mnn");
let rec_bytes = include_bytes!("../models/rec_model.mnn");
let charset_bytes = include_bytes!("../models/ppocr_keys.txt");

let engine = OcrEngine::from_bytes(det_bytes, rec_bytes, charset_bytes, None)?;
```

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"
```

You can also specify a specific branch or tag:

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"
branch = "next"
```

### Prerequisites

This library requires:
- Pre-trained PaddleOCR models converted to MNN format.
- A character set file for text recognition.

### MNN Linking Mode

By default, prebuilt MNN static libraries are automatically downloaded from [MNN-Prebuilds](https://github.com/zibo-chen/MNN-Prebuilds) releases. No cmake or C++ compiler toolchain is required for building.

Supported platforms for prebuilt downloads:
- Linux x86_64 / aarch64
- Windows x86_64 / i686
- macOS (universal: x86_64 + arm64)
- iOS arm64 / arm64-sim
- Android arm64-v8a / armeabi-v7a

For unsupported platforms, the build system automatically falls back to building MNN from source.

#### Force Build from Source

If you need custom MNN build options (e.g., GPU acceleration), you can force building from source:

```bash
cargo build --features build-mnn-from-source
```

#### Use Pre-built Dynamic Library

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-dynamic
```

#### Use Pre-built Static Library

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-static
```

#### Environment Variables

| Variable | Required | Description |
|---|---|---|
| `MNN_LIB_DIR` | Yes (for `mnn-dynamic` / `mnn-static`) | Directory containing pre-built MNN library (`libMNN.so` / `libMNN.dylib` / `libMNN.a`) |
| `MNN_INCLUDE_DIR` | No | Directory containing MNN headers. Falls back to `MNN_SOURCE_DIR/include` or `3rd_party/MNN/include` |
| `MNN_SOURCE_DIR` | No | Path to MNN source tree (used for headers or building from source) |

## API Architecture

This library provides a **Layered Inference API**, allowing you to choose the usage pattern that best fits your scenario:

```text
┌─────────────────────────────────────────────────┐
│         OcrEngine (End-to-End Pipeline)         │
│      Complete detection & recognition in one call │
├─────────────────────────────────────────────────┤
│  DetOnlyEngine  │  RecOnlyEngine   │  OcrEngine │
│  Detection Only │ Recognition Only │  Det + Rec │
├─────────────────────────────────────────────────┤
│     DetModel          │        RecModel         │
│   Text Det Model      │      Text Rec Model     │
├─────────────────────────────────────────────────┤
│            InferenceEngine (MNN)                │
│            Low-level Inference Engine           │
└─────────────────────────────────────────────────┘
```

### Three Usage Patterns

#### 1. End-to-End Recognition (Recommended) - Simplest

Use `OcrEngine` to complete the full OCR process with a single call:

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create OCR engine (using default config)
    let engine = OcrEngine::new(
        "models/PP-OCRv5_mobile_det.mnn",
        "models/PP-OCRv5_mobile_rec.mnn",
        "models/ppocr_keys_v5.txt",
        None,
    )?;
    
    // Load image
    let image = image::open("test.jpg")?;
    
    // Perform detection and recognition in one call
    let results = engine.recognize(&image)?;
    
    // Output results
    for result in results {
        println!("Text: {}", result.text);
        println!("Confidence: {:.2}%", result.confidence * 100.0);
        println!("Position: ({}, {})", result.bbox.rect.left(), result.bbox.rect.top());
    }
    
    Ok(())
}
```

#### 2. Layered Calls - More Flexible

Use `OcrEngine` but call detection and recognition separately. Useful for inserting custom processing:

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
    let image = image::open("test.jpg")?;
    
    // 1. Detection only first
    let text_boxes = engine.detect(&image)?;
    println!("Detected {} text regions", text_boxes.len());
    
    // Custom processing can be done here, e.g.:
    // - Filter unwanted regions
    // - Adjust box positions
    // - Sort by position, etc.
    
    // 2. Get detection model and manually crop
    let det_model = engine.det_model();
    let detections = det_model.detect_and_crop(&image)?;
    
    // 3. Batch recognize cropped images
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

#### 3. Independent Model Calls - Most Flexible

Create detection and recognition engines separately, or create a single-function engine:

```rust
use ocr_rs::{DetModel, RecModel, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Method A: Create detection and recognition models separately
    let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
    let rec_model = RecModel::from_file(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?.with_options(RecOptions::new().with_min_score(0.5));
    
    let image = image::open("test.jpg")?;
    
    // Detect and crop
    let detections = det_model.detect_and_crop(&image)?;
    
    // Batch recognize
    let images: Vec<_> = detections.iter().map(|(img, _)| img.clone()).collect();
    let results = rec_model.recognize_batch(&images)?;
    
    // Process results...
    
    // Method B: Create detection-only engine
    let det_only = OcrEngine::det_only("models/det_model.mnn", None)?;
    let text_boxes = det_only.detect(&image)?;
    
    // Method C: Create recognition-only engine
    let rec_only = OcrEngine::rec_only(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?;
    let text = rec_only.recognize_text(&cropped_image)?;
    
    Ok(())
}
```

## Usage Examples

### Basic Configuration Options

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use fast mode configuration
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

### GPU Acceleration

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, Backend};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use GPU acceleration
    let config = OcrEngineConfig::new()
        .with_backend(Backend::Metal);    // macOS: Metal
        // .with_backend(Backend::OpenCL); // Cross-platform: OpenCL
        // .with_backend(Backend::Vulkan); // Windows/Linux: Vulkan
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### Custom Detection and Recognition Parameters

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Custom configuration
    let config = OcrEngineConfig::new()
        .with_threads(8)
        .with_det_options(
            DetOptions::new()
                .with_max_side_len(1920)     // Higher detection resolution
                .with_box_threshold(0.6)     // Stricter bounding box threshold
                .with_merge_boxes(true)      // Merge adjacent text boxes
        )
        .with_rec_options(
            RecOptions::new()
                .with_min_score(0.5)         // Filter low confidence results
                .with_batch_size(16)         // Batch recognition size
        );
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### Document Orientation Model (Builder)

```rust
use ocr_rs::OcrEngineBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = OcrEngineBuilder::new()
        .with_det_model_path("models/PP-OCRv5_mobile_det.mnn")
        .with_rec_model_path("models/PP-OCRv5_mobile_rec.mnn")
        .with_charset_path("models/ppocr_keys_v5.txt")
        .with_ori_model_path("models/PP-LCNet_x1_0_doc_ori.mnn")
        .build()?;

    Ok(())
}
```

### Using Specific Language Models

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use Korean model
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

### Loading Models from Memory Bytes

Suitable for embedded deployment or scenarios requiring model encryption:

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read model bytes from file (or other sources)
    let det_bytes = std::fs::read("models/det_model.mnn")?;
    let rec_bytes = std::fs::read("models/rec_model.mnn")?;
    let charset_bytes = std::fs::read("models/ppocr_keys.txt")?;
    
    // Create engine from bytes
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

### Convenience Functions

```rust
use ocr_rs::ocr_file;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Perform OCR in one line of code
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

For more complete examples, please refer to the [examples](../examples) directory.

## Related Projects

- 🖥️ **[newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)** - A command-line tool based on this library, providing a simple and easy-to-use OCR CLI interface.
- 🔌 **[paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi)** - C API bindings for this library, enabling easy integration with other programming languages (Python, Node.js, Go, etc.).
- 🌐 **[newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service)** - An HTTP service based on this library, providing RESTful API interfaces. ⚠️ (Under construction)

## Performance Optimization Suggestions

### 1. Choose Appropriate Precision Modes

```rust
// Real-time processing
let config = OcrEngineConfig::fast();
```

### 2. Use GPU Acceleration

```rust
// macOS/iOS
let config = OcrEngineConfig::gpu();  // Uses Metal

// Other platforms
let config = OcrEngineConfig::new().with_backend(Backend::OpenCL);
```

### 3. Batch Processing

```rust
// Batch recognizing multiple text lines is much faster than one by one
let results = rec_model.recognize_batch(&images)?;
```

## Contribution

Contributions are welcome! Please feel free to submit Issues or Pull Requests.

## License

This project is licensed under the Apache License, Version 2.0 - see the [LICENSE](LICENSE) file for details.

## Acknowledgements

- [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) - Provided the original OCR models and research.
- [MNN](https://github.com/alibaba/MNN) - Provided the efficient neural network inference framework.
