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

fn binarize_bgra(bgra: &mut [u8]) {
    let pixel_count = bgra.len() / 4;
    if pixel_count == 0 {
        return;
    }

    // 1. 计算灰度值并生成直方图
    let mut grays = Vec::with_capacity(pixel_count);
    let mut histogram = [0u32; 256];
    for chunk in bgra.chunks_exact(4) {
        let b = chunk[0] as u32;
        let g = chunk[1] as u32;
        let r = chunk[2] as u32;
        // 使用标准心理学权重计算灰度
        let gray_val = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
        grays.push(gray_val);
        histogram[gray_val as usize] += 1;
    }

    // 2. 使用大津法 (Otsu's method) 计算最佳二值化阈值
    let mut sum = 0.0;
    for (i, &count) in histogram.iter().enumerate() {
        sum += i as f32 * count as f32;
    }

    let mut sum_b = 0.0;
    let mut w_b = 0u32;
    let mut var_max = 0.0;
    let mut threshold = 127u8;

    for t in 0..256 {
        w_b += histogram[t];
        if w_b == 0 {
            continue;
        }

        let w_f = pixel_count as u32 - w_b;
        if w_f == 0 {
            break;
        }

        sum_b += t as f32 * histogram[t] as f32;

        let m_b = sum_b / w_b as f32;
        let m_f = (sum - sum_b) / w_f as f32;

        let var_between = w_b as f32 * w_f as f32 * (m_b - m_f) * (m_b - m_f);

        if var_between > var_max {
            var_max = var_between;
            threshold = t as u8;
        }
    }

    // 3. 统计暗色像素占比，判断是否为深色背景以进行智能反色
    // (如果暗色像素占比过半，说明是深色背景，此时需要反色：原深色转白色，原亮色文本转黑色)
    let dark_pixels = grays.iter().filter(|&&g| g < threshold).count();
    let invert = dark_pixels > pixel_count / 2;

    // 4. 应用二值化
    for (i, chunk) in bgra.chunks_exact_mut(4).enumerate() {
        let g = grays[i];
        let val = if invert {
            if g < threshold { 255 } else { 0 }
        } else {
            if g < threshold { 0 } else { 255 }
        };
        chunk[0] = val; // B
        chunk[1] = val; // G
        chunk[2] = val; // R
        chunk[3] = 255; // A (不透明)
    }
}


fn save_temp_png(bgra: &[u8], width: u32, height: u32) -> Result<std::path::PathBuf> {
    let temp_dir = std::env::temp_dir();
    let temp_img_path = temp_dir.join("miaocr_temp.png");

    let rgba_bytes: Vec<u8> = bgra.chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();

    let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
        width,
        height,
        rgba_bytes,
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to convert image buffer for saving"))?;

    img_buffer.save(&temp_img_path)?;
    Ok(temp_img_path)
}

fn ocr_tesseract(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let temp_path = save_temp_png(bgra, width, height)?;
    let output = std::process::Command::new("tesseract")
        .arg(temp_path.to_str().unwrap())
        .arg("stdout")
        .arg("-l")
        .arg("chi_sim")
        .output();

    let _ = std::fs::remove_file(temp_path);

    match output {
        Ok(out) => {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                Ok(text)
            } else {
                let err = String::from_utf8_lossy(&out.stderr).to_string();
                Err(anyhow::anyhow!("Tesseract 运行出错: {}", err))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err(anyhow::anyhow!("未在系统 PATH 中找到 tesseract.exe\n请先安装 Tesseract OCR 并将其加入系统环境变量。"))
            } else {
                Err(anyhow::anyhow!("调用 Tesseract 失败: {}", e))
            }
        }
    }
}

fn ocr_paddle(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let temp_path = save_temp_png(bgra, width, height)?;
    let output = std::process::Command::new("paddleocr")
        .arg("--image_dir")
        .arg(temp_path.to_str().unwrap())
        .output();

    let _ = std::fs::remove_file(temp_path);

    match output {
        Ok(out) => {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                let mut lines = Vec::new();
                for line in text.lines() {
                    if let Some(start_idx) = line.find("('") {
                        if let Some(end_idx) = line[start_idx + 2..].find("',") {
                            let actual_text = &line[start_idx + 2..start_idx + 2 + end_idx];
                            lines.push(actual_text.to_string());
                        }
                    }
                }
                if lines.is_empty() {
                    Ok(text)
                } else {
                    Ok(lines.join("\n"))
                }
            } else {
                let err = String::from_utf8_lossy(&out.stderr).to_string();
                Err(anyhow::anyhow!("PaddleOCR 运行出错: {}", err))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err(anyhow::anyhow!("未在系统 PATH 中找到 paddleocr.exe\n请先通过 pip install paddleocr 安装，并确保在环境变量中。"))
            } else {
                Err(anyhow::anyhow!("调用 PaddleOCR 失败: {}", e))
            }
        }
    }
}

fn ocr_easy(bgra: &[u8], width: u32, height: u32) -> Result<String> {
    let temp_path = save_temp_png(bgra, width, height)?;
    let output = std::process::Command::new("easyocr")
        .arg("-l")
        .arg("ch_sim")
        .arg("-f")
        .arg(temp_path.to_str().unwrap())
        .output();

    let _ = std::fs::remove_file(temp_path);

    match output {
        Ok(out) => {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                let mut lines = Vec::new();
                for line in text.lines() {
                    if let Some(start_idx) = line.find(", '") {
                        if let Some(end_idx) = line[start_idx + 3..].find("',") {
                            let actual_text = &line[start_idx + 3..start_idx + 3 + end_idx];
                            lines.push(actual_text.to_string());
                        }
                    } else if let Some(start_idx) = line.find(", \"") {
                        if let Some(end_idx) = line[start_idx + 3..].find("\",") {
                            let actual_text = &line[start_idx + 3..start_idx + 3 + end_idx];
                            lines.push(actual_text.to_string());
                        }
                    }
                }
                if lines.is_empty() {
                    Ok(text)
                } else {
                    Ok(lines.join("\n"))
                }
            } else {
                let err = String::from_utf8_lossy(&out.stderr).to_string();
                Err(anyhow::anyhow!("EasyOCR 运行出错: {}", err))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err(anyhow::anyhow!("未在系统 PATH 中找到 easyocr.exe\n请先通过 pip install easyocr 安装，并确保在环境变量中。"))
            } else {
                Err(anyhow::anyhow!("调用 EasyOCR 失败: {}", e))
            }
        }
    }
}

fn ocr_api(bgra: &[u8], width: u32, height: u32, url: &str) -> Result<String> {
    let temp_path = save_temp_png(bgra, width, height)?;
    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg("-F")
        .arg(format!("image=@{}", temp_path.to_str().unwrap()))
        .arg(url)
        .output();

    let _ = std::fs::remove_file(temp_path);

    match output {
        Ok(out) => {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                Ok(text)
            } else {
                let err = String::from_utf8_lossy(&out.stderr).to_string();
                Err(anyhow::anyhow!("API 服务器错误: {}", err))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err(anyhow::anyhow!("系统未找到 curl.exe，请确保您的 Windows 环境正常。"))
            } else {
                Err(anyhow::anyhow!("发送 API 请求失败: {}", e))
            }
        }
    }
}

fn ocr_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    backend: BackendType,
    api_url: &str,
) -> Result<String> {
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

    // 动态缩放：对于小于 500x400 的中偏小选区，放大 2 倍以提升 OCR 解析精度
    let (mut bgra, final_width, final_height) = if capture_width < 500 && capture_height < 400 {
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

    // 应用二值化与智能反色算法预处理图片，去除抗锯齿杂色并将背景归一化为纯白底黑字
    binarize_bgra(&mut bgra);

    match backend {
        BackendType::WindowsNative => {
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
        BackendType::Tesseract => {
            let text = ocr_tesseract(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
        BackendType::PaddleOcr => {
            let text = ocr_paddle(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
        BackendType::EasyOcr => {
            let text = ocr_easy(&bgra, final_width, final_height)?;
            let clean: String = text
                .lines()
                .map(|l: &str| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(clean)
        }
        BackendType::CloudApi => {
            let text = ocr_api(&bgra, final_width, final_height, api_url)?;
            Ok(text)
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendType {
    WindowsNative,
    Tesseract,
    PaddleOcr,
    EasyOcr,
    CloudApi,
}

impl BackendType {
    fn display_name(&self) -> &'static str {
        match self {
            BackendType::WindowsNative => "Windows 原生",
            BackendType::Tesseract => "Tesseract (本地)",
            BackendType::PaddleOcr => "PaddleOCR (本地)",
            BackendType::EasyOcr => "EasyOCR (本地)",
            BackendType::CloudApi => "自定义 API",
        }
    }
}

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
    selected_backend: Arc<Mutex<BackendType>>,
    api_url: Arc<Mutex<String>>,

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

                            // 引擎选择 + 耗时统计 + 复制 — 合并成单行，避免换行
                            ui.horizontal(|ui| {
                                // 引擎选择器放在最左侧
                                ui.label(egui::RichText::new("引擎:").size(11.0).color(egui::Color32::from_rgb(100, 200, 255)));
                                let mut current_backend = *self.selected_backend.lock().unwrap();
                                let prev_backend = current_backend;
                                let combo = egui::ComboBox::from_id_source("backend_select")
                                    .selected_text(current_backend.display_name())
                                    .width(110.0);
                                combo.show_ui(ui, |ui| {
                                    ui.selectable_value(&mut current_backend, BackendType::WindowsNative, "Windows 原生");
                                    ui.selectable_value(&mut current_backend, BackendType::Tesseract, "Tesseract (本地)");
                                    ui.selectable_value(&mut current_backend, BackendType::PaddleOcr, "PaddleOCR (本地)");
                                    ui.selectable_value(&mut current_backend, BackendType::EasyOcr, "EasyOCR (本地)");
                                    ui.selectable_value(&mut current_backend, BackendType::CloudApi, "自定义 API");
                                });
                                if current_backend != prev_backend {
                                    *self.selected_backend.lock().unwrap() = current_backend;
                                }

                                // 如果选择了 CloudApi，紧随其后显示 URL 输入框
                                if current_backend == BackendType::CloudApi {
                                    let mut url = self.api_url.lock().unwrap().clone();
                                    ui.label(egui::RichText::new("URL:").size(11.0).color(egui::Color32::from_rgb(100, 200, 255)));
                                    if ui.add(egui::TextEdit::singleline(&mut url).desired_width(80.0)).changed() {
                                        *self.api_url.lock().unwrap() = url;
                                    }
                                }

                                // 耗时信息 + 复制按钮放右侧
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let text = self.text.lock().unwrap().clone();
                                    if ui.button("复制").clicked() {
                                        ui.output_mut(|o| o.copied_text = text);
                                    }
                                    let ms = *self.elapsed.lock().unwrap();
                                    let interval = *self.interval.lock().unwrap();
                                    if ms > 0 {
                                        ui.label(
                                            egui::RichText::new(format!("耗时{}ms|间隔{}ms", ms, interval))
                                                .color(egui::Color32::GRAY)
                                                .size(10.0),
                                        );
                                    }
                                });
                            });

                            ui.add_space(4.0);

                            let text = self.text.lock().unwrap().clone();
                            egui::ScrollArea::vertical()
                                .max_height(115.0)
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
    let shared_backend = Arc::new(Mutex::new(BackendType::WindowsNative));
    let shared_api_url = Arc::new(Mutex::new(String::from("http://127.0.0.1:8000/ocr")));

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
    let shared_backend_clone = shared_backend.clone();
    let shared_api_url_clone = shared_api_url.clone();

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
            let backend_for_thread = shared_backend_clone.clone();
            let api_url_for_thread = shared_api_url_clone.clone();

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
                        let backend = *backend_for_thread.lock().unwrap();
                        let url = api_url_for_thread.lock().unwrap().clone();
                        let start = std::time::Instant::now();
                        match ocr_region(x, y, w, h, backend, &url) {
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
                selected_backend: shared_backend,
                api_url: shared_api_url,
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