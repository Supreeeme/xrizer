use super::{
    InteractionProfile, MainAxisType, PathTranslation, ProfileProperties, Property,
    SkeletalInputBindings, StringToPath,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{Bindings, LegacyBindings, button_mask_from_id};
use crate::openxr_data::Hand;
use glam::Mat4;
use openvr::EVRButtonId::{ApplicationMenu, Axis0, Axis1, Grip, System};

pub struct SimpleController;

impl InteractionProfile for SimpleController {
    fn properties(&self) -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"generic"),
            openvr_controller_type: c"<unknown>",
            render_model_name: Property::BothHands(c"generic_controller"),
            main_axis: MainAxisType::Thumbstick,
            // TODO: These are just from the vive_controller. I'm not certain whether that's correct here
            registered_device_type: Property::PerHand {
                left: c"htc/vive_controllerLHR-00000001",
                right: c"htc/vive_controllerLHR-00000002",
            },
            serial_number: Property::PerHand {
                left: c"LHR-00000001",
                right: c"LHR-00000002",
            },
            tracking_system_name: c"lighthouse",
            manufacturer_name: c"HTC",
            legacy_buttons_mask: button_mask_from_ids!(System, ApplicationMenu, Grip, Axis0, Axis1),
        };
        &DEVICE_PROPERTIES
    }
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/khr/simple_controller"
    }
    fn has_required_extensions(&self, _: &openxr::ExtensionSet) -> bool {
        true
    }
    fn translate_map(&self) -> &'static [PathTranslation] {
        &[
            PathTranslation {
                from: "trigger",
                to: "select",
                stop: true,
            },
            PathTranslation {
                from: "application_menu",
                to: "menu",
                stop: true,
            },
        ]
    }

    fn legacy_bindings(&self, stp: &dyn StringToPath) -> LegacyBindings {
        LegacyBindings {
            extra: Bindings {
                grip_pose: stp.leftright("input/grip/pose"),
            },
            trigger: stp.leftright("input/select/click"),
            trigger_click: stp.leftright("input/select/click"),
            app_menu: stp.leftright("input/menu/click"),
            a: vec![],
            squeeze: stp.leftright("input/menu/click"),
            squeeze_click: stp.leftright("input/menu/click"),
            main_xy: vec![],
            main_xy_click: vec![],
            main_xy_touch: vec![],
            haptic: stp.leftright("output/haptic"),
        }
    }

    fn skeletal_input_bindings(&self, stp: &dyn StringToPath) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: Vec::new(),
            index_touch: stp.leftright("input/select/click"),
            index_curl: stp.leftright("input/select/click"),
            rest_curl: stp.leftright("input/menu/click"),
        }
    }

    fn legal_paths(&self) -> Box<[String]> {
        [
            "input/select/click",
            "input/menu/click",
            "input/grip/pose",
            "input/aim/pose",
            "output/haptic",
        ]
        .iter()
        .flat_map(|s| {
            [
                format!("/user/hand/left/{s}"),
                format!("/user/hand/right/{s}"),
            ]
        })
        .collect()
    }

    fn offset_grip_pose(&self, _: Hand) -> Mat4 {
        Mat4::IDENTITY
    }
}
