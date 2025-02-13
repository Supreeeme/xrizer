use std::sync::atomic::AtomicBool;

use openvr::{
    space_relation_to_openvr_pose, ETrackedDeviceClass, ETrackedDeviceProperty,
    ETrackedPropertyError, EVREye, TrackedDeviceIndex_t, TrackedDevicePose_t,
};
use openxr::ReferenceSpaceType;

use super::tracked_device::{TrackedDevice, TrackedDeviceType, XrTrackedDevice};
use crate::input::InteractionProfile;
use crate::openxr_data::{Compositor, OpenXrData, SessionData};
use crate::prop;

pub struct XrHMD<C: Compositor> {
    pub device: XrTrackedDevice<C>,
}

impl<C: Compositor> XrHMD<C> {
    pub fn new() -> Self {
        let mut hmd = Self {
            device: XrTrackedDevice::<C>::default(),
        };

        hmd.device.init(0, TrackedDeviceType::HMD);
        hmd.device.set_connected(true);

        hmd
    }

    pub fn get_ipd(&self, system: &crate::system::System) -> f32 {
        let views = system.get_views(ReferenceSpaceType::VIEW);

        views.views[EVREye::Left as usize].pose.position.x - views.views[EVREye::Right as usize].pose.position.x
    }
}

impl<C: Compositor> TrackedDevice<C> for XrHMD<C> {
    fn get_pose(
        &self,
        origin: openvr::ETrackingUniverseOrigin,
        _xr_data: &OpenXrData<C>,
        session_data: &SessionData,
        display_time: openxr::Time,
    ) -> Option<TrackedDevicePose_t> {
        let (hmd_location, hmd_velocity) = {
            session_data
                .view_space
                .relate(session_data.get_space_for_origin(origin), display_time)
                .unwrap()
        };

        Some(space_relation_to_openvr_pose(hmd_location, hmd_velocity))
    }

    fn device_index(&self) -> TrackedDeviceIndex_t {
        self.device.device_index()
    }

    fn get_type(&self) -> TrackedDeviceType {
        self.device.device_type
    }

    fn connected(&self) -> bool {
        self.device.connected()
    }

    fn last_connected_state(&self) -> &AtomicBool {
        self.device.last_connected_state()
    }

    fn get_bool_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> bool {
        prop!(
            ETrackedDeviceProperty::DeviceProvidesBatteryStatus_Bool,
            prop,
            false
        ); //what about quest?
        prop!(
            ETrackedDeviceProperty::HasDriverDirectModeComponent_Bool,
            prop,
            true
        );
        prop!(
            ETrackedDeviceProperty::ContainsProximitySensor_Bool,
            prop,
            true
        );
        prop!(ETrackedDeviceProperty::HasCameraComponent_Bool, prop, false);
        prop!(ETrackedDeviceProperty::HasDisplayComponent_Bool, prop, true);
        prop!(
            ETrackedDeviceProperty::HasVirtualDisplayComponent_Bool,
            prop,
            false
        );

        self.device.get_bool_property(prop, err)
    }

    fn get_float_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
        system: &crate::system::System,
    ) -> f32 {
        prop!(ETrackedDeviceProperty::DisplayFrequency_Float, prop, 90.0); //TODO: use real value
        prop!(
            ETrackedDeviceProperty::UserIpdMeters_Float,
            prop,
            self.get_ipd(system)
        );
        prop!(
            ETrackedDeviceProperty::SecondsFromVsyncToPhotons_Float,
            prop,
            0.0001
        ); //this value should be "good enough", seen in croteam games
        prop!(
            ETrackedDeviceProperty::UserHeadToEyeDepthMeters_Float,
            prop,
            0.0
        ); //this is used for eye relief, but seems too obscure to bother with. see https://github.com/ValveSoftware/openvr/issues/398

        self.device.get_float_property(prop, err, system)
    }

    fn get_int32_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> i32 {
        prop!(
            ETrackedDeviceProperty::DeviceClass_Int32,
            prop,
            ETrackedDeviceClass::HMD as i32
        );
        prop!(
            ETrackedDeviceProperty::ExpectedControllerCount_Int32,
            prop,
            2
        );
        prop!(
            ETrackedDeviceProperty::ExpectedTrackingReferenceCount_Int32,
            prop,
            0
        );

        self.device.get_int32_property(prop, err)
    }

    fn get_uint64_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> u64 {
        self.device.get_uint64_property(prop, err)
    }

    fn get_string_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> &str {
        let profile = self.get_interaction_profile();
        if let Some(profile) = profile {
            let property = profile.get_property(prop, self.get_type());
            if let Some(property) = property {
                return property.as_string().unwrap();
            }
        }

        prop!(
            ETrackedDeviceProperty::RegisteredDeviceType_String,
            prop,
            "oculus/F00BAAF00F"
        );
        prop!(
            ETrackedDeviceProperty::RenderModelName_String,
            prop,
            "oculusHmdRenderModel"
        );

        prop!(
            ETrackedDeviceProperty::ControllerType_String,
            prop,
            "oculus"
        ); // VRChat ignores the HMD if this isn't set..

        self.device.get_string_property(prop, err)
    }

    fn get_device(&self) -> &XrTrackedDevice<C> {
        &self.device
    }

    fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile) {
        self.device.set_interaction_profile(profile);
    }

    fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile> {
        self.device.get_interaction_profile()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
