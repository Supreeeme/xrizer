use super::{
    InteractionProfile, PathTranslation, ProfileProperties, SkeletalInputBindings, StringToPath,
};
use crate::input::legacy::LegacyBindings;
use crate::input::profiles::ms_motion_controller::HolographicController;
use crate::openxr_data::Hand;
use glam::Mat4;
use glam::Vec3;

pub struct SamsungOdysseyController;

impl InteractionProfile for SamsungOdysseyController {
    fn properties(&self) -> &'static ProfileProperties {
        HolographicController.properties() // HACK: The controller render model is probably wrong...
    }
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/samsung/odyssey_controller"
    }
    fn translate_map(&self) -> &'static [PathTranslation] {
        HolographicController.translate_map()
    }

    fn legacy_bindings(&self, stp: &dyn StringToPath) -> LegacyBindings {
        HolographicController.legacy_bindings(stp)
    }

    fn skeletal_input_bindings(&self, stp: &dyn StringToPath) -> SkeletalInputBindings {
        HolographicController.skeletal_input_bindings(stp)
    }

    fn legal_paths(&self) -> Box<[String]> {
        HolographicController.legal_paths()
    }

    fn offset_grip_pose(&self, _hand: Hand) -> Mat4 {
        Mat4::from_translation(Vec3::new(
            // From the models found here https://www.microsoft.com/en-us/download/details.aspx?id=56414
            0.0, 0.079738, -0.035449,
        ))
    }
}
