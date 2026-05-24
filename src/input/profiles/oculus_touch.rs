use super::{
    InteractionProfile, Left, MainAxisType, ProfileProperties, Property, Right,
    SkeletalInputBindings, legal_paths, paths::*,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{self, LegacyBindings, button_mask_from_id};
use crate::input::profiles::{DynInputPath, InputToXrPath};
use crate::openxr_data::Hand;
use glam::{EulerRot, Mat4, Quat, Vec3};

pub struct OculusTouch;

impl InteractionProfile for OculusTouch {
    type LegalPaths = legal_paths![
        Both::<
            (Squeeze, Value),
            (Trigger, Value),
            (Trigger, Touch),
            (Thumbstick, ()),
            (Thumbstick, Click),
            (Thumbstick, Touch),
            (Thumbrest, Touch),
        >,
        Left::<(X, Click), (X, Touch), (Y, Click), (Y, Touch), (Menu, Click)>,
        Right::<(A, Click), (A, Touch), (B, Click), (B, Touch)>
    ];
    fn properties() -> &'static ProfileProperties {
        use openvr::EVRButtonId::*;
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::PerHand {
                left: c"Oculus Quest2 (Left Controller)",
                right: c"Oculus Quest2 (Right Controller)",
            },
            openvr_controller_type: c"oculus_touch",
            render_model_name: Property::PerHand {
                left: c"oculus_quest2_controller_left",
                right: c"oculus_quest2_controller_right",
            },
            registered_device_type: Property::PerHand {
                left: c"oculus/WMHD315M3010GV_Controller_Left",
                right: c"oculus/WMHD315M3010GV_Controller_Right",
            },
            serial_number: Property::PerHand {
                left: c"WMHD315M3010GV_Controller_Left",
                right: c"WMHD315M3010GV_Controller_Right",
            },
            tracking_system_name: c"oculus",
            manufacturer_name: c"Oculus",
            main_axis: MainAxisType::Thumbstick,
            legacy_buttons_mask: button_mask_from_ids!(
                System,
                ApplicationMenu,
                Grip,
                A,
                Axis0,
                Axis1,
                Axis2
            ),
        };
        &DEVICE_PROPERTIES
    }
    fn translate_path(path: DynInputPath) -> Option<DynInputPath> {
        match path {
            p @ DynInputPath {
                subpath: DynSubpath::Squeeze | DynSubpath::Trigger,
                component: Some(DynComponent::Click),
                ..
            } => Some(p.with_component(DynComponent::Value)),
            p @ DynInputPath {
                subpath: DynSubpath::A | DynSubpath::B | DynSubpath::X | DynSubpath::Y,
                component: Some(DynComponent::Value),
                ..
            } => Some(p.with_component(DynComponent::Click)),
            _ => None,
        }
    }
    fn profile_path() -> &'static str {
        "/interaction_profiles/oculus/touch_controller"
    }
    fn has_required_extensions(_: &openxr::ExtensionSet) -> bool {
        true
    }

    fn legacy_bindings(c: &InputToXrPath<Self>) -> LegacyBindings {
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: c.pose(),
            },
            trigger: c.leftright::<Trigger, Value, _, _>(),
            trigger_click: c.leftright::<Trigger, Value, _, _>(),
            app_menu: [
                c.into::<Left<Y, Click>, _>(),
                c.into::<Right<B, Click>, _>(),
            ]
            .concat(),
            a: [
                c.into::<Left<X, Click>, _>(),
                c.into::<Right<A, Click>, _>(),
            ]
            .concat(),
            squeeze_click: c.leftright::<Squeeze, Value, _, _>(),
            squeeze: c.leftright::<Squeeze, Value, _, _>(),
            main_xy: c.leftright::<Thumbstick, (), _, _>(),
            main_xy_click: c.leftright::<Thumbstick, Click, _, _>(),
            main_xy_touch: c.leftright::<Thumbstick, Touch, _, _>(),
            haptic: c.haptics(),
        }
    }

    fn skeletal_input_bindings(c: &InputToXrPath<Self>) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: [
                c.leftright::<Thumbstick, Touch, _, _>(),
                c.into::<Left<X, Touch>, _>(),
                c.into::<Left<Y, Touch>, _>(),
                c.into::<Right<A, Touch>, _>(),
                c.into::<Right<B, Touch>, _>(),
                c.leftright::<Thumbrest, Touch, _, _>(),
            ]
            .concat(),
            index_touch: c.leftright::<Trigger, Touch, _, _>(),
            index_curl: c.leftright::<Trigger, Value, _, _>(),
            rest_curl: c.leftright::<Squeeze, Value, _, _>(),
        }
    }

    fn offset_grip_pose(hand: Hand) -> Mat4 {
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
    use super::{InteractionProfile, OculusTouch};
    use crate::input::tests::Fixture;
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = OculusTouch::profile_path();
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
