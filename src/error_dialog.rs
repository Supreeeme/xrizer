use egui::{
    Align, Color32, FontSelection, RichText, collapsing_header::CollapsingState, text::LayoutJob,
};
use egui_miniquad::EguiMq;
use miniquad::{EventHandler, GlContext, PassAction, RenderingBackend, conf::Conf};
use std::backtrace::Backtrace;
use std::process::Command;
use std::time::Instant;

pub fn dialog(error: String, backtrace: Backtrace) {
    let r = std::panic::catch_unwind(|| {
        miniquad::start(
            Conf {
                window_title: "xrizer error".to_string(),
                high_dpi: true,
                window_width: 400,
                window_height: 200,
                ..Default::default()
            },
            || Box::new(Dialog::new(error, backtrace)),
        )
    });
    if let Err(e) = r {
        log::error!("Error dialog panicked: {e:?}");
    }
}

fn ui(ctx: &egui::Context, info: &ErrorInfo) {
    egui::TopBottomPanel::top("header").show(ctx, |ui| {
        ui.centered_and_justified(|ui| {
            let mut job = LayoutJob::default();

            RichText::new("❌ ")
                .color(Color32::RED)
                .size(20.)
                .strong()
                .append_to(&mut job, ui.style(), FontSelection::Default, Align::Center);
            RichText::new("xrizer has crashed!")
                .heading()
                .strong()
                .append_to(&mut job, ui.style(), FontSelection::Default, Align::Center);

            ui.label(job);
        });
    });
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label("Error info:");
                ui.code(&info.error);
                let id = ui.next_auto_id();
                ui.vertical(|ui| {
                    CollapsingState::load_with_default_open(ui.ctx(), id, false)
                        .show_header(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Backtrace");
                                let mut click_start = None;
                                if ui.button("Copy to clipboard").clicked() {
                                    miniquad::window::clipboard_set(&format!("{}", info.backtrace));
                                    click_start = Some(Instant::now());
                                }
                                let id = ui.auto_id_with("success");
                                let mut visible = false;
                                ui.data_mut(|map| {
                                    let click_time =
                                        map.get_temp_mut_or_default::<Option<Instant>>(id);

                                    if click_time.is_none() {
                                        *click_time = click_start;
                                    }

                                    if let Some(time) = click_time {
                                        if time.elapsed().as_secs() < 1 {
                                            visible = true;
                                        } else {
                                            *click_time = None;
                                        }
                                    }
                                });

                                ui.add_visible(visible, egui::Label::new("✅ Copied!"));
                            });
                        })
                        .body(|ui| {
                            ui.code(format!("{}", info.backtrace));
                        });
                });

                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        miniquad::window::order_quit();
                    }
                    if ui.button("Open log file").clicked() {
                        let dir = std::env::var("XDG_STATE_HOME").unwrap_or_else(|_| {
                            format!("{}/.local/state", std::env::var("HOME").unwrap())
                        });

                        let path = std::path::Path::new(&dir).join("xrizer/xrizer.txt");
                        let _ = Command::new("xdg-open").arg(path).spawn();
                    }
                    if ui.button("Report on GitHub").clicked() {
                        let _ = webbrowser::open("https://github.com/Supreeeme/xrizer/issues/new?template=bug_report.yaml");
                    }
                })
            })
        })
    });
}

struct Dialog {
    egui_mq: EguiMq,
    mq: GlContext,
    info: ErrorInfo,
}

struct ErrorInfo {
    error: String,
    backtrace: Backtrace,
}

impl Dialog {
    fn new(error: String, backtrace: Backtrace) -> Self {
        let mut mq = GlContext::new();
        let egui_mq = EguiMq::new(&mut mq);
        println!("{}", miniquad::window::dpi_scale());
        egui_mq
            .egui_ctx()
            .set_pixels_per_point(miniquad::window::dpi_scale());
        Self {
            egui_mq,
            mq,
            info: ErrorInfo { error, backtrace },
        }
    }
}

impl EventHandler for Dialog {
    fn update(&mut self) {}
    fn draw(&mut self) {
        self.mq
            .begin_default_pass(PassAction::clear_color(0.0, 0.0, 0.0, 1.0));
        self.mq.end_render_pass();

        self.egui_mq.run(&mut self.mq, |_, ctx| {
            ui(ctx, &self.info);
        });
        self.egui_mq.draw(&mut self.mq);
        self.mq.commit_frame();
    }

    // boilerplate
    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.egui_mq.mouse_motion_event(x, y);
    }

    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        self.egui_mq.mouse_wheel_event(x, y);
    }

    fn mouse_button_down_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        self.egui_mq.mouse_button_down_event(button, x, y);
    }

    fn mouse_button_up_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        self.egui_mq.mouse_button_up_event(button, x, y);
    }

    fn char_event(&mut self, character: char, _: miniquad::KeyMods, _: bool) {
        self.egui_mq.char_event(character);
    }

    fn key_down_event(&mut self, keycode: miniquad::KeyCode, keymods: miniquad::KeyMods, _: bool) {
        self.egui_mq.key_down_event(keycode, keymods);
    }

    fn key_up_event(&mut self, keycode: miniquad::KeyCode, keymods: miniquad::KeyMods) {
        self.egui_mq.key_up_event(keycode, keymods);
    }
}
