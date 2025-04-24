use tracked_device::{TrackedDeviceType, XrTrackedDevice};

use openvr as vr;
use openxr as xr;

use crate::openxr_data::Hand;

mod controller;
mod hmd;
pub mod tracked_device;

pub struct TrackedDeviceList {
    devices: Vec<XrTrackedDevice>,
}

pub struct SubactionPaths {
    pub left: xr::Path,
    pub right: xr::Path,
}

impl SubactionPaths {
    pub fn new(instance: &xr::Instance) -> Self {
        let left = instance
            .string_to_path("/user/hand/left")
            .expect("Failed to convert string to path");
        let right = instance
            .string_to_path("/user/hand/right")
            .expect("Failed to convert string to path");

        Self { left, right }
    }
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
