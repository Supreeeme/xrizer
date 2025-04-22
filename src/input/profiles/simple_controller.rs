use glam::Mat4;

use super::{
    InteractionProfile, MainAxisType, PathTranslation, ProfileProperties, Property,
    SkeletalInputBindings, StringToPath,
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
            main_axis: MainAxisType::Thumbstick,
            // TODO: These are just from the vive_controller. I'm not certain whether that's correct here
            legacy_buttons_mask: (1 << 0) | (1 << 1) | (1 << 2) | (1 << 32) | (1 << 33),
            registered_device_type: Property::PerHand {
                left: c"htc/vive_controllerLHR-00000001",
                right: c"htc/vive_controllerLHR-00000002"
            },
            serial_number: Property::PerHand {
                left: c"LHR-00000001",
                right: c"LHR-00000002"
            },
            tracking_system_name: c"lighthouse",
            manufacturer_name: c"HTC",
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
            a: vec![],
            squeeze: stp.leftright("input/menu/click"),
            squeeze_click: stp.leftright("input/menu/click"),
            main_xy: vec![],
            main_xy_click: vec![],
            main_xy_touch: vec![],
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
