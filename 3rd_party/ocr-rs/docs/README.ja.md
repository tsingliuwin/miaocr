# Rust PaddleOCR

[English](../README.md) | [中文](./README.zh.md) | [日本語](./README.ja.md) | [한국어](./README.ko.md)

PaddleOCRモデルに基づく、軽量で高効率なOCR（光学文字認識）Rustライブラリです。本ライブラリはMNN推論フレームワークを利用して、高性能なテキスト検出および認識機能を提供します。

**本プロジェクトは純粋なRustライブラリであり**、OCRのコア機能の提供に専念しています。コマンドラインツールや他の言語バインディングが必要な場合は、以下を参照してください：
- 🖥️ **コマンドラインツール**：[newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)
- 🔌 **C APIバインディング**：[paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi) - 他のプログラミング言語との統合を容易にするC APIを提供
- 🌐 **HTTPサービス**：[newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service) ⚠️ (開発中)

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

## ✨ バージョン 2.0 の新機能

- 🎯 **新しい階層化API設計**：低レベルのモデル操作から高レベルのパイプラインまで、完全な階層化APIを提供。
- 🔧 **柔軟なモデル読み込み**：ファイルパスまたはメモリ上のバイトデータからのモデル読み込みをサポート。
- ⚙️ **設定可能な検出パラメータ**：検出閾値、解像度、精度モードなどのカスタマイズに対応。
- 🚀 **GPUアクセラレーション対応**：Metal、OpenCL、Vulkanなど、複数のGPUバックエンドをサポート。
- 📦 **バッチ処理の最適化**：テキスト認識のバッチ処理をサポートし、スループットを向上。
- 🔌 **独立エンジンモード**：検出エンジンまたは認識エンジンのみを単独で作成可能。

## 特徴

### コア機能
- **テキスト検出**：画像内のテキスト領域を正確に特定します。
- **テキスト認識**：検出された領域内のテキスト内容を認識します。
- **エンドツーエンド認識**：1回の呼び出しで検出と認識の全プロセスを完了します。
- **階層化APIアーキテクチャ**：エンドツーエンド、階層化呼び出し、独立モデルの3つの使用パターンをサポート。

### モデルサポート
- **マルチバージョン対応**：PP-OCRv4 および PP-OCRv5 モデルをサポートし、柔軟に選択可能。
- **多言語対応**：PP-OCRv5は11種類以上の専用言語モデルを提供し、100以上の言語をカバー。
- **複雑なシーンの認識**：手書き文字、縦書きテキスト、希少文字の認識能力を強化。
- **柔軟な読み込み方式**：ファイルパスまたはメモリバイトからのモデル読み込みに対応。

### パフォーマンス
- **高性能推論**：MNN推論フレームワークに基づき、高速かつ低メモリ消費を実現。
- **GPUアクセラレーション**：Metal、OpenCL、VulkanなどのGPUバックエンドをサポート。
- **バッチ処理**：テキスト認識のバッチ処理によりスループットを向上。

### 開発者体験
- **柔軟な設定**：検出閾値、解像度、精度モードなどのパラメータをカスタマイズ可能。
- **メモリ安全性**：自動メモリ管理によりメモリリークを防止。
- **純粋なRust実装**：外部ランタイム不要で、クロスプラットフォーム互換。
- **最小限の依存関係**：軽量で統合が容易。

## モデルバージョン

本ライブラリは3つのPaddleOCRモデルバージョンをサポートしています：

### PP-OCRv4
- **安定版**：十分に検証されており、互換性が良好です。
- **適用シーン**：高い正確性が求められる一般的なドキュメント認識。
- **モデルファイル**：
  - 検出モデル：`ch_PP-OCRv4_det_infer.mnn`
  - 認識モデル：`ch_PP-OCRv4_rec_infer.mnn`
  - 文字セット：`ppocr_keys_v4.txt`

### PP-OCRv5
- **最新版**：次世代の文字認識ソリューション。
- **多言語対応**：デフォルトモデル（`PP-OCRv5_mobile_rec.mnn`）は、簡体字中国語、繁体字中国語、英語、日本語、中国語ピンインをサポート。
- **専用言語モデル**：11種類以上の言語専用モデルを提供し、100以上の言語で最高のパフォーマンスを実現。
- **共有検出モデル**：すべてのV5言語モデルは同一の検出モデル（`PP-OCRv5_mobile_det.mnn`）を使用します。
- **シーン認識の強化**：
  - 中国語・英語の複雑な手書き文字認識能力が大幅に向上。
  - 縦書きテキストの認識を最適化。
  - 希少文字の認識能力を強化。
- **パフォーマンス向上**：PP-OCRv4と比較してエンドツーエンドで13%向上。
- **モデルファイル**（デフォルト多言語）：
  - 検出モデル：`PP-OCRv5_mobile_det.mnn`（全言語共通）
  - 認識モデル：`PP-OCRv5_mobile_rec.mnn`（デフォルト、中/英/日対応）
  - 文字セット：`ppocr_keys_v5.txt`
- **専用言語モデルファイル**（オプション）：
  - 認識モデル：`{lang}_PP-OCRv5_mobile_rec_infer.mnn`
  - 文字セット：`ppocr_keys_{lang}.txt`
  - 利用可能な言語コード：`arabic`, `cyrillic`, `devanagari`, `el`, `en`, `eslav`, `korean`, `latin`, `ta`, `te`, `th`

#### PP-OCRv5 言語モデル対応リスト

| モデル名 | 対応言語 |
|---------|-----------|
| **korean_PP-OCRv5_mobile_rec** | 韓国語、英語 |
| **latin_PP-OCRv5_mobile_rec** | フランス語、ドイツ語、アフリカーンス語、イタリア語、スペイン語、ボスニア語、ポルトガル語、チェコ語、ウェールズ語、デンマーク語、エストニア語、アイルランド語、クロアチア語、ウズベク語、ハンガリー語、セルビア語（ラテン文字）、インドネシア語、オック語、アイスランド語、リトアニア語、マオリ語、マレー語、オランダ語、ノルウェー語、ポーランド語、スロバキア語、スロベニア語、アルバニア語、スウェーデン語、スワヒリ語、タガログ語、トルコ語、ラテン語、アゼルバイジャン語、クルド語、ラトビア語、マルタ語、パーリ語、ルーマニア語、ベトナム語、フィンランド語、バスク語、ガリシア語、ルクセンブルク語、ロマンシュ語、カタルーニャ語、ケチュア語 |
| **eslav_PP-OCRv5_mobile_rec** | ロシア語、ベラルーシ語、ウクライナ語、英語 |
| **th_PP-OCRv5_mobile_rec** | タイ語、英語 |
| **el_PP-OCRv5_mobile_rec** | ギリシャ語、英語 |
| **en_PP-OCRv5_mobile_rec** | 英語 |
| **cyrillic_PP-OCRv5_mobile_rec** | ロシア語、ベラルーシ語、ウクライナ語、セルビア語（キリル文字）、ブルガリア語、モンゴル語、アブハズ語、アディゲ語、カバルド語、アヴァル語、ダルグワ語、イングーシ語、チェチェン語、ラク語、レズギ語、タバサラン語、カザフ語、キルギス語、タジク語、マケドニア語、タタール語、チュヴァシ語、バシキール語、マリ語、モルドバ語、ウドムルト語、コミ語、オセチア語、ブリヤート語、カルムイク語、トゥバ語、サハ語、カラカルパク語、英語 |
| **arabic_PP-OCRv5_mobile_rec** | アラビア語、ペルシア語、ウイグル語、ウルドゥー語、パシュトー語、クルド語、シンド語、バローチ語、英語 |
| **devanagari_PP-OCRv5_mobile_rec** | ヒンディー語、マラーティー語、ネパール語、ビハール語、マイティリー語、アンギカ語、ボージュプリー語、マガヒー語、サンターリー語、ネワール語、コンカニ語、サンスクリット語、ハリヤーンウィー語、英語 |
| **ta_PP-OCRv5_mobile_rec** | タミル語、英語 |
| **te_PP-OCRv5_mobile_rec** | テルグ語、英語 |

### PP-OCRv5 FP16
- **高効率版**：精度を犠牲にすることなく、より速い推論速度と低いメモリ使用量を提供します。
- **適用シーン**：高性能と低メモリ消費が求められるシーン。
- **パフォーマンス向上**：
  - 推論速度が約9%向上（FP16推論加速をサポートするデバイスではさらに向上）。
  - メモリ使用量が約8%減少。
  - モデルサイズが半減。
- **モデルファイル**：
  - 検出モデル：`PP-OCRv5_mobile_det_fp16.mnn`
  - 認識モデル：`PP-OCRv5_mobile_rec_fp16.mnn`
  - 文字セット：`ppocr_keys_v5.txt`

### モデル性能比較

| 特性 | PP-OCRv4 | PP-OCRv5 | PP-OCRv5 FP16 |
|---|---|---|---|
| 言語サポート | 中国語、英語 | 多言語（デフォルト中/英/日、11+専用モデル） | 多言語（デフォルト中/英/日、11+専用モデル） |
| 文字タイプ | 中国語、英語 | 簡体字/繁体字、英語、日本語、ピンイン | 簡体字/繁体字、英語、日本語、ピンイン |
| 手書き認識 | 基本的 | 大幅に強化 | 大幅に強化 |
| 縦書きテキスト | 基本的 | 最適化 | 最適化 |
| 希少文字 | 限定的 | 認識強化 | 認識強化 |
| 推論速度 (FPS) | 1.1 | 1.2 | 1.2 |
| メモリ (ピーク) | 422.22MB | 388.41MB | 388.41MB |
| モデルサイズ | 標準 | 標準 | 半減 |
| 推奨シーン | 一般文書 | 複雑なシーン・多言語 | 高性能要求・多言語 |

## 利用シーン

要件に応じて適切なAPIレベルを選択してください：

### シーン 1：OCR機能の迅速な統合
**使用：エンドツーエンド認識（OcrEngine）**

適している場合：
- プロトタイプの迅速な開発
- シンプルなドキュメント認識ニーズ
- 中間処理ステップが不要
- 最終的なテキスト結果のみが必要

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
let results = engine.recognize(&image)?;
```

### シーン 2：検出後のカスタム処理が必要
**使用：階層化呼び出し（OcrEngine の detect + recognize_batch）**

適している場合：
- 検出結果のフィルタリングや選別が必要
- テキストボックスの位置調整が必要
- 特定の順序でテキストを処理したい
- 検出ボックスの並べ替えやグループ化が必要

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
// 1. 検出
let mut boxes = engine.detect(&image)?;
// 2. カスタム処理（例：小さなボックスを除外）
boxes.retain(|b| b.rect.width() > 50);
// 3. 認識
let detections = engine.det_model().detect_and_crop(&image)?;
let results = engine.recognize_batch(&images)?;
```

### シーン 3：検出機能のみが必要
**使用：DetOnlyEngine**

適している場合：
- ドキュメントのレイアウト分析
- テキスト領域のアノテーションツール
- 前処理フロー（テキストの位置だけ知りたい場合）
- 他の認識エンジンとの組み合わせ

```rust
let det_engine = OcrEngine::det_only("models/det_model.mnn", None)?;
let text_boxes = det_engine.detect(&image)?;
// 検出ボックスを使用して他の処理を行う...
```

### シーン 4：認識機能のみが必要
**使用：RecOnlyEngine**

適している場合：
- テキスト位置が既知で、認識のみ行いたい
- 事前に切り出されたテキスト行画像を処理する
- 手書き認識（入力が1行の文字画像）
- 固定フォーマットテキストのバッチ認識

```rust
let rec_engine = OcrEngine::rec_only(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;
let text = rec_engine.recognize_text(&text_line_image)?;
```

### シーン 5：完全にカスタムなフロー
**使用：独立モデル（DetModel + RecModel）**

適している場合：
- カスタムな前処理フローが必要
- 検出と認識で異なる設定を使用したい
- 検出と認識の間に複雑な処理ロジックを挿入したい
- パフォーマンス最適化（例：検出結果の再利用）

```rust
let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
let rec_model = RecModel::from_file(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;

// 完全にカスタムな処理フロー...
```

### シーン 6：組み込みまたは暗号化デプロイ
**使用：バイトデータからの読み込み**

適している場合：
- 組み込みデバイス（バイナリにモデルをコンパイルして含める）
- モデルの暗号化が必要
- ネットワークから動的にモデルをダウンロードする
- カスタムなモデル保存形式

```rust
let det_bytes = include_bytes!("../models/det_model.mnn");
let rec_bytes = include_bytes!("../models/rec_model.mnn");
let charset_bytes = include_bytes!("../models/ppocr_keys.txt");

let engine = OcrEngine::from_bytes(det_bytes, rec_bytes, charset_bytes, None)?;
```

## インストール

`Cargo.toml` に以下を追加してください：

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"
```

特定のブランチやタグを指定することもできます：

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"
branch = "main"
```

### 前提条件

このライブラリには以下が必要です：
- MNN形式に変換された事前学習済みPaddleOCRモデル
- テキスト認識用の文字セットファイル

### MNNリンク方式

デフォルトでは、[MNN-Prebuilds](https://github.com/zibo-chen/MNN-Prebuilds)のリリースから事前構築されたMNN静的ライブラリが自動的にダウンロードされます。ビルドにはcmakeまたはC++コンパイラツールチェーンは必要ありません。

事前構築ダウンロードをサポートするプラットフォーム：
- Linux x86_64 / aarch64
- Windows x86_64 / i686
- macOS（ユニバーサル：x86_64 + arm64）
- iOS arm64 / arm64-sim
- Android arm64-v8a / armeabi-v7a

サポートされていないプラットフォームでは、ビルドシステムが自動的にMNNをソースからビルドする方式にフォールバックします。

#### ソースからの強制ビルド

カスタムMNNビルドオプション（例：GPUアクセラレーション）が必要な場合は、ソースからビルドを強制できます：

```bash
cargo build --features build-mnn-from-source
```

#### 事前構築動的ライブラリの使用

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-dynamic
```

#### 事前構築静的ライブラリの使用

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-static
```

#### 環境変数

| 変数 | 必須 | 説明 |
|---|---|---|
| `MNN_LIB_DIR` | はい（`mnn-dynamic` / `mnn-static`を使用する場合） | 事前構築MNNライブラリを含むディレクトリ（`libMNN.so` / `libMNN.dylib` / `libMNN.a`） |
| `MNN_INCLUDE_DIR` | いいえ | MNNヘッダーを含むディレクトリ。設定されていない場合は`MNN_SOURCE_DIR/include`または`3rd_party/MNN/include`にフォールバックします |
| `MNN_SOURCE_DIR` | いいえ | MNNソースツリーのパス（ヘッダーの取得またはソースからのビルドに使用） |

## API アーキテクチャ

本ライブラリは**階層化推論 API**を提供しており、シーンに合わせて最適な使用方法を選択できます：

```text
┌─────────────────────────────────────────────────┐
│         OcrEngine (エンドツーエンド Pipeline)    │
│          1回の呼び出しで検出と認識を完了           │
├─────────────────────────────────────────────────┤
│  DetOnlyEngine  │  RecOnlyEngine   │  OcrEngine │
│   検出のみ       │   認識のみ        │  検出+認識  │
├─────────────────────────────────────────────────┤
│     DetModel          │        RecModel         │
│   テキスト検出モデル    │      テキスト認識モデル   │
├─────────────────────────────────────────────────┤
│            InferenceEngine (MNN)                │
│              低レベル推論エンジン                 │
└─────────────────────────────────────────────────┘
```

### 3つの使用パターン

#### 1. エンドツーエンド認識（推奨）- 最も簡単

`OcrEngine` を使用して完全なOCRフローを実行します。1回の呼び出しで検出と認識が完了します：

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // OCRエンジンの作成（デフォルト設定を使用）
    let engine = OcrEngine::new(
        "models/PP-OCRv5_mobile_det.mnn",
        "models/PP-OCRv5_mobile_rec.mnn",
        "models/ppocr_keys_v5.txt",
        None,
    )?;
    
    // 画像の読み込み
    let image = image::open("test.jpg")?;
    
    // 1回の呼び出しで検出と認識を実行
    let results = engine.recognize(&image)?;
    
    // 結果の出力
    for result in results {
        println!("テキスト: {}", result.text);
        println!("信頼度: {:.2}%", result.confidence * 100.0);
        println!("位置: ({}, {})", result.bbox.rect.left(), result.bbox.rect.top());
    }
    
    Ok(())
}
```

#### 2. 階層化呼び出し - より柔軟

`OcrEngine` を使用しますが、検出と認識を別々に呼び出します。中間にカスタム処理を挿入する場合に適しています：

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
    let image = image::open("test.jpg")?;
    
    // 1. まず検出のみ実行
    let text_boxes = engine.detect(&image)?;
    println!("{} 個のテキスト領域を検出しました", text_boxes.len());
    
    // ここでカスタム処理が可能。例：
    // - 不要な領域をフィルタリング
    // - 検出ボックスの位置調整
    // - 位置によるソートなど
    
    // 2. 検出モデルを取得し、手動でクロップ（切り出し）
    let det_model = engine.det_model();
    let detections = det_model.detect_and_crop(&image)?;
    
    // 3. 切り出した画像をバッチ認識
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

#### 3. 独立モデル呼び出し - 最も柔軟

検出エンジンと認識エンジンを別々に作成するか、単機能エンジンのみを作成します：

```rust
use ocr_rs::{DetModel, RecModel, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方法A: 検出モデルと認識モデルを別々に作成
    let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
    let rec_model = RecModel::from_file(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?.with_options(RecOptions::new().with_min_score(0.5));
    
    let image = image::open("test.jpg")?;
    
    // 検出して切り出し
    let detections = det_model.detect_and_crop(&image)?;
    
    // バッチ認識
    let images: Vec<_> = detections.iter().map(|(img, _)| img.clone()).collect();
    let results = rec_model.recognize_batch(&images)?;
    
    // 結果の処理...
    
    // 方法B: 検出専用エンジンを作成
    let det_only = OcrEngine::det_only("models/det_model.mnn", None)?;
    let text_boxes = det_only.detect(&image)?;
    
    // 方法C: 認識専用エンジンを作成
    let rec_only = OcrEngine::rec_only(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?;
    let text = rec_only.recognize_text(&cropped_image)?;
    
    Ok(())
}
```

## 使用例

### 基本設定オプション

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 高速モード設定を使用
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

### GPU アクセラレーション

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, Backend};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // GPUアクセラレーションを使用
    let config = OcrEngineConfig::new()
        .with_backend(Backend::Metal);    // macOS: Metal
        // .with_backend(Backend::OpenCL); // クロスプラットフォーム: OpenCL
        // .with_backend(Backend::Vulkan); // Windows/Linux: Vulkan
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### 検出と認識のカスタムパラメータ

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // カスタム設定
    let config = OcrEngineConfig::new()
        .with_threads(8)
        .with_det_options(
            DetOptions::new()
                .with_max_side_len(1920)     // より高い検出解像度
                .with_box_threshold(0.6)     // より厳密なバウンディングボックス閾値
                .with_merge_boxes(true)      // 隣接するテキストボックスを統合
        )
        .with_rec_options(
            RecOptions::new()
                .with_min_score(0.5)         // 低信頼度の結果を除外
                .with_batch_size(16)         // バッチ認識サイズ
        );
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### 特定言語モデルの使用

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 韓国語モデルの使用
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

### メモリバイトからのモデル読み込み

組み込みデプロイやモデル暗号化が必要なシーンに適しています：

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ファイル（または他のソース）からモデルのバイトデータを読み込む
    let det_bytes = std::fs::read("models/det_model.mnn")?;
    let rec_bytes = std::fs::read("models/rec_model.mnn")?;
    let charset_bytes = std::fs::read("models/ppocr_keys.txt")?;
    
    // バイトデータからエンジンを作成
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

### 便利な関数

```rust
use ocr_rs::ocr_file;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1行のコードでOCRを実行
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

より完全な例については、[examples](../examples) ディレクトリを参照してください。

## 関連プロジェクト

- 🖥️ **[newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)** - 本ライブラリに基づくコマンドラインツール。シンプルで使いやすいOCR CLIインターフェースを提供。
- 🔌 **[paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi)** - C APIバインディング。他のプログラミング言語（Python、Node.js、Goなど）との統合を容易にします。
- 🌐 **[newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service)** - 本ライブラリに基づくHTTPサービス。RESTful APIインターフェースを提供。⚠️ (開発中)

## パフォーマンス最適化の提案

### 1. 適切な精度モードの選択

```rust
// リアルタイム処理シーン
let config = OcrEngineConfig::fast();
```

### 2. GPU アクセラレーションの使用

```rust
// macOS/iOS
let config = OcrEngineConfig::gpu();  // Metalを使用

// その他のプラットフォーム
let config = OcrEngineConfig::new().with_backend(Backend::OpenCL);
```

### 3. バッチ処理

```rust
// 複数のテキスト行をバッチ認識（1つずつ認識するよりはるかに高速）
let results = rec_model.recognize_batch(&images)?;
```

## 貢献

貢献は大歓迎です！IssueやPull Requestをお気軽に送信してください。

## ライセンス

本プロジェクトはApache License, Version 2.0の下でライセンスされています。詳細は[LICENSE](LICENSE)ファイルを参照してください。

## 謝辞

- [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) - 元のOCRモデルと研究を提供。
- [MNN](https://github.com/alibaba/MNN) - 高効率なニューラルネットワーク推論フレームワークを提供。