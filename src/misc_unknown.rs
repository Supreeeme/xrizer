// The interfaces in this file are missing in openvr.h and any other form of OpenVR documentation,
// but are used by games (typically Half Life Alyx.)

use log::debug;
use openvr::InterfaceImpl;
use seq_macro::seq;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub struct UnknownInterfaces {
    mailbox: Wrapper<Mailbox>,
    control_panel: Wrapper<ControlPanel>,
}

impl InterfaceImpl for UnknownInterfaces {
    fn supported_versions() -> &'static [&'static CStr] {
        &[c"IVRMailbox_001", c"IVRControlPanel_006"]
    }

    fn get_version(version: &CStr) -> Option<Box<dyn FnOnce(&Arc<Self>) -> *mut c_void>> {
        #[allow(
            clippy::redundant_guards,
            reason = "https://github.com/rust-lang/rust-clippy/issues/13681"
        )]
        match version {
            x if x == c"IVRMailbox_001" => Some(Box::new(|this| &this.mailbox as *const _ as _)),
            x if x == c"IVRControlPanel_006" => {
                Some(Box::new(|this| &this.control_panel as *const _ as _))
            }
            _ => None,
        }
    }
}

/// Wraps our unknown vtables.
#[repr(C)]
struct Wrapper<T: 'static> {
    vtable: &'static T,
}

impl<T> Default for Wrapper<T>
where
    T: 'static,
    &'static T: Default,
{
    fn default() -> Self {
        Self {
            vtable: Default::default(),
        }
    }
}

const UNKNOWN_TAG: &str = "unknown_interfaces";

macro_rules! gen_vtable {
    (struct $name:ident {
        $(
            fn $fn_name:ident($($arg:ident: $ty:ty),*$(,)?) $(-> $output:ty)? {$($tt:tt)*}
        )+
    }) => {
        #[repr(C)]
        struct $name {
            $(
                $fn_name: extern "C" fn(*mut Wrapper<$name> $(,$ty)*) $(-> $output)?
            ),*
        }

        impl $name {
            $(
            extern "C" fn $fn_name(_: *mut Wrapper<$name> $(,$arg:$ty)*) $(-> $output)? {$($tt)*}
            )*
        }

        impl Default for &'static $name {
            fn default() -> Self {
                &$name {
                    $(
                        $fn_name: $name::$fn_name
                    ),*
                }
            }
        }
    }
}

#[repr(transparent)]
#[derive(Debug)]
struct MailboxHandle(u64);

// IVRMailbox - used by Alyx, only seems to be documnted here:
// https://github.com/ValveSoftware/Proton/blob/proton_9.0/vrclient_x64/openvr_v1.10.30/openvr.h#L5108
// This implementation is adapted from OpenComposite:
// https://gitlab.com/znixian/OpenOVR/-/blob/34311dabf430d6051d7e97f6081842a5394d2a67/OpenOVR/Reimpl/BaseMailbox.cpp
gen_vtable! {
    struct Mailbox {
        fn undoc1(a: *const c_char, b: *mut MailboxHandle) -> c_int {
            let a = unsafe { CStr::from_ptr(a) };
            debug!(target: UNKNOWN_TAG, "Entered IVRMailbox::undoc1 with arguments a: {a:?}, b: {b:?}");
            unsafe { *b = MailboxHandle(24) };
            0
        }
        fn undoc2(handle: MailboxHandle) -> c_int {
            debug!(target: UNKNOWN_TAG, "Entered IVRMailbox::undoc2 with arugments handle: {handle:?}");
            0
        }
        fn undoc3(
            handle: MailboxHandle,
            a: *const c_char,
            b: *const c_char,
        ) -> c_int {
            let a = if a.is_null() {
                None
            } else {
                Some(unsafe { CStr::from_ptr(a) })
            };
            let b = if b.is_null() {
                None
            } else {
                Some(unsafe { CStr::from_ptr(b) })
            };
            debug!(target: UNKNOWN_TAG, "Entered IVRMailbox::undoc3 with arguments handle: {handle:?}, a: {a:?}, b: {b:?}");
            0
        }
        // Borrowed from OpenComposite
        fn undoc4(
            handle: MailboxHandle,
            out_buf: *mut c_char,
            out_len: u32,
            len: *mut u32,
        ) -> c_int {
            debug!(target: UNKNOWN_TAG, "Entered IVRMailbox::undoc4 with arguments handle: {handle:?}, out_buf: {out_buf:?}, out_len: {out_len:?}, c: {len:?}");

            static RECEIVED_MESSAGE: AtomicBool = AtomicBool::new(false);
            if !RECEIVED_MESSAGE.load(Ordering::Relaxed) {
                RECEIVED_MESSAGE.store(true, Ordering::Relaxed);

                let msg = cr#"{"type": "ready"}"#;
                let msg_len = msg.count_bytes() as u32 + 1;
                unsafe {
                    *len.as_mut().unwrap() = msg_len;
                }

                if out_len < msg_len {
                    return 2;
                }

                unsafe {
                    out_buf.copy_from(msg.as_ptr(), msg_len as usize);
                }

                debug!(target: UNKNOWN_TAG, "Sent ready message: {msg:?}");

                0
            } else {
                1
            }
        }
    }
}

// IVRControlPanel - used by Alyx.
// Without this interface, Alyx complains about "No response from Mongoose".
seq!(N in 0..=25 {
    gen_vtable!(struct ControlPanel {
        #(fn unknown_func_~N() {
            debug!(target: UNKNOWN_TAG, "Entered ControlPanel::unknown_func_{}", stringify!(N));
        })*
        // This function gets called at startup
        fn unknown_func_26() -> c_int {
            debug!(target: UNKNOWN_TAG, "Entered ControlPanel::unknown_func_26");
            0
        }
    });
});
