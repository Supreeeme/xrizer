//! A desktop text entry popup used to service OpenVR's `ShowKeyboard` requests.

use egui::{Key, TextEdit};
use egui_miniquad::EguiMq;
use miniquad::{EventHandler, GlContext, PassAction, RenderingBackend, conf::Conf};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct KeyboardRequest {
    pub description: String,
    pub existing_text: String,
    pub char_max: u32,
    pub multiline: bool,
    pub password: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyboardEventKind {
    CharInput(String),
    Done,
    Closed,
}

#[derive(Debug)]
pub struct KeyboardEvent {
    pub kind: KeyboardEventKind,
    /// Overlay the keyboard was opened for, or 0 for the scene application.
    pub overlay: u64,
    pub user_value: u64,
}

#[derive(Default)]
pub struct KeyboardState {
    pub active: bool,
    pub text: String,
    pub events: VecDeque<KeyboardEvent>,
}

/// Splits text into chunks that fit in `VREvent_Keyboard_t::cNewInput` (7 bytes + NUL),
/// never splitting a UTF-8 character.
fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if cur.len() + c.len_utf8() > 7 {
            chunks.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// Shows the popup on a background thread. When it finishes, `state` is updated and
/// the relevant keyboard events are queued.
pub fn show(
    state: Arc<Mutex<KeyboardState>>,
    request: KeyboardRequest,
    minimal: bool,
    overlay: u64,
    user_value: u64,
) {
    let result = Arc::new(Mutex::new(None));
    let result_clone = result.clone();
    let spawned = std::thread::Builder::new()
        .name("xrizer-keyboard".into())
        .spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                miniquad::start(
                    Conf {
                        window_title: "xrizer keyboard".to_string(),
                        high_dpi: true,
                        window_width: 450,
                        window_height: if request.multiline { 300 } else { 160 },
                        ..Default::default()
                    },
                    move || Box::new(Dialog::new(request, result_clone)),
                )
            }));
            if let Err(e) = r {
                log::error!("Keyboard dialog panicked: {e:?}");
            }

            let text: Option<String> = result.lock().unwrap().take();
            let mut state = state.lock().unwrap();
            state.active = false;
            if let Some(text) = text {
                if minimal {
                    for chunk in chunk_text(&text) {
                        state.events.push_back(KeyboardEvent {
                            kind: KeyboardEventKind::CharInput(chunk),
                            overlay,
                            user_value,
                        });
                    }
                }
                state.text = text;
                state.events.push_back(KeyboardEvent {
                    kind: KeyboardEventKind::Done,
                    overlay,
                    user_value,
                });
            }
            state.events.push_back(KeyboardEvent {
                kind: KeyboardEventKind::Closed,
                overlay,
                user_value,
            });
        });
    if let Err(e) = spawned {
        log::error!("Failed to spawn keyboard thread: {e}");
    }
}

struct Dialog {
    egui_mq: Option<EguiMq>,
    mq: GlContext,
    form: Form,
}

struct Form {
    request: KeyboardRequest,
    text: String,
    first_frame: bool,
    result: Arc<Mutex<Option<String>>>,
}

impl Dialog {
    fn new(request: KeyboardRequest, result: Arc<Mutex<Option<String>>>) -> Self {
        let mut mq = GlContext::new();
        let egui_mq = EguiMq::new(&mut mq);
        egui_mq
            .egui_ctx()
            .set_pixels_per_point(miniquad::window::dpi_scale());
        let text = request.existing_text.clone();
        Self {
            egui_mq: Some(egui_mq),
            mq,
            form: Form {
                request,
                text,
                first_frame: true,
                result,
            },
        }
    }
}

impl Form {
    fn submit(&self) {
        *self.result.lock().unwrap() = Some(self.text.clone());
        miniquad::window::order_quit();
    }

    fn ui(&mut self, ctx: &egui::Context) {
        let mut submit = false;
        let mut cancel = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.request.description.is_empty() {
                ui.label(&self.request.description);
            }
            let edit = if self.request.multiline {
                TextEdit::multiline(&mut self.text).desired_rows(6)
            } else {
                TextEdit::singleline(&mut self.text)
            }
            .password(self.request.password)
            .desired_width(f32::INFINITY);
            let response = ui.add(edit);
            if self.first_frame {
                response.request_focus();
                self.first_frame = false;
            }
            if self.request.char_max > 0 {
                let max = self.request.char_max as usize;
                if self.text.chars().count() > max {
                    self.text = self.text.chars().take(max).collect();
                }
            }

            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if enter && !self.request.multiline {
                submit = true;
            }
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    submit = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if submit {
            self.submit();
        } else if cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            miniquad::window::order_quit();
        }
    }
}

impl EventHandler for Dialog {
    fn update(&mut self) {}
    fn draw(&mut self) {
        self.mq
            .begin_default_pass(PassAction::clear_color(0.0, 0.0, 0.0, 1.0));
        self.mq.end_render_pass();

        // egui_mq borrows self mutably, so move the UI state out for the closure.
        let mut egui_mq = self.egui_mq.take().unwrap();
        egui_mq.run(&mut self.mq, |_, ctx| self.form.ui(ctx));
        egui_mq.draw(&mut self.mq);
        self.egui_mq = Some(egui_mq);
        self.mq.commit_frame();
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.egui_mq.as_mut().unwrap().mouse_motion_event(x, y);
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        self.egui_mq.as_mut().unwrap().mouse_wheel_event(x, y);
    }
    fn mouse_button_down_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        self.egui_mq.as_mut().unwrap().mouse_button_down_event(button, x, y);
    }
    fn mouse_button_up_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        self.egui_mq.as_mut().unwrap().mouse_button_up_event(button, x, y);
    }
    fn char_event(&mut self, character: char, _: miniquad::KeyMods, _: bool) {
        self.egui_mq.as_mut().unwrap().char_event(character);
    }
    fn key_down_event(&mut self, keycode: miniquad::KeyCode, keymods: miniquad::KeyMods, _: bool) {
        self.egui_mq.as_mut().unwrap().key_down_event(keycode, keymods);
    }
    fn key_up_event(&mut self, keycode: miniquad::KeyCode, keymods: miniquad::KeyMods) {
        self.egui_mq.as_mut().unwrap().key_up_event(keycode, keymods);
    }
}

#[cfg(test)]
mod tests {
    use super::chunk_text;

    #[test]
    fn chunks_respect_utf8() {
        assert_eq!(chunk_text("hello world"), ["hello w", "orld"]);
        assert_eq!(chunk_text("ééééé"), ["ééé", "éé"]);
        assert!(chunk_text("").is_empty());
    }
}
