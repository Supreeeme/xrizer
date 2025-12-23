use std::ffi::CStr;

use crate::input::legacy::button_mask_from_id;
use crate::{button_mask_from_ids, input::profiles::Property};
use openvr::EVRButtonId::{ApplicationMenu, Axis0, Axis1, Axis2, Grip, System, A};

pub mod hp_motion_controller;
pub mod ms_motion_controller;
pub mod samsung_odyssey_controller;

const TRACKING_SYSTEM_NAME: &'static CStr = c"holographic";
const SERIAL_NUMBER: Property<&'static CStr> = Property::PerHand {
    left: c"MRSOURCE0",
    right: c"MRSOURCE1",
};
const OG_OPENVR_CONTROLLER_TYPE: &'static CStr = c"holographic_controller";
const OG_LEGACY_BUTTONS_MASK: u64 =
    button_mask_from_ids!(System, ApplicationMenu, Grip, Axis0, Axis1, Axis2, A);
