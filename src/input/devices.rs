use controller::XrController;
use hmd::XrHMD;
use tracked_device::{TrackedDevice, TrackedDeviceType};

pub mod controller;
pub mod hmd;
pub mod tracked_device;

pub struct XrTrackedDevices {
    devices: Vec<Box<dyn TrackedDevice>>,
}

impl XrTrackedDevices {
    pub fn new(instance: &openxr::Instance) -> Self {
        let mut devices = Self {
            devices: Vec::new(),
        };

        devices.add_device(Box::new(XrHMD::new()));

        for hand in ["/user/hand/left", "/user/hand/right"] {
            let device_type = TrackedDeviceType::try_from(hand).unwrap();
            devices.add_device(Box::new(XrController::new(instance, device_type)));
        }

        devices
    }

    pub fn add_device(&mut self, device: Box<dyn TrackedDevice>) {
        self.devices.push(device);
    }

    pub fn get_devices(&self) -> &[Box<dyn TrackedDevice>] {
        &self.devices
    }

    pub fn get_devices_mut(&mut self) -> &mut [Box<dyn TrackedDevice>] {
        &mut self.devices
    }

    pub fn get_device(&self, index: usize) -> Option<&Box<dyn TrackedDevice>> {
        self.devices.get(index)
    }

    pub fn get_device_mut(&mut self, index: usize) -> Option<&mut Box<dyn TrackedDevice>> {
        self.devices.get_mut(index)
    }

    pub fn get_device_by_type(
        &self,
        device_type: tracked_device::TrackedDeviceType,
    ) -> Option<&Box<dyn TrackedDevice>> {
        self.devices
            .iter()
            .find(|device| device.get_type() == device_type)
    }

    pub fn get_device_mut_by_type(
        &mut self,
        device_type: tracked_device::TrackedDeviceType,
    ) -> Option<&mut Box<dyn TrackedDevice>> {
        self.devices
            .iter_mut()
            .find(|device| device.get_type() == device_type)
    }

    pub fn get_hmd(&self) -> Option<&XrHMD> {
        self.get_device_by_type(tracked_device::TrackedDeviceType::HMD)
            .and_then(|dev| dev.as_any().downcast_ref::<XrHMD>())
    }

    pub fn get_controller(
        &self,
        hand: tracked_device::TrackedDeviceType,
    ) -> Option<&controller::XrController> {
        assert!(
            hand == tracked_device::TrackedDeviceType::LeftHand || hand == tracked_device::TrackedDeviceType::RightHand, 
            "XrController can only be created for TrackedDeviceType::LeftHand or TrackedDeviceType::RightHand"
        );

        self.get_device_by_type(hand)
            .and_then(|dev| dev.as_any().downcast_ref::<controller::XrController>())
    }
}
