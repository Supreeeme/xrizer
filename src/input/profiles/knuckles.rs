use glam::{EulerRot, Mat4, Quat, Vec3};

use super::{
    InteractionProfile, MainAxisType, PathTranslation, ProfileProperties, Property,
    SkeletalInputBindings, StringToPath,
};
use crate::input::legacy::LegacyBindings;
use crate::openxr_data::Hand;

pub struct Knuckles;

impl InteractionProfile for Knuckles {
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/valve/index_controller"
    }
    fn properties(&self) -> &'static ProfileProperties {
        &ProfileProperties {
            model: c"Knuckles",
            openvr_controller_type: c"knuckles",
            render_model_name: Property::PerHand {
                left: c"valve_controller_knu_1_0_left",
                right: c"valve_controller_knu_1_0_right",
            },
            main_axis: MainAxisType::Thumbstick,
            legacy_buttons_mask: (1 << 0) | (1 << 1) | (1 << 2) | (1 << 7) | (1 << 32) | (1 << 33) | (1 << 34)
        }
    }
    fn translate_map(&self) -> &'static [PathTranslation] {
        &[
            PathTranslation {
                from: "pull",
                to: "value",
                stop: false,
            },
            PathTranslation {
                from: "input/grip",
                to: "input/squeeze",
                stop: false,
            },
            PathTranslation {
                from: "squeeze/click",
                to: "squeeze/value",
                stop: true,
            },
            PathTranslation {
                // button bindings
                from: "squeeze/touch",
                to: "squeeze/value",
                stop: true,
            },
            PathTranslation {
                from: "squeeze/grab",
                to: "squeeze/force",
                stop: true,
            },
            PathTranslation {
                from: "trackpad/click",
                to: "trackpad/force",
                stop: true,
            },
        ]
    }

    fn legal_paths(&self) -> Box<[String]> {
        let click_and_touch = ["input/a", "input/b", "input/trigger", "input/thumbstick"]
            .iter()
            .flat_map(|p| [format!("{p}/click"), format!("{p}/touch")]);
        let x_and_y = ["input/thumbstick", "input/trackpad"]
            .iter()
            .flat_map(|p| [format!("{p}/x"), format!("{p}/y"), p.to_string()]);
        let misc = [
            "input/squeeze/value",
            "input/squeeze/force",
            "input/trigger/value",
            "input/trackpad/force",
            "input/trackpad/touch",
            "input/grip/pose",
            "input/aim/pose",
            "output/haptic",
        ]
        .into_iter()
        .map(String::from);

        click_and_touch
            .chain(x_and_y)
            .chain(misc)
            .flat_map(|p| {
                [
                    format!("/user/hand/left/{p}"),
                    format!("/user/hand/right/{p}"),
                ]
            })
            .collect()
    }

    fn legacy_bindings(&self, stp: &dyn StringToPath) -> LegacyBindings {
        LegacyBindings {
            grip_pose: stp.leftright("input/grip/pose"),
            aim_pose: stp.leftright("input/aim/pose"),
            app_menu: stp.leftright("input/b/click"),
            a: stp.leftright("input/a/click"),
            trigger: stp.leftright("input/trigger/value"),
            trigger_click: stp.leftright("input/trigger/click"),
            squeeze: stp.leftright("input/squeeze/value"),
            squeeze_click: stp.leftright("input/squeeze/value"),
            main_xy: stp.leftright("input/thumbstick"),
            main_xy_click: stp.leftright("input/thumbstick/click"),
            main_xy_touch: stp.leftright("input/thumbstick/touch"),
        }
    }

    fn skeletal_input_bindings(&self, stp: &dyn StringToPath) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: stp
                .leftright("input/thumbstick/touch")
                .into_iter()
                .chain(stp.leftright("input/trackpad/touch"))
                .collect(),
            index_touch: stp.leftright("input/trigger/touch"),
            index_curl: stp.leftright("input/trigger/value"),
            rest_curl: stp.leftright("input/squeeze/value"),
        }
    }

    fn offset_grip_pose(&self, hand: Hand) -> Mat4 {
        match hand {
            Hand::Left => Mat4::from_rotation_translation(
                Quat::from_euler(
                    EulerRot::XYZ,
                    15.392_f32.to_radians(),
                    -2.071_f32.to_radians(),
                    0.303_f32.to_radians(),
                ),
                Vec3::new(0.0, -0.015, 0.13),
            )
            .inverse(),
            Hand::Right => Mat4::from_rotation_translation(
                Quat::from_euler(
                    EulerRot::XYZ,
                    15.392_f32.to_radians(),
                    2.071_f32.to_radians(),
                    -0.303_f32.to_radians(),
                ),
                Vec3::new(0.0, -0.015, 0.13),
            )
            .inverse(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InteractionProfile, Knuckles};
    use crate::input::{tests::Fixture, ActionData};
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = Knuckles.profile_path();
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/a/click".into(),
                "/user/hand/right/input/a/click".into(),
                "/user/hand/left/input/b/click".into(),
                "/user/hand/right/input/b/click".into(),
                "/user/hand/left/input/trigger/touch".into(),
                "/user/hand/right/input/trigger/touch".into(),
                "/user/hand/left/input/thumbstick/click".into(),
                "/user/hand/right/input/thumbstick/click".into(),
                "/user/hand/left/input/thumbstick/touch".into(),
                "/user/hand/right/input/thumbstick/touch".into(),
                "/user/hand/right/input/trackpad/touch".into(),
            ],
        );

        f.verify_bindings::<f32>(
            path,
            c"/actions/set1/boolact_asfloat",
            [
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
                "/user/hand/left/input/squeeze/value".into(),
                "/user/hand/left/input/trackpad/force".into(),
                "/user/hand/right/input/trackpad/force".into(),
            ],
        );

        let handle = f.get_action_handle(c"/actions/set1/in/boolact");
        let data = f.input.openxr.session_data.get();
        let actions = data.input_data.get_loaded_actions().unwrap();
        let action = actions.try_get_action(handle).unwrap();
        let extra = actions.try_get_extra(handle).unwrap();

        let ActionData::Bool(_) = action else {
            panic!("no");
        };

        let grab_data = extra.grab_action.as_ref().unwrap();
        let p = f.input.openxr.instance.string_to_path(path).unwrap();
        let suggested = fakexr::get_suggested_bindings(grab_data.force_action.as_raw(), p);
        assert!(suggested.contains(&"/user/hand/right/input/squeeze/force".to_string()));

        f.verify_bindings::<f32>(
            path,
            c"/actions/set1/in/vec1act",
            [
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
                "/user/hand/left/input/squeeze/force".into(),
                "/user/hand/right/input/squeeze/value".into(),
            ],
        );

        f.verify_bindings::<xr::Vector2f>(
            path,
            c"/actions/set1/in/vec2act",
            [
                "/user/hand/left/input/trackpad".into(),
                "/user/hand/right/input/trackpad".into(),
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
