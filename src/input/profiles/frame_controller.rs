use super::{
    InteractionProfile, Left, MainAxisType, ProfileProperties, Property, Right,
    SkeletalInputBindings, legal_paths, paths::*,
};
use crate::input::legacy::{self, LegacyBindings};
use crate::openxr_data::Hand;
use glam::{EulerRot, Mat4, Quat, Vec3};

pub struct FrameController;

impl InteractionProfile for FrameController {
    type LegalPaths = legal_paths![
        Both::<
            (Bumper, Click),
            (Bumper, Touch),
            (Squeeze, Click),
            (Squeeze, Touch),
            (Squeeze, Value),
            (Trigger, Click),
            (Trigger, Touch),
            (Trigger, Value),
            (Thumbstick, Click),
            (Thumbstick, Touch),
            (Thumbstick, ()),
        >,
        Left::<
            (DpadUp, Click),
            (DpadUp, Touch),
            (DpadLeft, Click),
            (DpadLeft, Touch),
            (DpadDown, Click),
            (DpadDown, Touch),
            (DpadRight, Click),
            (DpadRight, Touch),
            (View, Click),
            (View, Touch),
        >,
        Right::<
            (A, Click),
            (A, Touch),
            (B, Click),
            (B, Touch),
            (X, Click),
            (X, Touch),
            (Y, Click),
            (Y, Touch),
            (Menu, Click),
            (Menu, Touch),
        >
    ];

    fn profile_path() -> &'static str {
        "/interaction_profiles/valve/frame_controller_valve"
    }
    fn has_required_extensions(exts: &openxr::ExtensionSet) -> bool {
        exts.other
            .contains(&b"XR_VALVE_frame_controller_interaction\0".to_vec())
    }
    fn properties() -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::PerHand {
                left: c"Roy_Left",
                right: c"Roy_Right",
            },
            openvr_controller_type: c"frame_controller",
            render_model_name: Property::PerHand {
                left: c"{frame_controller}frame_controller_left",
                right: c"{frame_controller}frame_controller_right",
            },
            main_axis: MainAxisType::Thumbstick,
            registered_device_type: Property::BothHands(c""), // TODO make this optional, Frame controller does not have this
            serial_number: Property::PerHand {
                left: c"000000000001",
                right: c"000000000002",
            },
            tracking_system_name: c"cv",
            manufacturer_name: c"Valve",
            legacy_buttons_mask: 0,
        };
        &DEVICE_PROPERTIES
    }

    fn legacy_bindings(c: &super::InputToXrPath<Self>) -> LegacyBindings {
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: c.pose(),
            },
            app_menu: [
                c.into::<Left<DpadUp, Click>, _>(),
                c.into::<Left<DpadLeft, Click>, _>(),
                c.into::<Left<DpadRight, Click>, _>(),
                c.into::<Right<Y, Click>, _>(),
                c.into::<Right<X, Click>, _>(),
                c.into::<Right<B, Click>, _>(),
            ]
            .concat(),
            a: [
                c.into::<Left<DpadDown, Click>, _>(),
                c.into::<Right<A, Click>, _>(),
            ]
            .concat(),
            trigger: c.leftright::<Trigger, Value, _, _>(),
            trigger_click: c.leftright::<Trigger, Click, _, _>(),
            squeeze: c.leftright::<Squeeze, Value, _, _>(),
            squeeze_click: c.leftright::<Squeeze, Click, _, _>(),
            main_xy: c.leftright::<Thumbstick, (), _, _>(),
            main_xy_click: c.leftright::<Thumbstick, Click, _, _>(),
            main_xy_touch: c.leftright::<Thumbstick, Touch, _, _>(),
            haptic: c.haptics(),
        }
    }

    fn skeletal_input_bindings(c: &super::InputToXrPath<Self>) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: [
                c.leftright::<Thumbstick, Touch, _, _>(),
                c.into::<Left<DpadUp, Touch>, _>(),
                c.into::<Left<DpadLeft, Touch>, _>(),
                c.into::<Left<DpadDown, Touch>, _>(),
                c.into::<Left<DpadRight, Touch>, _>(),
                c.into::<Left<View, Touch>, _>(),
                c.into::<Right<A, Touch>, _>(),
                c.into::<Right<B, Touch>, _>(),
                c.into::<Right<X, Touch>, _>(),
                c.into::<Right<Y, Touch>, _>(),
                c.into::<Right<Menu, Touch>, _>(),
            ]
            .concat(),
            index_touch: [
                c.leftright::<Bumper, Touch, _, _>(),
                c.leftright::<Trigger, Touch, _, _>(),
            ]
            .concat(),
            index_curl: [
                c.leftright::<Bumper, Click, _, _>(),
                c.leftright::<Trigger, Value, _, _>(),
            ]
            .concat(),
            rest_curl: c.leftright::<Squeeze, Value, _, _>(),
        }
    }

    fn offset_grip_pose(hand: Hand) -> Mat4 {
        match hand {
            Hand::Left => Mat4::from_rotation_translation(
                Quat::from_euler(EulerRot::XYZ, 2.8091_f32.to_radians(), 0.0_f32, 0.0_f32),
                Vec3::new(-0.003117, -0.004277, 0.099501),
            )
            .inverse(),
            Hand::Right => Mat4::from_rotation_translation(
                Quat::from_euler(EulerRot::XYZ, 2.8091_f32.to_radians(), 0.0_f32, 0.0_f32),
                Vec3::new(0.003117, -0.004277, 0.099501),
            )
            .inverse(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameController, InteractionProfile};
    use crate::input::tests::Fixture;
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = FrameController::profile_path();
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/dpad_up/click".into(),
                "/user/hand/left/input/dpad_left/click".into(),
                "/user/hand/left/input/dpad_down/click".into(),
                "/user/hand/left/input/dpad_right/click".into(),
                "/user/hand/right/input/a/click".into(),
                "/user/hand/right/input/b/click".into(),
                "/user/hand/right/input/x/click".into(),
                "/user/hand/right/input/y/click".into(),
                "/user/hand/left/input/view/click".into(),
                "/user/hand/right/input/menu/click".into(),
                "/user/hand/left/input/bumper/click".into(),
                "/user/hand/right/input/bumper/click".into(),
                "/user/hand/left/input/trigger/click".into(),
                "/user/hand/right/input/thumbstick/click".into(),
                "/user/hand/right/input/thumbstick/touch".into(),
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
