// 无条件 GUI 子系统：debug 构建也不弹终端。日志与 panic 全走文件
//（safe_eprint → ~/.miaocr/run.log，panic hook → panic.log），
// 开发时 tail -f ~/.miaocr/run.log 即可看实时输出。
#![windows_subsystem = "windows"]

//! 喵OCR / 喵Agent 的 GPUI 前端入口（gpui-kit 0.9）。
//!
//! 阻塞型工作（截屏、OCR 推理、同步 HTTP）全部跑在 [`ocr_worker`] 的常驻线程里，
//! UI 只负责渲染与调度，避免 GPUI 前台执行器被卡住。

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle};
use gpui_kit::component::group_box::GroupBox;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::TitleBar;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::*;
use smol::Timer;
use std::sync::Arc;
use vision::{BackendType, InstallState};

mod agent_bridge;
mod mac_perm;
mod ocr_worker;
#[cfg(target_os = "windows")]
mod region_picker;
mod win_top;
use agent_bridge::{data_dir, AppSettings};
use ocr_worker::{spawn_ocr_worker, BaiduCfg, OcrJob};

const ENGINE_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::WindowsNative, "Windows 原生 (系统自带)"),
    (BackendType::MacNative, "macOS 原生 (系统自带)"),
    (BackendType::Tesseract, "Tesseract (传统多语言)"),
    (BackendType::OcrRs, "PP-OCRv5 (MNN 内置, 极速)"),
    (BackendType::OarOcr, "PP-OCRv6 (ONNX 内置, 高精)"),
    (BackendType::PaddleOcr, "PaddleOCR-json (外置, 高精)"),
    (BackendType::RapidOcr, "RapidOCR-json (外置, 稳定)"),
    (BackendType::BaiduAiStudio, "百度 AI Studio (云端)"),
];

#[cfg(not(target_os = "windows"))]
/// 选区覆盖窗口与主窗口共享的状态。
struct SelectShared {
    image: Option<Arc<RenderImage>>,
    screen_size: (u32, u32),
    screen_origin: (i32, i32),
    drag_start: Option<Point<Pixels>>,
    drag_end: Option<Point<Pixels>>,
    /// 当前鼠标位置（画十字参考线用）。
    hover: Option<Point<Pixels>>,
    done: Option<(i32, i32, u32, u32)>,
    /// 用户按 Esc / 右键取消。
    cancelled: bool,
}

#[cfg(not(target_os = "windows"))]
static SELECT: std::sync::OnceLock<std::sync::Mutex<SelectShared>> = std::sync::OnceLock::new();

#[cfg(not(target_os = "windows"))]
fn select_shared() -> &'static std::sync::Mutex<SelectShared> {
    SELECT.get_or_init(|| {
        std::sync::Mutex::new(SelectShared {
            image: None,
            screen_size: (0, 0),
            screen_origin: (0, 0),
            drag_start: None,
            drag_end: None,
            hover: None,
            done: None,
            cancelled: false,
        })
    })
}

#[cfg(not(target_os = "windows"))]
/// 选区遮罩：选区内保持原图亮度，选区外压暗。
fn mask_panel(x: f32, y: f32, w: f32, h: f32) -> Div {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w.max(0.0)))
        .h(px(h.max(0.0)))
        .bg(rgba(0x000000a0))
}

#[cfg(not(target_os = "windows"))]
/// 四个角上的方形手柄。
fn corner_handles(x: f32, y: f32, w: f32, h: f32) -> Vec<Div> {
    let s = 3.0f32;
    vec![(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
        .into_iter()
        .map(|(cx, cy)| {
            div()
                .absolute()
                .left(px(cx - s))
                .top(px(cy - s))
                .w(px(s * 2.0))
                .h(px(s * 2.0))
                .bg(rgb(0xffffff))
                .border_1()
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
/// 1px 十字参考线。
fn crosshair(hover: Point<Pixels>, vw: f32, vh: f32) -> [Div; 2] {
    let hx: f32 = hover.x.into();
    let hy: f32 = hover.y.into();
    [
        div()
            .absolute()
            .left_0()
            .top(px(hy))
            .w(px(vw))
            .h(px(1.0))
            .bg(rgba(0xffffff40)),
        div()
            .absolute()
            .top_0()
            .left(px(hx))
            .w(px(1.0))
            .h(px(vh))
            .bg(rgba(0xffffff40)),
    ]
}

#[cfg(not(target_os = "windows"))]
/// 顶部居中的操作提示胶囊。
fn hint_pill(text: &str) -> Div {
    div()
        .absolute()
        .top(px(18.0))
        .left_0()
        .w_full()
        .flex()
        .justify_center()
        .child(
            div()
                .px_3()
                .py_1()
                .rounded_full()
                .bg(rgba(0x000000aa))
                .text_sm()
                .text_color(rgb(0xffffff))
                .child(text.to_string()),
        )
}

#[cfg(not(target_os = "windows"))]
/// 选区覆盖窗口的视图。
struct SelectView {
    /// 键盘焦点：覆盖窗口没有可聚焦元素时 Esc 收不到，必须自己持有一个。
    focus: FocusHandle,
}

#[cfg(not(target_os = "windows"))]
impl Render for SelectView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (shot, drag_start, drag_end, hover, screen_size) = {
            let g = select_shared().lock().unwrap();
            (
                g.image.clone(),
                g.drag_start,
                g.drag_end,
                g.hover,
                g.screen_size,
            )
        };

        let viewport = window.viewport_size();
        let vw: f32 = viewport.width.into();
        let vh: f32 = viewport.height.into();
        let (sw, sh) = screen_size;

        // 视口坐标 -> 屏幕物理像素
        let to_screen = |p: Point<Pixels>| -> (i32, i32) {
            let x: f32 = p.x.into();
            let y: f32 = p.y.into();
            (
                (x * sw as f32 / vw.max(1.0)) as i32,
                (y * sh as f32 / vh.max(1.0)) as i32,
            )
        };

        let mut el = div().size_full().bg(rgb(0x000000)).track_focus(&self.focus);
        if let Some(image) = shot {
            el = el.child(img(image).size_full().object_fit(ObjectFit::Fill));
        }

        // 当前选区（视口逻辑坐标）
        let sel = match (drag_start, drag_end) {
            (Some(s), Some(e)) => {
                let sx: f32 = s.x.into();
                let sy: f32 = s.y.into();
                let ex: f32 = e.x.into();
                let ey: f32 = e.y.into();
                Some((sx.min(ex), sy.min(ey), (ex - sx).abs(), (ey - sy).abs()))
            }
            _ => None,
        };

        // ── 遮罩：选区内不盖，选区外压暗 ──
        match sel {
            Some((x, y, w, h)) => {
                el = el
                    .child(mask_panel(0.0, 0.0, vw, y))
                    .child(mask_panel(0.0, y + h, vw, vh - (y + h)))
                    .child(mask_panel(0.0, y, x, h))
                    .child(mask_panel(x + w, y, vw - (x + w), h));
            }
            None => {
                el = el.child(mask_panel(0.0, 0.0, vw, vh));
            }
        }

        // ── 十字参考线（拖拽时淡出，避免干扰）──
        if let Some(hover) = hover {
            if drag_start.is_none() {
                for line in crosshair(hover, vw, vh) {
                    el = el.child(line);
                }
            }
        }

        // ── 选区框 + 手柄 + 尺寸徽标 ──
        if let Some((x, y, w, h)) = sel {
            // 换算成物理像素，徽标显示的是用户真正框到的屏幕尺寸
            let (px1, py1) = to_screen(point(px(x), px(y)));
            let (px2, py2) = to_screen(point(px(x + w), px(y + h)));
            let phys_w = (px2 - px1).unsigned_abs();
            let phys_h = (py2 - py1).unsigned_abs();
            let too_small = phys_w <= 10 || phys_h <= 10;
            let accent = if too_small { rgb(0xff5c5c) } else { rgb(0x2b8cff) };

            el = el.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(w))
                    .h(px(h))
                    .border_2()
                    .border_color(accent),
            );

            for handle in corner_handles(x, y, w, h) {
                el = el.child(handle.border_color(accent));
            }

            // 徽标贴在选区上方，顶部空间不够时落到下方
            let label = if too_small {
                format!("{phys_w} × {phys_h}  太小")
            } else {
                format!("{phys_w} × {phys_h}")
            };
            let badge_top = if y > 34.0 { y - 28.0 } else { y + h + 8.0 };
            el = el.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(badge_top))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(if too_small { rgb(0x7f1d1d) } else { rgb(0x1d4ed8) })
                    .text_xs()
                    .text_color(rgb(0xffffff))
                    .child(label),
            );
        }

        el = el.child(hint_pill(if drag_start.is_some() {
            "松开完成选区"
        } else {
            "按住左键拖拽框选 · 右键 / Esc 取消"
        }));

        // ── 交互 ──
        // 延迟关窗：不能在鼠标/键盘事件派发中直接 remove_window，
        // 否则 Win32 DestroyWindow 会在输入派发期间阻塞前台队列（未响应）。
        let cancel_overlay = move |window: &mut Window, cx: &mut Context<SelectView>| {
            select_shared().lock().unwrap().cancelled = true;
            window
                .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                    Timer::after(std::time::Duration::from_millis(30)).await;
                    cx.update(|window, _cx| {
                        window.remove_window();
                    })
                    .ok();
                })
                .detach();
        };

        el.on_mouse_down(
            MouseButton::Left,
            cx.listener(|_this, ev: &MouseDownEvent, _w, cx| {
                let mut g = select_shared().lock().unwrap();
                g.drag_start = Some(ev.position);
                g.drag_end = None;
                g.hover = Some(ev.position);
                drop(g);
                cx.notify();
            }),
        )
        .on_mouse_move(cx.listener(|_this, ev: &MouseMoveEvent, _w, cx| {
            let mut g = select_shared().lock().unwrap();
            g.hover = Some(ev.position);
            if g.drag_start.is_some() {
                g.drag_end = Some(ev.position);
            }
            drop(g);
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |_this, _ev: &MouseDownEvent, window, cx| {
                cancel_overlay(window, cx);
            }),
        )
        .on_key_down(cx.listener(move |_this, ev: &KeyDownEvent, window, cx| {
            if ev.keystroke.key == "escape" {
                cancel_overlay(window, cx);
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|_this, ev: &MouseUpEvent, window, cx| {
                let result = {
                    let mut g = select_shared().lock().unwrap();
                    if let Some(start) = g.drag_start.take() {
                        let viewport = window.viewport_size();
                        let (sw, sh) = g.screen_size;
                        let vw: f32 = viewport.width.into();
                        let vh: f32 = viewport.height.into();
                        let to_screen = |p: Point<Pixels>| -> (i32, i32) {
                            let px: f32 = p.x.into();
                            let py: f32 = p.y.into();
                            (
                                (px * sw as f32 / vw.max(1.0)) as i32,
                                (py * sh as f32 / vh.max(1.0)) as i32,
                            )
                        };
                        let (x1, y1) = to_screen(start);
                        let (x2, y2) = to_screen(ev.position);
                        let (ox, oy) = g.screen_origin;
                        let (sx, sy) = (x1.min(x2) + ox, y1.min(y2) + oy);
                        let (w, h) = ((x2 - x1).unsigned_abs(), (y2 - y1).unsigned_abs());
                        if w > 10 && h > 10 {
                            g.done = Some((sx, sy, w, h));
                            Some(())
                        } else {
                            // 太小：退回未选区状态，让用户重来
                            g.drag_end = None;
                            None
                        }
                    } else {
                        None
                    }
                };
                if result.is_some() {
                    // 不能在鼠标事件派发中直接销毁窗口（前台队列停摆 = 未响应），
                    // 也不能跨窗口 update 主窗口实体（输入回调里同步重入渲染同样卡死）；
                    // 主窗口由 start_select 的等待任务轮询 done 后自行继续。
                    tlog("mouseup: 计划延迟关闭覆盖窗口");
                    window
                        .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                            Timer::after(std::time::Duration::from_millis(30)).await;
                            cx.update(|window, _cx| {
                                window.remove_window();
                            })
                            .ok();
                            tlog("mouseup: 延迟 remove_window 完成");
                        })
                        .detach();
                } else {
                    tlog("mouseup: 选区过小, 已重置");
                    cx.notify();
                }
            }),
        )
    }
}

#[cfg(not(target_os = "windows"))]
/// 写入选区共享状态，并在「截屏所在的那块显示器」上打开全屏覆盖窗口。
///
/// 注意：不要用 `app.displays()` 反查显示器——GPUI 的 display 坐标与 Win32 的
/// 多显示器原点不一定同源，实测会开错屏。截屏时 `screenshots::Screen::from_point`
/// 已经给出了正确的屏幕原点与尺寸，直接用它构造 `Fullscreen(bounds)` 最稳。
fn show_overlay(cx: &mut AsyncApp, captured: ocr_worker::CaptureResult, _cursor: (i32, i32)) {
    let image = captured.render;
    let (sw, sh) = captured.screen_size;
    let (ox, oy) = captured.origin;
    let _ = cx.update(|_app| {
        let mut g = select_shared().lock().unwrap();
        g.image = Some(image);
        g.screen_size = captured.screen_size;
        g.screen_origin = captured.origin;
        g.drag_start = None;
        g.drag_end = None;
        g.hover = None;
        g.done = None;
        g.cancelled = false;
    });
    tlog("show_overlay: 共享状态已写入");
    let bounds = Bounds {
        origin: point(px(ox as f32), px(oy as f32)),
        size: size(px(sw as f32), px(sh as f32)),
    };
    tlog(&format!("show_overlay: 打开覆盖窗口于 {bounds:?}"));
    let opened = cx.update(|app| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Fullscreen(bounds)),
                titlebar: None,
                focus: true,
                ..Default::default()
            },
            app,
            |window, cx| {
                let view = cx.new(|cx| SelectView {
                    focus: cx.focus_handle(),
                });
                // 让覆盖层自己拿到键盘焦点，否则 Esc 无处可投递
                let handle = view.read(cx).focus.clone();
                handle.focus(window, cx);
                view
            },
        )
    });
    match &opened {
        Ok(_) => tlog("show_overlay: 覆盖窗口已打开"),
        Err(e) => tlog(&format!("show_overlay: 覆盖窗口打开失败: {e:?}")),
    }
}

/// 平台默认引擎：macOS 用系统原生，其余回落到 Windows 原生。
fn default_backend() -> BackendType {
    if cfg!(target_os = "macos") {
        BackendType::MacNative
    } else {
        BackendType::WindowsNative
    }
}

/// 从 settings.json 的 `selected_backend`（BackendType::log_name）还原引擎。
fn backend_from_name(name: &str) -> BackendType {
    match name {
        "MacNative" => BackendType::MacNative,
        "Tesseract" => BackendType::Tesseract,
        "PaddleOcr" => BackendType::PaddleOcr,
        "RapidOcr" => BackendType::RapidOcr,
        "BaiduAiStudio" => BackendType::BaiduAiStudio,
        "OcrRs" => BackendType::OcrRs,
        "OarOcr" => BackendType::OarOcr,
        _ => default_backend(),
    }
}

struct AppState {
    /// 常驻 OCR 工作线程的任务投递端。
    jobs: std::sync::mpsc::Sender<OcrJob>,
    /// 实时 OCR 文本。
    text: String,
    busy: bool,
    /// 选区进行中（防止重复点击）。
    selecting: bool,
    /// 临时状态提示（选区时显示在底栏）。
    status_hint: String,
    backend: BackendType,
    settings: AppSettings,
    /// 设置页里的可编辑输入框。
    input_baidu_token: Entity<InputState>,
    input_baidu_model: Entity<InputState>,
    /// 保存/重载后的提示。
    settings_hint: String,
    show_settings: bool,
    /// 当前选区。
    region: Option<(i32, i32, u32, u32)>,
    /// 覆盖层刚选完、待 Render 消费并启动轮询的区域。
    pending_region: Option<(i32, i32, u32, u32)>,
    /// 轮询代号：重新选区时 +1，旧轮询任务检测到代号变化即退出。
    poll_gen: u64,
    paused: bool,
    last_ms: Option<u128>,
    interval_ms: u64,
    topmost: bool,
    tess_state: InstallState,
    paddle_state: InstallState,
    rapid_state: InstallState,
    ocr_rs_state: InstallState,
    oar_ocr_state: InstallState,
}

impl AppState {
    /// 百度云端 OCR 参数（来自 settings.json）。
    fn baidu_cfg(&self) -> BaiduCfg {
        BaiduCfg {
            token: self.settings.baidu_token.clone(),
            model: self.settings.baidu_model().to_string(),
            use_orientation: self.settings.baidu_use_orientation,
            use_unwarping: self.settings.baidu_use_unwarping,
            use_chart: self.settings.baidu_use_chart,
        }
    }

    /// 点击「选区」：Windows 下由工作线程全局轮询鼠标，拖拽时只显示细边框。
    #[cfg(target_os = "windows")]
    fn start_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tlog("start_select: 入口(主线程, 原生选区)");
        if !mac_perm::ensure() {
            self.text = "缺少屏幕录制权限，请到 系统设置 → 隐私与安全性 → 屏幕录制 中授权后重试".into();
            cx.notify();
            return;
        }
        if self.selecting {
            return;
        }

        let entity = cx.entity();
        let jobs = self.jobs.clone();
        let was_paused = self.paused;
        self.selecting = true;
        self.paused = true;
        self.status_hint = "请在屏幕上拖拽选择区域（右键 / Esc 取消）".into();
        cx.notify();

        let (resp_tx, resp_rx) =
            smol::channel::bounded::<Result<Option<(i32, i32, u32, u32)>, String>>(1);
        window
            .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                let sent = jobs.send(OcrJob::PickRegion { resp: resp_tx }).is_ok();
                let result = if sent {
                    resp_rx
                        .recv()
                        .await
                        .unwrap_or_else(|_| Err("选区工作线程已退出".into()))
                } else {
                    Err("选区工作线程不可用".into())
                };
                cx.update_entity(&entity, |s: &mut AppState, cx: &mut Context<AppState>| {
                    s.selecting = false;
                    s.status_hint.clear();
                    match result {
                        Ok(Some(region)) => {
                            tlog(&format!("start_select: 收到选区 {region:?}"));
                            s.pending_region = Some(region);
                        }
                        Ok(None) => {
                            tlog("start_select: 用户取消了选区");
                            s.paused = was_paused;
                        }
                        Err(e) => {
                            tlog(&format!("start_select: 选区失败 {e}"));
                            s.text = format!("选区失败: {e}");
                            s.paused = was_paused;
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    /// 点击「选区」：非 Windows 平台沿用全屏覆盖层。
    #[cfg(not(target_os = "windows"))]
    fn start_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tlog("start_select: 入口(主线程)");
        if !mac_perm::ensure() {
            self.text = "缺少屏幕录制权限，请到 系统设置 → 隐私与安全性 → 屏幕录制 中授权后重试".into();
            cx.notify();
            return;
        }
        let cursor = cursor_pos();
        let entity = cx.entity();
        let jobs = self.jobs.clone();
        let (resp_tx, resp_rx) =
            smol::channel::bounded::<Result<ocr_worker::CaptureResult, String>>(1);
        cx.notify();
        window
            .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                tlog("start_select: 前台任务启动, 等待截屏...");
                let _ = jobs.send(OcrJob::CaptureForSelect {
                    cursor,
                    resp: resp_tx,
                });
                let captured = match resp_rx.recv().await {
                    Ok(Ok(c)) => Some(c),
                    Ok(Err(e)) => {
                        let msg = format!("截屏失败: {e}");
                        cx.update_entity(&entity, |s: &mut AppState, cx: &mut Context<AppState>| {
                            s.text = msg;
                            cx.notify();
                        });
                        None
                    }
                    Err(_) => {
                        cx.update_entity(&entity, |s: &mut AppState, cx: &mut Context<AppState>| {
                            s.text = "截屏失败: 工作线程已退出".into();
                            cx.notify();
                        });
                        None
                    }
                };
                let Some(captured) = captured else { return };
                tlog("start_select: 截屏应答到达");
                show_overlay(cx, captured, cursor);
                tlog("start_select: 覆盖窗口已打开, 等待选区完成");
                let mut waited_ms = 0u32;
                loop {
                    Timer::after(std::time::Duration::from_millis(50)).await;
                    waited_ms += 50;
                    let (done, cancelled) = {
                        let mut g = select_shared().lock().unwrap();
                        (g.done.take(), std::mem::take(&mut g.cancelled))
                    };
                    if cancelled {
                        tlog("start_select: 用户取消了选区");
                        break;
                    }
                    if let Some(region) = done {
                        tlog(&format!("start_select: 收到选区 {region:?}"));
                        let _ = cx.update_entity(&entity, |s: &mut AppState, cx| {
                            s.pending_region = Some(region);
                            cx.notify();
                        });
                        break;
                    }
                    if waited_ms > 300_000 {
                        tlog("start_select: 等待选区超时");
                        break;
                    }
                }
            })
            .detach();
    }

    /// 选区完成后启动常驻轮询：每轮按实际耗时自适应下一次间隔。
    fn begin_poll(
        &mut self,
        region: (i32, i32, u32, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tlog(&format!("begin_poll: 选区 {region:?}"));
        self.region = Some(region);
        self.paused = false;
        self.poll_gen = self.poll_gen.wrapping_add(1);
        let gen = self.poll_gen;
        let entity = cx.entity();
        let jobs = self.jobs.clone();
        cx.notify();
        window
            .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                // 等覆盖窗口完全关闭，避免把选区蒙层截进去
                Timer::after(std::time::Duration::from_millis(120)).await;
                loop {
                    let snap = cx.update_entity(&entity, |s: &mut AppState, _| {
                        (
                            s.poll_gen,
                            s.paused,
                            s.busy,
                            s.region,
                            s.backend,
                            s.baidu_cfg(),
                        )
                    });
                    let (cur_gen, paused, busy, region, backend, baidu) = snap;
                    if cur_gen != gen {
                        break;
                    }
                    let Some((x, y, w, h)) = region else { break };
                    if paused || busy {
                        Timer::after(std::time::Duration::from_millis(120)).await;
                        continue;
                    }
                    cx.update_entity(&entity, |s: &mut AppState, cx| {
                        s.busy = true;
                        cx.notify();
                    });
                    let (resp_tx, resp_rx) = smol::channel::bounded::<anyhow::Result<String>>(1);
                    let started = std::time::Instant::now();
                    if jobs
                        .send(OcrJob::Region {
                            x,
                            y,
                            w,
                            h,
                            backend,
                            baidu,
                            resp: resp_tx,
                        })
                        .is_err()
                    {
                        let _ = cx.update_entity(&entity, |s: &mut AppState, cx| {
                            s.busy = false;
                            s.text = "识别失败: OCR 工作线程不可用".into();
                            cx.notify();
                        });
                        break;
                    }
                    let result = resp_rx
                        .recv()
                        .await
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("OCR 工作线程已退出")));
                    let elapsed = started.elapsed().as_millis();
                    // 间隔跟随内容活动：变化 → 贴回下限（快速跟进连续变化）；
                    // 不变 → 每轮放宽 1.5 倍，上限 1260ms，静态内容下省 CPU。
                    // 下限与 OCR 耗时挂钩（慢引擎不会背靠背满速重识别）。
                    let next = cx.update_entity(&entity, |s: &mut AppState, cx| {
                        let new_text = match result {
                            Ok(t) => t,
                            Err(e) => format!("识别失败: {e}"),
                        };
                        let base = (elapsed as u64).clamp(120, 1200) + 60;
                        let changed = s.text != new_text;
                        s.text = new_text;
                        s.busy = false;
                        s.last_ms = Some(elapsed);
                        s.interval_ms = if changed {
                            base
                        } else {
                            (s.interval_ms * 3 / 2).clamp(base, 1260)
                        };
                        cx.notify();
                        s.interval_ms
                    });
                    Timer::after(std::time::Duration::from_millis(next)).await;
                }
            })
            .detach();
    }

    fn status_line(&self) -> String {
        if !self.status_hint.is_empty() {
            return self.status_hint.clone();
        }
        if self.region.is_none() {
            return "就绪".to_string();
        }
        if self.paused {
            return "已暂停".to_string();
        }
        // 轮询期间不再切换「识别中…」文案：busy 只用于防重入，
        // 状态栏保持稳定，仅每轮结束后更新「上次耗时」。
        let last = match self.last_ms {
            Some(ms) => format!("{ms} ms"),
            None => "—".to_string(),
        };
        format!("就绪 · 上次 {last} · 间隔 {} ms", self.interval_ms.max(1))
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        if self.region.is_none() {
            tlog("toggle_pause: 尚未选区, 忽略");
            return;
        }
        self.paused = !self.paused;
        tlog(&format!("toggle_pause: paused={}", self.paused));
        cx.notify();
    }

    fn clear_output(&mut self, cx: &mut Context<Self>) {
        self.text.clear();
        cx.notify();
    }

    fn copy_text(&mut self, cx: &mut Context<Self>) {
        if self.text.is_empty() {
            tlog("copy_text: 没有可复制内容");
            return;
        }
        tlog(&format!("copy_text: {} 字", self.text.chars().count()));
        cx.write_to_clipboard(ClipboardItem::new_string(self.text.clone()));
        cx.notify();
    }

    fn toggle_topmost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let want = !self.topmost;
        let ok = win_top::apply_topmost(window, want);
        tlog(&format!("toggle_topmost: want={want} ok={ok}"));
        if ok {
            self.topmost = want;
        }
        cx.notify();
    }

    fn state_of(&mut self, b: BackendType) -> &mut InstallState {
        match b {
            BackendType::Tesseract => &mut self.tess_state,
            BackendType::PaddleOcr => &mut self.paddle_state,
            BackendType::RapidOcr => &mut self.rapid_state,
            BackendType::OcrRs => &mut self.ocr_rs_state,
            BackendType::OarOcr => &mut self.oar_ocr_state,
            _ => &mut self.ocr_rs_state,
        }
    }

    fn current_state(&self) -> InstallState {
        match self.backend {
            BackendType::Tesseract => self.tess_state.clone(),
            BackendType::PaddleOcr => self.paddle_state.clone(),
            BackendType::RapidOcr => self.rapid_state.clone(),
            BackendType::OcrRs => self.ocr_rs_state.clone(),
            BackendType::OarOcr => self.oar_ocr_state.clone(),
            // 系统原生 / 云端引擎无需安装检测：直接视为可用，
            // 否则状态永远停在 Unchecked，标签会一直显示「检测…」。
            _ => InstallState::Available,
        }
    }

    fn engine_state_label(&self) -> String {
        match self.current_state() {
            InstallState::Unchecked => "检测…".to_string(),
            InstallState::Checking => "检测中…".to_string(),
            InstallState::Available => "已安装".to_string(),
            InstallState::NotInstalled => "未安装".to_string(),
            InstallState::Installing(m) => m,
            InstallState::Failed(_) => "失败".to_string(),
        }
    }

    /// 若当前引擎尚未检测，则异步检测并刷新状态。
    fn ensure_backend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let backend = self.backend;
        let needs_check = matches!(
            backend,
            BackendType::Tesseract
                | BackendType::PaddleOcr
                | BackendType::RapidOcr
                | BackendType::OcrRs
                | BackendType::OarOcr
        ) && matches!(*self.state_of(backend), InstallState::Unchecked);
        if !needs_check {
            return;
        }
        *self.state_of(backend) = InstallState::Checking;
        let entity = cx.entity();
        window
            .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                let available = cx
                    .background_executor()
                    .spawn(async move {
                        match backend {
                            BackendType::Tesseract => vision::detect_tesseract(),
                            BackendType::PaddleOcr => vision::detect_paddle(),
                            BackendType::RapidOcr => vision::detect_rapid(),
                            BackendType::OcrRs => vision::detect_ocr_rs(),
                            BackendType::OarOcr => vision::detect_oar_ocr(),
                            _ => true,
                        }
                    })
                    .await;
                cx.update_entity(&entity, |state: &mut AppState, cx: &mut Context<AppState>| {
                    let slot = state.state_of(backend);
                    *slot = if available {
                        InstallState::Available
                    } else {
                        InstallState::NotInstalled
                    };
                    cx.notify();
                });
            })
            .detach();
    }

    /// 触发当前引擎的一键安装（vision 安装器写 Arc<Mutex>，UI 轮询回写 Entity）。
    fn install_backend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let backend = self.backend;
        let state_arc = Arc::new(std::sync::Mutex::new(InstallState::Installing(
            "开始安装...".into(),
        )));
        match backend {
            BackendType::Tesseract => vision::start_tesseract_install(state_arc.clone()),
            BackendType::PaddleOcr => vision::start_paddle_install(state_arc.clone()),
            BackendType::RapidOcr => vision::start_rapid_install(state_arc.clone()),
            BackendType::OcrRs => vision::start_ocr_rs_install(state_arc.clone()),
            BackendType::OarOcr => vision::start_oar_ocr_install(state_arc.clone()),
            _ => return,
        }
        let entity = cx.entity();
        window
            .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                loop {
                    Timer::after(std::time::Duration::from_millis(300)).await;
                    let current = state_arc.lock().unwrap().clone();
                    let done = matches!(
                        current,
                        InstallState::Available | InstallState::Failed(_)
                    );
                    let snapshot = current;
                    cx.update_entity(
                        &entity,
                        |state: &mut AppState, cx: &mut Context<AppState>| {
                            *state.state_of(backend) = snapshot.clone();
                            cx.notify();
                        },
                    );
                    if done {
                        break;
                    }
                }
            })
            .detach();
    }

    /// 把输入框内容写回配置文件并应用到运行时。
    fn save_settings(&mut self, cx: &mut Context<Self>) {
        let baidu_token = self.input_baidu_token.read(cx).value().to_string();
        let baidu_model = self.input_baidu_model.read(cx).value().to_string();

        self.settings.baidu_token = baidu_token;
        self.settings.baidu_model = if baidu_model.trim().is_empty() {
            "PaddleOCR-VL-1.6".to_string()
        } else {
            baidu_model
        };
        self.settings.save();

        tlog(&format!(
            "save_settings: baidu_model={}",
            self.settings.baidu_model
        ));
        self.settings_hint = "已保存".to_string();
        cx.notify();
    }

    /// 从磁盘重载，并同步回输入框。
    fn reload_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings = AppSettings::load();
        let backend = self
            .settings
            .selected_backend
            .as_deref()
            .map(backend_from_name)
            .unwrap_or_else(default_backend);
        self.backend = backend;
        tools::set_current_backend(backend);
        let s = self.settings.clone();
        self.input_baidu_token
            .update(cx, |st, cx| st.set_value(s.baidu_token, window, cx));
        self.input_baidu_model
            .update(cx, |st, cx| st.set_value(s.baidu_model, window, cx));
        tlog("reload_config: 已从磁盘重载");
        self.settings_hint = "已重载".to_string();
        cx.notify();
    }

    /// 百度 OCR 三个开关之一被点击。
    fn set_baidu_flag(&mut self, which: u8, value: bool) {
        match which {
            0 => self.settings.baidu_use_orientation = value,
            1 => self.settings.baidu_use_unwarping = value,
            _ => self.settings.baidu_use_chart = value,
        }
        self.settings.save();
    }

    /// 统一标题栏：整条可拖拽（未消费的点击落到原生 caption 拖拽），
    /// children 里放交互元素依然能收到点击（GPUI 会先派发 NC 鼠标事件）。
    /// 右端由 TitleBar 自动接自绘窗口控件（最小化/最大化/关闭），
    /// 我们的图标按钮就在最小化图标前面。
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let e_pin = entity.clone();
        let e_settings = entity.clone();
        let e_back = entity.clone();
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let in_settings = self.show_settings;

        // 左：标题。设置页显示「设置」，主视图显示品牌 + 副标题。
        let left = if in_settings {
            div().text_sm().text_color(fg).child("设置")
        } else {
            div()
                .flex()
                .items_baseline()
                .gap_2()
                .child(div().text_sm().text_color(fg).child("喵OCR"))
                .child(div().text_xs().text_color(muted).child("屏幕智能助手"))
        };

        // 右：图标按钮，紧贴窗口控件（最小化前）。设置页换成返回箭头。
        // 注意：按钮外面包一层「防拖拽护罩」——Button 的 mouse_down 不阻断冒泡，
        // 若任由事件冒到 TitleBar 的 should_move 拖拽逻辑，点击时手只要动 1px
        // 就会被劫持成拖动窗口，click 永远走不完。
        let right = if in_settings {
            titlebar_clickable(
                Button::new("titlebar-back")
                    .ghost()
                    .compact()
                    .icon(IconName::ArrowLeft)
                    .tooltip("返回")
                    .on_click(move |_ev, _w, cx| {
                        e_back.update(cx, |s: &mut AppState, cx| {
                            s.show_settings = false;
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
        } else {
            let pin_icon = if self.topmost {
                IconName::PinOff
            } else {
                IconName::Pin
            };
            titlebar_clickable(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("titlebar-pin")
                            .ghost()
                            .compact()
                            .icon(pin_icon)
                            .tooltip(if self.topmost { "取消置顶" } else { "置顶" })
                            .on_click(move |_ev, window, cx| {
                                e_pin.update(
                                    cx,
                                    |s: &mut AppState, cx: &mut Context<AppState>| {
                                        s.toggle_topmost(window, cx);
                                    },
                                );
                            }),
                    )
                    .child(
                        Button::new("titlebar-settings")
                            .ghost()
                            .compact()
                            .icon(IconName::Settings)
                            .tooltip("设置")
                            .on_click(move |_ev, _w, cx| {
                                e_settings.update(cx, |s: &mut AppState, cx| {
                                    s.show_settings = true;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .into_any_element()
        };

        TitleBar::new().child(left).child(right)
    }

    fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let muted = cx.theme().muted_foreground;
        let success = cx.theme().success;
        let entity = cx.entity();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(bg)
            // ── 百度配置 ──
            .child(
                GroupBox::new()
                    .title("百度 AI Studio（云端 OCR）")
                    .child(Self::field_row("Token", &self.input_baidu_token))
                    .child(Self::field_row("云端模型", &self.input_baidu_model))
                    .child(Self::check_row(
                        "方向矫正",
                        self.settings.baidu_use_orientation,
                        0,
                        cx,
                    ))
                    .child(Self::check_row(
                        "去畸变",
                        self.settings.baidu_use_unwarping,
                        1,
                        cx,
                    ))
                    .child(Self::check_row(
                        "图表识别",
                        self.settings.baidu_use_chart,
                        2,
                        cx,
                    )),
            )
            // ── 操作 ──
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("save-settings")
                            .primary()
                            .label("保存")
                            .on_click({
                                let e = entity.clone();
                                move |_ev, _w, cx| {
                                    e.update(cx, |s: &mut AppState, cx| {
                                        s.save_settings(cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("reload-settings")
                            .outline()
                            .label("重载")
                            .on_click({
                                let e = entity.clone();
                                move |_ev, window, cx| {
                                    e.update(cx, |s: &mut AppState, cx| {
                                        s.reload_config(window, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(success)
                            .child(self.settings_hint.clone()),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("百度 Token 存于 settings.json；保存后立即生效，也可点「重载」从磁盘读回。"),
            )
    }

    /// 一行「标签 + 输入框」。
    fn field_row(label: &'static str, input: &Entity<InputState>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .py_1()
            .child(div().text_xs().child(label))
            .child(div().w_full().child(Input::new(input)))
            .into_any_element()
    }

    fn check_row(label: &'static str, on: bool, which: u8, cx: &mut Context<Self>) -> AnyElement {
        Checkbox::new(("baidu-flag", which as usize))
            .label(label)
            .checked(on)
            .on_click(cx.listener(move |this, checked: &bool, _w, cx| {
                this.set_baidu_flag(which, *checked);
                cx.notify();
            }))
            .into_any_element()
    }

    /// 引擎安装状态的彩色标签。
    fn engine_tag(&self) -> Tag {
        let tag = match self.current_state() {
            InstallState::Available => Tag::success(),
            InstallState::NotInstalled => Tag::warning(),
            InstallState::Checking | InstallState::Installing(_) => Tag::info(),
            InstallState::Failed(_) => Tag::danger(),
            InstallState::Unchecked => Tag::secondary(),
        };
        tag.child(self.engine_state_label())
    }

    fn render_float(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let card = cx.theme().group_box;
        let backend_label = self.backend.display_name().to_string();

        let entity = cx.entity();
        let e_select = entity.clone();
        let e_clear = entity.clone();
        let e_pause = entity.clone();
        let e_copy = entity.clone();
        let e_install = entity.clone();

        let engine_tag = self.engine_tag();
        let show_install = self.current_state() == InstallState::NotInstalled;
        let has_text = !self.text.is_empty();
        let text = self.text.clone();
        let status = self.status_line();
        let paused = self.paused;
        let current = self.backend;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(bg)
            // ── 引擎 ──
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("engine-picker")
                            .outline()
                            .icon(IconName::ScanText)
                            .label(format!("{backend_label}  ▾"))
                            // 下拉菜单走 Popover 浮层（默认 TopLeft 锚 =
                            // 在按钮下方展开、左对齐；anchor 命名的是浮层
                            // 自己的角，传 BottomLeft 会向上展开顶出窗口）：
                            // 不占布局空间，点击外部自动收起，选中项带勾选态。
                            .dropdown_menu(move |menu, _window, _cx| {
                                let mut menu = menu;
                                for (b, label) in ENGINE_OPTIONS {
                                    let b = *b;
                                    let e = entity.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(*label)
                                            .checked(b == current)
                                            .on_click(move |_ev, _w, cx| {
                                                e.update(
                                                    cx,
                                                    |s: &mut AppState, cx| {
                                                        s.backend = b;
                                                        tools::set_current_backend(b);
                                                        s.settings.selected_backend =
                                                            Some(b.log_name().to_string());
                                                        s.settings.save();
                                                        cx.notify();
                                                    },
                                                );
                                            }),
                                    );
                                }
                                menu
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(engine_tag)
                            .child(if show_install {
                                Button::new("install").primary().label("安装").on_click({
                                    let e = e_install;
                                    move |_ev, window, cx| {
                                        e.update(
                                            cx,
                                            |s: &mut AppState, cx: &mut Context<AppState>| {
                                                s.install_backend(window, cx);
                                            },
                                        );
                                    }
                                })
                                .into_any_element()
                            } else {
                                div().into_any_element()
                            }),
                    ),
            )
            // ── 操作 ──
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("select")
                            .primary()
                            .icon(IconName::ScanText)
                            .label("选区")
                            .flex_1()
                            .on_click({
                                let e = e_select;
                                move |_ev, window, cx| {
                                    e.update(
                                        cx,
                                        |s: &mut AppState, cx: &mut Context<AppState>| {
                                            s.start_select(window, cx);
                                        },
                                    );
                                }
                            }),
                    ),
            )
            // ── 内容 ──
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .p_3()
                    .bg(card)
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .child(if has_text {
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .text_sm()
                                    .text_color(fg)
                                    .id("ocr-output")
                                    .overflow_y_scrollbar()
                                    .child(text),
                            )
                            .into_any_element()
                    } else {
                        Empty::new()
                            .flex_1()
                            .header(
                                EmptyHeader::new()
                                    .media(EmptyMedia::new().child(Icon::new(IconName::ScanText)))
                                    .title(EmptyTitle::new().child("暂无识别内容"))
                                    .description(
                                        EmptyDescription::new()
                                            .child("点击「选区」框选屏幕，识别文字会显示在这里"),
                                    ),
                            )
                            .into_any_element()
                    }),
            )
            // ── 底栏 ──
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_xs().text_color(muted).child(status))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Button::new("pause")
                                    .ghost()
                                    .icon(if paused { IconName::Play } else { IconName::Pause })
                                    .label(if paused { "继续" } else { "暂停" })
                                    .on_click({
                                        let e = e_pause;
                                        move |_ev, _w, cx| {
                                            e.update(
                                                cx,
                                                |s: &mut AppState, cx: &mut Context<AppState>| {
                                                    s.toggle_pause(cx);
                                                },
                                            );
                                        }
                                    }),
                            )
                            .child(
                                Button::new("copy")
                                    .ghost()
                                    .icon(IconName::Copy)
                                    .label("复制")
                                    .on_click({
                                        let e = e_copy;
                                        move |_ev, _w, cx| {
                                            e.update(
                                                cx,
                                                |s: &mut AppState, cx: &mut Context<AppState>| {
                                                    s.copy_text(cx);
                                                },
                                            );
                                        }
                                    }),
                            )
                            .child(
                                Button::new("clear")
                                    .ghost()
                                    .icon(IconName::Eraser)
                                    .label("清空")
                                    .on_click({
                                        let e = e_clear;
                                        move |_ev, _w, cx| {
                                            e.update(
                                                cx,
                                                |s: &mut AppState, cx: &mut Context<AppState>| {
                                                    s.clear_output(cx);
                                                },
                                            );
                                        }
                                    }),
                            ),
                    ),
            )
    }
}

/// 标题栏内可点击元素的「防拖拽护罩」：在冒泡到 TitleBar 的拖拽逻辑
/// （should_move → start_window_move）之前吃掉鼠标按下事件，保证 click
/// 能走完。Button 自身不阻断冒泡，所以这层护罩必不可少。
fn titlebar_clickable(child: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .child(child)
}

impl Render for AppState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 覆盖层选完区后由 start_select 的等待任务写入，这里启动常驻轮询
        if let Some(region) = self.pending_region.take() {
            tlog("render: 选区就绪, 启动轮询");
            self.begin_poll(region, window, cx);
        }
        self.ensure_backend(window, cx);
        // 统一标题栏：主视图与设置视图共用，右端自动接窗口控件（最小化/最大化/关闭）。
        let content = if self.show_settings {
            self.render_settings(cx).into_any_element()
        } else {
            self.render_float(cx).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_title_bar(cx))
            .child(content)
    }
}

/// 安装 panic 钩子：panic 信息同时写文件与 stderr（文件更可靠）。
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {info}");
        let path = agent_bridge::data_dir().join("panic.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write;
            let _ = writeln!(f, "{msg}");
        }
    }));
}

/// 日志落盘路径：优先 ~/.miaocr/run.log；不可写时回退到系统临时目录并告警一次。
fn log_file_path() -> std::path::PathBuf {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let primary = data_dir().join("run.log");
        if std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&primary)
            .is_ok()
        {
            return primary;
        }
        let fallback = std::env::temp_dir().join("miaocr_run.log");
        eprintln!(
            "[log] {} 不可写（权限被拒绝），回退到 {}",
            primary.display(),
            fallback.display()
        );
        fallback
    })
    .clone()
}

/// 诊断日志：打印相对启动时刻的毫秒时间戳。
pub(crate) fn safe_eprint(msg: &str) {
    use std::io::Write;
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(msg.as_bytes());
    let _ = err.write_all(b"\n");
    // 同时落盘：stderr 管道会随启动会话断开，文件写入才能持续保存。
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path())
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn tlog(msg: &str) {
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t0 = T0.get_or_init(std::time::Instant::now);
    safe_eprint(&format!("[ui +{:?}ms] {msg}", t0.elapsed().as_millis()));
}

#[cfg(not(target_os = "windows"))]
fn cursor_pos() -> (i32, i32) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut pt = POINT::default();
        unsafe {
            if GetCursorPos(&mut pt).is_ok() {
                return (pt.x, pt.y);
            }
        }
    }
    (0, 0)
}

/// 平台默认字体：gpui-kit 自带资源不含中文字形，这里补一份系统字体。
fn load_fonts(cx: &mut App) {
    #[cfg(target_os = "windows")]
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ];
    #[cfg(target_os = "macos")]
    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
    ];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let candidates: [&str; 0] = [];

    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let fonts = vec![std::borrow::Cow::Owned(bytes)];
            if cx.text_system().add_fonts(fonts).is_ok() {
                tlog(&format!("load_fonts: 已加载 {path}"));
                break;
            }
        }
    }
}

fn main() {
    install_panic_hook();
    let probe = std::env::args().any(|a| a == "--probe");
    // 调试用：直接以设置页启动，方便在没有鼠标注入的情况下检查布局。
    let start_in_settings = std::env::args().any(|a| a == "--settings");
    tlog(&format!(
        "main: probe={probe} start_in_settings={start_in_settings}"
    ));
    // 注册完整图标资源：gpui-kit 默认只内嵌组件自用的 106 个图标，
    // 我们用到 Pin / Sparkles / Eraser 等，需要显式接管 asset source。
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx: &mut App| {
        let jobs = spawn_ocr_worker();
        cx.spawn(async move |_cx: &mut AsyncApp| {
            let t0 = std::time::Instant::now();
            loop {
                Timer::after(std::time::Duration::from_millis(300)).await;
                safe_eprint(&format!("[hb +{:?}ms] heartbeat", t0.elapsed().as_millis()));
            }
        })
        .detach();
        #[cfg(not(target_os = "windows"))]
        if probe {
            let jobs_probe = jobs.clone();
            cx.spawn(async move |cx: &mut AsyncApp| {
                tlog("[probe] 1 秒后开始截屏");
                Timer::after(std::time::Duration::from_secs(1)).await;
                let cursor = cursor_pos();
                let (resp_tx, resp_rx) =
                    smol::channel::bounded::<Result<ocr_worker::CaptureResult, String>>(1);
                let _ = jobs_probe.send(OcrJob::CaptureForSelect {
                    cursor,
                    resp: resp_tx,
                });
                match resp_rx.recv().await {
                    Ok(Ok(captured)) => {
                        tlog("[probe] 截屏完成, 打开覆盖窗口");
                        show_overlay(cx, captured, cursor);
                        tlog("[probe] 覆盖窗口已打开, 存活观察 60 秒...");
                        Timer::after(std::time::Duration::from_secs(60)).await;
                        tlog("[probe] 通过: 60 秒无崩溃");
                        std::process::exit(0);
                    }
                    _ => {
                        tlog("[probe] 截屏失败");
                        std::process::exit(1);
                    }
                }
            })
            .detach();
        }

        #[cfg(target_os = "windows")]
        if probe {
            cx.spawn(async move |cx: &mut AsyncApp| {
                tlog("[probe] 1 秒后启动原生选区");
                Timer::after(std::time::Duration::from_secs(1)).await;
                let result = cx
                    .background_executor()
                    .spawn(async { region_picker::pick_region() })
                    .await;
                tlog(&format!("[probe] 选区结束: {result:?}"));
                std::process::exit(if result.is_ok() { 0 } else { 1 });
            })
            .detach();
        }

        vision::init(
            data_dir(),
            Arc::new(|msg: &str| safe_eprint(&format!("[vision] {msg}"))),
            Arc::new(|| {}),
        );
        gpui_kit::init(cx);
        load_fonts(cx);
        let settings = AppSettings::load();
        let backend = settings
            .selected_backend
            .as_deref()
            .map(backend_from_name)
            .unwrap_or_else(default_backend);
        tools::set_current_backend(backend);
        gpui_kit::open_window(
            // TitleBar::window_options：隐藏原生标题栏、由组件自绘
            //（拖拽 / 双击 / 窗口控件全走 GPUI 的命中测试，行为原生）。
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(520.0), px(620.0)),
                    cx,
                ))),
                ..TitleBar::window_options()
            },
            cx,
            move |window, cx| {
                window.set_window_title("喵OCR");
                let input_baidu_token = cx.new(|cx| {
                    InputState::new(window, cx)
                        .masked(true)
                        .placeholder("百度 AI Studio Token")
                        .default_value(settings.baidu_token.clone())
                });
                let input_baidu_model = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("PaddleOCR-VL-1.6")
                        .default_value(settings.baidu_model.clone())
                });
                cx.new(|_| AppState {
                    jobs: jobs.clone(),
                    text: String::new(),
                    busy: false,
                    selecting: false,
                    status_hint: String::new(),
                    backend,
                    settings,
                    input_baidu_token,
                    input_baidu_model,
                    settings_hint: String::new(),
                    show_settings: start_in_settings,
                    region: None,
                    pending_region: None,
                    poll_gen: 0,
                    paused: false,
                    last_ms: None,
                    interval_ms: 200,
                    topmost: false,
                    tess_state: InstallState::Unchecked,
                    paddle_state: InstallState::Unchecked,
                    rapid_state: InstallState::Unchecked,
                    ocr_rs_state: InstallState::Unchecked,
                    oar_ocr_state: InstallState::Unchecked,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
