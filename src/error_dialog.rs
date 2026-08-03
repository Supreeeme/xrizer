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

/// Copy via a clipboard manager if one is available - miniquad's clipboard only
/// serves pastes while our window is alive (and is unimplemented on some
/// backends), while wl-copy/xclip/xsel keep the selection around.
fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::Stdio;

    const WAYLAND_TOOLS: &[(&str, &[&str])] = &[("wl-copy", &[])];
    const X11_TOOLS: &[(&str, &[&str])] = &[
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];

    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let tools: Vec<_> = if wayland {
        WAYLAND_TOOLS.iter().chain(X11_TOOLS).collect()
    } else {
        X11_TOOLS.iter().chain(WAYLAND_TOOLS).collect()
    };

    for (cmd, args) in tools {
        let Ok(mut child) = Command::new(cmd)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        let wrote = child
            .stdin
            .take()
            .map(|mut stdin| stdin.write_all(text.as_bytes()).is_ok())
            .unwrap_or(false);
        if wrote && child.wait().is_ok_and(|s| s.success()) {
            return true;
        }
    }

    miniquad::window::clipboard_set(text);
    false
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
                                    let persistent = copy_to_clipboard(&format!(
                                        "{}\n{}",
                                        info.error, info.backtrace
                                    ));
                                    click_start = Some((Instant::now(), persistent));
                                }
                                let id = ui.auto_id_with("success");
                                let mut visible = None;
                                ui.data_mut(|map| {
                                    let click_time =
                                        map.get_temp_mut_or_default::<Option<(Instant, bool)>>(id);

                                    if click_time.is_none() {
                                        *click_time = click_start;
                                    }

                                    if let Some((time, persistent)) = click_time {
                                        if time.elapsed().as_secs() < 3 {
                                            visible = Some(*persistent);
                                        } else {
                                            *click_time = None;
                                        }
                                    }
                                });

                                let label = match visible {
                                    Some(false) => "✅ Copied! (paste before closing this window)",
                                    _ => "✅ Copied!",
                                };
                                ui.add_visible(visible.is_some(), egui::Label::new(label));
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

                        let path = std::path::Path::new(&dir)
                            .join(format!("xrizer/xrizer-{}.txt", std::process::id()));
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
