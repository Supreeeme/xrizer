use glam::Mat4;

use super::{
    InteractionProfile, PathTranslation, ProfileProperties, Property, SkeletalInputBindings,
    StringToPath,
};
use crate::input::legacy::LegacyBindings;
use crate::openxr_data::Hand;

pub struct SimpleController;

impl InteractionProfile for SimpleController {
    fn properties(&self) -> &'static ProfileProperties {
        &ProfileProperties {
            model: c"generic",
            openvr_controller_type: c"<unknown>",
            render_model_name: Property::BothHands(c"generic_controller"),
            has_joystick: false,
            has_trackpad: false,
        }
    }
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/khr/simple_controller"
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
            grip_pose: stp.leftright("input/grip/pose"),
            aim_pose: stp.leftright("input/aim/pose"),
            trigger: stp.leftright("input/select/click"),
            trigger_click: stp.leftright("input/select/click"),
            app_menu: stp.leftright("input/menu/click"),
            squeeze: stp.leftright("input/menu/click"),
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
