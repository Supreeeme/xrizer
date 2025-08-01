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
use glam::Quat;
use glam::Vec3;
use openvr::EVRButtonId::{ApplicationMenu, Axis0, Axis1, Axis2, Grip, System, A};

pub struct ReverbG2Controller;

impl InteractionProfile for ReverbG2Controller {
    fn properties(&self) -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"WindowsMR"), // "VAC-151B" controllers
            openvr_controller_type: c"hpmotioncontroller",
            render_model_name: Property::BothHands(c"hpmotioncontroller"),
            main_axis: MainAxisType::Thumbstick,
            // TODO: I'm not certain whether that's correct here, THIS IS A GUESS
            registered_device_type: Property::PerHand {
                left: c"WindowsMR/hpmotioncontrollerLHR-00000001",
                right: c"WindowsMR/hpmotioncontrollerLHR-00000002",
            },
            serial_number: Property::PerHand {
                left: c"hpmotioncontrollerLHR-00000001",
                right: c"hpmotioncontrollerLHR-00000002",
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
                A,
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
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: stp.leftright("input/grip/pose"),
            },
            trigger: stp.leftright("input/trigger/value"),
            trigger_click: stp.leftright("input/trigger/value"),
            app_menu: stp.leftright("input/menu/click"),
            a: vec![
                stp("/user/hand/left/input/x/click"),
                stp("/user/hand/right/input/a/click"),
            ],
            squeeze: stp.leftright("input/squeeze/click"),
            squeeze_click: stp.leftright("input/squeeze/click"),
            main_xy: stp.leftright("input/thumbstick"),
            main_xy_click: stp.leftright("input/thumbstick/click"),
            main_xy_touch: vec![],
        }
    }

    fn skeletal_input_bindings(&self, stp: &dyn StringToPath) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: stp
                .leftright("input/thumbstick/click")
                .into_iter()
                .chain(stp.left("input/x/click"))
                .chain(stp.left("input/y/click"))
                .chain(stp.right("input/a/click"))
                .chain(stp.right("input/b/click"))
                .collect(),
            index_touch: stp.leftright("input/trigger/value"),
            index_curl: stp.leftright("input/trigger/value"),
            rest_curl: stp.leftright("input/squeeze/click"),
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
            "input/menu/click",
            "input/squeeze/click",
            "input/trigger/value",
            "input/thumbstick/x",
            "input/thumbstick/y",
            "input/thumbstick/click",
            "input/thumbstick",
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
        });

        left_only.chain(right_only).chain(both).collect()
    }

    fn offset_grip_pose(&self, hand: Hand) -> Mat4 {
        // From Monado
        Mat4::from_rotation_translation(
            Quat::from_xyzw(0.300705, 0.000000, 0.000000, 0.953717),
            Vec3::new(
                0.000683 * {
                    match hand {
                        Hand::Left => 1.0,
                        Hand::Right => -1.0,
                    }
                },
                -0.015332,
                0.068270,
            ),
        )
    }
}
