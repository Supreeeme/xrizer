use super::{InteractionProfile, MainAxisType, ProfileProperties, Property, SkeletalInputBindings};
use crate::{input::legacy::LegacyBindings, openxr_data::Hand};
use glam::Mat4;

pub struct ViveTracker;

impl InteractionProfile for ViveTracker {
    type LegalPaths = ();
    fn profile_path() -> &'static str {
        "/interaction_profiles/htc/vive_tracker_htcx"
    }
    fn has_required_extensions(_: &openxr::ExtensionSet) -> bool {
        true
    }
    fn properties() -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"Vive Tracker Handheld Object"),
            openvr_controller_type: c"vive_tracker_handheld_object",
            render_model_name: Property::BothHands(c"vr_tracker_vive_3_0"),
            main_axis: MainAxisType::Thumbstick,
            registered_device_type: Property::BothHands(c"vive_tracker"),
            serial_number: Property::BothHands(c"vive_tracker"), // This gets replaced
            tracking_system_name: c"lighthouse",
            manufacturer_name: c"HTC",
            legacy_buttons_mask: 0u64, // This is the closest thing I could think of to NOOP this
        };

        &DEVICE_PROPERTIES
    }

    fn legacy_bindings(_: &super::InputToXrPath<Self>) -> LegacyBindings {
        unimplemented!()
    }

    fn skeletal_input_bindings(_: &super::InputToXrPath<Self>) -> SkeletalInputBindings {
        unimplemented!()
    }

    fn offset_grip_pose(_: Hand) -> Mat4 {
        Mat4::IDENTITY
    }
}
