use std::sync::atomic::AtomicBool;

use openvr::{
    k_unMaxTrackedDeviceCount, space_relation_to_openvr_pose, ETrackedDeviceClass,
    ETrackedDeviceProperty, TrackedDeviceIndex_t,
};
use openxr::AnyGraphics;

use crate::{openxr_data::Compositor, prop, runtime_extensions::xr_mndx_xdev_space::Xdev};

use super::tracked_device::{
    TrackedDevice, TrackedDeviceType, XrTrackedDevice, RESERVED_DEVICE_INDECES,
};

pub const MAX_GENERIC_TRACKERS: u32 = k_unMaxTrackedDeviceCount - RESERVED_DEVICE_INDECES;

pub struct XrGenericTracker<C: Compositor> {
    pub device: XrTrackedDevice<C>,
    space: openxr::Space,
    _name: String,
    serial_number: String,
}

impl<C: Compositor> XrGenericTracker<C> {
    pub fn new(
        index: TrackedDeviceIndex_t,
        dev: Xdev,
        session: &openxr::Session<AnyGraphics>,
    ) -> Self {
        let s = dev.space.unwrap();

        let space = unsafe { openxr::Space::reference_from_raw(session.to_owned(), s) };

        let mut tracker = Self {
            device: XrTrackedDevice::<C>::default(),
            space,
            _name: dev.properties.name(),
            serial_number: dev.properties.serial(),
        };

        tracker
            .device
            .init(index, TrackedDeviceType::GenericTracker);
        tracker.device.set_connected(true);

        tracker
    }
}

impl<C: Compositor> TrackedDevice<C> for XrGenericTracker<C> {
    fn get_pose(
        &self,
        origin: openvr::ETrackingUniverseOrigin,
        _xr_data: &crate::openxr_data::OpenXrData<C>,
        session_data: &crate::openxr_data::SessionData,
        display_time: openxr::Time,
    ) -> Option<openvr::TrackedDevicePose_t> {
        let (location, velocity) = self
            .space
            .relate(session_data.get_space_for_origin(origin), display_time)
            .unwrap();

        Some(space_relation_to_openvr_pose(location, velocity))
    }

    fn device_index(&self) -> TrackedDeviceIndex_t {
        self.device.device_index()
    }

    fn get_type(&self) -> super::tracked_device::TrackedDeviceType {
        self.device.get_type()
    }

    fn connected(&self) -> bool {
        self.device.connected()
    }

    fn last_connected_state(&self) -> &AtomicBool {
        self.device.last_connected_state()
    }

    fn set_interaction_profile(&self, profile: &'static dyn crate::input::InteractionProfile) {
        self.device.set_interaction_profile(profile);
    }

    fn get_interaction_profile(&self) -> Option<&'static dyn crate::input::InteractionProfile> {
        self.device.get_interaction_profile()
    }

    fn get_bool_property(
        &self,
        prop: openvr::ETrackedDeviceProperty,
        err: *mut openvr::ETrackedPropertyError,
    ) -> bool {
        self.device.get_bool_property(prop, err)
    }

    fn get_float_property(
        &self,
        prop: openvr::ETrackedDeviceProperty,
        err: *mut openvr::ETrackedPropertyError,
        system: &crate::system::System,
    ) -> f32 {
        self.device.get_float_property(prop, err, system)
    }

    fn get_int32_property(
        &self,
        prop: openvr::ETrackedDeviceProperty,
        err: *mut openvr::ETrackedPropertyError,
    ) -> i32 {
        prop!(
            ETrackedDeviceProperty::DeviceClass_Int32,
            prop,
            ETrackedDeviceClass::GenericTracker as i32
        );

        self.device.get_int32_property(prop, err)
    }

    fn get_uint64_property(
        &self,
        prop: openvr::ETrackedDeviceProperty,
        err: *mut openvr::ETrackedPropertyError,
    ) -> u64 {
        prop!(ETrackedDeviceProperty::CurrentUniverseId_Uint64, prop, 1); //Oculus Rift Universe

        self.device.get_uint64_property(prop, err)
    }

    fn get_string_property(
        &self,
        prop: openvr::ETrackedDeviceProperty,
        err: *mut openvr::ETrackedPropertyError,
    ) -> &str {
        let profile = self.get_interaction_profile();
        if let Some(profile) = profile {
            let property = profile.get_property(prop, TrackedDeviceType::LeftHand);
            if let Some(property) = property {
                return property.as_string().unwrap();
            }
        }

        prop!(ETrackedDeviceProperty::SerialNumber_String, prop, self.serial_number.as_str());

        self.device.get_string_property(prop, err)
    }

    fn get_device(&self) -> &XrTrackedDevice<C> {
        &self.device
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
