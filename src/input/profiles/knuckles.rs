use super::{
    InteractionProfile, MainAxisType, ProfileProperties, Property, SkeletalInputBindings,
    legal_paths, paths::*,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{self, LegacyBindings, button_mask_from_id};
use crate::input::profiles::DynInputPath;
use crate::openxr_data::Hand;
use glam::{EulerRot, Mat4, Quat, Vec3};
use openvr::EVRButtonId;
use std::iter::Iterator;

pub struct Knuckles;

impl InteractionProfile for Knuckles {
    type LegalPaths = legal_paths![
        Both::<
            (A, Click),
            (A, Touch),
            (B, Click),
            (B, Touch),
            (Trigger, Click),
            (Trigger, Touch),
            (Trigger, Value),
            (Squeeze, Value),
            (Squeeze, Force),
            (Thumbstick, Click),
            (Thumbstick, Touch),
            (Thumbstick, ()),
            (Trackpad, Force),
            (Trackpad, Touch),
            (Trackpad, ()),
        >
    ];

    fn profile_path() -> &'static str {
        "/interaction_profiles/valve/index_controller"
    }
    fn has_required_extensions(_: &openxr::ExtensionSet) -> bool {
        true
    }
    fn properties() -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::PerHand {
                left: c"Knuckles Left",
                right: c"Knuckles Right",
            },
            openvr_controller_type: c"knuckles",
            render_model_name: Property::PerHand {
                left: c"{indexcontroller}valve_controller_knu_1_0_left",
                right: c"{indexcontroller}valve_controller_knu_1_0_right",
            },
            main_axis: MainAxisType::Thumbstick,
            registered_device_type: Property::PerHand {
                left: c"valve/index_controllerLHR-FFFFFFF1",
                right: c"valve/index_controllerLHR-FFFFFFF2",
            },
            serial_number: Property::PerHand {
                left: c"LHR-FFFFFFF1",
                right: c"LHR-FFFFFFF2",
            },
            tracking_system_name: c"lighthouse",
            manufacturer_name: c"Valve",
            legacy_buttons_mask: button_mask_from_ids!(
                EVRButtonId::System,
                EVRButtonId::ApplicationMenu,
                EVRButtonId::Grip,
                EVRButtonId::A,
                EVRButtonId::Axis0,
                EVRButtonId::Axis1,
                EVRButtonId::Axis2
            ),
        };
        &DEVICE_PROPERTIES
    }
    fn translate_path(path: DynInputPath) -> Option<DynInputPath> {
        match path {
            p @ DynInputPath {
                subpath: DynSubpath::Trackpad,
                component: Some(DynComponent::Click),
                ..
            } => Some(p.with_component(DynComponent::Force)),
            p @ DynInputPath {
                subpath: DynSubpath::Squeeze,
                component: Some(DynComponent::Touch),
                ..
            } => Some(p.with_component(DynComponent::Value)),
            _ => None,
        }
    }

    fn legacy_bindings(c: &super::InputToXrPath<Self>) -> LegacyBindings {
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: c.pose(),
            },
            app_menu: c.leftright::<B, Click, _, _>(),
            a: c.leftright::<A, Click, _, _>(),
            trigger: c.leftright::<Trigger, Value, _, _>(),
            trigger_click: c.leftright::<Trigger, Click, _, _>(),
            squeeze: c.leftright::<Squeeze, Value, _, _>(),
            squeeze_click: c.leftright::<Squeeze, Value, _, _>(),
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
                .chain(c.leftright::<Trackpad, Touch, _, _>())
                .chain(c.leftright::<A, Touch, _, _>())
                .chain(c.leftright::<B, Touch, _, _>())
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
    use crate::input::{ActionData, tests::Fixture};
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = Knuckles::profile_path();
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/a/click".into(),
                "/user/hand/right/input/a/click".into(),
                "/user/hand/left/input/b/click".into(),
                "/user/hand/right/input/b/click".into(),
                "/user/hand/left/input/squeeze/value".into(),
                "/user/hand/left/input/trigger/touch".into(),
                "/user/hand/right/input/trigger/touch".into(),
                "/user/hand/left/input/thumbstick/click".into(),
                "/user/hand/right/input/thumbstick/click".into(),
                "/user/hand/left/input/thumbstick/touch".into(),
                "/user/hand/right/input/thumbstick/touch".into(),
                "/user/hand/right/input/trackpad/touch".into(),
                "/user/hand/left/input/trackpad/force".into(),
                "/user/hand/right/input/trackpad/force".into(),
            ],
        );

        f.verify_bindings::<f32>(
            path,
            c"/actions/set1/boolact_asfloat",
            [
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
                "/user/hand/left/input/squeeze/value".into(),
            ],
        );

        f.verify_bindings::<f32>(
            path,
            c"/user/hand/left/input/trackpad/click-/actions/set1",
            ["/user/hand/left/input/trackpad/force".into()],
        );

        let handle = f.get_action_handle(c"/actions/set1/in/boolact");
        let data = f.input.openxr.session_data.get();
        let actions = data.input_data.get_loaded_actions().unwrap();
        let action = actions.try_get_action(handle).unwrap();
        let extra = actions.try_get_extra(handle).unwrap();

        let ActionData::Bool(_) = action else {
            panic!("no");
        };

        let grab_data = extra.grab_actions.as_ref().unwrap();
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

        f.verify_bindings::<xr::Vector2f>(
            path,
            c"/actions/set1/in/scrollact",
            ["/user/hand/left/input/thumbstick".into()],
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
