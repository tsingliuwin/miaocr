# Rust PaddleOCR

[English](README.md) | [中文](../README.zh.md) | [日本語](README.ja.md) | [한국어](README.ko.md)

PaddleOCR 모델을 기반으로 한 경량 고효율 OCR(광학 문자 인식) Rust 라이브러리입니다. 이 라이브러리는 MNN 추론 프레임워크를 활용하여 고성능 텍스트 검출 및 인식 기능을 제공합니다.

**이 프로젝트는 순수 Rust 라이브러리이며**, OCR 핵심 기능 제공에 중점을 둡니다. 명령줄 도구나 다른 언어 바인딩이 필요한 경우 다음을 참조하세요:
- 🖥️ **명령줄 도구**: [newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)
- 🔌 **C API 바인딩**: [paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi) - 다른 프로그래밍 언어와의 통합을 용이하게 하는 C API 제공
- 🌐 **HTTP 서비스**: [newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service) ⚠️ (개발 중)

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

## ✨ 버전 2.0 새로운 기능

- 🎯 **새로운 계층형 API 설계**: 저수준 모델부터 고수준 파이프라인까지 완전한 계층형 API 제공
- 🔧 **유연한 모델 로딩**: 파일 경로 또는 메모리 바이트에서 모델 로딩 지원
- ⚙️ **설정 가능한 검출 매개변수**: 검출 임계값, 해상도, 정밀도 모드 등 사용자 정의 지원
- 🚀 **GPU 가속 지원**: Metal, OpenCL, Vulkan 등 다양한 GPU 백엔드 지원
- 📦 **배치 처리 최적화**: 처리량을 높이기 위한 텍스트 인식 배치 처리 지원
- 🔌 **독립 엔진 모드**: 검출 엔진 또는 인식 엔진만 별도로 생성 가능

## 특징

### 핵심 기능
- **텍스트 검출**: 이미지 내 텍스트 영역을 정확하게 포착
- **텍스트 인식**: 검출된 영역의 텍스트 내용을 인식
- **엔드투엔드(End-to-End) 인식**: 한 번의 호출로 검출 및 인식 전체 프로세스 완료
- **계층형 API 아키텍처**: 엔드투엔드, 계층형 호출, 독립 모델의 세 가지 사용 방식 지원

### 모델 지원
- **다중 버전 모델 지원**: PP-OCRv4 및 PP-OCRv5 모델을 지원하여 유연하게 선택 가능
- **다국어 지원**: PP-OCRv5는 11개 이상의 전용 언어 모델을 제공하며 100개 이상의 언어를 커버
- **복잡한 시나리오 인식**: 손글씨, 세로쓰기 텍스트, 희귀 문자 인식 능력 강화
- **유연한 로딩 방식**: 파일 경로 또는 메모리 바이트로부터 모델 로딩 지원

### 성능 특성
- **고성능 추론**: MNN 추론 프레임워크 기반으로 속도가 빠르고 메모리 점유율이 낮음
- **GPU 가속**: Metal, OpenCL, Vulkan 등 다양한 GPU 백엔드 지원
- **배치 처리**: 텍스트 인식 배치 처리를 지원하여 처리량 향상

### 개발 경험
- **유연한 설정**: 검출 임계값, 해상도, 정밀도 모드 등의 매개변수 사용자 정의 가능
- **메모리 안전**: 자동 메모리 관리로 메모리 누수 방지
- **순수 Rust 구현**: 외부 런타임이 필요 없으며 크로스 플랫폼 호환
- **최소 의존성**: 가볍고 통합하기 쉬움

## 모델 버전

이 라이브러리는 세 가지 PaddleOCR 모델 버전을 지원합니다:

### PP-OCRv4
- **안정 버전**: 충분한 검증을 거쳐 호환성이 좋음
- **적용 시나리오**: 높은 정확도가 요구되는 일반 문서 인식
- **모델 파일**:
  - 검출 모델: `ch_PP-OCRv4_det_infer.mnn`
  - 인식 모델: `ch_PP-OCRv4_rec_infer.mnn`
  - 문자셋: `ppocr_keys_v4.txt`

### PP-OCRv5
- **최신 버전**: 차세대 문자 인식 솔루션
- **다국어 지원**: 기본 모델(`PP-OCRv5_mobile_rec.mnn`)은 중국어(간체/번체), 영어, 일본어, 중국어 병음 지원
- **전용 언어 모델**: 11개 이상의 언어 전용 모델을 제공하여 100개 이상의 언어에서 최상의 성능 제공
- **공유 검출 모델**: 모든 V5 언어 모델은 동일한 검출 모델(`PP-OCRv5_mobile_det.mnn`)을 사용
- **시나리오 인식 강화**:
  - 중문/영문 복잡한 손글씨 인식 능력 대폭 향상
  - 세로쓰기 텍스트 인식 최적화
  - 희귀 문자 인식 능력 강화
- **성능 향상**: PP-OCRv4 대비 엔드투엔드 성능 13% 향상
- **모델 파일** (기본 다국어):
  - 검출 모델: `PP-OCRv5_mobile_det.mnn` (모든 언어 공유)
  - 인식 모델: `PP-OCRv5_mobile_rec.mnn` (기본, 중/영/일 지원)
  - 문자셋: `ppocr_keys_v5.txt`
- **전용 언어 모델 파일** (선택 사항):
  - 인식 모델: `{lang}_PP-OCRv5_mobile_rec_infer.mnn`
  - 문자셋: `ppocr_keys_{lang}.txt`
  - 사용 가능한 언어 코드: `arabic`, `cyrillic`, `devanagari`, `el`, `en`, `eslav`, `korean`, `latin`, `ta`, `te`, `th`

#### PP-OCRv5 언어 모델 상세 지원 목록

| 모델명 | 지원 언어 |
|---------|-----------|
| **korean_PP-OCRv5_mobile_rec** | 한국어, 영어 |
| **latin_PP-OCRv5_mobile_rec** | 프랑스어, 독일어, 아프리칸스어, 이탈리아어, 스페인어, 보스니아어, 포르투갈어, 체코어, 웨일스어, 덴마크어, 에스토니아어, 아일랜드어, 크로아티아어, 우즈베크어, 헝가리어, 세르비아어(라틴), 인도네시아어, 오크어, 아이슬란드어, 리투아니아어, 마오리어, 말레이어, 네덜란드어, 노르웨이어, 폴란드어, 슬로바키아어, 슬로베니아어, 알바니아어, 스웨덴어, 스와힐리어, 타갈로그어, 튀르키예어, 라틴어, 아제르바이잔어, 쿠르드어, 라트비아어, 몰타어, 팔리어, 루마니아어, 베트남어, 핀란드어, 바스크어, 갈리시아어, 룩셈부르크어, 로만슈어, 카탈루냐어, 케추아어 |
| **eslav_PP-OCRv5_mobile_rec** | 러시아어, 벨라루스어, 우크라이나어, 영어 |
| **th_PP-OCRv5_mobile_rec** | 태국어, 영어 |
| **el_PP-OCRv5_mobile_rec** | 그리스어, 영어 |
| **en_PP-OCRv5_mobile_rec** | 영어 |
| **cyrillic_PP-OCRv5_mobile_rec** | 러시아어, 벨라루스어, 우크라이나어, 세르비아어(키릴), 불가리아어, 몽골어, 압하지야어, 아디게어, 카바르디아어, 아바르어, 다르건어, 인구시어, 체첸어, 라크어, 레즈기어, 타바사란어, 카자흐어, 키르기스어, 타지크어, 마케도니아어, 타타르어, 추바슈어, 바시키르어, 마리어, 몰도바어, 우드무르트어, 코미어, 오세트어, 부랴트어, 칼미크어, 투바어, 사하어, 카라칼파크어, 영어 |
| **arabic_PP-OCRv5_mobile_rec** | 아랍어, 페르시아어, 위구르어, 우르두어, 파슈토어, 쿠르드어, 신디어, 발루치어, 영어 |
| **devanagari_PP-OCRv5_mobile_rec** | 힌디어, 마라티어, 네팔어, 비하르어, 마이틸리어, 앙가어, 보즈푸리어, 마가이어, 산탈리어, 네와르어, 콘칸어, 산스크리트어, 하리얀비어, 영어 |
| **ta_PP-OCRv5_mobile_rec** | 타밀어, 영어 |
| **te_PP-OCRv5_mobile_rec** | 텔루구어, 영어 |

### PP-OCRv5 FP16
- **고효율 버전**: 정확도를 희생하지 않으면서 더 빠른 추론 속도와 더 낮은 메모리 사용량 제공
- **적용 시나리오**: 고성능과 낮은 메모리 사용량이 요구되는 환경
- **성능 향상**:
  - 추론 속도 약 9% 향상 (FP16 추론 가속을 지원하는 장치에서는 성능이 더 높음)
  - 메모리 사용량 약 8% 감소
  - 모델 크기 절반으로 감소
- **모델 파일**:
  - 검출 모델: `PP-OCRv5_mobile_det_fp16.mnn`
  - 인식 모델: `PP-OCRv5_mobile_rec_fp16.mnn`
  - 문자셋: `ppocr_keys_v5.txt`

### 모델 성능 비교

| 특성 | PP-OCRv4 | PP-OCRv5 | PP-OCRv5 FP16 |
|---|---|---|---|
| 언어 지원 | 중국어, 영어 | 다국어 (기본 중/영/일, 11+ 전용 모델) | 다국어 (기본 중/영/일, 11+ 전용 모델) |
| 문자 유형 | 중국어, 영어 | 간체/번체, 영어, 일본어, 병음 | 간체/번체, 영어, 일본어, 병음 |
| 손글씨 인식 | 기본 지원 | 대폭 강화 | 대폭 강화 |
| 세로쓰기 | 기본 지원 | 최적화 | 최적화 |
| 희귀 문자 | 제한적 지원 | 인식 강화 | 인식 강화 |
| 추론 속도 (FPS) | 1.1 | 1.2 | 1.2 |
| 메모리 (피크) | 422.22MB | 388.41MB | 388.41MB |
| 모델 크기 | 표준 | 표준 | 절반 |
| 권장 시나리오 | 일반 문서 | 복잡한 시나리오 및 다국어 | 고성능 및 다국어 |

## 적용 시나리오

요구 사항에 따라 적절한 API 레벨을 선택하세요:

### 시나리오 1: OCR 기능의 빠른 통합
**사용: 엔드투엔드 인식 (`OcrEngine`)**

적합한 경우:
- 빠른 프로토타입 개발
- 간단한 문서 인식 요구 사항
- 중간 처리 단계 불필요
- 최종 텍스트 결과만 필요

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
let results = engine.recognize(&image)?;
```

### 시나리오 2: 검출 후처리 커스터마이징 필요
**사용: 계층형 호출 (`OcrEngine`의 detect + recognize_batch)**

적합한 경우:
- 검출 결과 필터링 또는 선별 필요
- 텍스트 상자 위치 조정 필요
- 특정 순서로 텍스트 처리 필요
- 검출 박스 정렬 또는 그룹화 필요

```rust
let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
// 1. 검출
let mut boxes = engine.detect(&image)?;
// 2. 사용자 정의 처리 (예: 작은 박스 필터링)
boxes.retain(|b| b.rect.width() > 50);
// 3. 인식
let detections = engine.det_model().detect_and_crop(&image)?;
let results = engine.recognize_batch(&images)?;
```

### 시나리오 3: 검출 기능만 필요
**사용: `DetOnlyEngine`**

적합한 경우:
- 문서 레이아웃 분석
- 텍스트 영역 라벨링 도구
- 전처리 워크플로우 (텍스트 위치만 알면 되는 경우)
- 다른 인식 엔진과 함께 사용

```rust
let det_engine = OcrEngine::det_only("models/det_model.mnn", None)?;
let text_boxes = det_engine.detect(&image)?;
// 검출 박스를 사용하여 다른 처리 수행...
```

### 시나리오 4: 인식 기능만 필요
**사용: `RecOnlyEngine`**

적합한 경우:
- 텍스트 위치를 이미 알고 있으며 인식만 필요한 경우
- 미리 잘라낸 텍스트 라인 이미지 처리
- 손글씨 인식 (입력이 한 줄의 텍스트 이미지)
- 고정된 형식의 텍스트 배치 인식

```rust
let rec_engine = OcrEngine::rec_only(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;
let text = rec_engine.recognize_text(&text_line_image)?;
```

### 시나리오 5: 완전히 커스터마이징된 흐름
**사용: 독립 모델 (`DetModel` + `RecModel`)**

적합한 경우:
- 사용자 정의 전처리 워크플로우 필요
- 검출과 인식에 다른 설정 사용 필요
- 검출과 인식 사이에 복잡한 처리 로직 삽입 필요
- 성능 최적화 (예: 검출 결과 재사용)

```rust
let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
let rec_model = RecModel::from_file(
    "models/rec_model.mnn",
    "models/ppocr_keys.txt",
    None
)?;

// 완전히 커스터마이징된 처리 흐름...
```

### 시나리오 6: 임베디드 또는 암호화 배포
**사용: 바이트에서 모델 로딩**

적합한 경우:
- 임베디드 장치 (바이너리에 모델 컴파일 포함)
- 모델 암호화 필요
- 네트워크에서 동적으로 모델 다운로드
- 사용자 정의 모델 저장 형식

```rust
let det_bytes = include_bytes!("../models/det_model.mnn");
let rec_bytes = include_bytes!("../models/rec_model.mnn");
let charset_bytes = include_bytes!("../models/ppocr_keys.txt");

let engine = OcrEngine::from_bytes(det_bytes, rec_bytes, charset_bytes, None)?;
```

## 설치

`Cargo.toml`에 다음을 추가하세요:

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"
```

특정 브랜치나 태그를 지정할 수도 있습니다:

```toml
[dependencies.rust-paddle-ocr]
git = "https://github.com/zibo-chen/rust-paddle-ocr.git"
branch = "main"
```

### 전제 조건

이 라이브러리는 다음이 필요합니다:
- MNN 형식으로 변환된 사전 학습된 PaddleOCR 모델
- 텍스트 인식을 위한 문자셋 파일

### MNN 연결 방식

기본적으로 [MNN-Prebuilds](https://github.com/zibo-chen/MNN-Prebuilds) 릴리스에서 미리 빌드된 MNN 정적 라이브러리가 자동으로 다운로드됩니다. 빌드하기 위해 cmake 또는 C++ 컴파일러 도구 체인이 필요하지 않습니다.

미리 빌드된 다운로드를 지원하는 플랫폼:
- Linux x86_64 / aarch64
- Windows x86_64 / i686
- macOS (유니버설: x86_64 + arm64)
- iOS arm64 / arm64-sim
- Android arm64-v8a / armeabi-v7a

지원되지 않는 플랫폼의 경우 빌드 시스템이 자동으로 소스에서 MNN을 빌드하는 방식으로 폴백합니다.

#### 소스에서 강제 빌드

사용자 정의 MNN 빌드 옵션(예: GPU 가속)이 필요한 경우 소스에서 빌드를 강제할 수 있습니다:

```bash
cargo build --features build-mnn-from-source
```

#### 미리 빌드된 동적 라이브러리 사용

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-dynamic
```

#### 미리 빌드된 정적 라이브러리 사용

```bash
MNN_LIB_DIR=/path/to/mnn/lib MNN_INCLUDE_DIR=/path/to/mnn/include \
  cargo build --features mnn-static
```

#### 환경 변수

| 변수 | 필수 | 설명 |
|---|---|---|
| `MNN_LIB_DIR` | 예 (`mnn-dynamic` / `mnn-static`을 사용할 때) | 미리 빌드된 MNN 라이브러리가 포함된 디렉토리 (`libMNN.so` / `libMNN.dylib` / `libMNN.a`) |
| `MNN_INCLUDE_DIR` | 아니오 | MNN 헤더가 포함된 디렉토리. 설정되지 않은 경우 `MNN_SOURCE_DIR/include` 또는 `3rd_party/MNN/include`로 폴백합니다 |
| `MNN_SOURCE_DIR` | 아니오 | MNN 소스 트리 경로 (헤더 가져오기 또는 소스에서 빌드하는 데 사용) |

## API 아키텍처

이 라이브러리는 **계층형 추론 API**를 제공하여 시나리오에 따라 사용 방식을 유연하게 선택할 수 있습니다:

```text
┌─────────────────────────────────────────────────┐
│         OcrEngine (엔드투엔드 Pipeline)           │
│         한 번의 호출로 검출 및 인식 완료            │
├─────────────────────────────────────────────────┤
│  DetOnlyEngine  │  RecOnlyEngine   │  OcrEngine │
│   검출만 수행    │   인식만 수행     │  검출+인식  │
├─────────────────────────────────────────────────┤
│     DetModel          │        RecModel         │
│   텍스트 검출 모델      │      텍스트 인식 모델     │
├─────────────────────────────────────────────────┤
│            InferenceEngine (MNN)                │
│              저수준 추론 엔진                     │
└─────────────────────────────────────────────────┘
```

### 세 가지 사용 방식

#### 1. 엔드투엔드 인식 (추천) - 가장 간단함

`OcrEngine`을 사용하여 전체 OCR 프로세스를 수행합니다. 한 번의 호출로 검출과 인식을 완료합니다:

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // OCR 엔진 생성 (기본 설정 사용)
    let engine = OcrEngine::new(
        "models/PP-OCRv5_mobile_det.mnn",
        "models/PP-OCRv5_mobile_rec.mnn",
        "models/ppocr_keys_v5.txt",
        None,
    )?;
    
    // 이미지 로드
    let image = image::open("test.jpg")?;
    
    // 한 번의 호출로 검출 및 인식 수행
    let results = engine.recognize(&image)?;
    
    // 결과 출력
    for result in results {
        println!("텍스트: {}", result.text);
        println!("신뢰도: {:.2}%", result.confidence * 100.0);
        println!("위치: ({}, {})", result.bbox.rect.left(), result.bbox.rect.top());
    }
    
    Ok(())
}
```

#### 2. 계층형 호출 - 더 유연함

`OcrEngine`을 사용하지만 검출과 인식을 별도로 호출합니다. 중간에 사용자 정의 처리를 삽입해야 하는 경우에 적합합니다:

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = OcrEngine::new(det_path, rec_path, charset_path, None)?;
    let image = image::open("test.jpg")?;
    
    // 1. 먼저 검출만 수행
    let text_boxes = engine.detect(&image)?;
    println!("{} 개의 텍스트 영역이 검출되었습니다", text_boxes.len());
    
    // 여기서 사용자 정의 처리를 할 수 있습니다. 예:
    // - 불필요한 영역 필터링
    // - 검출 박스 위치 조정
    // - 위치별 정렬 등
    
    // 2. 검출 모델을 가져와 수동으로 크롭(Crop)
    let det_model = engine.det_model();
    let detections = det_model.detect_and_crop(&image)?;
    
    // 3. 크롭된 이미지를 배치 인식
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

#### 3. 독립 모델 호출 - 가장 유연함

검출 엔진과 인식 엔진을 별도로 생성하거나 단일 기능 엔진만 생성합니다:

```rust
use ocr_rs::{DetModel, RecModel, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 방식 A: 검출 및 인식 모델을 별도로 생성
    let det_model = DetModel::from_file("models/det_model.mnn", None)?;
    
    let rec_model = RecModel::from_file(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?.with_options(RecOptions::new().with_min_score(0.5));
    
    let image = image::open("test.jpg")?;
    
    // 검출 및 크롭
    let detections = det_model.detect_and_crop(&image)?;
    
    // 배치 인식
    let images: Vec<_> = detections.iter().map(|(img, _)| img.clone()).collect();
    let results = rec_model.recognize_batch(&images)?;
    
    // 결과 처리...
    
    // 방식 B: 검출 전용 엔진 생성
    let det_only = OcrEngine::det_only("models/det_model.mnn", None)?;
    let text_boxes = det_only.detect(&image)?;
    
    // 방식 C: 인식 전용 엔진 생성
    let rec_only = OcrEngine::rec_only(
        "models/rec_model.mnn",
        "models/ppocr_keys.txt",
        None
    )?;
    let text = rec_only.recognize_text(&cropped_image)?;
    
    Ok(())
}
```

## 사용 예제

### 기본 설정 옵션

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 고속 모드 설정 사용
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

### GPU 가속

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, Backend};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // GPU 가속 사용
    let config = OcrEngineConfig::new()
        .with_backend(Backend::Metal);    // macOS: Metal
        // .with_backend(Backend::OpenCL); // 크로스 플랫폼: OpenCL
        // .with_backend(Backend::Vulkan); // Windows/Linux: Vulkan
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### 검출 및 인식 매개변수 사용자 정의

```rust
use ocr_rs::{OcrEngine, OcrEngineConfig, DetOptions, RecOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 사용자 정의 설정
    let config = OcrEngineConfig::new()
        .with_threads(8)
        .with_det_options(
            DetOptions::new()
                .with_max_side_len(1920)     // 더 높은 검출 해상도
                .with_box_threshold(0.6)     // 더 엄격한 바운딩 박스 임계값
                .with_merge_boxes(true)      // 인접 텍스트 상자 병합
        )
        .with_rec_options(
            RecOptions::new()
                .with_min_score(0.5)         // 낮은 신뢰도 결과 필터링
                .with_batch_size(16)         // 배치 인식 크기
        );
    
    let engine = OcrEngine::new(det_path, rec_path, charset_path, Some(config))?;
    
    Ok(())
}
```

### 특정 언어 모델 사용

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 한국어 모델 사용
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

### 메모리 바이트에서 모델 로딩

임베디드 배포 또는 모델 암호화가 필요한 시나리오에 적합합니다:

```rust
use ocr_rs::OcrEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 파일(또는 다른 소스)에서 모델 바이트 데이터 읽기
    let det_bytes = std::fs::read("models/det_model.mnn")?;
    let rec_bytes = std::fs::read("models/rec_model.mnn")?;
    let charset_bytes = std::fs::read("models/ppocr_keys.txt")?;
    
    // 바이트 데이터에서 엔진 생성
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

### 편리한 함수

```rust
use ocr_rs::ocr_file;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 한 줄의 코드로 OCR 수행
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

더 완전한 예제는 [examples](../examples) 디렉토리를 참조하세요.

## 관련 프로젝트

- 🖥️ **[newbee-ocr-cli](https://github.com/zibo-chen/newbee-ocr-cli)** - 이 라이브러리를 기반으로 한 명령줄 도구로, 간단하고 사용하기 쉬운 OCR CLI 인터페이스를 제공합니다.
- 🔌 **[paddle-ocr-capi](https://github.com/zibo-chen/paddle-ocr-capi)** - C API 바인딩. 다른 프로그래밍 언어(Python, Node.js, Go 등)와의 통합을 용이하게 합니다.
- 🌐 **[newbee-ocr-service](https://github.com/zibo-chen/newbee-ocr-service)** - 이 라이브러리를 기반으로 한 HTTP 서비스로, RESTful API 인터페이스를 제공합니다. ⚠️ (개발 중)

## 성능 최적화 제안

### 1. 적절한 정밀도 모드 선택

```rust
// 실시간 처리 시나리오
let config = OcrEngineConfig::fast();
```

### 2. GPU 가속 사용

```rust
// macOS/iOS
let config = OcrEngineConfig::gpu();  // Metal 사용

// 기타 플랫폼
let config = OcrEngineConfig::new().with_backend(Backend::OpenCL);
```

### 3. 배치 처리

```rust
// 여러 텍스트 라인을 배치 인식 (하나씩 인식하는 것보다 훨씬 빠름)
let results = rec_model.recognize_batch(&images)?;
```

## 기여

기여는 언제나 환영합니다! Issue나 Pull Request를 자유롭게 제출해 주세요.

## 라이선스

이 프로젝트는 Apache License, Version 2.0에 따라 라이선스가 부여됩니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참조하세요.

## 감사의 말

- [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) - 원본 OCR 모델 및 연구 제공
- [MNN](https://github.com/alibaba/MNN) - 고효율 신경망 추론 프레임워크 제공