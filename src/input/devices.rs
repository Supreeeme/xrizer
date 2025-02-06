use controller::XrController;
use hmd::XrHMD;
use openvr::k_unMaxTrackedDeviceCount;
use tracked_device::{TrackedDevice, TrackedDeviceType};

use crate::openxr_data::{Compositor, OpenXrData, SessionData};

pub mod controller;
pub mod hmd;
pub mod tracked_device;

pub enum DeviceContainer<C: Compositor> {
    HMD(XrHMD<C>),
    Controller(XrController<C>),
}

macro_rules! handle_variants {
    ($value:expr, |$var:ident| $action:block) => {
        match $value {
            $crate::input::devices::DeviceContainer::HMD($var) => $action,
            $crate::input::devices::DeviceContainer::Controller($var) => $action,
        }
    };
}

impl<C: Compositor> TrackedDevice<C> for DeviceContainer<C> {
    fn get_pose(
        &self,
        origin: openvr::ETrackingUniverseOrigin,
        xr_data: &OpenXrData<C>,
        session_data: &SessionData,
        display_time: openxr::Time,
    ) -> Option<openvr::TrackedDevicePose_t> {
        handle_variants!(self, |device| {
            return device.get_pose(origin, xr_data, session_data, display_time);
        })
    }

    fn get_type(&self) -> TrackedDeviceType {
        handle_variants!(self, |device| { return device.get_type() })
    }

    fn connected(&self) -> bool {
        handle_variants!(self, |device| { return device.connected() })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        handle_variants!(self, |device| { return device.as_any() })
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        handle_variants!(self, |device| { return device.as_any_mut() })
    }
}

pub struct XrTrackedDeviceManager<C: Compositor> {
    devices: Vec<DeviceContainer<C>>,
}

impl<C: Compositor> XrTrackedDeviceManager<C> {
    pub fn new(instance: &openxr::Instance) -> Self {
        let mut devices = Self {
            devices: Vec::with_capacity(k_unMaxTrackedDeviceCount as usize),
        };

        devices.add_device(DeviceContainer::HMD(XrHMD::new()));

        for hand in ["/user/hand/left", "/user/hand/right"] {
            let device_type = TrackedDeviceType::try_from(hand).unwrap();
            devices.add_device(DeviceContainer::Controller(XrController::new(
                instance,
                device_type,
            )));
        }

        devices
    }

    pub fn add_device(&mut self, device: DeviceContainer<C>) {
        if self.devices.len() as u32 == k_unMaxTrackedDeviceCount {
            panic!("Cannot add more than {} devices", k_unMaxTrackedDeviceCount);
        }
        self.devices.push(device);
    }

    pub fn get_devices(&self) -> &[DeviceContainer<C>] {
        &self.devices
    }

    pub fn get_devices_mut(&mut self) -> &mut [DeviceContainer<C>] {
        &mut self.devices
    }

    pub fn get_device(&self, index: usize) -> Option<&DeviceContainer<C>> {
        self.devices.get(index)
    }

    pub fn get_device_mut(&mut self, index: usize) -> Option<&mut DeviceContainer<C>> {
        self.devices.get_mut(index)
    }

    pub fn get_device_by_type(
        &self,
        device_type: tracked_device::TrackedDeviceType,
    ) -> Option<&DeviceContainer<C>> {
        self.devices
            .iter()
            .find(|device| device.get_type() == device_type)
    }

    pub fn get_device_mut_by_type(
        &mut self,
        device_type: tracked_device::TrackedDeviceType,
    ) -> Option<&mut DeviceContainer<C>> {
        self.devices
            .iter_mut()
            .find(|device| device.get_type() == device_type)
    }

    pub fn get_hmd(&self) -> Option<&XrHMD<C>> {
        self.get_device_by_type(tracked_device::TrackedDeviceType::HMD)
            .and_then(|dev| dev.as_any().downcast_ref::<XrHMD<C>>())
    }

    pub fn get_controller(
        &self,
        hand: tracked_device::TrackedDeviceType,
    ) -> Option<&controller::XrController<C>> {
        assert!(
            hand == tracked_device::TrackedDeviceType::LeftHand || hand == tracked_device::TrackedDeviceType::RightHand,
            "XrController can only be created for TrackedDeviceType::LeftHand or TrackedDeviceType::RightHand"
        );

        self.get_device_by_type(hand)
            .and_then(|dev| dev.as_any().downcast_ref::<controller::XrController<C>>())
    }
}
