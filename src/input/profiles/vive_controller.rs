use super::{
    DynInputPath, InteractionProfile, MainAxisType, ProfileProperties, Property,
    SkeletalInputBindings,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{self, LegacyBindings, button_mask_from_id};
use crate::input::profiles::{legal_paths, paths::*};
use crate::openxr_data::Hand;
use glam::Mat4;
use openvr::EVRButtonId::{ApplicationMenu, Axis0, Axis1, Grip, System};

pub struct ViveWands;

impl InteractionProfile for ViveWands {
    type LegalPaths = legal_paths![
        Both::<
            (Squeeze, Click),
            (Menu, Click),
            (Trigger, Click),
            (Trigger, Value),
            (Trackpad, Click),
            (Trackpad, Touch),
            (Trackpad, ()),
            (Trackpad, Vec2X),
            (Trackpad, Vec2Y),
        >
    ];

    fn properties() -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"Vive. MV"),
            openvr_controller_type: c"vive_controller",
            render_model_name: Property::BothHands(c"vr_controller_vive_1_5"),
            main_axis: MainAxisType::Trackpad,
            registered_device_type: Property::PerHand {
                left: c"htc/vive_controllerLHR-00000001",
                right: c"htc/vive_controllerLHR-00000002",
            },
            serial_number: Property::PerHand {
                left: c"LHR-00000001",
                right: c"LHR-00000002",
            },
            tracking_system_name: c"lighthouse",
            manufacturer_name: c"HTC",
            legacy_buttons_mask: button_mask_from_ids!(System, ApplicationMenu, Grip, Axis0, Axis1),
        };
        &DEVICE_PROPERTIES
    }
    fn profile_path() -> &'static str {
        "/interaction_profiles/htc/vive_controller"
    }
    fn has_required_extensions(_: &openxr::ExtensionSet) -> bool {
        true
    }
    fn translate_path(path: DynInputPath) -> Option<DynInputPath> {
        match path {
            DynInputPath {
                hand,
                subpath: subpath @ DynSubpath::Trigger,
                component: Some(DynComponent::Click),
            } => Some(DynInputPath {
                hand,
                subpath,
                component: Some(DynComponent::Value),
            }),
            DynInputPath {
                hand,
                subpath: subpath @ DynSubpath::Squeeze,
                component: Some(DynComponent::Value),
            } => Some(DynInputPath {
                hand,
                subpath,
                component: Some(DynComponent::Click),
            }),
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
            app_menu: c.leftright::<Menu, Click, _, _>(),
            a: vec![],
            squeeze: c.leftright::<Squeeze, Click, _, _>(),
            squeeze_click: c.leftright::<Squeeze, Click, _, _>(),
            main_xy: c.leftright::<Trackpad, (), _, _>(),
            main_xy_click: c.leftright::<Trackpad, Click, _, _>(),
            main_xy_touch: c.leftright::<Trackpad, Touch, _, _>(),
            haptic: c.haptics(),
        }
    }

    fn skeletal_input_bindings(c: &super::InputToXrPath<Self>) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: c
                .leftright::<Trackpad, Click, _, _>()
                .into_iter()
                .chain(c.leftright::<Trackpad, Touch, _, _>())
                .collect(),
            index_touch: c.leftright::<Trigger, Click, _, _>(),
            index_curl: c.leftright::<Trigger, Value, _, _>(),
            rest_curl: c.leftright::<Squeeze, Click, _, _>(),
        }
    }

    fn offset_grip_pose(_: Hand) -> Mat4 {
        Mat4::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::{InteractionProfile, ViveWands};
    use crate::input::tests::Fixture;
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        let path = ViveWands::profile_path();
        f.load_actions(c"actions.json");
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/squeeze/click".into(),
                "/user/hand/right/input/squeeze/click".into(),
                "/user/hand/left/input/menu/click".into(),
                "/user/hand/right/input/menu/click".into(),
                "/user/hand/left/input/trackpad/click".into(),
                "/user/hand/left/input/trackpad/touch".into(),
            ],
        );

        // bindings for boolact reading from float inputs
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
                "/user/hand/right/input/squeeze/click".into(),
            ],
        );

        f.verify_bindings::<xr::Vector2f>(
            path,
            c"/actions/set1/in/vec2act",
            [
                "/user/hand/left/input/trackpad".into(),
                "/user/hand/right/input/trackpad".into(),
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
