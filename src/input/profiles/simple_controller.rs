use super::{InteractionProfile, PathTranslation, StringToPath};
use crate::input::{devices::tracked_device::TrackedDeviceType, legacy::LegacyBindings};
use std::ffi::CStr;

pub struct SimpleController;

impl InteractionProfile for SimpleController {
    fn openvr_controller_type(&self) -> &'static CStr {
        c"generic" // meaningless really
    }
    fn model(&self) -> &'static CStr {
        c"<unknown>"
    }
    fn render_model_name(&self, _: TrackedDeviceType) -> &'static CStr {
        c"generic_controller"
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
}
