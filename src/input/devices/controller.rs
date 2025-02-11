use openvr::{
    space_relation_to_openvr_pose, ETrackedDeviceProperty, ETrackedPropertyError, EVRButtonId,
    EVRControllerAxisType, TrackedDevicePose_t,
};
use openxr::{SpaceLocation, SpaceVelocity};

use crate::{
    input::InteractionProfile,
    misc_unknown::button_mask_from_id,
    openxr_data::{Compositor, OpenXrData, SessionData},
    prop, tracy_span,
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
        self.device.connected()
    }

    fn get_bool_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> bool {
        prop!(
            ETrackedDeviceProperty::DeviceProvidesBatteryStatus_Bool,
            prop,
            true
        );

        self.device.get_bool_property(prop, err)
    }

    fn get_float_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
        system: &crate::system::System,
    ) -> f32 {
        self.device.get_float_property(prop, err, system)
    }

    fn get_int32_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> i32 {
        //Apparently this is pretending to be a CV1
        prop!(
            ETrackedDeviceProperty::Axis0Type_Int32,
            prop,
            EVRControllerAxisType::Joystick as i32
        );
        prop!(
            ETrackedDeviceProperty::Axis1Type_Int32,
            prop,
            EVRControllerAxisType::Trigger as i32
        );
        prop!(
            ETrackedDeviceProperty::Axis2Type_Int32,
            prop,
            EVRControllerAxisType::Trigger as i32
        );
        prop!(
            ETrackedDeviceProperty::Axis3Type_Int32,
            prop,
            EVRControllerAxisType::None as i32
        );
        prop!(
            ETrackedDeviceProperty::Axis4Type_Int32,
            prop,
            EVRControllerAxisType::None as i32
        );

        self.device.get_int32_property(prop, err)
    }

    fn get_uint64_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> u64 {
        if prop == ETrackedDeviceProperty::SupportedButtons_Uint64 {
            return button_mask_from_id(EVRButtonId::System)
                | button_mask_from_id(EVRButtonId::ApplicationMenu)
                | button_mask_from_id(EVRButtonId::Grip)
                | button_mask_from_id(EVRButtonId::Axis2)
                | button_mask_from_id(EVRButtonId::DPad_Left)
                | button_mask_from_id(EVRButtonId::DPad_Up)
                | button_mask_from_id(EVRButtonId::DPad_Down)
                | button_mask_from_id(EVRButtonId::DPad_Right)
                | button_mask_from_id(EVRButtonId::A)
                | button_mask_from_id(EVRButtonId::SteamVR_Touchpad)
                | button_mask_from_id(EVRButtonId::SteamVR_Trigger);
        }

        self.device.get_uint64_property(prop, err)
    }

    fn get_string_property(&self, prop: ETrackedDeviceProperty, err: *mut ETrackedPropertyError) -> &str {
        let profile = self.get_interaction_profile().unwrap();

        let property = profile.get_property(prop, self.get_type());
        if let Some(property) = property {
            return property.as_string().unwrap();
        }

        match self.get_type() {
            TrackedDeviceType::LeftHand => {
                prop!(
                    ETrackedDeviceProperty::RegisteredDeviceType_String,
                    prop,
                    "oculus/F00BAAF00F_Controller_Left"
                );
            }
            TrackedDeviceType::RightHand => {
                prop!(
                    ETrackedDeviceProperty::RegisteredDeviceType_String,
                    prop,
                    "oculus/F00BAAF00F_Controller_Right"
                );
            }
            _ => unreachable!()
        }

        self.device.get_string_property(prop, err)
    }

    fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile) {
        self.device.set_interaction_profile(profile);
    }

    fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile> {
        self.device.get_interaction_profile()
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
