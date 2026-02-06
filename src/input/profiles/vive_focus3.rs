use super::{
    InteractionProfile, MainAxisType, PathTranslation, ProfileProperties, Property,
    SkeletalInputBindings, StringToPath,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{self, LegacyBindings, button_mask_from_id};
use crate::openxr_data::Hand;
use glam::{EulerRot, Mat4, Quat, Vec3};
use openvr::EVRButtonId::{A, ApplicationMenu, Axis1, Axis2, Axis3, Axis4, Grip, System};

pub struct ViveFocus3;

impl InteractionProfile for ViveFocus3 {
    fn properties(&self) -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"vive_focus3_controller"),
            openvr_controller_type: c"vive_focus3_controller",
            render_model_name: Property::PerHand {
                left: c"vive_focus3_controller_left",
                right: c"vive_focus3_controller_right",
            },
            registered_device_type: Property::BothHands(
                c"htc_business_streaming/vive_focus3_controller",
            ),
            serial_number: Property::PerHand {
                left: c"CTL_LEFT",
                right: c"CTL_RIGHT",
            },
            tracking_system_name: c"htc_eyes",
            manufacturer_name: c"htc_rr",
            main_axis: MainAxisType::Thumbstick,
            legacy_buttons_mask: button_mask_from_ids!(
                System,
                ApplicationMenu,
                Grip,
                A,
                Axis1,
                Axis2,
                Axis3,
                Axis4,
            ),
        };
        &DEVICE_PROPERTIES
    }
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/htc/vive_focus3_controller"
    }
    fn translate_map(&self) -> &'static [PathTranslation] {
        &[
            PathTranslation {
                from: "x/touch",
                to: "x/click",
                stop: true,
            },
            PathTranslation {
                from: "y/touch",
                to: "y/click",
                stop: true,
            },
            PathTranslation {
                from: "a/touch",
                to: "a/click",
                stop: true,
            },
            PathTranslation {
                from: "b/touch",
                to: "b/click",
                stop: true,
            },
            PathTranslation {
                from: "input/grip",
                to: "input/squeeze",
                stop: false,
            },
            PathTranslation {
                from: "pull",
                to: "value",
                stop: true,
            },
            PathTranslation {
                from: "application_menu",
                to: "menu",
                stop: true,
            },
            PathTranslation {
                from: "joystick",
                to: "thumbstick",
                stop: true,
            },
        ]
    }

    fn legacy_bindings(&self, stp: &dyn StringToPath) -> LegacyBindings {
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: stp.leftright("input/grip/pose"),
            },
            trigger: stp.leftright("input/trigger/value"),
            trigger_click: stp.leftright("input/trigger/click"),
            app_menu: vec![
                stp("/user/hand/left/input/y/click"),
                stp("/user/hand/right/input/b/click"),
            ],
            a: vec![
                stp("/user/hand/left/input/x/click"),
                stp("/user/hand/right/input/a/click"),
            ],
            squeeze_click: stp.leftright("input/squeeze/click"),
            squeeze: stp.leftright("input/squeeze/value"),
            main_xy: stp.leftright("input/thumbstick"),
            main_xy_click: stp.leftright("input/thumbstick/click"),
            main_xy_touch: stp.leftright("input/thumbstick/touch"),
            haptic: stp.leftright("output/haptic"),
        }
    }

    fn skeletal_input_bindings(&self, stp: &dyn StringToPath) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: stp
                .leftright("input/thumbstick/touch")
                .into_iter()
                .chain(stp.left("input/x/click"))
                .chain(stp.left("input/y/click"))
                .chain(stp.right("input/a/click"))
                .chain(stp.right("input/b/click"))
                .chain(stp.leftright("input/thumbrest/touch"))
                .collect(),
            index_touch: stp.leftright("input/trigger/touch"),
            index_curl: stp.leftright("input/trigger/value"),
            rest_curl: stp.leftright("input/squeeze/value"),
        }
    }

    fn legal_paths(&self) -> Box<[String]> {
        let left_only = ["input/x/click", "input/y/click", "input/menu/click"]
            .iter()
            .map(|p| format!("/user/hand/left/{p}"));
        let right_only = ["input/a/click", "input/b/click"]
            .iter()
            .map(|p| format!("/user/hand/right/{p}"));

        let both = [
            "input/squeeze/value",
            "input/squeeze/click",
            "input/squeeze/touch",
            "input/trigger/value",
            "input/trigger/click",
            "input/trigger/touch",
            "input/thumbstick",
            "input/thumbstick/x",
            "input/thumbstick/y",
            "input/thumbstick/click",
            "input/thumbstick/touch",
            "input/thumbrest/touch",
            "input/grip/pose",
            "input/aim/pose",
            "output/haptic",
        ]
        .iter()
        .flat_map(|p| {
            [
                format!("/user/hand/left/{p}"),
                format!("/user/hand/right/{p}"),
            ]
        });

        left_only.chain(right_only).chain(both).collect()
    }

    fn offset_grip_pose(&self, hand: Hand) -> Mat4 {
        match hand {
            Hand::Left => Mat4::from_rotation_translation(
                Quat::from_euler(
                    EulerRot::XYZ,
                    20.6_f32.to_radians(),
                    0.0_f32.to_radians(),
                    0.0_f32.to_radians(),
                ),
                Vec3::new(0.007, -0.00182941, 0.1019482),
            )
            .inverse(),
            Hand::Right => Mat4::from_rotation_translation(
                Quat::from_euler(
                    EulerRot::XYZ,
                    20.6_f32.to_radians(),
                    0.0_f32.to_radians(),
                    0.0_f32.to_radians(),
                ),
                Vec3::new(-0.007, -0.00182941, 0.1019482),
            )
            .inverse(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InteractionProfile, ViveFocus3};
    use crate::input::tests::Fixture;
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = ViveFocus3.profile_path();
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/x/click".into(),
                "/user/hand/left/input/y/click".into(),
                "/user/hand/right/input/a/click".into(),
                "/user/hand/right/input/b/click".into(),
                "/user/hand/right/input/thumbstick/click".into(),
                "/user/hand/right/input/thumbstick/touch".into(),
                "/user/hand/left/input/menu/click".into(),
            ],
        );

        f.verify_bindings::<f32>(
            path,
            c"/actions/set1/boolact_asfloat",
            [
                "/user/hand/left/input/squeeze/value".into(),
                "/user/hand/right/input/squeeze/value".into(),
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
            ],
        );

        f.verify_bindings::<f32>(
            path,
            c"/actions/set1/in/vec1act",
            [
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
            ],
        );

        f.verify_bindings::<xr::Vector2f>(
            path,
            c"/actions/set1/in/vec2act",
            [
                "/user/hand/left/input/thumbstick".into(),
                "/user/hand/right/input/thumbstick".into(),
            ],
        );

        f.verify_bindings::<xr::Haptic>(
            path,
            c"/actions/set1/in/vib",
            [
                "/user/hand/left/output/haptic".into(),
                "/user/hand/right/output/haptic".into(),
            ],
        );
    }
}
