use ipc_channel::ipc::{self, IpcSender};
use log::info;
use openxr::{self as xr};
use std::env;
use xrizer::input::{
    skeletal_input::{ipc::IPCMessage, SkeletalInputActionData, SkeletalInputActionStates},
    Profiles,
};

struct SkeletalInputXr {
    instance: xr::Instance,
    session: xr::Session<xr::headless::Headless>,
    action_data: SkeletalInputActionData,
}

impl SkeletalInputXr {
    fn new() -> Self {
        #[cfg(not(test))]
        let entry = xr::Entry::linked();

        #[cfg(test)]
        let entry =
            unsafe { xr::Entry::from_get_instance_proc_addr(fakexr::get_instance_proc_addr) }
                .unwrap();

        let supported_exts = entry.enumerate_extensions().unwrap();
        let mut exts = xr::ExtensionSet::default();
        exts.mnd_headless = supported_exts.mnd_headless;

        let instance = entry
            .create_instance(
                &xr::ApplicationInfo {
                    application_name: "XRizer Skeletal Input",
                    application_version: 0,
                    ..Default::default()
                },
                &exts,
                &[],
            )
            .unwrap();

        let system_id = instance
            .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
            .unwrap();

        let info = xr::headless::SessionCreateInfo {};

        let (session, ..) =
            unsafe { instance.create_session::<xr::headless::Headless>(system_id, &info) }.unwrap();

        let mut buf = xr::EventDataBuffer::new();
        loop {
            if let Some(xr::Event::SessionStateChanged(state)) =
                instance.poll_event(&mut buf).unwrap()
            {
                if state.state() == xr::SessionState::READY {
                    break;
                }
            }
        }

        info!(
            "OpenXR session state changed: {:?}",
            xr::SessionState::READY
        );
        session
            .begin(xr::ViewConfigurationType::PRIMARY_STEREO)
            .unwrap();
        info!("Began OpenXR session.");

        let left_hand = instance.string_to_path("/user/hand/left").unwrap();
        let right_hand = instance.string_to_path("/user/hand/right").unwrap();

        let actions = SkeletalInputActionData::new(&instance, left_hand, right_hand);
        for profile in Profiles::get().profiles_iter() {
            info!("Loading bindings for profile {}", profile.profile_path());
            const fn constrain<F>(f: F) -> F
            where
                F: for<'a> Fn(&'a str) -> xr::Path,
            {
                f
            }
            let stp = constrain(|s| instance.string_to_path(s).unwrap());
            let bindings = profile.skeletal_input_bindings(&stp);
            let profile = stp(profile.profile_path());
            instance
                .suggest_interaction_profile_bindings(
                    profile,
                    &bindings.binding_iter(&actions.actions).collect::<Vec<_>>(),
                )
                .unwrap();
        }

        session.attach_action_sets(&[&actions.set]).unwrap();

        Self {
            instance,
            session,
            action_data: actions,
        }
    }

    fn get_action_states(&self, subaction: xr::Path) -> SkeletalInputActionStates {
        let actions = &self.action_data.actions;
        SkeletalInputActionStates {
            thumb_touch: actions
                .thumb_touch
                .state(&self.session, subaction)
                .unwrap()
                .current_state,
            index_touch: actions
                .index_touch
                .state(&self.session, subaction)
                .unwrap()
                .current_state,
            index_curl: actions
                .index_curl
                .state(&self.session, subaction)
                .unwrap()
                .current_state,
            rest_curl: actions
                .rest_curl
                .state(&self.session, subaction)
                .unwrap()
                .current_state,
        }
    }

    fn sync_actions(&self) {
        self.session
            .sync_actions(&[xr::ActiveActionSet::new(&self.action_data.set)])
            .unwrap();
    }
}

fn main() {
    init_logging();
    info!("Starting...");
    let server0_name = env::var("IPC").unwrap();
    let server0_tx = IpcSender::connect(server0_name).unwrap();

    let (notify_sender, notify_receiver) = ipc::channel::<IPCMessage>().unwrap();
    let (result_sender, result_receiver) = ipc::channel::<IPCMessage>().unwrap();

    info!("Initiating IPC connection...");
    server0_tx.send((notify_sender, result_receiver)).unwrap();

    info!("Connected! Starting OpenXR...");

    let xr = SkeletalInputXr::new();

    loop {
        if let Ok(message) = notify_receiver.recv() {
            match message {
                IPCMessage::SyncActions => {
                    xr.sync_actions();
                    if result_sender.send(IPCMessage::Ack).is_err() {
                        break;
                    }
                }
                IPCMessage::GetHand(is_left) => {
                    let path = xr
                        .instance
                        .string_to_path(if is_left {
                            "/user/hand/left"
                        } else {
                            "/user/hand/right"
                        })
                        .unwrap();

                    if result_sender
                        .send(IPCMessage::HandData(xr.get_action_states(path)))
                        .is_err()
                    {
                        break;
                    }
                }
                msg => panic!("Unexpected IPC message: {:?}", msg),
            }
        } else {
            break;
        }
    }

    info!("Parent process has disconnected. Exiting...");
}

fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();

    ONCE.call_once(|| {
        let mut builder = env_logger::Builder::new();
        #[allow(unused_mut)]
        let mut startup_err: Option<String> = None;

        #[cfg(not(test))]
        {
            use std::path::Path;

            struct ComboWriter(std::fs::File, std::io::Stderr);

            impl std::io::Write for ComboWriter {
                fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                    let _ = self.0.write(buf)?;
                    self.1.write(buf)
                }

                fn flush(&mut self) -> std::io::Result<()> {
                    self.0.flush()?;
                    self.1.flush()
                }
            }

            let state_dir = std::env::var("XDG_STATE_HOME")
                .or_else(|_| std::env::var("HOME").map(|h| h + "/.local/state"));

            if let Ok(state) = state_dir {
                let path = Path::new(&state).join("xrizer");
                let mut setup = || {
                    let path = path.join("xrizer-skeletal-input.txt");
                    match std::fs::File::create(path) {
                        Ok(file) => {
                            let writer = ComboWriter(file, std::io::stderr());
                            builder.target(env_logger::Target::Pipe(Box::new(writer)));
                        }
                        Err(e) => startup_err = Some(format!("Failed to create log file: {e:?}")),
                    }
                };

                match std::fs::create_dir_all(&path) {
                    Ok(_) => setup(),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => setup(),
                    err => {
                        startup_err = Some(format!(
                            "Failed to create log directory ({path:?}): {err:?}"
                        ))
                    }
                }
            }

            std::panic::set_hook(Box::new(|info| {
                log::error!("{info}");
                let backtrace = std::backtrace::Backtrace::force_capture();
                log::error!("Backtrace: \n{backtrace}");
                std::process::abort();
            }));
        }

        // safety: who cares lol
        unsafe {
            time::util::local_offset::set_soundness(time::util::local_offset::Soundness::Unsound)
        };

        builder
            .filter_level(log::LevelFilter::Info)
            .parse_default_env()
            .is_test(cfg!(test))
            .format(|buf, record| {
                use std::io::Write;
                use time::macros::format_description;

                let style = buf.default_level_style(record.level());
                let now = time::OffsetDateTime::now_local()
                    .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
                let now = now
                    .format(format_description!(
                        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]"
                    ))
                    .unwrap();

                write!(buf, "[{now} {style}{:5}{style:#}", record.level())?;
                if let Some(path) = record.module_path() {
                    write!(buf, " {}", path)?;
                }
                writeln!(buf, " {:?}] {}", std::thread::current().id(), record.args())
            })
            .init();

        if let Some(err) = startup_err {
            log::warn!("{err}");
        }
    });
}

// TODO: It'd be nice to write tests here, but fakexr doesn't appear to work with a headless session yet
