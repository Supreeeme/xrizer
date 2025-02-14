use super::{
    DevicePropertyTypes, HandValueType, InteractionProfile, PathTranslation, StringToPath,
};
use crate::input::devices::tracked_device::TrackedDeviceType;
use crate::input::legacy::LegacyBindings;
use openvr::ETrackedDeviceProperty;

pub struct ViveTracker;

impl InteractionProfile for ViveTracker {
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/htc/vive_tracker_htcx"
    }

    fn model(&self, _hand: TrackedDeviceType) -> &'static str {
        self.get_property(
            ETrackedDeviceProperty::ModelNumber_String,
            TrackedDeviceType::LeftHand,
        )
        .unwrap()
        .as_string()
        .unwrap()
    }

    fn hmd_properties(&self) -> &'static [(ETrackedDeviceProperty, DevicePropertyTypes)] {
        &[]
    }

    fn controller_properties(
        &self,
    ) -> &'static [(ETrackedDeviceProperty, HandValueType<DevicePropertyTypes>)] {
        &[
            (
                ETrackedDeviceProperty::ModelNumber_String,
                HandValueType {
                    left: DevicePropertyTypes::String("Vive Tracker Handheld Object"),
                    right: None,
                },
            ),
            (
                ETrackedDeviceProperty::RenderModelName_String,
                HandValueType {
                    left: DevicePropertyTypes::String("vive_tracker"),
                    right: None,
                },
            ),
            (
                ETrackedDeviceProperty::ControllerType_String,
                HandValueType {
                    left: DevicePropertyTypes::String("vive_tracker_handheld_object"),
                    right: None,
                },
            ),
        ]
    }

    fn openvr_controller_type(&self) -> &'static str {
        self.get_property(
            ETrackedDeviceProperty::ControllerType_String,
            TrackedDeviceType::LeftHand,
        )
        .unwrap()
        .as_string()
        .unwrap()
    }

    fn render_model_name(&self, hand: TrackedDeviceType) -> &'static str {
        match hand {
            TrackedDeviceType::LeftHand => self
                .get_property(ETrackedDeviceProperty::RenderModelName_String, hand)
                .unwrap()
                .as_string()
                .unwrap(),

            TrackedDeviceType::RightHand => self
                .get_property(ETrackedDeviceProperty::RenderModelName_String, hand)
                .unwrap()
                .as_string()
                .unwrap(),
            _ => unreachable!(),
        }
    }

    fn translate_map(&self) -> &'static [PathTranslation] {
        &[]
    }

    fn legal_paths(&self) -> Box<[String]> {
        [].into()
    }

    fn legacy_bindings(&self, _: &dyn StringToPath) -> Option<LegacyBindings> {
        None
    }
}
