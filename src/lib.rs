#![deny(clippy::all)]

mod applications;
mod chaperone;
mod clientcore;
mod compositor;
mod extended_display;
mod graphics_backends;
mod input;
mod misc_unknown;
mod openxr_data;
mod overlay;
mod overlayview;
mod rendermodels;
mod screenshots;
mod settings;
mod system;

#[cfg(not(test))]
mod error_dialog;

use clientcore::ClientCore;
use openvr as vr;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
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
                    let path = path.join("xrizer.txt");
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
                error_dialog::dialog(format!("{info}"), backtrace);
                std::process::abort();
            }));
        }

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
#[no_mangle]
pub unsafe extern "C" fn VRClientCoreFactory(
    interface_name: *const c_char,
    return_code: *mut i32,
) -> *mut c_void {
    let interface = unsafe { CStr::from_ptr(interface_name) };
    ClientCore::new(interface)
        .map(|c| {
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
        })
        .unwrap_or(std::ptr::null_mut())
}

/// Needed for Proton, but seems unused.
#[no_mangle]
pub extern "C" fn HmdSystemFactory(
    _interface_name: *const c_char,
    _return_code: *mut i32,
) -> *mut c_void {
    unimplemented!()
}
