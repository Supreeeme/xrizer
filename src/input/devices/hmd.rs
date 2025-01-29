use openvr::TrackedDevicePose_t;

use super::tracked_device::{TrackedDevice, TrackedDeviceType, XrTrackedDevice};

pub struct XrHMD {
    pub device: XrTrackedDevice,
}

impl XrHMD {
    pub fn new() -> Self {
        Self {
            device: XrTrackedDevice::default(),
        }
    }
}

impl TrackedDevice for XrHMD {
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