use super::tracked_device::{BaseDevice, TrackedDevice};

use log::trace;
use openvr::{self as vr, space_relation_to_openvr_pose};
use openxr as xr;

use crate::{
    input::devices::tracked_device::TrackedDeviceType,
    openxr_data::{OpenXrData, SessionData},
    tracy_span,
};

pub struct XrController {
    base: BaseDevice,

    pub hand_path: &'static str,
    pub subaction_path: xr::Path,
}

impl XrController {
    pub fn new(instance: &xr::Instance, device_type: TrackedDeviceType) -> Self {
        assert!(
            device_type == TrackedDeviceType::LeftHand
                || device_type == TrackedDeviceType::RightHand,
            "Invalid device type \"{}\" for controller",
            device_type
        );

        let hand_path = match device_type {
            TrackedDeviceType::LeftHand => "/user/hand/left",
            TrackedDeviceType::RightHand => "/user/hand/right",
            _ => unreachable!(),
        };

        let subaction_path = instance.string_to_path(hand_path).unwrap();

        Self {
            base: BaseDevice::new(device_type.into(), device_type),
            hand_path,
            subaction_path,
        }
    }
}

impl TrackedDevice for XrController {
    fn get_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        tracy_span!("XrController::get_pose");

        let legacy_actions = session_data.input_data.legacy_actions.get()?;

        let spaces = match self.get_type() {
            TrackedDeviceType::LeftHand => &legacy_actions.left_spaces,
            TrackedDeviceType::RightHand => &legacy_actions.right_spaces,
            _ => return None,
        };

        let (location, velocity) = if let Some(raw) = spaces.try_get_or_init_raw(
            &self.get_interaction_profile(),
            session_data,
            &legacy_actions.actions,
        ) {
            raw.relate(
                session_data.get_space_for_origin(origin),
                xr_data.display_time.get(),
            )
            .ok()?
        } else {
            trace!("Failed to get raw space, returning empty pose");
            (xr::SpaceLocation::default(), xr::SpaceVelocity::default())
        };

        Some(space_relation_to_openvr_pose(location, velocity))
    }

    fn get_base_device(&self) -> &BaseDevice {
        &self.base
    }
}
