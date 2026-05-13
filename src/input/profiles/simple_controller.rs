use super::{
    DynInputPath, InteractionProfile, MainAxisType, ProfileProperties, Property,
    SkeletalInputBindings, legal_paths, paths::*,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{Bindings, LegacyBindings, button_mask_from_id};
use crate::openxr_data::Hand;
use glam::Mat4;
use openvr::EVRButtonId as btn;

pub struct SimpleController;

impl InteractionProfile for SimpleController {
    type LegalPaths = legal_paths![Both::<(Select, Click), (Menu, Click)>];
    fn properties() -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::BothHands(c"generic"),
            openvr_controller_type: c"<unknown>",
            render_model_name: Property::BothHands(c"generic_controller"),
            main_axis: MainAxisType::Thumbstick,
            // TODO: These are just from the vive_controller. I'm not certain whether that's correct here
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
            legacy_buttons_mask: button_mask_from_ids!(
                btn::System,
                btn::ApplicationMenu,
                btn::Grip,
                btn::Axis0,
                btn::Axis1
            ),
        };
        &DEVICE_PROPERTIES
    }
    fn profile_path() -> &'static str {
        "/interaction_profiles/khr/simple_controller"
    }
    fn has_required_extensions(_: &openxr::ExtensionSet) -> bool {
        true
    }
    fn translate_path(path: DynInputPath) -> Option<DynInputPath> {
        match path {
            DynInputPath {
                subpath: DynSubpath::Trigger,
                component: Some(DynComponent::Click),
                ..
            } => Some(DynInputPath {
                subpath: DynSubpath::Select,
                ..path
            }),
            _ => None,
        }
    }

    fn legacy_bindings(c: &super::InputToXrPath<Self>) -> LegacyBindings {
        LegacyBindings {
            extra: Bindings {
                grip_pose: c.pose(),
            },
            trigger: c.leftright::<Select, Click, _, _>(),
            trigger_click: c.leftright::<Select, Click, _, _>(),
            app_menu: c.leftright::<Menu, Click, _, _>(),
            a: vec![],
            squeeze: c.leftright::<Menu, Click, _, _>(),
            squeeze_click: c.leftright::<Menu, Click, _, _>(),
            main_xy: vec![],
            main_xy_click: vec![],
            main_xy_touch: vec![],
            haptic: c.haptics(),
        }
    }

    fn skeletal_input_bindings(c: &super::InputToXrPath<Self>) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: Vec::new(),
            index_touch: c.leftright::<Select, Click, _, _>(),
            index_curl: c.leftright::<Select, Click, _, _>(),
            rest_curl: c.leftright::<Menu, Click, _, _>(),
        }
    }

    fn offset_grip_pose(_: Hand) -> Mat4 {
        Mat4::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::{InteractionProfile, SimpleController};
    use crate::input::tests::{ActionType, Fixture};
    use openxr as xr;

    impl Fixture {
        fn verify_no_bindings<T: ActionType>(&self, interaction_profile: &str, action_name: &CStr) {
            let handle = self.get_action_handle(action_name);
            let action = self.get_action::<T>(handle);
            let profile = self
                .input
                .openxr
                .instance
                .string_to_path(interaction_profile)
                .unwrap();

            assert!(
                fakexr::check_no_suggested_bindings(action, profile),
                "Expected no bindings for action {:?} - got {:#?}",
                action_name,
                fakexr::get_suggested_bindings(action, profile)
            );
        }
    }

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        let path = SimpleController::profile_path();
        f.load_actions(c"actions.json");
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/menu/click".into(),
                "/user/hand/right/input/menu/click".into(),
                "/user/hand/left/input/select/click".into(),
                "/user/hand/right/input/select/click".into(),
            ],
        );

        f.verify_no_bindings::<f32>(path, c"/actions/set1/in/vec1act");

        f.verify_no_bindings::<xr::Vector2f>(path, c"/actions/set1/in/vec2act");

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
