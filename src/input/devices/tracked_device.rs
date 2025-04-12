use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use openvr as vr;
use openxr as xr;

use crate::{
    input::InteractionProfile,
    openxr_data::{AtomicPath, Hand, OpenXrData, SessionData},
};

#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(dead_code)]
pub enum TrackedDeviceType {
    Hmd,
    Controller {
        hand: Hand,
        subaction_path: xr::Path,
    },
}

impl fmt::Display for TrackedDeviceType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Hmd => write!(f, "HMD"),
            Self::Controller { hand, .. } => match hand {
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
            1 => Ok(Self::Controller {
                hand: Hand::Left,
                subaction_path: xr::Path::default(),
            }),
            2 => Ok(Self::Controller {
                hand: Hand::Right,
                subaction_path: xr::Path::default(),
            }),
            _ => Err(()),
        }
    }
}

pub struct XrTrackedDevice {
    device_type: TrackedDeviceType,
    interaction_profile: Mutex<Option<&'static dyn InteractionProfile>>,
    profile_path: AtomicPath,
    connected: AtomicBool,
    previous_connected: AtomicBool,
}

impl XrTrackedDevice {
    pub fn new(device_type: TrackedDeviceType) -> Self {
        Self {
            device_type,
            interaction_profile: Mutex::new(None),
            profile_path: AtomicPath::new(),
            connected: if device_type == TrackedDeviceType::Hmd {
                true.into()
            } else {
                false.into()
            },
            previous_connected: false.into(),
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

    pub fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile) {
        self.interaction_profile.lock().unwrap().replace(profile);
    }

    pub fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile> {
        self.interaction_profile.lock().unwrap().as_ref().copied()
    }

    pub fn get_profile_path(&self) -> xr::Path {
        self.profile_path.load()
    }

    pub fn set_profile_path(&self, path: xr::Path) {
        self.profile_path.store(path);
    }

    pub fn compare_exchange_connected(&self) -> Result<bool, bool> {
        let current = self.connected();

        self.previous_connected.compare_exchange(
            !current,
            current,
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
    }

    pub fn get_type(&self) -> TrackedDeviceType {
        self.device_type
    }
}
