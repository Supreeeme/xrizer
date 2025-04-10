use controller::ControllerVariables;
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

#[allow(dead_code)]
impl TrackedDeviceList {
    pub fn new(xr_instance: &xr::Instance) -> Self {
        Self {
            devices: vec![
                XrTrackedDevice::new(TrackedDeviceType::Hmd),
                XrTrackedDevice::new(TrackedDeviceType::Controller(ControllerVariables::new(
                    xr_instance,
                    Hand::Left,
                ))),
                XrTrackedDevice::new(TrackedDeviceType::Controller(ControllerVariables::new(
                    xr_instance,
                    Hand::Right,
                ))),
            ],
        }
    }

    pub fn get_device(&self, device_index: vr::TrackedDeviceIndex_t) -> Option<&XrTrackedDevice> {
        self.devices.get(device_index as usize)
    }

    pub fn get_device_mut(
        &mut self,
        device_index: vr::TrackedDeviceIndex_t,
    ) -> Option<&mut XrTrackedDevice> {
        self.devices.get_mut(device_index as usize)
    }

    /// This function is only intended to be used for the HMD and controllers. For other devices, it'll return the first match.
    pub fn get_device_by_type(&self, device_type: TrackedDeviceType) -> Option<&XrTrackedDevice> {
        self.devices
            .iter()
            .find(|device| device.get_type() == device_type)
    }

    /// This function is only intended to be used for the HMD and controllers. For other devices, it'll return the first match.
    pub fn get_device_by_type_mut(
        &mut self,
        device_type: TrackedDeviceType,
    ) -> Option<&mut XrTrackedDevice> {
        self.devices
            .iter_mut()
            .find(|device| device.get_type() == device_type)
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

    pub fn len(&self) -> usize {
        self.devices.len()
    }
}
