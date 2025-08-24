use super::{
    InteractionProfile, MainAxisType, PathTranslation, ProfileProperties, Property,
    SkeletalInputBindings, StringToPath,
};
use crate::button_mask_from_ids;
use crate::input::legacy;
use crate::input::legacy::button_mask_from_id;
use crate::input::legacy::LegacyBindings;
use crate::openxr_data::Hand;
use glam::Mat4;
use glam::Vec3;
use openvr::EVRButtonId::{ApplicationMenu, Axis0, Axis1, Axis2, Grip, System, A};

pub struct HolographicController;

impl InteractionProfile for HolographicController {
    fn properties(&self) -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"WindowsMR"), // "VAC-151B" controllers
            openvr_controller_type: c"holographic_controller",
            render_model_name: Property::BothHands(c"holographic_controller"),
            main_axis: MainAxisType::Thumbstick,
            // TODO: I'm not certain whether that's correct here, THIS IS A GUESS
            registered_device_type: Property::PerHand {
                left: c"WindowsMR/holographic_controllerLHR-00000001",
                right: c"WindowsMR/holographic_controllerLHR-00000002",
            },
            serial_number: Property::PerHand {
                left: c"holographic_controllerLHR-00000001",
                right: c"holographic_controllerLHR-00000002",
            },
            tracking_system_name: c"WindowsMR", // TODO: Not sure if this is right, THIS IS A GUESS
            manufacturer_name: c"WindowsMR",
            legacy_buttons_mask: button_mask_from_ids!(
                System,
                ApplicationMenu,
                Grip,
                Axis0,
                Axis1,
                Axis2,
                A
            ),
        };
        &DEVICE_PROPERTIES
    }
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/microsoft/motion_controller"
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
                from: "squeeze/value",
                to: "squeeze/click",
                stop: true,
            },
            PathTranslation {
                from: "application_menu",
                to: "menu",
                stop: false,
            },
            PathTranslation {
                from: "trigger/click",
                to: "trigger/value",
                stop: true,
            },
            PathTranslation {
                from: "joystick",
                to: "thumbstick",
                stop: false,
            },
        ]
    }

    fn legacy_bindings(&self, stp: &dyn StringToPath) -> LegacyBindings {
        // Bindings mostly from OpenComposite
        // Games that use legacy bindings will typically just not use the thumbstick,
        // but most users would probably prefer to use it, so let's use it.
        // And also, games that use the face button can instead use the trackpad as one big button
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: stp.leftright("input/grip/pose"),
            },
            trigger: stp.leftright("input/trigger/value"),
            trigger_click: stp.leftright("input/trigger/value"),
            app_menu: stp.leftright("input/menu/click"),
            a: stp.leftright("input/trackpad/click"),
            squeeze: stp.leftright("input/squeeze/click"),
            squeeze_click: stp.leftright("input/squeeze/click"),
            main_xy: stp.leftright("input/thumbstick"),
            main_xy_click: stp.leftright("input/thumbstick/click"),
            main_xy_touch: vec![],
        }
    }

    fn skeletal_input_bindings(&self, stp: &dyn StringToPath) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: stp.leftright("input/trackpad/touch"),
            index_touch: stp.leftright("input/trigger/value"),
            index_curl: stp.leftright("input/trigger/value"),
            rest_curl: stp.leftright("input/squeeze/click"),
        }
    }

    fn legal_paths(&self) -> Box<[String]> {
        [
            "input/menu/click",
            "input/squeeze/click",
            "input/trigger/value",
            "input/thumbstick/x",
            "input/thumbstick/y",
            "input/thumbstick/click",
            "input/thumbstick",
            "input/trackpad/x",
            "input/trackpad/y",
            "input/trackpad/click",
            "input/trackpad/touch",
            "input/trackpad",
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

    fn offset_grip_pose(&self, _hand: Hand) -> Mat4 {
        Mat4::from_translation(Vec3::new(
            // From the models found here https://www.microsoft.com/en-us/download/details.aspx?id=56414
            0.0, 0.026310, -0.078693,
        ))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::{HolographicController, InteractionProfile};
    use crate::input::tests::Fixture;
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        base_verify_bindings(HolographicController.profile_path());
    }

    // Separate so the the tests can be reused for the Samsung Odyssey controllers
    pub(crate) fn base_verify_bindings(path: &'static str) {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/thumbstick/click".into(),
                "/user/hand/right/input/thumbstick/click".into(),
                "/user/hand/left/input/squeeze/click".into(),
                "/user/hand/right/input/squeeze/click".into(),
                "/user/hand/left/input/menu/click".into(),
                "/user/hand/right/input/menu/click".into(),
                "/user/hand/left/input/trackpad/click".into(),
                "/user/hand/right/input/trackpad/click".into(),
                "/user/hand/left/input/trackpad/touch".into(),
                "/user/hand/right/input/trackpad/touch".into(),
            ],
        );

        f.verify_bindings::<f32>(
            path,
            c"/actions/set1/boolact_asfloat",
            [
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
