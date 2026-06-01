use anyhow::Result;
use eframe::egui;
use screenshots::Screen;
use std::sync::{Arc, Mutex};
use windows::{
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Globalization::Language,
    Media::Ocr::OcrEngine,
    Storage::Streams::Buffer,
    Win32::System::WinRT::IBufferByteAccess,
    core::{HSTRING, Interface},
};

fn ocr_region(x: i32, y: i32, width: u32, height: u32) -> Result<String> {
    let screens = Screen::all()?;
    let center_x = x + (width as i32) / 2;
    let center_y = y + (height as i32) / 2;

    // Find the screen containing this center point
    let screen = screens
        .iter()
        .find(|s| {
            let info = s.display_info;
            center_x >= info.x
                && center_x < info.x + info.width as i32
                && center_y >= info.y
                && center_y < info.y + info.height as i32
        })
        .unwrap_or(&screens[0]);

    let info = screen.display_info;
    let local_x = (x - info.x).max(0);
    let local_y = (y - info.y).max(0);

    // Clamp width and height to fit inside the screen boundaries
    let capture_width = width.min((info.width as i32 - local_x).max(0) as u32);
    let capture_height = height.min((info.height as i32 - local_y).max(0) as u32);

    if capture_width == 0 || capture_height == 0 {
        return Ok(String::new());
    }

    let image = screen.capture_area(local_x, local_y, capture_width, capture_height)?;
    let raw: &[u8] = image.as_raw();

    // Dynamic upscaling: only resize if the selection is small (e.g. under 400x300)
    let (bgra, final_width, final_height) = if capture_width < 400 && capture_height < 300 {
        // Reconstruct ImageBuffer from raw RGBA bytes
        let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            capture_width,
            capture_height,
            raw.to_vec(),
        )
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;

        // Scale up 2x using CatmullRom sharpening filter to improve OCR accuracy
        let scaled_width = capture_width * 2;
        let scaled_height = capture_height * 2;
        let resized_img = image::imageops::resize(
            &img_buffer,
            scaled_width,
            scaled_height,
            image::imageops::FilterType::CatmullRom,
        );

        // Convert RGBA to BGRA
        let bgra_bytes: Vec<u8> = resized_img
            .as_raw()
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();

        (bgra_bytes, scaled_width, scaled_height)
    } else {
        // Convert RGBA to BGRA directly without resizing
        let bgra_bytes: Vec<u8> = raw
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();

        (bgra_bytes, capture_width, capture_height)
    };

    let bitmap = SoftwareBitmap::CreateWithAlpha(
        BitmapPixelFormat::Bgra8,
        final_width as i32,
        final_height as i32,
        BitmapAlphaMode::Premultiplied,
    )?;

    let buf = Buffer::Create(bgra.len() as u32)?;
    buf.SetLength(bgra.len() as u32)?;
    unsafe {
        let byte_access: IBufferByteAccess = buf.cast()?;
        let data = std::slice::from_raw_parts_mut(byte_access.Buffer()?, bgra.len());
        data.copy_from_slice(&bgra);
    }
    bitmap.CopyFromBuffer(&buf)?;

    let lang = Language::CreateLanguage(&HSTRING::from("zh-CN"))?;

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if let Ok(supported) = OcrEngine::IsLanguageSupported(&lang) {
            println!("Windows OCR 'zh-CN' 支持情况: {}", supported);
        }
        if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
            if let Ok(rec_lang) = engine.RecognizerLanguage() {
                if let Ok(tag) = rec_lang.LanguageTag() {
                    println!("当前实际使用的 OCR 语言: {}", tag);
                }
            }
        }
    });

    let engine = OcrEngine::TryCreateFromLanguage(&lang)?;
    let result = engine.RecognizeAsync(&bitmap)?.get()?;
    let text = result.Text()?.to_string();

    let clean: String = text
        .lines()
        .map(|l: &str| l.replace(' ', ""))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(clean)
}

fn get_cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    use windows::Win32::Foundation::POINT;
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }
}

// ─── 悬浮窗应用 ──────────────────────────────────────────

#[derive(PartialEq)]
enum AppMode {
    Float,      // 悬浮面板状态
    Selecting,  // 屏幕选区状态
}

#[derive(Clone, Copy)]
struct ScreenInfo {
    _id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f32,
}

struct FloatApp {
    mode: AppMode,
    select_step: u8,
    frame_delay: u8,

    // OCR 共享状态
    ocr_region: Arc<Mutex<Option<(i32, i32, u32, u32)>>>,
    paused: Arc<Mutex<bool>>,
    text: Arc<Mutex<String>>,
    elapsed: Arc<Mutex<u128>>,
    interval: Arc<Mutex<u128>>,

    // 折叠展开状态
    expanded: bool,

    // 选区相关的临时截图数据
    screenshot_texture: Option<egui::TextureHandle>,
    screenshot_raw: Vec<u8>,
    screenshot_width: u32,
    screenshot_height: u32,
    active_screen_info: Option<ScreenInfo>,
    drag_start: Option<egui::Pos2>,
    drag_end: Option<egui::Pos2>,

    // 记忆悬浮窗位置，以便选区完后复位
    float_pos: egui::Pos2,
}

impl eframe::App for FloatApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 截图隐藏逻辑的状态机处理
        if self.select_step == 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(-10000.0, -10000.0)));
            self.select_step = 2;
            self.frame_delay = 5;
        } else if self.select_step == 2 {
            if self.frame_delay > 0 {
                self.frame_delay -= 1;
                ctx.request_repaint(); // 强制刷新以将隐藏事件发往 Windows 窗口管理器
            } else {
                // 截图鼠标所在的屏幕
                if let Some(screens) = Screen::all().ok() {
                    let (mx, my) = get_cursor_pos().unwrap_or((0, 0));
                    let active = screens
                        .iter()
                        .find(|s| {
                            let info = s.display_info;
                            mx >= info.x
                                && mx < info.x + info.width as i32
                                && my >= info.y
                                && my < info.y + info.height as i32
                        })
                        .unwrap_or(&screens[0]);

                    if let Ok(image) = active.capture() {
                        self.screenshot_raw = image.as_raw().to_vec();
                        self.screenshot_width = active.display_info.width;
                        self.screenshot_height = active.display_info.height;
                        let info = active.display_info;
                        self.active_screen_info = Some(ScreenInfo {
                            _id: info.id,
                            x: info.x,
                            y: info.y,
                            width: info.width,
                            height: info.height,
                            scale_factor: info.scale_factor,
                        });
                    }
                }
                self.select_step = 3;
            }
        } else if self.select_step == 3 {
            if let Some(info) = &self.active_screen_info {
                let scale = info.scale_factor;
                let logical_width = info.width as f32 / scale;
                let logical_height = info.height as f32 / scale;
                let logical_x = info.x as f32 / scale;
                let logical_y = info.y as f32 / scale;

                // 调整当前窗口位置和尺寸以匹配激活屏幕全屏
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(logical_x, logical_y)));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(logical_width, logical_height)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));

                self.mode = AppMode::Selecting;
                self.drag_start = None;
                self.drag_end = None;
                self.screenshot_texture = None;
            }
            self.select_step = 0;
        }

        // 当处于悬浮窗模式且没有进行隐藏截图时，持续记录当前窗口坐标，用于选区完成后复位
        if self.mode == AppMode::Float && self.select_step == 0 {
            if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                if rect.min.x > -9000.0 && rect.min.y > -9000.0 {
                    self.float_pos = rect.min;
                }
            }
        }

        // 2. 选择区域模式渲染
        if self.mode == AppMode::Selecting {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                .show(ctx, |ui| {
                    let rect = ui.available_rect_before_wrap();

                    if self.screenshot_texture.is_none() && !self.screenshot_raw.is_empty() {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [self.screenshot_width as usize, self.screenshot_height as usize],
                            &self.screenshot_raw,
                        );
                        self.screenshot_texture = Some(ctx.load_texture(
                            "screenshot",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }

                    if let Some(tex) = &self.screenshot_texture {
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }

                    // 黑色半透明覆盖遮罩
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 80),
                    );

                    // 绘制顶部操作说明
                    ui.painter().text(
                        egui::pos2(rect.width() / 2.0, 40.0),
                        egui::Align2::CENTER_TOP,
                        "拖拽选择持续识别区域  |  ESC 取消",
                        egui::FontId::proportional(18.0),
                        egui::Color32::WHITE,
                    );

                    let response = ui.interact(rect, ui.id(), egui::Sense::drag());

                    if response.drag_started() {
                        self.drag_start = ctx.input(|i| i.pointer.press_origin());
                    }
                    if response.dragged() {
                        self.drag_end = ctx.input(|i| i.pointer.hover_pos());
                    }

                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                        let sel = egui::Rect::from_two_pos(start, end);

                        if let Some(tex) = &self.screenshot_texture {
                            let uv_min = egui::pos2(sel.min.x / rect.width(), sel.min.y / rect.height());
                            let uv_max = egui::pos2(sel.max.x / rect.width(), sel.max.y / rect.height());
                            ui.painter().image(
                                tex.id(),
                                sel,
                                egui::Rect::from_min_max(uv_min, uv_max),
                                egui::Color32::WHITE,
                            );
                        }

                        ui.painter().rect_stroke(
                            sel,
                            0.0,
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)),
                        );

                        ui.painter().text(
                            sel.min + egui::vec2(4.0, -18.0),
                            egui::Align2::LEFT_TOP,
                            format!("{:.0} × {:.0}", sel.width(), sel.height()),
                            egui::FontId::proportional(13.0),
                            egui::Color32::WHITE,
                        );
                    }

                    if response.drag_stopped() {
                        if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                            let sel = egui::Rect::from_two_pos(start, end);
                            if sel.width() > 10.0 && sel.height() > 10.0 {
                                if let Some(info) = &self.active_screen_info {
                                    let scale = info.scale_factor;
                                    let x = (sel.min.x * scale) as i32 + info.x;
                                    let y = (sel.min.y * scale) as i32 + info.y;
                                    let w = (sel.width() * scale) as u32;
                                    let h = (sel.height() * scale) as u32;

                                    *self.ocr_region.lock().unwrap() = Some((x, y, w, h));
                                    *self.paused.lock().unwrap() = false;
                                }
                            }
                        }
                        // 还原到悬浮窗尺寸
                        let size = if self.expanded {
                            egui::vec2(350.0, 260.0)
                        } else {
                            egui::vec2(200.0, 42.0)
                        };
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.float_pos));
                        self.mode = AppMode::Float;
                    }

                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        let size = if self.expanded {
                            egui::vec2(350.0, 260.0)
                        } else {
                            egui::vec2(200.0, 42.0)
                        };
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.float_pos));
                        self.mode = AppMode::Float;
                    }
                });
        }

        // 3. 悬浮卡片模式渲染
        if self.mode == AppMode::Float {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::viewport::WindowLevel::AlwaysOnTop));

            // 半透明暗色卡片风格
            let card_frame = egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 25, 220))
                .rounding(egui::Rounding::same(8.0))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30)))
                .inner_margin(egui::Margin::symmetric(8.0, 6.0));

            egui::CentralPanel::default()
                .frame(card_frame)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        // 顶部操作栏
                        ui.horizontal(|ui| {
                            let title_lbl = ui.label(
                                egui::RichText::new("miaocr")
                                    .strong()
                                    .color(egui::Color32::from_rgb(100, 200, 255)),
                            );
                            let title_rect = title_lbl.rect;
                            let drag_res = ui.interact(title_rect, ui.id().with("drag"), egui::Sense::drag());
                            if drag_res.dragged() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                            drag_res.on_hover_text("按住此处拖拽窗口");

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // 展开 / 收起折叠按钮
                                let exp_text = if self.expanded { "折叠" } else { "展开" };
                                if ui.button(exp_text).clicked() {
                                    self.expanded = !self.expanded;
                                    let size = if self.expanded {
                                        egui::vec2(350.0, 260.0)
                                    } else {
                                        egui::vec2(200.0, 42.0)
                                    };
                                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                                }

                                // 播放 / 暂停按钮 (只有在有有效选区时才显示)
                                let has_region = self.ocr_region.lock().unwrap().is_some();
                                if has_region {
                                    let is_paused = *self.paused.lock().unwrap();
                                    let play_pause_btn = if is_paused { "继续" } else { "暂停" };
                                    let btn_res = ui.button(play_pause_btn);
                                    if btn_res.clicked() {
                                        let mut p = self.paused.lock().unwrap();
                                        *p = !*p;
                                    }
                                    btn_res.on_hover_text(if is_paused { "继续识别" } else { "暂停识别" });
                                }

                                // 选择选区按钮
                                let sel_btn = ui.button("选区");
                                if sel_btn.clicked() {
                                    if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                                        if rect.min.x > -9000.0 && rect.min.y > -9000.0 {
                                            self.float_pos = rect.min;
                                        }
                                    }
                                    self.select_step = 1;
                                }
                                sel_btn.on_hover_text("选择识别区域");
                            });
                        });
                    // 如果处于展开状态，显示文本区和复制
                        if self.expanded {
                            ui.separator();

                            ui.horizontal(|ui| {
                                let ms = *self.elapsed.lock().unwrap();
                                let interval = *self.interval.lock().unwrap();
                                if ms > 0 {
                                    ui.label(
                                        egui::RichText::new(format!("耗时: {} ms | 间隔: {} ms", ms, interval))
                                            .color(egui::Color32::GRAY)
                                            .size(11.0),
                                    );
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let text = self.text.lock().unwrap().clone();
                                    if ui.button("复制").clicked() {
                                        ui.output_mut(|o| o.copied_text = text);
                                    }
                                });
                            });

                            ui.add_space(4.0);

                            let text = self.text.lock().unwrap().clone();
                            egui::ScrollArea::vertical()
                                .max_height(160.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut text.as_str())
                                            .desired_width(f32::INFINITY)
                                            .font(egui::FontId::proportional(14.0)),
                                    );
                                });
                        }
                    });
                });
        }
    }
}

// ─── 主入口 ──────────────────────────────────────────────

struct LogManager {
    file: Option<std::fs::File>,
    last_text: Option<String>,
    last_header_pos: u64,
    start_time: Option<chrono::DateTime<chrono::Local>>,
}

impl LogManager {
    fn new() -> Self {
        use std::fs::OpenOptions;
        use std::io::Seek;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("ocr_log.txt")
            .ok();
        
        let mut mgr = Self {
            file,
            last_text: None,
            last_header_pos: 0,
            start_time: None,
        };

        if let Some(ref mut f) = mgr.file {
            let _ = f.seek(std::io::SeekFrom::End(0));
        }
        mgr
    }

    fn reset(&mut self) {
        self.last_text = None;
        self.last_header_pos = 0;
        self.start_time = None;
    }

    fn log(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        let file = match &mut self.file {
            Some(f) => f,
            None => return,
        };

        use std::io::{Write, Seek, SeekFrom};
        let now = chrono::Local::now();
        let time_str = now.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        if let Some(ref last) = self.last_text {
            if last == text {
                // 如果结果与之前一样，则定位到上一条记录的头部，覆写结束时间
                if file.seek(SeekFrom::Start(self.last_header_pos)).is_ok() {
                    let start_str = self.start_time.unwrap().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                    // 覆写时间头部，格式和长度必须保持完全一致以避免文本发生位移 (包括毫秒在内恰好 60 字节)
                    let header = format!("=== [{} ~ {}] ===\r\n", start_str, time_str);
                    let _ = file.write_all(header.as_bytes());
                    let _ = file.flush();
                }
                return;
            }
        }

        // 如果结果不同或为新会话，则在文件末尾写入新片段
        let _ = file.seek(SeekFrom::End(0));
        if let Ok(pos) = file.stream_position() {
            self.last_header_pos = pos;
        } else {
            self.last_header_pos = 0;
        }

        self.last_text = Some(text.to_string());
        self.start_time = Some(now);

        let header = format!("=== [{} ~ {}] ===\r\n", time_str, time_str);
        let _ = file.write_all(header.as_bytes());
        let _ = file.write_all(text.as_bytes());
        let _ = file.write_all(b"\r\n\r\n");
        let _ = file.flush();
    }
}

fn main() -> Result<()> {
    // 1. 初始化跨线程共享的状态
    let shared_text = Arc::new(Mutex::new(String::from("等待选择区域...")));
    let shared_elapsed = Arc::new(Mutex::new(0u128));
    let shared_interval = Arc::new(Mutex::new(1000u128));
    let shared_paused = Arc::new(Mutex::new(true));
    let shared_region = Arc::new(Mutex::new(None));

    // 2. 设定无边框、始终置顶且支持透明背景的悬浮胶囊参数
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([200.0, 42.0])
            .with_position([100.0, 100.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_transparent(true),
        ..Default::default()
    };

    let shared_text_clone = shared_text.clone();
    let shared_elapsed_clone = shared_elapsed.clone();
    let shared_interval_clone = shared_interval.clone();
    let shared_paused_clone = shared_paused.clone();
    let shared_region_clone = shared_region.clone();

    eframe::run_native(
        "miaocr",
        options,
        Box::new(move |cc| {
            // 加载系统内置微软雅黑中文字体
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "chinese".to_owned(),
                egui::FontData::from_static(include_bytes!(
                    "C:\\Windows\\Fonts\\msyh.ttc"
                )),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "chinese".to_owned());
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());

            // 3. 启动常驻 OCR 后台轮询线程，并传入 egui_ctx 用于 UI 自动重绘
            let ctx_clone = cc.egui_ctx.clone();
            let text_for_thread = shared_text_clone.clone();
            let elapsed_for_thread = shared_elapsed_clone.clone();
            let interval_for_thread = shared_interval_clone.clone();
            let paused_for_thread = shared_paused_clone.clone();
            let region_for_thread = shared_region_clone.clone();

            std::thread::spawn(move || {
                let mut history = std::collections::VecDeque::with_capacity(10);
                let mut log_mgr = LogManager::new();
                loop {
                    let is_paused = *paused_for_thread.lock().unwrap();
                    if is_paused {
                        history.clear();
                        log_mgr.reset();
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        continue;
                    }

                    let opt_region = *region_for_thread.lock().unwrap();
                    if let Some((x, y, w, h)) = opt_region {
                        let start = std::time::Instant::now();
                        match ocr_region(x, y, w, h) {
                            Ok(text) => {
                                let ms = start.elapsed().as_millis();
                                
                                log_mgr.log(&text);
                                *text_for_thread.lock().unwrap() = text;
                                *elapsed_for_thread.lock().unwrap() = ms;

                                if history.len() >= 10 {
                                    history.pop_front();
                                }
                                history.push_back(ms);
                            }
                            Err(e) => {
                                *text_for_thread.lock().unwrap() = format!("识别失败: {}", e);
                            }
                        }
                    }

                    // 动态决定下一次识别的休眠时间（取最近 10 次的最大耗时，辅以 50ms 下限及 2000ms 上限保护）
                    let sleep_ms = if history.is_empty() {
                        1000
                    } else {
                        let max_elapsed = *history.iter().max().unwrap_or(&0);
                        max_elapsed.max(50).min(2000)
                    };

                    *interval_for_thread.lock().unwrap() = sleep_ms as u128;

                    // 请求 UI 重绘以更新最新结果、耗时和间隔
                    ctx_clone.request_repaint();

                    std::thread::sleep(std::time::Duration::from_millis(sleep_ms as u64));
                }
            });

            Box::new(FloatApp {
                mode: AppMode::Float,
                select_step: 0,
                frame_delay: 0,
                ocr_region: shared_region,
                paused: shared_paused,
                text: shared_text,
                elapsed: shared_elapsed,
                interval: shared_interval,
                expanded: false,
                screenshot_texture: None,
                screenshot_raw: Vec::new(),
                screenshot_width: 0,
                screenshot_height: 0,
                active_screen_info: None,
                drag_start: None,
                drag_end: None,
                float_pos: egui::pos2(100.0, 100.0),
            }) as Box<dyn eframe::App>
        }),
    )
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}