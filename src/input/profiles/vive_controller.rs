use openvr::ETrackedDeviceProperty;

use super::{
    DevicePropertyTypes, HandValueType, InteractionProfile, PathTranslation, StringToPath,
};
use crate::input::{devices::tracked_device::TrackedDeviceType, legacy::LegacyBindings};

pub struct ViveWands;

impl InteractionProfile for ViveWands {
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/htc/vive_controller"
    }

    fn model(&self, hand: TrackedDeviceType) -> &'static str {
        match hand {
            TrackedDeviceType::LeftHand => self
                .get_property(ETrackedDeviceProperty::ModelNumber_String, hand)
                .unwrap()
                .as_string()
                .unwrap(),

            TrackedDeviceType::RightHand => self
                .get_property(ETrackedDeviceProperty::ModelNumber_String, hand)
                .unwrap()
                .as_string()
                .unwrap(),
            _ => unreachable!(),
        }
    }

    fn hmd_properties(&self) -> &'static [(ETrackedDeviceProperty, DevicePropertyTypes)] {
        &[
            (
                ETrackedDeviceProperty::ManufacturerName_String,
                DevicePropertyTypes::String("HTC"),
            ),
            (
                ETrackedDeviceProperty::ModelNumber_String,
                DevicePropertyTypes::String("Vive. MV"),
            ),
            (
                ETrackedDeviceProperty::ControllerType_String,
                DevicePropertyTypes::String("vive"),
            ),
        ]
    }

    fn controller_properties(
        &self,
    ) -> &'static [(ETrackedDeviceProperty, HandValueType<DevicePropertyTypes>)] {
        &[
            (
                ETrackedDeviceProperty::ModelNumber_String,
                HandValueType {
                    left: DevicePropertyTypes::String("Vive. Controller MV"),
                    right: None,
                },
            ),
            (
                ETrackedDeviceProperty::RenderModelName_String,
                HandValueType {
                    left: DevicePropertyTypes::String("vr_controller_vive_1_5"),
                    right: None,
                },
            ),
            (
                ETrackedDeviceProperty::ControllerType_String,
                HandValueType {
                    left: DevicePropertyTypes::String("vive_controller"),
                    right: None,
                },
            ),
        ]
    }

    fn openvr_controller_type(&self) -> &'static str {
        self.get_property(
            ETrackedDeviceProperty::ControllerType_String,
            TrackedDeviceType::LeftHand,
        )
        .unwrap()
        .as_string()
        .unwrap()
    }

    fn render_model_name(&self, hand: TrackedDeviceType) -> &'static str {
        match hand {
            TrackedDeviceType::LeftHand => self
                .get_property(ETrackedDeviceProperty::RenderModelName_String, hand)
                .unwrap()
                .as_string()
                .unwrap(),

            TrackedDeviceType::RightHand => self
                .get_property(ETrackedDeviceProperty::RenderModelName_String, hand)
                .unwrap()
                .as_string()
                .unwrap(),
            _ => unreachable!(),
        }
    }

    fn translate_map(&self) -> &'static [PathTranslation] {
        &[
            PathTranslation {
                from: "grip",
                to: "squeeze",
                stop: true,
            },
            PathTranslation {
                from: "trigger/pull",
                to: "trigger/value",
                stop: true,
            },
            PathTranslation {
                from: "trigger/click",
                to: "trigger/value",
                stop: true,
            },
            PathTranslation {
                from: "application_menu",
                to: "menu",
                stop: true,
            },
        ]
    }

    fn legal_paths(&self) -> Box<[String]> {
        [
            "input/squeeze/click",
            "input/menu/click",
            "input/trigger/click",
            "input/trigger/value",
            "input/trackpad",
            "input/trackpad/x",
            "input/trackpad/y",
            "input/trackpad/click",
            "input/trackpad/touch",
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

    fn legacy_bindings(&self, stp: &dyn StringToPath) -> LegacyBindings {
        LegacyBindings {
            grip_pose: stp.leftright("input/grip/pose"),
            aim_pose: stp.leftright("input/aim/pose"),
            trigger: stp.leftright("input/trigger/value"),
            trigger_click: stp.leftright("input/trigger/click"),
            app_menu: stp.leftright("input/menu/click"),
            squeeze: stp.leftright("input/squeeze/click"),
        }
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
        let path = ViveWands.profile_path();
        f.load_actions(c"actions.json");
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/squeeze/click".into(),
                "/user/hand/right/input/squeeze/click".into(),
                "/user/hand/left/input/menu/click".into(),
                "/user/hand/right/input/menu/click".into(),
                // Suggesting float paths for boolean inputs is legal
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
                "/user/hand/left/input/trackpad/click".into(),
                "/user/hand/left/input/trackpad/touch".into(),
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
