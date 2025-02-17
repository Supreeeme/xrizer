pub mod knuckles;
pub mod oculus_touch;
pub mod simple_controller;
pub mod vive_controller;
pub mod vive_tracker;

use super::{
    action_manifest::ControllerType, devices::tracked_device::TrackedDeviceType,
    legacy::LegacyBindings,
};
use knuckles::Knuckles;
use oculus_touch::Touch;
use openvr::ETrackedDeviceProperty;
use openxr as xr;
use simple_controller::SimpleController;
use vive_controller::ViveWands;

#[allow(private_interfaces, dead_code)]
pub trait InteractionProfile: Sync + Send {
    fn profile_path(&self) -> &'static str;
    /// Corresponds to Prop_ModelNumber_String
    /// Can be pulled from a SteamVR System Report
    fn model(&self, _: TrackedDeviceType) -> &'static str;
    /// Corresponds to Prop_ControllerType_String
    /// Can be pulled from a SteamVR System Report
    fn openvr_controller_type(&self) -> &'static str;

    fn hmd_properties(&self) -> &'static [(ETrackedDeviceProperty, DevicePropertyTypes)];
    fn controller_properties(
        &self,
    ) -> &'static [(ETrackedDeviceProperty, HandValueType<DevicePropertyTypes>)];

    fn get_property(
        &self,
        prop: ETrackedDeviceProperty,
        hand: TrackedDeviceType,
    ) -> Option<DevicePropertyTypes> {
        if hand == TrackedDeviceType::Unknown {
            return None;
        }

        let controller_props = self.controller_properties();
        let hmd_props = self.hmd_properties();

        let controller_prop = controller_props.iter().find(|(p, _)| *p == prop);
        let hmd_prop = hmd_props.iter().find(|(p, _)| *p == prop);

        if controller_prop.is_none() && hmd_prop.is_none() {
            return None;
        }

        let controller_value = controller_prop.map(|(_, v)| v);
        let hmd_value = hmd_prop.map(|(_, v)| v);

        if controller_value.is_none() {
            return hmd_value.copied();
        }

        let controller_value = controller_value.unwrap();

        if hand == TrackedDeviceType::RightHand && controller_value.right.is_some() {
            return controller_value.right;
        } else {
            return Some(controller_value.left);
        }
    }

    /// Corresponds to RenderModelName_String
    /// Can be found in SteamVR under resources/rendermodels (some are in driver subdirs)
    fn render_model_name(&self, _: TrackedDeviceType) -> &'static str;
    fn translate_map(&self) -> &'static [PathTranslation];

    fn legal_paths(&self) -> Box<[String]>;
    fn legacy_bindings(&self, string_to_path: &dyn StringToPath) -> Option<LegacyBindings>;
    fn offset_grip_pose(&self, pose: xr::Posef) -> xr::Posef {
        pose
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(dead_code)]
pub(super) enum DevicePropertyTypes {
    Bool(bool),
    Float(f32),
    Int32(i32),
    Uint64(u64),
    String(&'static str),
}

#[allow(dead_code)]
impl DevicePropertyTypes {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DevicePropertyTypes::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_float(&self) -> Option<f32> {
        match self {
            DevicePropertyTypes::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn as_int32(&self) -> Option<i32> {
        match self {
            DevicePropertyTypes::Int32(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_uint64(&self) -> Option<u64> {
        match self {
            DevicePropertyTypes::Uint64(u) => Some(*u),
            _ => None,
        }
    }
    pub fn as_string(&self) -> Option<&'static str> {
        match self {
            DevicePropertyTypes::String(s) => Some(*s),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(super) struct HandValueType<T> {
    pub left: T,
    pub right: Option<T>,
}

pub(super) struct PathTranslation {
    pub from: &'static str,
    pub to: &'static str,
    pub stop: bool,
}

pub(super) trait StringToPath: for<'a> Fn(&'a str) -> xr::Path {
    #[inline]
    fn leftright(&self, path: &'static str) -> Vec<xr::Path> {
        vec![
            self(&format!("/user/hand/left/{path}")),
            self(&format!("/user/hand/right/{path}")),
        ]
    }
}
impl<F> StringToPath for F where F: for<'a> Fn(&'a str) -> xr::Path {}

pub struct Profiles {
    pub(super) list: &'static [(ControllerType, &'static dyn InteractionProfile)],
}

impl Profiles {
    #[inline]
    pub fn get() -> &'static Self {
        // Add supported interaction profiles here.
        static P: Profiles = Profiles {
            list: &[
                (ControllerType::ViveController, &ViveWands),
                (ControllerType::Knuckles, &Knuckles),
                (ControllerType::OculusTouch, &Touch),
                (ControllerType::ViveController, &SimpleController),
            ],
        };
        &P
    }

    #[inline]
    pub fn profiles_iter(&self) -> impl Iterator<Item = &'static dyn InteractionProfile> {
        self.list.iter().map(|(_, p)| *p)
    }

    pub fn profile_from_name(&self, name: &str) -> Option<&'static dyn InteractionProfile> {
        self.list
            .iter()
            .find_map(|(_, p)| (p.profile_path() == name).then_some(*p))
    }
}
