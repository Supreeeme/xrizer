use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use openvr as vr;

use crate::{
    input::InteractionProfile,
    openxr_data::{AtomicPath, Hand, OpenXrData, SessionData},
};

use super::controller::ControllerVariables;

#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(dead_code)]
pub enum TrackedDeviceType {
    Hmd,
    Controller(ControllerVariables),
}

impl fmt::Display for TrackedDeviceType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Hmd => write!(f, "HMD"),
            Self::Controller(vars) => match vars.hand {
                Hand::Left => write!(f, "Left Hand"),
                Hand::Right => write!(f, "Right Hand"),
            },
        }
    }
}

impl TryFrom<vr::TrackedDeviceIndex_t> for TrackedDeviceType {
    type Error = ();

    fn try_from(value: vr::TrackedDeviceIndex_t) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Hmd),
            1 => Ok(Self::Controller(ControllerVariables::default())),
            2 => Ok(Self::Controller(ControllerVariables::default())),
            _ => Err(()),
        }
    }
}

pub struct XrTrackedDevice {
    pub device_type: TrackedDeviceType,
    pub interaction_profile: Mutex<Option<&'static dyn InteractionProfile>>,
    pub profile_path: AtomicPath,
    pub connected: AtomicBool,
    pub previous_connected: AtomicBool,
}

impl XrTrackedDevice {
    pub fn new(device_type: TrackedDeviceType) -> Self {
        Self {
            device_type,
            interaction_profile: Mutex::new(None),
            profile_path: AtomicPath::new(),
            connected: AtomicBool::new(false),
            previous_connected: AtomicBool::new(false),
        }
    }

    pub fn get_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        match self.device_type {
            TrackedDeviceType::Hmd => self.get_hmd_pose(xr_data, session_data, origin),
            TrackedDeviceType::Controller { .. } => {
                self.get_controller_pose(xr_data, session_data, origin)
            }
        }
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::Relaxed);
    }

    pub fn get_type(&self) -> TrackedDeviceType {
        self.device_type
    }

    pub fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile) {
        self.interaction_profile.lock().unwrap().replace(profile);
    }

    pub fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile> {
        self.interaction_profile.lock().unwrap().as_ref().copied()
    }
}
