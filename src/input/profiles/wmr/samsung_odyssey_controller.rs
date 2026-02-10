use super::super::{
    InteractionProfile, PathTranslation, ProfileProperties, SkeletalInputBindings, StringToPath,
};
use super::ms_motion_controller::HolographicController;
use crate::input::legacy::LegacyBindings;
use crate::input::profiles::{wmr, MainAxisType, Property};
use crate::openxr_data::Hand;
use glam::Mat4;
use glam::Vec3;

pub struct SamsungOdysseyController;

impl InteractionProfile for SamsungOdysseyController {
    fn properties(&self) -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::PerHand {
                left:c"WindowsMR: 0x04E8/0x065D/0/1",
                right: c"WindowsMR: 0x04E8/0x065D/0/2",
            },
            openvr_controller_type: wmr::OG_OPENVR_CONTROLLER_TYPE,
            render_model_name: Property::PerHand {
                left: c"C:\\Users\\steamuser\\AppData\\Local\\Microsoft/Windows/OpenVR\\controller_1629_1256_1\\controller.obj",
                right: c"C:\\Users\\steamuser\\AppData\\Local\\Microsoft/Windows/OpenVR\\controller_1629_1256_2\\controller.obj"
            },
            main_axis: MainAxisType::Thumbstick,
            // The official driver doesn't seem return any thing for this?
            registered_device_type: Property::PerHand {
                left: c"WindowsMR/MRSOURCE0",
                right: c"WindowsMR/MRSOURCE1",
            },
            serial_number: wmr::SERIAL_NUMBER,
            tracking_system_name: wmr::TRACKING_SYSTEM_NAME,
            manufacturer_name: c"WindowsMR: 0x04E8",
            legacy_buttons_mask: wmr::OG_LEGACY_BUTTONS_MASK,
        };
        &DEVICE_PROPERTIES
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

#[cfg(test)]
mod tests {
    use super::{InteractionProfile, SamsungOdysseyController};
    use crate::input::profiles::wmr::ms_motion_controller;

    #[test]
    fn verify_bindings() {
        ms_motion_controller::tests::base_verify_bindings(SamsungOdysseyController.profile_path());
    }
}
