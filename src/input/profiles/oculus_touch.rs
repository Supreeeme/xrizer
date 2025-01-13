use super::{InteractionProfile, PathTranslation, ProfileProperties, Property, StringToPath};
use crate::input::legacy::LegacyBindings;
use crate::openxr_data::Hand;
use glam::{EulerRot, Mat4, Quat, Vec3};

pub struct Touch;

impl InteractionProfile for Touch {
    fn properties(&self) -> &'static ProfileProperties {
        &ProfileProperties {
            model: c"oculus_touch",
            openvr_controller_type: c"Miramar",
            render_model_name: Property::PerHand {
                left: c"oculus_quest_controller_left",
                right: c"oculus_quest_controller_right",
            },
            has_joystick: true,
            has_trackpad: false,
        }
    }
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/oculus/touch_controller"
    }
    fn translate_map(&self) -> &'static [PathTranslation] {
        &[
            PathTranslation {
                from: "trigger/click",
                to: "trigger/value",
                stop: true,
            },
            PathTranslation {
                from: "grip/click",
                to: "squeeze/value",
                stop: true,
            },
            PathTranslation {
                from: "trigger/pull",
                to: "trigger/value",
                stop: true,
            },
            PathTranslation {
                from: "trigger/click",
                to: "trigger/value",
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
            grip_pose: stp.leftright("input/grip/pose"),
            aim_pose: stp.leftright("input/aim/pose"),
            trigger: stp.leftright("input/trigger/value"),
            trigger_click: stp.leftright("input/trigger/value"),
            app_menu: vec![], // TODO
            squeeze: stp.leftright("input/squeeze/value"),
        }
    }

    fn legal_paths(&self) -> Box<[String]> {
        let left_only = [
            "input/x/click",
            "input/x/touch",
            "input/y/click",
            "input/y/touch",
            "input/menu/click",
        ]
        .iter()
        .map(|p| format!("/user/hand/left/{p}"));
        let right_only = [
            "input/a/click",
            "input/a/touch",
            "input/b/click",
            "input/b/touch",
        ]
        .iter()
        .map(|p| format!("/user/hand/right/{p}"));

        let both = [
            "input/squeeze/value",
            "input/trigger/value",
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
    use super::{InteractionProfile, Touch};
    use crate::input::tests::Fixture;
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = Touch.profile_path();
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/x/click".into(),
                "/user/hand/left/input/y/click".into(),
                "/user/hand/right/input/a/click".into(),
                "/user/hand/right/input/b/click".into(),
                "/user/hand/left/input/squeeze/value".into(),
                "/user/hand/right/input/squeeze/value".into(),
                "/user/hand/right/input/thumbstick/click".into(),
                "/user/hand/right/input/thumbstick/touch".into(),
                "/user/hand/left/input/menu/click".into(),
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
