#![deny(clippy::all)]

mod applications;
mod chaperone;
mod clientcore;
mod compositor;
mod graphics_backends;
mod input;
mod misc_unknown;
mod openxr_data;
mod overlay;
mod overlayview;
mod quirks;
mod rendermodels;
mod screenshots;
mod settings;
mod system;

#[cfg(not(test))]
mod error_dialog;

use clientcore::ClientCore;
use openvr as vr;
use std::ffi::{CStr, c_char, c_void};
use std::sync::OnceLock;
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicU64, Ordering},
};

macro_rules! warn_unimplemented {
    ($function:literal) => {
        crate::warn_once!("{} unimplemented ({}:{})", $function, file!(), line!());
    };
}
use warn_unimplemented;
macro_rules! warn_once {
    ($literal:literal $(,$($tt:tt)*)?) => {{
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            log::warn!(concat!("[ONCE] ", $literal) $(,$($tt)*)?);
        });
    }}
}
use warn_once;

#[cfg(feature = "tracing")]
macro_rules! tracy_span {
    ($($tt:tt)*) => {
        let _span = tracy_client::span!($($tt)*);
    }
}

#[cfg(not(feature = "tracing"))]
macro_rules! tracy_span {
    ($($tt:tt)*) => {};
}
use tracy_span;

#[cfg(feature = "tracing")]
tracy_client::register_demangler!();

macro_rules! atomic_float {
    ($name:ident, $float:ty, $atomic:ty) => {
        #[derive(Default)]
        struct $name($atomic);

        impl $name {
            fn new(value: $float) -> Self {
                Self(value.to_bits().into())
            }

            #[allow(dead_code)]
            #[inline]
            fn load(&self) -> $float {
                <$float>::from_bits(self.0.load(Ordering::Relaxed))
            }

            #[allow(dead_code)]
            #[inline]
            fn store(&self, value: $float) {
                self.0.store(value.to_bits(), Ordering::Relaxed)
            }

            #[allow(dead_code)]
            #[inline]
            fn swap(&self, value: $float) -> $float {
                <$float>::from_bits(self.0.swap(value.to_bits(), Ordering::Relaxed))
            }
        }

        impl From<$float> for $name {
            fn from(value: $float) -> Self {
                Self::new(value)
            }
        }
    };
}

atomic_float!(AtomicF32, f32, AtomicU32);
atomic_float!(AtomicF64, f64, AtomicU64);

/// Per-process log files older than this get removed on startup.
const LOG_MAX_AGE: std::time::Duration = std::time::Duration::from_hours(48);

/// Remove old per-process log files so the log directory doesn't accumulate
/// one file per xrizer process forever. Files newer than the cutoff are kept,
/// as they may belong to a concurrently running process.
fn prune_old_logs(dir: &std::path::Path, keep_newer_than: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with("xrizer-") || !name.ends_with(".txt") {
            continue;
        }
        let too_old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t < keep_newer_than);
        if too_old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod log_prune_tests {
    use super::prune_old_logs;
    use std::time::{Duration, SystemTime};

    #[test]
    fn prune_only_removes_old_xrizer_logs() {
        let dir = std::env::temp_dir().join(format!("xrizer-prune-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("xrizer-123.txt"), "a").unwrap();
        std::fs::write(dir.join("xrizer-456.txt"), "b").unwrap();
        std::fs::write(dir.join("unrelated.txt"), "c").unwrap();

        // cutoff in the past: nothing is old enough to remove
        prune_old_logs(&dir, SystemTime::now() - Duration::from_secs(60));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 3);

        // cutoff in the future: all xrizer logs count as old
        prune_old_logs(&dir, SystemTime::now() + Duration::from_secs(60));
        let remaining: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(remaining, vec!["unrelated.txt".to_string()]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();

    ONCE.call_once(|| {
        let mut builder = env_logger::Builder::new();
        #[allow(unused_mut)]
        let mut startup_err: Option<String> = None;

        builder.filter_level(log::LevelFilter::Info);
        builder.parse_default_env();

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

            if let Ok(state) = &state_dir {
                let path = Path::new(state).join("xrizer");
                let mut setup = || {
                    // Multiple processes (e.g. a launcher's VR probe and the game itself)
                    // can run concurrently; a shared truncating log file lets one clobber
                    // the other's output, so give each process its own file.
                    let path = path.join(format!("xrizer-{}.txt", std::process::id()));
                    match std::fs::File::create(path) {
                        Ok(file) => {
                            let writer = ComboWriter(file, std::io::stderr());
                            builder.target(env_logger::Target::Pipe(Box::new(writer)));
                        }
                        Err(e) => startup_err = Some(format!("Failed to create log file: {e:?}")),
                    }
                };

                match std::fs::create_dir_all(&path) {
                    Ok(_) => {
                        prune_old_logs(&path, std::time::SystemTime::now() - LOG_MAX_AGE);
                        setup()
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => setup(),
                    err => {
                        startup_err = Some(format!(
                            "Failed to create log directory ({path:?}): {err:?}"
                        ))
                    }
                }
            }

            // Some launch setups don't deliver RUST_LOG to the game process
            // (e.g. launchers that sanitize the environment), so fall back to
            // a filter file next to the logs.
            if std::env::var_os("RUST_LOG").is_none()
                && let Ok(state) = &state_dir
                && let Ok(filter) =
                    std::fs::read_to_string(Path::new(state).join("xrizer/log_filter"))
                && !filter.trim().is_empty()
            {
                builder.parse_filters(filter.trim());
            }

            std::panic::set_hook(Box::new(|info| {
                log::error!("{info}");
                let backtrace = std::backtrace::Backtrace::force_capture();
                log::error!("Backtrace: \n{backtrace}");
                error_dialog::dialog(format!("{info}"), backtrace);
                std::process::abort();
            }));
        }

        builder
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
                    write!(buf, " {path}")?;
                }
                writeln!(buf, " {:?}] {}", std::thread::current().id(), record.args())
            })
            .init();

        let mut version = env!("VERGEN_GIT_DESCRIBE");
        if version == "VERGEN_IDEMPOTENT_OUTPUT" {
            version = env!("CARGO_PKG_VERSION");
        }
        log::info!("Initializing XRizer version {version}");
        if let Some(err) = startup_err {
            log::warn!("{err}");
        }
    });
}

/// # Safety
///
/// interface_name must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn VRClientCoreFactory(
    interface_name: *const c_char,
    return_code: *mut i32,
) -> *mut c_void {
    let interface = unsafe { CStr::from_ptr(interface_name) };

    struct ClientCorePtr(*mut c_void);
    // SAFETY: Vtables are fine to send across threads.
    unsafe impl Send for ClientCorePtr {}
    unsafe impl Sync for ClientCorePtr {}

    static C: OnceLock<ClientCorePtr> = OnceLock::new();
    if C.get().is_none() {
        let ret = ClientCore::new(interface).map(|c| {
            if let Some(ret) = unsafe { return_code.as_mut() } {
                *ret = 0;
            }
            let vtable = match c.base.get().unwrap() {
                clientcore::Vtable::V2(v) => v as *const _ as *const vr::IVRClientCore002 as _,
                clientcore::Vtable::V3(v) => v as *const _ as *const vr::IVRClientCore003 as _,
            };
            // Leak it!
            let _ = Arc::into_raw(c);
            vtable
        });

        if let Some(c) = ret {
            C.set(ClientCorePtr(c)).unwrap_or_else(|_| unreachable!());
        }
    }

    C.get().map(|c| c.0).unwrap_or(std::ptr::null_mut())
}

/// Needed for Proton, but seems unused.
#[unsafe(no_mangle)]
pub extern "C" fn HmdSystemFactory(
    _interface_name: *const c_char,
    _return_code: *mut i32,
) -> *mut c_void {
    unimplemented!()
}
