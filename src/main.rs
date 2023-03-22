// This disables console output. remove this for debugging until another logging method is
// implemented
#![windows_subsystem = "windows"]

use std::{
    path::PathBuf,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, RwLock,
    },
};

use eframe::{epaint::Shadow, NativeOptions};
use egui::{
    text::CCursor, text_edit::CCursorRange, Color32, FontFamily, FontId, Frame, Key, Margin, Pos2,
    Rgba, ScrollArea, Separator, TextEdit, Vec2,
};
use serde::{Deserialize, Serialize};
use windows_hotkeys::{
    keys::{ModKey, VKey},
    HotkeyManager,
};

use popup_gpt::{chatgpt::ChatGPT, model::CompletionResponse};

const IN_FONT: FontId = FontId {
    size: 16.0,
    family: FontFamily::Monospace,
};

const OUT_FONT: FontId = FontId {
    size: 16.0,
    family: FontFamily::Monospace,
};

// Todo: Either remove the dead code or actually use the full response mode
#[allow(dead_code)]
enum GUIMsg {
    CompletionResponse(CompletionResponse),
    PartialCompletionResponse(CompletionResponse),
    Flush,
}
unsafe impl Send for GUIMsg {}

struct App {
    settings: Settings,

    // UI State
    prompt: String,
    response: String,
    response_render_len: usize,
    loading: bool,
    focus_input: bool,

    com: (Sender<GUIMsg>, Receiver<GUIMsg>),
    hotkey_mgr: HotkeyManager<()>,
    chatgpt: Arc<RwLock<ChatGPT>>,

    window_handle: u64,

    // Window moving / scaling helpers
    window_scale_direction: Vec2,
    window_pointer_offset: Vec2,
}

impl App {
    fn new(settings: Settings) -> Self {
        let mut hkm = HotkeyManager::new();
        hkm.register(VKey::K, &[ModKey::Ctrl, ModKey::Alt], || {})
            .unwrap();

        let chatgpt = ChatGPT::new(settings.openai_token.clone());
        let chatgpt = Arc::new(RwLock::new(chatgpt));

        let com = channel();

        Self {
            settings,
            chatgpt,
            hotkey_mgr: hkm,
            com,
            focus_input: true,
            loading: false,
            prompt: String::new(),
            response: String::new(),
            response_render_len: 0,
            window_handle: 0,
            window_scale_direction: Vec2::ZERO,
            window_pointer_offset: Vec2::ZERO,
        }
    }

    fn show_window(&mut self, shown: bool) {
        use winapi::um::winuser::GetActiveWindow;
        use winapi::um::winuser::{ShowWindow, SW_HIDE, SW_SHOW};

        if self.window_handle == 0 {
            self.window_handle = unsafe { GetActiveWindow() as u64 };
        }

        if self.window_handle != 0 {
            let cmd_show = match shown {
                false => SW_HIDE,
                true => SW_SHOW,
            };
            unsafe { ShowWindow(self.window_handle as _, cmd_show) };
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        match self.com.1.try_recv() {
            Ok(GUIMsg::CompletionResponse(resp)) if self.loading => {
                self.response = resp.primary_response().unwrap().to_string();
                self.loading = false;
            }
            Ok(GUIMsg::PartialCompletionResponse(resp)) if self.loading => {
                if let Some(delta) = resp
                    .choices
                    .first()
                    .unwrap()
                    .delta
                    .as_ref()
                    .map(|delta| delta.content.as_ref())
                    .flatten()
                {
                    self.response.push_str(delta);
                    ctx.request_repaint();
                }
            }
            Ok(GUIMsg::Flush) if self.loading => {
                self.loading = false;
            }
            _ => (),
        }

        if self.response_render_len + 1 < self.response.len() {
            self.response_render_len += 1;
            while !self.response.is_char_boundary(self.response_render_len) {
                self.response_render_len += 1;
            }
            ctx.request_repaint();
        }

        egui::CentralPanel::default()
            .frame(Frame {
                inner_margin: Margin::same(10.0),
                outer_margin: Margin::same(20.0),
                fill: Color32::from_rgba_unmultiplied(50, 54, 62, 230),
                rounding: egui::Rounding::same(5.0),
                shadow: Shadow::small_light(),
                ..Default::default()
            })
            .show(ctx, |ui| {
                let prompt_input = TextEdit::singleline(&mut self.prompt)
                    .font(IN_FONT)
                    .margin(Vec2::new(0.0, 0.0))
                    .text_color(Color32::from_gray(255))
                    .lock_focus(true)
                    .frame(false);

                let prompt_input = ui.add_sized(
                    Vec2 {
                        y: 20.0,
                        ..ui.available_size()
                    },
                    prompt_input,
                );

                if self.focus_input {
                    self.focus_input = false;

                    let mut state = TextEdit::load_state(ctx, prompt_input.id).unwrap();
                    state.set_ccursor_range(Some(CCursorRange::two(
                        CCursor::new(0),
                        CCursor::new(self.prompt.chars().count()),
                    )));
                    TextEdit::store_state(ctx, prompt_input.id, state);

                    prompt_input.request_focus();
                }

                ui.add(Separator::default());

                let mut response = &self.response[..self.response_render_len];
                let out = TextEdit::multiline(&mut response)
                    .font(OUT_FONT)
                    .margin(Vec2::new(0.0, 0.0))
                    .text_color(Color32::from_rgb(180, 180, 190))
                    .frame(false);

                ScrollArea::new([false, true])
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .always_show_scroll(true)
                    .show(ui, |ui| {
                        ui.add_sized(
                            Vec2 {
                                ..ui.available_size()
                            },
                            out,
                        );
                    });
            });

        ctx.input(|inp| {
            if inp.key_down(Key::Enter) {
                if !self.loading {
                    self.loading = true;
                    self.response.clear();
                    self.response_render_len = 0;

                    let prompt = self.prompt.clone();
                    let chatgpt = Arc::clone(&self.chatgpt);
                    let (tx_stream, rx_stream) = channel();
                    let sender = self.com.0.clone();
                    let ctx = ctx.clone();

                    std::thread::spawn(move || {
                        let _resp = chatgpt
                            .write()
                            .unwrap()
                            .ask_stream(prompt, tx_stream)
                            .unwrap();
                        sender.send(GUIMsg::Flush).unwrap();
                    });

                    let sender = self.com.0.clone();
                    std::thread::spawn(move || {
                        while let Ok(resp) = rx_stream.recv() {
                            sender
                                .send(GUIMsg::PartialCompletionResponse(resp))
                                .unwrap();
                            ctx.request_repaint();
                        }
                    });
                }
            }

            if inp.key_pressed(Key::Escape) {
                self.show_window(false);

                // Wait for hotkey
                self.hotkey_mgr.handle_hotkey();

                self.focus_input = true;

                // Start a new conversation
                self.prompt.clear();
                self.chatgpt.write().unwrap().clear_conversation();

                self.show_window(true);
            }

            if inp.modifiers.alt {
                let size = frame.info().window_info.size;
                let pos = frame.info().window_info.position.unwrap();

                // Move Window
                frame.drag_window();

                // Scale Window
                if inp.pointer.secondary_pressed() {
                    let point = inp.pointer.press_origin().unwrap();
                    self.window_scale_direction.x = if point.x > size.x / 2.0 { 1.0 } else { -1.0 };
                    self.window_scale_direction.y = if point.y > size.y / 2.0 { 1.0 } else { -1.0 };
                }

                if inp.pointer.secondary_down() {
                    let mut pos_delta = Vec2::ZERO;
                    let mut size_delta = inp.pointer.delta();

                    if size_delta != Vec2::ZERO {
                        // Compensate for false offsets cause by moving the window
                        size_delta += self.window_pointer_offset;

                        if self.window_scale_direction.x < 0.0 {
                            pos_delta.x += size_delta.x;
                            size_delta.x *= self.window_scale_direction.x;
                        }

                        if self.window_scale_direction.y < 0.0 {
                            pos_delta.y += size_delta.y;
                            size_delta.y *= self.window_scale_direction.y;
                        }

                        frame.set_window_size(size + size_delta);
                        frame.set_window_pos(pos + pos_delta);

                        self.window_pointer_offset = pos_delta;
                    }
                }

                if inp.pointer.secondary_released() {
                    self.window_pointer_offset = Vec2::ZERO;

                    self.settings.window_pos_x = Some(pos.x);
                    self.settings.window_pos_y = Some(pos.y);
                    self.settings.window_size_x = Some(size.x);
                    self.settings.window_size_y = Some(size.y);

                    std::fs::write(
                        &self.settings.file_location,
                        serde_json::to_string_pretty(&self.settings).unwrap(),
                    )
                    .unwrap();
                }
            }
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    #[serde(skip)]
    file_location: PathBuf,
    openai_token: String,
    window_pos_x: Option<f32>,
    window_pos_y: Option<f32>,
    window_size_x: Option<f32>,
    window_size_y: Option<f32>,
}

fn main() {
    let settings_dir = dirs::config_dir().unwrap().join("popup-gpt");
    if !settings_dir.exists() {
        std::fs::create_dir(&settings_dir).unwrap();
    }
    let settings_path = settings_dir.join("popup-gpt.json");

    let settings = std::fs::read_to_string(&settings_path).unwrap();
    let mut settings: Settings = serde_json::from_str(&settings).unwrap();
    settings.file_location = settings_path;

    let mut opts = NativeOptions {
        always_on_top: true,
        decorated: false,
        drag_and_drop_support: true,
        resizable: false,
        transparent: true,
        vsync: true,
        centered: true,
        ..Default::default()
    };

    match (settings.window_pos_x, settings.window_pos_y) {
        (Some(x), Some(y)) => {
            opts.initial_window_pos = Some(Pos2::new(x, y));
            opts.centered = false;
        }
        _ => (),
    }
    match (settings.window_size_x, settings.window_size_y) {
        (Some(x), Some(y)) => opts.initial_window_size = Some(Vec2::new(x, y)),
        _ => opts.initial_window_size = Some(Vec2::new(800.0, 300.0)),
    }

    eframe::run_native(
        "Popup-GPT",
        opts,
        Box::new(|_cc| Box::new(App::new(settings))),
    )
    .unwrap();
}
