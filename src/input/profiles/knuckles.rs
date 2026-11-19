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

/// Index -> Vive wand compatibility mode, for games that only speak the legacy
/// `GetControllerState` API and only understand HTC Vive wands.
///
/// Enabled automatically for known legacy-only games (matched by executable name, see
/// `BUILTIN_APPS` and `~/.config/xrizer/index_as_vive_apps`), or for every app by setting
/// `XRIZER_INDEX_AS_VIVE` to anything other than `0`/empty.
/// When active, Knuckles controllers identify as Vive wands (controller type, model,
/// render model, axis types, supported buttons) and the legacy button layout is remapped:
///
/// | Vive wand               | Index controller                    |
/// |-------------------------|-------------------------------------|
/// | Trackpad (Axis0) xy     | Thumbstick                          |
/// | Trackpad click / touch  | Thumbstick click / touch            |
/// | A button                | A (bottom face button)              |
/// | Trigger (Axis1)         | Trigger                             |
/// | Grip button             | Squeeze                             |
/// | Application menu        | B                                   |
pub fn index_as_vive() -> bool {
    /// Games known to only work with Vive wands. Matched case-insensitively against the
    /// executable name (without `.exe`).
    const BUILTIN_APPS: &[&str] = &["SkyrimVR", "sksevr_loader"];

    static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        if std::env::var("XRIZER_INDEX_AS_VIVE").is_ok_and(|v| !v.is_empty() && v != "0") {
            log::info!("XRIZER_INDEX_AS_VIVE is set: Index controllers will present as Vive wands");
            return true;
        }

        let Some(app) = crate::openxr_data::get_app_name() else {
            return false;
        };

        // Extra apps can be listed one per line in $XDG_CONFIG_HOME/xrizer/index_as_vive_apps
        // (default ~/.config/xrizer/index_as_vive_apps), since env vars often don't reach
        // games launched through Wine/Proton.
        let extra = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))
            .and_then(|d| std::fs::read_to_string(d.join("xrizer").join("index_as_vive_apps")).ok())
            .unwrap_or_default();

        let matched = BUILTIN_APPS
            .iter()
            .copied()
            .chain(extra.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')))
            .any(|name| name.eq_ignore_ascii_case(&app));

        if matched {
            log::info!("{app} is on the Index->Vive list: Index controllers will present as Vive wands");
        } else {
            log::info!("Index->Vive compat not enabled for app '{app}'");
        }
        matched
    });
    *ENABLED
}

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
        if index_as_vive() {
            // Reuse the Vive wand's identity, but keep Index's serial/registered
            // device type so the device stays unique and is still tracked as lighthouse.
            static VIVE_COMPAT_PROPERTIES: ProfileProperties = ProfileProperties {
                model: Property::BothHands(c"Vive. MV"),
                openvr_controller_type: c"vive_controller",
                render_model_name: Property::BothHands(c"vr_controller_vive_1_5"),
                main_axis: MainAxisType::Trackpad,
                registered_device_type: Property::PerHand {
                    left: c"htc/vive_controllerLHR-FFFFFFF1",
                    right: c"htc/vive_controllerLHR-FFFFFFF2",
                },
                serial_number: Property::PerHand {
                    left: c"LHR-FFFFFFF1",
                    right: c"LHR-FFFFFFF2",
                },
                tracking_system_name: c"lighthouse",
                manufacturer_name: c"HTC",
                legacy_buttons_mask: button_mask_from_ids!(
                    EVRButtonId::System,
                    EVRButtonId::ApplicationMenu,
                    EVRButtonId::Grip,
                    EVRButtonId::A,
                    EVRButtonId::Axis0,
                    EVRButtonId::Axis1
                ),
            };
            return &VIVE_COMPAT_PROPERTIES;
        }
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
        if index_as_vive() {
            // The bottom face button (A) stays a separate A button, as on Oculus/Touch
            // controllers, rather than acting as a trackpad click.
            return LegacyBindings {
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
            };
        }
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
        if index_as_vive() {
            // The Vive render model's origin matches the raw OpenXR grip pose.
            return Mat4::IDENTITY;
        }
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
