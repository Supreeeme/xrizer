use super::{
    InteractionProfile, MainAxisType, ProfileProperties, Property, SkeletalInputBindings,
    legal_paths, paths::*,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{self, LegacyBindings, button_mask_from_id};
use crate::input::profiles::DynInputPath;
use crate::openxr_data::Hand;
use glam::{EulerRot, Mat4, Quat, Vec3};
use openvr::EVRButtonId as btn;

pub struct ViveFocus3;

impl InteractionProfile for ViveFocus3 {
    type LegalPaths = legal_paths![
        Both::<
            (Squeeze, Value),
            (Squeeze, Click),
            (Squeeze, Touch),
            (Trigger, Value),
            (Trigger, Click),
            (Trigger, Touch),
            (Thumbstick, ()),
            (Thumbstick, Click),
            (Thumbstick, Touch),
            (Thumbrest, Touch),
        >,
        Left::<(X, Click), (Y, Click), (Menu, Click)>,
        Right::<(A, Click), (B, Click)>
    ];

    fn properties() -> &'static ProfileProperties {
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
                btn::System,
                btn::ApplicationMenu,
                btn::Grip,
                btn::A,
                btn::Axis1,
                btn::Axis2,
                btn::Axis3,
                btn::Axis4,
            ),
        };
        &DEVICE_PROPERTIES
    }
    fn profile_path() -> &'static str {
        "/interaction_profiles/htc/vive_focus3_controller"
    }
    fn has_required_extensions(enabled_extensions: &openxr::ExtensionSet) -> bool {
        enabled_extensions.htc_vive_focus3_controller_interaction
    }
    fn translate_path(path: DynInputPath) -> Option<DynInputPath> {
        match path {
            path @ DynInputPath {
                subpath: DynSubpath::A | DynSubpath::B | DynSubpath::X | DynSubpath::Y,
                component: Some(DynComponent::Touch),
                ..
            } => Some(path.with_component(DynComponent::Click)),
            _ => None,
        }
    }

    fn legacy_bindings(c: &super::InputToXrPath<Self>) -> LegacyBindings {
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: c.pose(),
            },
            trigger: c.leftright::<Trigger, Value, _, _>(),
            trigger_click: c.leftright::<Trigger, Click, _, _>(),
            app_menu: [
                c.into::<super::Left<Y, Click>, _>(),
                c.into::<super::Right<B, Click>, _>(),
            ]
            .concat(),
            a: [
                c.into::<super::Left<X, Click>, _>(),
                c.into::<super::Right<A, Click>, _>(),
            ]
            .concat(),
            squeeze_click: c.leftright::<Squeeze, Click, _, _>(),
            squeeze: c.leftright::<Squeeze, Value, _, _>(),
            main_xy: c.leftright::<Thumbstick, (), _, _>(),
            main_xy_click: c.leftright::<Thumbstick, Click, _, _>(),
            main_xy_touch: c.leftright::<Thumbstick, Touch, _, _>(),
            haptic: c.haptics(),
        }
    }

    fn skeletal_input_bindings(c: &super::InputToXrPath<Self>) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: c
                .leftright::<Thumbstick, Touch, _, _>()
                .into_iter()
                .chain(c.into::<super::Left<X, Click>, _>())
                .chain(c.into::<super::Left<Y, Click>, _>())
                .chain(c.into::<super::Right<A, Click>, _>())
                .chain(c.into::<super::Right<B, Click>, _>())
                .chain(c.leftright::<Thumbrest, Touch, _, _>())
                .collect(),
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
    use super::{InteractionProfile, ViveFocus3};
    use crate::input::tests::Fixture;
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = ViveFocus3::profile_path();
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
