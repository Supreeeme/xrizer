// Interfaces not available in openvr.h that are used by specific games.
// Alyx-era stubs live in `alyx`, legacy game stubs in `legacy`.

mod alyx;
mod legacy;

use alyx::{ControlPanel, Mailbox};
use legacy::{ChaperoneSetup, DriverManager, ExtendedDisplay, Resources, TrackedCamera};
use log::info;
use openvr::InterfaceImpl;
use std::ffi::{CStr, c_void};
use std::sync::Arc;

#[derive(Default)]
pub struct UnknownInterfaces {
    mailbox: Wrapper<Mailbox>,
    control_panel: Wrapper<ControlPanel>,
    chaperone_setup: Wrapper<ChaperoneSetup>,
    extended_display: Wrapper<ExtendedDisplay>,
    tracked_camera: Wrapper<TrackedCamera>,
    resources: Wrapper<Resources>,
    driver_manager: Wrapper<DriverManager>,
}

impl InterfaceImpl for UnknownInterfaces {
    fn supported_versions() -> &'static [&'static CStr] {
        &[
            c"IVRMailbox_001",
            c"IVRControlPanel_006",
            c"IVRChaperoneSetup_005",
            c"IVRExtendedDisplay_001",
            c"IVRTrackedCamera_003",
            c"IVRResources_001",
            c"IVRDriverManager_001",
        ]
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
            x if x == c"IVRChaperoneSetup_005" => Some(Box::new(|this| {
                info!(target: UNKNOWN_TAG, "providing legacy interface IVRChaperoneSetup_005");
                &this.chaperone_setup as *const _ as _
            })),
            x if x == c"IVRExtendedDisplay_001" => Some(Box::new(|this| {
                info!(target: UNKNOWN_TAG, "providing legacy interface IVRExtendedDisplay_001");
                &this.extended_display as *const _ as _
            })),
            x if x == c"IVRTrackedCamera_003" => Some(Box::new(|this| {
                info!(target: UNKNOWN_TAG, "providing legacy interface IVRTrackedCamera_003");
                &this.tracked_camera as *const _ as _
            })),
            x if x == c"IVRResources_001" => Some(Box::new(|this| {
                info!(target: UNKNOWN_TAG, "providing legacy interface IVRResources_001");
                &this.resources as *const _ as _
            })),
            x if x == c"IVRDriverManager_001" => Some(Box::new(|this| {
                info!(target: UNKNOWN_TAG, "providing legacy interface IVRDriverManager_001");
                &this.driver_manager as *const _ as _
            })),
            _ => None,
        }
    }
}

/// Wraps a legacy vtable struct so its address is used as the interface pointer.
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
