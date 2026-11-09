//! 常驻 OCR 工作线程。
//!
//! 根因修复：此前 run_ocr / ask_ai / start_select 把阻塞型任务
//! （DXGI 截屏、MNN/ONNX 推理、WinRT RecognizeAsync().get() 阻塞等待、
//! 同步 HTTP）直接跑在 GPUI 前台执行器（主线程）上，导致选区后整个
//! UI 卡死；其中 WinRT 的 .get() 在 STA 主线程上等待是典型死锁。
//!
//! 设计：单条常驻工作线程串行处理所有阻塞任务。
//!  - vision 的引擎缓存是 thread_local!（MNN FFI 非线程安全），
//!    固定单线程可让引擎常驻热加载，切换后端时 clear_engines_except
//!    也在同一线程生效。
//!  - 任务队列用 std::sync::mpsc（线程自然阻塞等待），
//!    应答用 smol::channel（UI 侧异步 await，不占任何线程）。

use std::sync::mpsc::Sender;
#[cfg(not(target_os = "windows"))]
use std::sync::Arc;

use vision::BackendType;

use crate::agent_bridge::{run_agent, AgentConfig, AgentContext};

/// 百度 AI Studio 云端 OCR 参数（来自 ~/.miaocr/settings.json）。
#[derive(Clone, Default)]
pub struct BaiduCfg {
    pub token: String,
    pub model: String,
    pub use_orientation: bool,
    pub use_unwarping: bool,
    pub use_chart: bool,
}

/// 选区截屏的应答：整屏 BMP 字节 + 屏幕尺寸 + 屏幕原点（多显示器坐标）。
#[cfg(not(target_os = "windows"))]
pub struct CaptureResult {
    pub render: Arc<gpui_kit::RenderImage>,
    pub screen_size: (u32, u32),
    pub origin: (i32, i32),
}

/// 一条待处理任务，各变体自带一次性应答通道。
pub enum OcrJob {
    /// 截取鼠标所在屏幕整屏并编码为 BMP（非 Windows 的覆盖窗口背景）。
    #[cfg(not(target_os = "windows"))]
    CaptureForSelect {
        cursor: (i32, i32),
        resp: smol::channel::Sender<Result<CaptureResult, String>>,
    },
    /// Windows 原生选区：全局轮询鼠标，拖拽时只显示细边框。
    #[cfg(target_os = "windows")]
    PickRegion {
        resp: smol::channel::Sender<Result<Option<(i32, i32, u32, u32)>, String>>,
    },
    /// 对屏幕区域执行 OCR。
    Region {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        backend: BackendType,
        baidu: BaiduCfg,
        resp: smol::channel::Sender<anyhow::Result<String>>,
    },
    /// 让 Agent 回答用户问题（同步 HTTP + 多轮工具循环）。
    Agent {
        ctx: AgentContext,
        config: AgentConfig,
        /// 用户输入的问题；空串表示「请简要说明这段内容」。
        question: String,
        /// 会话历史（UI 持有，本轮结束后随回答一起返回）。
        history: Vec<llm::ChatMessage>,
        resp: smol::channel::Sender<Result<(String, Vec<llm::ChatMessage>), String>>,
    },
}

/// 启动常驻工作线程，返回任务投递端（可克隆）。
pub fn spawn_ocr_worker() -> Sender<OcrJob> {
    let (tx, rx) = std::sync::mpsc::channel::<OcrJob>();
    std::thread::Builder::new()
        .name("ocr-worker".into())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                match job {
                    #[cfg(not(target_os = "windows"))]
                    OcrJob::CaptureForSelect { cursor, resp } => {
                        let t0 = std::time::Instant::now();
                        let result = capture_for_select(cursor);
                        crate::safe_eprint(&format!("[wk] CaptureForSelect 完成, 耗时 {:?}", t0.elapsed()));
                        let _ = smol::block_on(resp.send(result));
                    }
                    #[cfg(target_os = "windows")]
                    OcrJob::PickRegion { resp } => {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            crate::region_picker::pick_region()
                        }))
                        .unwrap_or_else(|p| Err(format!("选区线程 panic: {p:?}")));
                        let _ = smol::block_on(resp.send(result));
                    }
                    OcrJob::Region {
                        x,
                        y,
                        w,
                        h,
                        backend,
                        baidu,
                        resp,
                    } => {
                        let t0 = std::time::Instant::now();
                        crate::safe_eprint(&format!("[wk] 收到 Region 任务 ({x},{y},{w}x{h}, {backend:?})"));
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            vision::ocr_region(
                                x,
                                y,
                                w,
                                h,
                                backend,
                                &baidu.token,
                                &baidu.model,
                                baidu.use_orientation,
                                baidu.use_unwarping,
                                baidu.use_chart,
                            )
                        }))
                        .unwrap_or_else(|p| Err(anyhow::anyhow!("OCR 引擎线程 panic: {p:?}")));
                        crate::safe_eprint(&format!("[wk] Region 完成, 耗时 {:?}", t0.elapsed()));
                        let _ = smol::block_on(resp.send(result));
                    }
                    OcrJob::Agent {
                        ctx,
                        config,
                        question,
                        history,
                        resp,
                    } => {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            run_agent(&ctx, &config, &question, history)
                        }))
                        .unwrap_or_else(|p| Err(format!("Agent 线程 panic: {p:?}")));
                        let _ = smol::block_on(resp.send(result));
                    }
                }
            }
        })
        .expect("启动 OCR 工作线程失败");
    tx
}

/// 抓取鼠标所在屏幕整屏, 并在 worker 线程完成 RGBA->BGRA 原地转换,
/// 直接产出可上传 GPU 的 RenderImage (覆盖窗口首帧零解码零转换)。
#[cfg(not(target_os = "windows"))]
fn capture_for_select(cursor: (i32, i32)) -> Result<CaptureResult, String> {
    let t0 = std::time::Instant::now();
    let (mx, my) = cursor;
    let screen =
        screenshots::Screen::from_point(mx, my).map_err(|e| format!("定位屏幕失败: {e}"))?;
    let origin = (screen.display_info.x, screen.display_info.y);
    let img = screen.capture().map_err(|e| format!("截屏失败: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let mut buf = image::RgbaImage::from_raw(w, h, img.into_raw())
        .ok_or_else(|| "截屏数据无效".to_string())?;
    // GPUI RenderImage 约定 BGRA 字节序, 原地交换 R/B。
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let frame = image::Frame::new(buf);
    let render = Arc::new(gpui_kit::RenderImage::new(smallvec::smallvec![frame]));
    crate::safe_eprint(&format!(
        "[wk] capture+swap+render_image {:?} ({}x{})",
        t0.elapsed(),
        w,
        h
    ));
    Ok(CaptureResult {
        render,
        screen_size: (w, h),
        origin,
    })
}
