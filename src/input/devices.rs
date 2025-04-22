use tracked_device::{TrackedDeviceType, XrTrackedDevice};

use openvr as vr;

use crate::openxr_data::Hand;

mod controller;
mod hmd;
pub mod tracked_device;

pub struct TrackedDeviceList {
    devices: Vec<XrTrackedDevice>,
}

impl TrackedDeviceList {
    pub fn new() -> Self {
        Self {
            devices: vec![
                XrTrackedDevice::new(TrackedDeviceType::Hmd),
                XrTrackedDevice::new(TrackedDeviceType::Controller { hand: Hand::Left }),
                XrTrackedDevice::new(TrackedDeviceType::Controller { hand: Hand::Right }),
            ],
        }
    }

    pub fn get_device(&self, device_index: vr::TrackedDeviceIndex_t) -> Option<&XrTrackedDevice> {
        self.devices.get(device_index as usize)
    }

    pub fn get_hmd(&self) -> &XrTrackedDevice {
        unsafe { self.devices.get_unchecked(0) }
    }

    pub fn get_controller(&self, hand: Hand) -> &XrTrackedDevice {
        unsafe { self.devices.get_unchecked(hand as usize) }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, XrTrackedDevice> {
        self.devices.iter()
    }
}
