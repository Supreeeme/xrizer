use openvr::{space_relation_to_openvr_pose, TrackedDevicePose_t};
use openxr::{SpaceLocation, SpaceVelocity};

use crate::{
    openxr_data::{Compositor, OpenXrData, SessionData},
    tracy_span,
};
use log::trace;

use super::tracked_device::{TrackedDevice, TrackedDeviceType, XrTrackedDevice};

pub struct XrController<C: Compositor> {
    device: XrTrackedDevice<C>,
    pub subaction_path: openxr::Path,
    pub hand_path: &'static str,
}

impl<C: Compositor> XrController<C> {
    pub fn new(instance: &openxr::Instance, device_type: TrackedDeviceType) -> Self {
        assert!(device_type == TrackedDeviceType::LeftHand || device_type == TrackedDeviceType::RightHand, "XrController can only be created for TrackedDeviceType::LeftHand or TrackedDeviceType::RightHand");

        let hand_path = match device_type {
            TrackedDeviceType::LeftHand => "/user/hand/left",
            TrackedDeviceType::RightHand => "/user/hand/right",
            _ => unreachable!(),
        };

        let mut controller = Self {
            device: XrTrackedDevice::default(),
            subaction_path: instance.string_to_path(hand_path).unwrap(),
            hand_path,
        };

        controller.device.init(device_type as u32, device_type);

        controller
    }

    pub fn get_device(&self) -> &XrTrackedDevice<C> {
        &self.device
    }
}

impl<C: Compositor> TrackedDevice<C> for XrController<C> {
    fn get_pose(
        &self,
        origin: openvr::ETrackingUniverseOrigin,
        xr_data: &OpenXrData<C>,
        session_data: &SessionData,
        display_time: openxr::Time,
    ) -> Option<TrackedDevicePose_t> {
        tracy_span!();
        let legacy = session_data.input_data.legacy_actions.get()?;

        let spaces = match self.get_type() {
            TrackedDeviceType::LeftHand => &legacy.left_spaces,
            TrackedDeviceType::RightHand => &legacy.right_spaces,
            _ => return None,
        };

        let (location, velocity) = if let Some(raw) =
            spaces.try_get_or_init_raw(xr_data, &session_data, &legacy.actions, display_time)
        {
            raw.relate(session_data.get_space_for_origin(origin), display_time)
                .unwrap()
        } else {
            trace!("failed to get raw space, making empty pose");
            (SpaceLocation::default(), SpaceVelocity::default())
        };

        Some(space_relation_to_openvr_pose(location, velocity))
    }

    fn get_type(&self) -> TrackedDeviceType {
        self.device.device_type
    }

    fn connected(&self) -> bool {
        self.device.is_connected()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
