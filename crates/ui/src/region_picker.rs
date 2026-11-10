//! Windows 优雅选区：截图覆盖层 + 压暗遮罩 + 十字线 + 放大镜 + 尺寸徽标。
//!
//! 设计（Snipaste / 系统截图风格）：
//! - 进入时用 GDI 截一张覆盖整个虚拟桌面（多屏一次到位）的图，随后一个
//!   置顶覆盖窗口接管全桌面：未选中区域压暗到约 55%，选区内部保持原亮度
//!   （punch-through），拖拽时实时跟随；
//! - 覆盖层天然独占鼠标事件，底层应用不再收到拖拽 / 右键（旧实现「直接在
//!   桌面上选」会把底层文字误选中，右键还会弹出对方的菜单）；
//! - 鼠标交互全部走窗口消息；Esc 仍用 `GetAsyncKeyState` 每 10ms 轮询——
//!   窗口是 `WS_EX_NOACTIVATE`（键盘焦点留在原应用），而低级钩子回调一旦
//!   超时会被系统静默移除（我们踩过这个坑）；
//! - 松开后返回虚拟桌面物理坐标，直接交给 `vision::ocr_region`。
//!
//! 渲染全部 GDI：截图先做成两份 DIB（原图 bright + 压暗图 dim），每帧把
//! dim 铺进帧缓冲、把选区位置换成 bright，再叠加边框 / 手柄 / 徽标 / 十字线 /
//! 放大镜，脏区一次 BitBlt 呈现（WM_ERASEBKGND 直接跳过，无闪烁）。

#![cfg(target_os = "windows")]

use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLORONCOLOR, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DeleteDC, DeleteObject, DrawTextW, DT_CENTER, DT_SINGLELINE, DT_VCENTER, EndPaint, GdiFlush,
    GetDC, GetMonitorInfoW, GetStockObject, GetTextExtentPoint32W, InvalidateRect, LineTo,
    MonitorFromPoint, MoveToEx, NULL_BRUSH, NULL_PEN, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, Rectangle,
    ReleaseDC, RoundRect, SRCCOPY, SelectObject, SetBkMode, SetStretchBltMode, SetTextColor,
    StretchBlt, TRANSPARENT, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HBRUSH, HDC,
    HFONT, HGDIOBJ, HPEN, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, CREATESTRUCTW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetCursorPos, GetSystemMetrics, GetWindowLongPtrW, GWLP_USERDATA, HTCLIENT, HWND_TOPMOST,
    IDC_CROSS, KillTimer, LoadCursorW, MSG, RegisterClassW, SetCursor, SetTimer, SetWindowLongPtrW,
    SetWindowPos, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, TranslateMessage, WM_ERASEBKGND,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_RBUTTONDOWN, WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

// ── 视觉常量（COLORREF = 0x00BBGGRR）──
const ACCENT: COLORREF = COLORREF(0x00ff8c2b); // #2B8CFF 主蓝
const DANGER: COLORREF = COLORREF(0x005c5cff); // #FF5C5C 过小警示
const BADGE_BG: COLORREF = COLORREF(0x00d84e1d); // #1D4ED8 徽标底
const BADGE_BG_BAD: COLORREF = COLORREF(0x001d1d7f); // #7F1D1D
const PILL_BG: COLORREF = COLORREF(0x00202020); // 提示 / 放大镜标签底
const WHITE: COLORREF = COLORREF(0x00ffffff);
const CROSS: COLORREF = COLORREF(0x00c8c8c8); // 十字线
const GRID: COLORREF = COLORREF(0x005a5a5a); // 放大镜网格
const DARK_EDGE: COLORREF = COLORREF(0x00282828); // 放大镜外框

const BORDER: i32 = 2; // 选区边框宽
const HANDLE: i32 = 8; // 四角手柄边长
const MIN_SIZE: i32 = 10; // 小于此尺寸视为无效选区（原地重置）
const TIMEOUT_MS: u64 = 180_000; // 整体超时
const POLL_INTERVAL: Duration = Duration::from_millis(10); // Esc 轮询间隔

// 放大镜：15×15 像素放大 8 倍 = 120×120
const MAG_N: i32 = 15;
const MAG_Z: i32 = 8;
const MAG_BOX: i32 = MAG_N * MAG_Z;
const MAG_LABEL_H: i32 = 22;
const MAG_TOTAL_H: i32 = MAG_BOX + 4 + MAG_LABEL_H + 2;

const CLASS_NAME: &str = "MiaOcrRegionPicker";
const TIMER_ID: usize = 1;

/// 选区结果：虚拟桌面物理坐标 (x, y, w, h)。
pub type Region = (i32, i32, u32, u32);

/// 等待用户在屏幕上拖拽出一块区域。
///
/// 返回 `Ok(None)` 表示取消（右键 / Esc / 超时），`Ok(Some(rect))` 表示完成。
/// 单击（没有拖出足够大的区域）会原地重置，继续等待下一次拖拽。
pub fn pick_region() -> Result<Option<Region>, String> {
    crate::safe_eprint("[picker] start");
    unsafe {
        // 用户可能是按住左键点「选区」按钮进来的，先等这一下松开，
        // 否则会把按钮位置当成拖拽起点。
        while key_down(VK_LBUTTON) {
            std::thread::sleep(POLL_INTERVAL);
        }

        ensure_class()?;
        let (vx, vy, vw, vh) = virtual_screen();
        if vw <= 0 || vh <= 0 {
            return Err("虚拟桌面尺寸无效".into());
        }

        let mut outcome = Outcome::default();
        let overlay = Overlay::create(vx, vy, vw, vh, &mut outcome)?;
        let raw = Box::into_raw(overlay);

        let hwnd = match create_window(vx, vy, vw, vh, raw.cast()) {
            Ok(h) => h,
            Err(e) => {
                // 窗口没建成：接管回 Box，避免泄漏（WM_NCDESTROY 不会来）。
                drop(Box::from_raw(raw));
                return Err(e);
            }
        };
        crate::safe_eprint(&format!("[picker] overlay {vw}x{vh} at ({vx},{vy})"));

        // 置顶显示（不抢焦点）。SetWindowPos 一步完成显示 + TOPMOST。
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        // 10ms 定时器：驱动 Esc 轮询与整体超时（消息循环空闲时也保持心跳）。
        let _ = SetTimer(hwnd, TIMER_ID, POLL_INTERVAL.as_millis() as u32, None);

        // 标准消息循环：所有交互都由 wndproc 处理，结束后 outcome 被填好。
        let mut msg = MSG::default();
        loop {
            let r = GetMessageW(&mut msg, None, 0, 0).0;
            if r == 0 || r == -1 {
                // 正常情况下不会走到（没有人 PostQuitMessage），防御性退出。
                outcome.cancelled = true;
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if outcome.finished {
                break;
            }
        }

        let _ = KillTimer(hwnd, TIMER_ID);
        // WM_NCDESTROY 里释放 Overlay（GDI 资源随 Drop 回收）。
        let _ = DestroyWindow(hwnd);

        if outcome.cancelled {
            crate::safe_eprint("[picker] cancelled");
            return Ok(None);
        }
        if let Some(region) = outcome.region {
            crate::safe_eprint(&format!("[picker] done {region:?}"));
            return Ok(Some(region));
        }
        Ok(None)
    }
}

/// pick_region 与 wndproc 之间的出口（栈上局部，Overlay 持裸指针）。
#[derive(Default)]
struct Outcome {
    region: Option<Region>,
    cancelled: bool,
    finished: bool,
}

// ── GDI 资源 ──────────────────────────────────────────────────────────

/// 一块 32bpp 顶朝下的 DIB 及其私有 DC（尺寸由 Overlay 的 vw/vh 统一记账）。
struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    prev: HGDIOBJ,
    bits: *mut u8,
}

impl Dib {
    unsafe fn create(screen: HDC, w: i32, h: i32) -> Result<Self, String> {
        let dc = CreateCompatibleDC(screen);
        if dc.is_invalid() {
            return Err("CreateCompatibleDC 失败".into());
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // 负高 = 顶朝下，与屏幕像素顺序一致
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bmp = match CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(e) => {
                let _ = DeleteDC(dc);
                return Err(format!("CreateDIBSection 失败: {e}"));
            }
        };
        let prev = SelectObject(dc, bmp);
        Ok(Self {
            dc,
            bmp,
            prev,
            bits: bits.cast(),
        })
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.dc, self.prev);
            let _ = DeleteObject(self.bmp);
            let _ = DeleteDC(self.dc);
        }
    }
}

// ── 覆盖层 ────────────────────────────────────────────────────────────

/// 覆盖层全部状态。经 GWLP_USERDATA 挂在窗口上，随 WM_NCDESTROY 释放。
struct Overlay {
    // 虚拟桌面几何（窗口即整个虚拟桌面，client 坐标 = 物理 - 原点）
    vx: i32,
    vy: i32,
    vw: i32,
    vh: i32,
    // 截图双份 + 帧缓冲
    bright: Dib,
    dim: Dib,
    frame: Dib,
    // 字体 / 画笔 / 画刷（构造时建好，Drop 时释放）
    font_ui: HFONT,
    font_mono: HFONT,
    pen_accent: HPEN,
    pen_white: HPEN,
    pen_cross: HPEN,
    pen_grid: HPEN,
    pen_dark: HPEN,
    pen_red: HPEN,
    brush_accent: HBRUSH,
    brush_badge: HBRUSH,
    brush_badge_bad: HBRUSH,
    brush_pill: HBRUSH,
    // 交互状态（client 坐标）
    cursor: (i32, i32),
    anchor: Option<(i32, i32)>,
    // 提示胶囊（放在鼠标所在显示器顶部居中）
    hint_area: RECT,
    // 出口 & 期限
    outcome: *mut Outcome,
    deadline: Instant,
}

impl Overlay {
    unsafe fn create(
        vx: i32,
        vy: i32,
        vw: i32,
        vh: i32,
        outcome: &mut Outcome,
    ) -> Result<Box<Self>, String> {
        // 1) 截屏：整个虚拟桌面一次 BitBlt 进 bright。
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("获取屏幕 DC 失败".into());
        }
        let t0 = Instant::now();
        let bright = Dib::create(screen, vw, vh)?;
        let dim = Dib::create(screen, vw, vh)?;
        let frame = Dib::create(screen, vw, vh)?;
        let blt = BitBlt(bright.dc, 0, 0, vw, vh, screen, vx, vy, SRCCOPY);
        ReleaseDC(None, screen);
        blt.map_err(|e| format!("截屏失败: {e}"))?;
        // GDI 可能批量执行，读 bits 前冲一下，保证 dim 拷贝读到最新像素。
        let _ = GdiFlush();
        crate::safe_eprint(&format!("[picker] capture {:?} ({}x{})", t0.elapsed(), vw, vh));

        // 2) dim：把 bright 压暗到约 55%。
        let n = (vw as usize) * (vh as usize) * 4;
        let src = std::slice::from_raw_parts(bright.bits, n);
        let dst = std::slice::from_raw_parts_mut(dim.bits, n);
        for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            d[0] = dim_byte(s[0]);
            d[1] = dim_byte(s[1]);
            d[2] = dim_byte(s[2]);
            d[3] = 255;
        }

        let hint_area = hint_area(vx, vy, vw, vh);
        let o = Box::new(Self {
            vx,
            vy,
            vw,
            vh,
            bright,
            dim,
            frame,
            font_ui: ui_font(-15, 600),
            font_mono: mono_font(-13),
            pen_accent: CreatePen(PS_SOLID, BORDER, ACCENT),
            pen_white: CreatePen(PS_SOLID, 1, WHITE),
            pen_cross: CreatePen(PS_SOLID, 1, CROSS),
            pen_grid: CreatePen(PS_SOLID, 1, GRID),
            pen_dark: CreatePen(PS_SOLID, 2, DARK_EDGE),
            pen_red: CreatePen(PS_SOLID, 2, DANGER),
            brush_accent: CreateSolidBrush(ACCENT),
            brush_badge: CreateSolidBrush(BADGE_BG),
            brush_badge_bad: CreateSolidBrush(BADGE_BG_BAD),
            brush_pill: CreateSolidBrush(PILL_BG),
            cursor: initial_cursor(vx, vy),
            anchor: None,
            hint_area,
            outcome,
            deadline: Instant::now() + Duration::from_millis(TIMEOUT_MS),
        });
        Ok(o)
    }

    /// 当前选区（client 坐标，w/h 恒 >= 0）。
    fn sel_rect(&self) -> Option<RECT> {
        let a = self.anchor?;
        let c = self.cursor;
        Some(RECT {
            left: a.0.min(c.0),
            top: a.1.min(c.1),
            right: a.0.max(c.0),
            bottom: a.1.max(c.1),
        })
    }

    fn too_small(r: &RECT) -> bool {
        (r.right - r.left) <= MIN_SIZE || (r.bottom - r.top) <= MIN_SIZE
    }

    fn badge_text(sel: &RECT) -> String {
        let (w, h) = ((sel.right - sel.left).max(0), (sel.bottom - sel.top).max(0));
        if Self::too_small(sel) {
            format!("{} × {}  太小", w, h)
        } else {
            format!("{} × {}", w, h)
        }
    }

    /// 徽标矩形：默认在选区上方，顶部空间不够时落到下方。宽度按当前字体量取。
    unsafe fn badge_rect(&self, sel: &RECT) -> RECT {
        let (tw, th) = self.measure(&Self::badge_text(sel), self.font_ui);
        let bw = tw + 20;
        let bh = th + 10;
        let top = if sel.top >= bh + 8 { sel.top - bh - 8 } else { sel.bottom + 8 };
        RECT {
            left: sel.left,
            top,
            right: sel.left + bw,
            bottom: top + bh,
        }
    }

    /// 放大镜整体矩形；拖拽中不显示。
    ///
    /// 这是**完整绘制范围**（内容 + 2px 外框笔 + 标签 + 余量），失效也按它来——
    /// 若失效范围比实际画出来的小，边框笔越界的像素不会被擦除，移动时就会
    /// 留下拖尾。绘制侧用 `left + 2 / top + 2` 内缩出内容原点。
    unsafe fn mag_rect(&self) -> Option<RECT> {
        if self.anchor.is_some() {
            return None;
        }
        let (cx, cy) = self.cursor;
        let (vw, vh) = (self.vw, self.vh);
        // 优先放在光标右下，靠边翻到对侧，最后整体夹回屏内。
        let w = MAG_BOX + 4;
        let h = MAG_TOTAL_H + 4;
        let mut mx = cx + 20;
        let mut my = cy + 20;
        if mx + w > vw {
            mx = cx - 20 - w;
        }
        if my + h > vh {
            my = cy - 20 - h;
        }
        let mx = mx.clamp(0, (vw - w).max(0));
        let my = my.clamp(0, (vh - h).max(0));
        Some(RECT {
            left: mx,
            top: my,
            right: mx + w,
            bottom: my + h,
        })
    }

    /// 当前所有「随状态移动的画面元素」包围盒——失效前后各取一次求并集。
    /// 提示胶囊位置固定，不在此列（文案变化时单独失效）。
    unsafe fn art_rects(&self) -> Vec<RECT> {
        let mut v = Vec::with_capacity(5);
        match self.sel_rect() {
            Some(sel) => {
                let m = BORDER + HANDLE / 2 + 1; // 边框 + 手柄越界量
                v.push(expand(&sel, m));
                v.push(self.badge_rect(&sel));
            }
            None => {
                // 十字线（两条全屏细线）
                let (cx, cy) = self.cursor;
                v.push(RECT {
                    left: 0,
                    top: cy,
                    right: self.vw,
                    bottom: cy + 1,
                });
                v.push(RECT {
                    left: cx,
                    top: 0,
                    right: cx + 1,
                    bottom: self.vh,
                });
                if let Some(m) = self.mag_rect() {
                    v.push(m);
                }
            }
        }
        v
    }

    /// 量文本尺寸（临时把 font 选入 frame DC，量完还原）。
    unsafe fn measure(&self, text: &str, font: HFONT) -> (i32, i32) {
        let old = SelectObject(self.frame.dc, font);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut sz = SIZE::default();
        let ok = GetTextExtentPoint32W(self.frame.dc, &wide, &mut sz).as_bool();
        SelectObject(self.frame.dc, old);
        if ok {
            (sz.cx, sz.cy)
        } else {
            (text.len() as i32 * 8, 16)
        }
    }

    // ── 绘制 ──────────────────────────────────────────────────────────

    /// WM_PAINT：脏区擦除（dim）→ 选区 punch-through（bright）→ 装饰 → 呈现。
    unsafe fn paint(&mut self, hwnd: HWND) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.is_invalid() {
            return;
        }
        let clip = ps.rcPaint;
        let f = self.frame.dc;

        // 1) 用 dim 铺底（同时擦掉上一帧画在 frame 里的装饰）。
        let cw = (clip.right - clip.left).max(0);
        let ch = (clip.bottom - clip.top).max(0);
        if cw > 0 && ch > 0 {
            let _ = BitBlt(
                f,
                clip.left,
                clip.top,
                cw,
                ch,
                self.dim.dc,
                clip.left,
                clip.top,
                SRCCOPY,
            );
        }

        // 2) 选区内换回原图（punch-through）。
        if let Some(sel) = self.sel_rect() {
            if let Some(inter) = rect_intersect(&sel, &clip) {
                let _ = BitBlt(
                    f,
                    inter.left,
                    inter.top,
                    inter.right - inter.left,
                    inter.bottom - inter.top,
                    self.bright.dc,
                    inter.left,
                    inter.top,
                    SRCCOPY,
                );
            }
        }

        // 3) 装饰（画进 frame；最终呈现受 BeginPaint 裁剪，越界部分只是白画）。
        SetBkMode(f, TRANSPARENT);
        if let Some(sel) = self.sel_rect() {
            self.draw_selection(f, &sel);
        } else {
            self.draw_crosshair(f);
            self.draw_magnifier(f);
        }
        self.draw_hint(f);

        // 4) 一次呈现。
        if cw > 0 && ch > 0 {
            let _ = BitBlt(hdc, clip.left, clip.top, cw, ch, f, clip.left, clip.top, SRCCOPY);
        }
        let _ = EndPaint(hwnd, &ps);
    }

    unsafe fn draw_selection(&mut self, f: HDC, sel: &RECT) {
        let bad = Self::too_small(sel);
        let accent_pen = if bad { self.pen_red } else { self.pen_accent };
        let badge_brush = if bad { self.brush_badge_bad } else { self.brush_badge };

        // 边框：accent 笔 + NULL_BRUSH（只描边不填充）。
        let old_pen = SelectObject(f, accent_pen);
        let old_brush = SelectObject(f, GetStockObject(NULL_BRUSH));
        // Rectangle 的笔以几何边为中心，外扩 1 让边框贴在选区外侧。
        let _ = Rectangle(
            f,
            sel.left - 1,
            sel.top - 1,
            sel.right + 1,
            sel.bottom + 1,
        );

        // 四角手柄：1px 白描边 + accent 填充
        let half = HANDLE / 2;
        SelectObject(f, self.pen_white);
        SelectObject(f, self.brush_accent);
        for (cx, cy) in [
            (sel.left, sel.top),
            (sel.right, sel.top),
            (sel.left, sel.bottom),
            (sel.right, sel.bottom),
        ] {
            let _ = Rectangle(f, cx - half, cy - half, cx + half, cy + half);
        }
        SelectObject(f, old_pen);
        SelectObject(f, old_brush);

        // 尺寸徽标（圆角深蓝底 + 白字）
        let badge = self.badge_rect(sel);
        let old_pen = SelectObject(f, GetStockObject(NULL_PEN));
        let old_brush = SelectObject(f, badge_brush);
        let _ = RoundRect(f, badge.left, badge.top, badge.right, badge.bottom, 6, 6);
        SelectObject(f, old_pen);
        SelectObject(f, old_brush);
        let text = Self::badge_text(sel);
        self.draw_text(f, &text, badge, self.font_ui, WHITE);
    }

    unsafe fn draw_crosshair(&mut self, f: HDC) {
        let (cx, cy) = self.cursor;
        let old_pen = SelectObject(f, self.pen_cross);
        let _ = MoveToEx(f, 0, cy, None);
        let _ = LineTo(f, self.vw, cy);
        let _ = MoveToEx(f, cx, 0, None);
        let _ = LineTo(f, cx, self.vh);
        SelectObject(f, old_pen);
    }

    /// 放大镜：光标附近 15×15 像素放大 8 倍 + 网格 + 中心像素高亮 + 坐标/颜色。
    unsafe fn draw_magnifier(&mut self, f: HDC) {
        let Some(box_r) = self.mag_rect() else { return };
        // 内容原点 = 完整范围向内缩 2px（给 2px 外框笔留位，见 mag_rect 注释）。
        let mx = box_r.left + 2;
        let my = box_r.top + 2;
        let (cx, cy) = self.cursor;

        // 采样窗口以光标为中心，夹在 DIB 内（贴边时视野向内滑）。
        let sx = (cx - MAG_N / 2).clamp(0, self.vw - MAG_N);
        let sy = (cy - MAG_N / 2).clamp(0, self.vh - MAG_N);

        // 放大内容（整数倍 + COLORONCOLOR，像素清晰不糊）
        let _ = SetStretchBltMode(f, COLORONCOLOR);
        let _ = StretchBlt(
            f,
            mx,
            my,
            MAG_BOX,
            MAG_BOX,
            self.bright.dc,
            sx,
            sy,
            MAG_N,
            MAG_N,
            SRCCOPY,
        );

        // 网格（每 8px 一条，跳过最外圈——那是边框的位置；线条只用到笔）
        let old_pen = SelectObject(f, self.pen_grid);
        for i in 1..MAG_N {
            let t = mx + i * MAG_Z;
            let _ = MoveToEx(f, t, my, None);
            let _ = LineTo(f, t, my + MAG_BOX);
            let t = my + i * MAG_Z;
            let _ = MoveToEx(f, mx, t, None);
            let _ = LineTo(f, mx + MAG_BOX, t);
        }

        // 之后的 Rectangle / 外框都只要描边：NULL_BRUSH
        let old_brush = SelectObject(f, GetStockObject(NULL_BRUSH));

        // 中心像素高亮（红框）
        SelectObject(f, self.pen_red);
        let c = mx + (MAG_N / 2) * MAG_Z;
        let _ = Rectangle(f, c, c, c + MAG_Z, c + MAG_Z);

        // 外框：深色 2px + 内侧 1px 白
        SelectObject(f, self.pen_dark);
        let _ = Rectangle(f, mx - 1, my - 1, mx + MAG_BOX + 1, my + MAG_BOX + 1);
        SelectObject(f, self.pen_white);
        let _ = Rectangle(f, mx + 1, my + 1, mx + MAG_BOX - 1, my + MAG_BOX - 1);
        SelectObject(f, old_pen);
        SelectObject(f, old_brush);

        // 标签：光标屏幕坐标 + 中心像素颜色
        let rgb = self.pixel_rgb(cx, cy);
        let label = format!(
            "{}, {}  #{:02X}{:02X}{:02X}",
            cx + self.vx,
            cy + self.vy,
            rgb.0,
            rgb.1,
            rgb.2
        );
        let label_r = RECT {
            left: mx,
            top: my + MAG_BOX + 4,
            right: mx + MAG_BOX,
            bottom: my + MAG_BOX + 4 + MAG_LABEL_H,
        };
        let old_pen = SelectObject(f, GetStockObject(NULL_PEN));
        let old_brush = SelectObject(f, self.brush_pill);
        let _ = RoundRect(
            f,
            label_r.left,
            label_r.top,
            label_r.right,
            label_r.bottom,
            4,
            4,
        );
        SelectObject(f, old_pen);
        SelectObject(f, old_brush);
        self.draw_text(f, &label, label_r, self.font_mono, WHITE);
    }

    unsafe fn draw_hint(&mut self, f: HDC) {
        let text = if self.anchor.is_some() {
            "松开左键完成选区"
        } else {
            "拖拽框选区域 · 右键 / Esc 取消"
        };
        let old_pen = SelectObject(f, GetStockObject(NULL_PEN));
        let old_brush = SelectObject(f, self.brush_pill);
        let _ = RoundRect(
            f,
            self.hint_area.left,
            self.hint_area.top,
            self.hint_area.right,
            self.hint_area.bottom,
            17,
            17,
        );
        SelectObject(f, old_pen);
        SelectObject(f, old_brush);
        self.draw_text(f, text, self.hint_area, self.font_ui, WHITE);
    }

    /// 在矩形内居中画一行文本（RECT 按值传，避免和 &mut self 抢借用）。
    unsafe fn draw_text(&mut self, f: HDC, text: &str, rect: RECT, font: HFONT, color: COLORREF) {
        let old_font = SelectObject(f, font);
        SetTextColor(f, color);
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut r = rect;
        let _ = DrawTextW(f, &mut wide, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        SelectObject(f, old_font);
    }

    /// 从 bright DIB 读一个像素的 RGB（client 坐标，越界返回黑）。
    unsafe fn pixel_rgb(&self, x: i32, y: i32) -> (u32, u32, u32) {
        if x < 0 || y < 0 || x >= self.vw || y >= self.vh {
            return (0, 0, 0);
        }
        let off = (y as usize * self.vw as usize + x as usize) * 4;
        let bits = std::slice::from_raw_parts(self.bright.bits, off + 4);
        (bits[off + 2] as u32, bits[off + 1] as u32, bits[off] as u32)
    }

    // ── 交互处理（wndproc 调用）───────────────────────────────────────

    /// 更新光标并失效前后画面元素。
    unsafe fn move_cursor(&mut self, hwnd: HWND, pt: (i32, i32)) {
        let before = self.art_rects();
        self.cursor = pt;
        let after = self.art_rects();
        for r in before.into_iter().chain(after) {
            let _ = InvalidateRect(hwnd, Some(&r), false);
        }
    }

    unsafe fn begin_drag(&mut self, hwnd: HWND, pt: (i32, i32)) {
        let before = self.art_rects();
        self.anchor = Some(pt);
        self.cursor = pt;
        let after = self.art_rects();
        // 提示胶囊文案从「拖拽框选…」换成「松开完成」，位置不变。
        for r in before.into_iter().chain(after).chain([self.hint_area]) {
            let _ = InvalidateRect(hwnd, Some(&r), false);
        }
        crate::safe_eprint(&format!("[picker] down at {:?}", (pt.0 + self.vx, pt.1 + self.vy)));
    }

    unsafe fn end_drag(&mut self, hwnd: HWND, pt: (i32, i32)) {
        self.cursor = pt;
        let Some(a) = self.anchor else { return };
        let sel = RECT {
            left: a.0.min(pt.0),
            top: a.1.min(pt.1),
            right: a.0.max(pt.0),
            bottom: a.1.max(pt.1),
        };
        let (w, h) = (sel.right - sel.left, sel.bottom - sel.top);
        if w > MIN_SIZE && h > MIN_SIZE {
            // 完成：换算回虚拟桌面物理坐标。
            (*self.outcome).region =
                Some((sel.left + self.vx, sel.top + self.vy, w as u32, h as u32));
            (*self.outcome).finished = true;
            crate::safe_eprint(&format!(
                "[picker] up -> ({},{},{}x{})",
                sel.left + self.vx,
                sel.top + self.vy,
                w,
                h
            ));
        } else {
            // 过小：原地重置，等待下一次拖拽（单击不会退出选区模式）。
            let before = self.art_rects();
            self.anchor = None;
            let after = self.art_rects();
            for r in before.into_iter().chain(after).chain([self.hint_area]) {
                let _ = InvalidateRect(hwnd, Some(&r), false);
            }
            crate::safe_eprint("[picker] too small, reset");
        }
    }

    unsafe fn cancel(&mut self) {
        (*self.outcome).cancelled = true;
        (*self.outcome).finished = true;
    }

    /// WM_TIMER：Esc 轮询 + 整体超时。
    unsafe fn on_timer(&mut self) {
        if key_down(VK_ESCAPE) || Instant::now() > self.deadline {
            self.cancel();
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font_ui);
            let _ = DeleteObject(self.font_mono);
            let _ = DeleteObject(self.pen_accent);
            let _ = DeleteObject(self.pen_white);
            let _ = DeleteObject(self.pen_cross);
            let _ = DeleteObject(self.pen_grid);
            let _ = DeleteObject(self.pen_dark);
            let _ = DeleteObject(self.pen_red);
            let _ = DeleteObject(self.brush_accent);
            let _ = DeleteObject(self.brush_badge);
            let _ = DeleteObject(self.brush_badge_bad);
            let _ = DeleteObject(self.brush_pill);
        }
    }
}

// ── 窗口过程 ──────────────────────────────────────────────────────────

unsafe fn overlay_of(hwnd: HWND) -> Option<&'static mut Overlay> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Overlay;
    (p as usize != 0).then(|| &mut *p)
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // 第一条消息：把 CreateWindowExW 传入的 Overlay 指针挂到窗口上。
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCDESTROY => {
            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Overlay;
            if p as usize != 0 {
                drop(Box::from_raw(p));
                let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ERASEBKGND => LRESULT(1), // 背景擦除由 paint 全权负责，防闪烁
        WM_SETCURSOR => {
            // 客户区内把箭头换成十字光标。
            if ((wparam.0 & 0xffff) as u32) == HTCLIENT {
                if let Ok(cur) = LoadCursorW(None, IDC_CROSS) {
                    SetCursor(cur);
                }
                LRESULT(1)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_TIMER => {
            if let Some(o) = overlay_of(hwnd) {
                o.on_timer();
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(o) = overlay_of(hwnd) {
                o.move_cursor(hwnd, (x_of(lparam), y_of(lparam)));
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(o) = overlay_of(hwnd) {
                o.begin_drag(hwnd, (x_of(lparam), y_of(lparam)));
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(o) = overlay_of(hwnd) {
                o.end_drag(hwnd, (x_of(lparam), y_of(lparam)));
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            // 覆盖层吃掉右键：底层应用不会收到（也就不会弹菜单）。
            if let Some(o) = overlay_of(hwnd) {
                o.cancel();
            }
            LRESULT(0)
        }
        WM_PAINT => {
            if let Some(o) = overlay_of(hwnd) {
                o.paint(hwnd);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── 基础设施 ──────────────────────────────────────────────────────────

fn x_of(lparam: LPARAM) -> i32 {
    (lparam.0 & 0xffff) as u16 as i16 as i32
}

fn y_of(lparam: LPARAM) -> i32 {
    ((lparam.0 >> 16) & 0xffff) as u16 as i16 as i32
}

fn key_down(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0 }
}

unsafe fn virtual_screen() -> (i32, i32, i32, i32) {
    (
        GetSystemMetrics(SM_XVIRTUALSCREEN),
        GetSystemMetrics(SM_YVIRTUALSCREEN),
        GetSystemMetrics(SM_CXVIRTUALSCREEN),
        GetSystemMetrics(SM_CYVIRTUALSCREEN),
    )
}

unsafe fn initial_cursor(vx: i32, vy: i32) -> (i32, i32) {
    let mut pt = POINT::default();
    if GetCursorPos(&mut pt).is_ok() {
        (pt.x - vx, pt.y - vy)
    } else {
        (0, 0)
    }
}

/// 提示胶囊：鼠标所在显示器顶部居中（换算到 client 坐标）。
unsafe fn hint_area(vx: i32, vy: i32, vw: i32, vh: i32) -> RECT {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let m = if GetMonitorInfoW(mon, &mut mi).as_bool() {
        mi.rcMonitor
    } else {
        RECT {
            left: vx,
            top: vy,
            right: vx + vw,
            bottom: vy + vh,
        }
    };
    let w = 400.min(vw - 16);
    let h = 34;
    let left = (m.left + (m.right - m.left - w) / 2 - vx).clamp(8, vw - w - 8);
    let top = (m.top + 16 - vy).clamp(8, vh - h - 8);
    RECT {
        left,
        top,
        right: left + w,
        bottom: top + h,
    }
}

fn dim_byte(v: u8) -> u8 {
    ((v as u32 * 11 + 5) / 20) as u8 // ≈ 0.55
}

fn rect_intersect(a: &RECT, b: &RECT) -> Option<RECT> {
    let left = a.left.max(b.left);
    let top = a.top.max(b.top);
    let right = a.right.min(b.right);
    let bottom = a.bottom.min(b.bottom);
    (right > left && bottom > top).then_some(RECT {
        left,
        top,
        right,
        bottom,
    })
}

fn expand(r: &RECT, m: i32) -> RECT {
    RECT {
        left: r.left - m,
        top: r.top - m,
        right: r.right + m,
        bottom: r.bottom + m,
    }
}

// CreateFontW 的字体参数在 windows-0.58 里是裸 u32，常量是 newtype，统一转一下。
unsafe fn ui_font(height: i32, weight: i32) -> HFONT {
    let face = wide("Segoe UI");
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        DEFAULT_PITCH.0 as u32,
        PCWSTR::from_raw(face.as_ptr()),
    )
}

unsafe fn mono_font(height: i32) -> HFONT {
    let face = wide("Consolas");
    CreateFontW(
        height,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        DEFAULT_PITCH.0 as u32,
        PCWSTR::from_raw(face.as_ptr()),
    )
}

fn ensure_class() -> Result<(), String> {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| unsafe {
        let class = wide(CLASS_NAME);
        let hinstance = GetModuleHandleW(None).expect("GetModuleHandleW failed");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR::from_raw(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
    Ok(())
}

unsafe fn create_window(
    vx: i32,
    vy: i32,
    vw: i32,
    vh: i32,
    param: *const std::ffi::c_void,
) -> Result<HWND, String> {
    let class = wide(CLASS_NAME);
    let title = wide("miaocr-region-picker");
    let hinstance: HINSTANCE = GetModuleHandleW(None).map_err(|e| e.to_string())?.into();
    CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        PCWSTR::from_raw(class.as_ptr()),
        PCWSTR::from_raw(title.as_ptr()),
        WS_POPUP,
        vx,
        vy,
        vw,
        vh,
        None,
        None,
        hinstance,
        Some(param),
    )
    .map_err(|e| e.to_string())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ── 单元测试（纯逻辑部分）────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_byte_scales_to_55_percent() {
        assert_eq!(dim_byte(0), 0);
        assert_eq!(dim_byte(100), 55);
        assert_eq!(dim_byte(255), 140); // 255*11/20 = 140.25 → 140
        assert!(dim_byte(255) < 255);
    }

    #[test]
    fn rect_intersect_basic() {
        let a = RECT {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let b = RECT {
            left: 50,
            top: 50,
            right: 150,
            bottom: 150,
        };
        let i = rect_intersect(&a, &b).unwrap();
        assert_eq!((i.left, i.top, i.right, i.bottom), (50, 50, 100, 100));
        let c = RECT {
            left: 200,
            top: 200,
            right: 300,
            bottom: 300,
        };
        assert!(rect_intersect(&a, &c).is_none());
    }

    #[test]
    fn expand_grows_all_sides() {
        let r = RECT {
            left: 10,
            top: 10,
            right: 20,
            bottom: 20,
        };
        let e = expand(&r, 5);
        assert_eq!((e.left, e.top, e.right, e.bottom), (5, 5, 25, 25));
    }

    #[test]
    fn lparam_xy_extraction() {
        // 低 16 位 x，高 16 位 y，均按有符号 16 位还原。
        let l = LPARAM(0x0010_0020);
        assert_eq!((x_of(l), y_of(l)), (0x20, 0x10));
    }
}
