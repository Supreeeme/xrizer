use super::{
    InteractionProfile, MainAxisType, PathTranslation, ProfileProperties, Property,
    SkeletalInputBindings, StringToPath,
};
use crate::{input::legacy::LegacyBindings, openxr_data::Hand};
use glam::Mat4;

pub struct ViveTracker;

impl InteractionProfile for ViveTracker {
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/valve/index_controller"
    }
    fn properties(&self) -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"Vive Tracker Handheld Object"),
            openvr_controller_type: c"vive_tracker_handheld_object",
            render_model_name: Property::BothHands(c"vive_tracker"),
            main_axis: MainAxisType::Thumbstick,
            registered_device_type: Property::BothHands(c"vive_tracker"),
            serial_number: Property::BothHands(c"vive_tracker"), // This gets replaced
            tracking_system_name: c"lighthouse",
            manufacturer_name: c"HTC",
            legacy_buttons_mask: 0u64, // This is the closest thing I could think of to NOOP this
        };

        &DEVICE_PROPERTIES
    }
    fn translate_map(&self) -> &'static [PathTranslation] {
        &[]
    }

    fn legal_paths(&self) -> Box<[String]> {
        [].into()
    }

    fn legacy_bindings(&self, _: &dyn StringToPath) -> LegacyBindings {
        todo!()
    }

    fn skeletal_input_bindings(&self, _: &dyn StringToPath) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: Vec::new(),
            index_touch: Vec::new(),
            index_curl: Vec::new(),
            rest_curl: Vec::new(),
        }
    }

    fn offset_grip_pose(&self, _: Hand) -> Mat4 {
        Mat4::IDENTITY
    }
}
