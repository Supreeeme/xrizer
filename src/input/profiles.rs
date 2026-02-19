pub mod knuckles;
pub mod oculus_touch;
pub mod oculus_touch_plus;
pub mod simple_controller;
pub mod vive_controller;
pub mod vive_focus3;
#[cfg(feature = "monado")]
pub mod vive_tracker;
use super::{
    action_manifest::ControllerType, legacy::LegacyBindings, skeletal::SkeletalInputBindings,
};
use crate::openxr_data::Hand;
use glam::Mat4;
use knuckles::Knuckles;
use oculus_touch::Touch;
use oculus_touch_plus::TouchPlus;
use openxr as xr;
use simple_controller::SimpleController;
use std::ffi::CStr;
use vive_controller::ViveWands;
use vive_focus3::ViveFocus3;

#[allow(private_interfaces)]
pub trait InteractionProfile: Sync + Send {
    fn profile_path(&self) -> &'static str;
    fn has_required_extensions(&self, enabled_extensions: &xr::ExtensionSet) -> bool;
    fn properties(&self) -> &'static ProfileProperties;
    fn translate_map(&self) -> &'static [PathTranslation];

    fn legal_paths(&self) -> Box<[String]>;
    fn legacy_bindings(&self, string_to_path: &dyn StringToPath) -> LegacyBindings;
    /// Can be extracted from SteamVR rendermodel files, it is the inverse of the "grip" or "openxr_grip" value
    fn offset_grip_pose(&self, _: Hand) -> Mat4;
    fn skeletal_input_bindings(&self, string_to_path: &dyn StringToPath) -> SkeletalInputBindings;
}

pub enum Property<T> {
    BothHands(T),
    PerHand { left: T, right: T },
}

impl<T> Property<T> {
    pub fn get(&self, hand: Hand) -> &T {
        match self {
            Self::BothHands(property) => property,
            Self::PerHand { left, right } => match hand {
                Hand::Left => left,
                Hand::Right => right,
            },
        }
    }
}

pub enum MainAxisType {
    Thumbstick,
    Trackpad,
}

pub struct ProfileProperties {
    /// Corresponds to Prop_ModelNumber_String
    /// Can be pulled from a SteamVR System Report
    pub model: Property<&'static CStr>,
    /// Corresponds to Prop_ControllerType_String
    /// Can be pulled from a SteamVR System Report
    pub openvr_controller_type: &'static CStr,
    /// Corresponds to RenderModelName_String
    /// Can be found in SteamVR under resources/rendermodels (some are in driver subdirs)
    pub render_model_name: Property<&'static CStr>,
    pub main_axis: MainAxisType,
    /// Corresponds to Prop_RegisteredDeviceType_String
    pub registered_device_type: Property<&'static CStr>,
    /// Corresponds to Prop_SerialNumber_String
    pub serial_number: Property<&'static CStr>,
    /// Corresponds to Prop_TrackingSystemName_String
    pub tracking_system_name: &'static CStr,
    /// Corresponds to Prop_ManufacturerName_String
    pub manufacturer_name: &'static CStr,
    /// Corresponds to Prop_SupportedButtons_Uint64
    /// Can be pulled from a SteamVR System Report
    pub legacy_buttons_mask: u64,
}

pub(super) struct PathTranslation {
    pub from: &'static str,
    pub to: &'static str,
    pub stop: bool,
}

pub trait StringToPath: for<'a> Fn(&'a str) -> xr::Path {
    #[inline]
    fn leftright(&self, path: &'static str) -> Vec<xr::Path> {
        vec![
            self(&format!("/user/hand/left/{path}")),
            self(&format!("/user/hand/right/{path}")),
        ]
    }
    #[inline]
    fn left(&self, path: &'static str) -> Vec<xr::Path> {
        vec![self(&format!("/user/hand/left/{path}"))]
    }
    #[inline]
    fn right(&self, path: &'static str) -> Vec<xr::Path> {
        vec![self(&format!("/user/hand/right/{path}"))]
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
                (ControllerType::OculusTouch, &TouchPlus),
                (ControllerType::OculusTouch, &Touch),
                (ControllerType::ViveController, &SimpleController),
                (ControllerType::ViveFocus3, &ViveFocus3),
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
