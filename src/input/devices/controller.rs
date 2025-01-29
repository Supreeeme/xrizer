use std::sync::Mutex;

use openvr::TrackedDevicePose_t;

use crate::input::InteractionProfile;

use super::tracked_device::{TrackedDevice, TrackedDeviceType, XrTrackedDevice};

pub struct XrController {
    device: XrTrackedDevice,
    pub subaction_path: openxr::Path,
    hand_path: &'static str,
}

impl XrController {
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

    pub fn get_device(&self) -> &XrTrackedDevice {
        &self.device
    }
}

impl TrackedDevice for XrController {
    fn get_pose(&self, origin: openvr::ETrackingUniverseOrigin) -> Option<TrackedDevicePose_t> {
        todo!()
    }

    fn get_type(&self) -> TrackedDeviceType {
        self.device.get_type()
    }

    fn connected(&self) -> bool {
        self.device.connected()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
