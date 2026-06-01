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
    let bgra: Vec<u8> = resized_img
        .as_raw()
        .chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();

    let bitmap = SoftwareBitmap::CreateWithAlpha(
        BitmapPixelFormat::Bgra8,
        scaled_width as i32,
        scaled_height as i32,
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

// ─── 选区窗口 ────────────────────────────────────────────

struct SelectorApp {
    screenshot_texture: Option<egui::TextureHandle>,
    screenshot_rgba: Vec<u8>,
    screenshot_width: u32,
    screenshot_height: u32,
    drag_start: Option<egui::Pos2>,
    drag_end: Option<egui::Pos2>,
    done: bool,
    result: Option<(i32, i32, u32, u32)>,
}

impl SelectorApp {
    fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            screenshot_texture: None,
            screenshot_rgba: rgba,
            screenshot_width: width,
            screenshot_height: height,
            drag_start: None,
            drag_end: None,
            done: false,
            result: None,
        }
    }
}

impl eframe::App for SelectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.screenshot_texture.is_none() {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [self.screenshot_width as usize, self.screenshot_height as usize],
                &self.screenshot_rgba,
            );
            self.screenshot_texture = Some(ctx.load_texture(
                "screenshot",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

                if let Some(tex) = &self.screenshot_texture {
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }

                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 80),
                );

                ui.painter().text(
                    egui::pos2(rect.width() / 2.0, 30.0),
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
                        sel.min + egui::vec2(4.0, 4.0),
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
                            self.result = Some((
                                sel.min.x as i32,
                                sel.min.y as i32,
                                sel.width() as u32,
                                sel.height() as u32,
                            ));
                            self.done = true;
                        }
                    }
                }

                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.done = true;
                }
            });

        if self.done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

struct SelectorWrapper {
    inner: SelectorApp,
    result_out: Arc<Mutex<Option<(i32, i32, u32, u32)>>>,
}

impl eframe::App for SelectorWrapper {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
        if self.inner.done {
            if let Some(r) = self.inner.result {
                *self.result_out.lock().unwrap() = Some(r);
            }
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

fn select_region() -> Option<(i32, i32, u32, u32)> {
    let screens = Screen::all().ok()?;

    // 打印屏幕信息方便调试
    for s in &screens {
        let info = s.display_info;
        println!(
            "屏幕: id={} x={} y={} width={} height={} scale={}",
            info.id, info.x, info.y, info.width, info.height, info.scale_factor
        );
    }

    // 获取当前鼠标位置，找到鼠标所在的屏幕
    let (mx, my) = get_cursor_pos().unwrap_or((0, 0));
    println!("鼠标当前位置: ({}, {})", mx, my);

    let active_screen = screens
        .iter()
        .find(|s| {
            let info = s.display_info;
            mx >= info.x
                && mx < info.x + info.width as i32
                && my >= info.y
                && my < info.y + info.height as i32
        })
        .unwrap_or(&screens[0]);

    let info = active_screen.display_info;
    println!(
        "激活屏幕: id={} x={} y={} width={} height={} scale={}",
        info.id, info.x, info.y, info.width, info.height, info.scale_factor
    );

    // 仅截取激活屏幕的内容
    let image = active_screen.capture().ok()?;
    let canvas = image.as_raw().to_vec();
    let total_width = info.width;
    let total_height = info.height;

    let result = Arc::new(Mutex::new(None));
    let result_clone = result.clone();

    let scale = info.scale_factor;
    // egui 用逻辑像素，物理像素除以缩放比得到逻辑尺寸
    let logical_width = total_width as f32 / scale;
    let logical_height = total_height as f32 / scale;
    let logical_x = info.x as f32 / scale;
    let logical_y = info.y as f32 / scale;

    println!(
        "激活屏幕逻辑尺寸: {}x{} 逻辑位置: ({}, {}) scale={}",
        logical_width, logical_height, logical_x, logical_y, scale
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([logical_width, logical_height])
            .with_position([logical_x, logical_y])
            .with_decorations(false)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "miaocr-selector",
        options,
        Box::new(move |cc| {
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

            Box::new(SelectorWrapper {
                inner: SelectorApp::new(canvas, total_width, total_height),
                result_out: result_clone,
            }) as Box<dyn eframe::App>
        }),
    )
    .ok()?;

    // 选区坐标是逻辑像素，转回物理像素
    let x = result.lock().ok()?.take();
    x.map(|(rx, ry, rw, rh)| {
        (
            (rx as f32 * scale) as i32 + info.x,
            (ry as f32 * scale) as i32 + info.y,
            (rw as f32 * scale) as u32,
            (rh as f32 * scale) as u32,
        )
    })
}

// ─── 结果悬浮窗 ──────────────────────────────────────────

struct ResultApp {
    text: Arc<Mutex<String>>,
    elapsed: Arc<Mutex<u128>>,
}

impl eframe::App for ResultApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 每500ms刷新一次
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("miaocr");
                let ms = *self.elapsed.lock().unwrap();
                if ms > 0 {
                    ui.label(
                        egui::RichText::new(format!("{}ms", ms))
                            .color(egui::Color32::GRAY)
                            .size(12.0),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = self.text.lock().unwrap().clone();
                    if ui.button("复制").clicked() {
                        ui.output_mut(|o| o.copied_text = text);
                    }
                });
            });

            ui.separator();

            let text = self.text.lock().unwrap().clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut text.as_str())
                        .desired_width(f32::INFINITY)
                        .font(egui::FontId::proportional(15.0)),
                );
            });
        });
    }
}

// ─── 主函数 ──────────────────────────────────────────────

fn main() -> Result<()> {
    // 1. 选区
    let region = select_region();
    let (x, y, w, h) = match region {
        None => {
            println!("未选择区域，退出");
            return Ok(());
        }
        Some(r) => r,
    };

    println!("识别区域: {}x{}+{}+{}", w, h, x, y);

    // 2. 共享状态
    let shared_text = Arc::new(Mutex::new(String::from("识别中...")));
    let shared_elapsed = Arc::new(Mutex::new(0u128));

    let text_for_thread = shared_text.clone();
    let elapsed_for_thread = shared_elapsed.clone();

    // 3. 后台识别线程
    std::thread::spawn(move || loop {
        let start = std::time::Instant::now();
        match ocr_region(x, y, w, h) {
            Ok(text) => {
                let ms = start.elapsed().as_millis();
                *text_for_thread.lock().unwrap() = text;
                *elapsed_for_thread.lock().unwrap() = ms;
            }
            Err(e) => {
                *text_for_thread.lock().unwrap() = format!("识别失败: {}", e);
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    });

    // 4. 结果悬浮窗
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_position([20.0, 20.0])
            .with_title("miaocr")
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "miaocr",
        options,
        Box::new(move |cc| {
            // 加载中文字体
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

            Box::new(ResultApp {
                text: shared_text,
                elapsed: shared_elapsed,
            }) as Box<dyn eframe::App>
        }),
    )
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}