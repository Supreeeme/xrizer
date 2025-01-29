pub mod knuckles;
pub mod oculus_touch;
pub mod simple_controller;
pub mod vive_controller;

use super::{action_manifest::ControllerType, devices::tracked_device::TrackedDeviceType, legacy::LegacyBindings};
use knuckles::Knuckles;
use oculus_touch::Touch;
use openxr as xr;
use simple_controller::SimpleController;
use std::ffi::CStr;
use vive_controller::ViveWands;

#[allow(private_interfaces)]
pub trait InteractionProfile: Sync + Send {
    fn profile_path(&self) -> &'static str;
    /// Corresponds to Prop_ModelNumber_String
    /// Can be pulled from a SteamVR System Report
    fn model(&self) -> &'static CStr;
    /// Corresponds to Prop_ControllerType_String
    /// Can be pulled from a SteamVR System Report
    fn openvr_controller_type(&self) -> &'static CStr;
    /// Corresponds to RenderModelName_String
    /// Can be found in SteamVR under resources/rendermodels (some are in driver subdirs)
    fn render_model_name(&self, _: TrackedDeviceType) -> &'static CStr;
    fn translate_map(&self) -> &'static [PathTranslation];

    fn legal_paths(&self) -> Box<[String]>;
    fn legacy_bindings(&self, string_to_path: &dyn StringToPath) -> LegacyBindings;
    fn offset_grip_pose(&self, pose: xr::Posef) -> xr::Posef {
        pose
    }
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
