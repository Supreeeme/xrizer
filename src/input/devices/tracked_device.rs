use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use enum_dispatch::enum_dispatch;
use openvr as vr;

use crate::{
    input::InteractionProfile,
    openxr_data::{AtomicPath, Hand, OpenXrData, SessionData},
};

#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(dead_code)]
pub enum TrackedDeviceType {
    HMD,
    LeftHand,
    RightHand,
    GenericTracker,
    Unknown,
}

impl fmt::Display for TrackedDeviceType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::HMD => write!(f, "HMD"),
            Self::LeftHand => write!(f, "Left Hand"),
            Self::RightHand => write!(f, "Right Hand"),
            Self::GenericTracker => write!(f, "Generic Tracker"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl Into<vr::TrackedDeviceIndex_t> for TrackedDeviceType {
    fn into(self) -> vr::TrackedDeviceIndex_t {
        match self {
            Self::HMD => vr::k_unTrackedDeviceIndex_Hmd,
            Self::LeftHand => vr::k_unTrackedDeviceIndex_Hmd + 1,
            Self::RightHand => vr::k_unTrackedDeviceIndex_Hmd + 2,
            Self::GenericTracker => vr::k_unTrackedDeviceIndex_Hmd + 3,
            Self::Unknown => vr::k_unTrackedDeviceIndexInvalid,
        }
    }
}

impl From<Hand> for TrackedDeviceType {
    fn from(hand: Hand) -> Self {
        match hand {
            Hand::Left => Self::LeftHand,
            Hand::Right => Self::RightHand,
        }
    }
}

impl Into<vr::ETrackedControllerRole> for TrackedDeviceType {
    fn into(self) -> vr::ETrackedControllerRole {
        match self {
            Self::LeftHand => vr::ETrackedControllerRole::LeftHand,
            Self::RightHand => vr::ETrackedControllerRole::RightHand,
            _ => vr::ETrackedControllerRole::Invalid,
        }
    }
}

impl Into<vr::ETrackedDeviceClass> for TrackedDeviceType {
    fn into(self) -> vr::ETrackedDeviceClass {
        match self {
            Self::HMD => vr::ETrackedDeviceClass::HMD,
            Self::LeftHand | Self::RightHand => vr::ETrackedDeviceClass::Controller,
            Self::GenericTracker => vr::ETrackedDeviceClass::GenericTracker,
            Self::Unknown => vr::ETrackedDeviceClass::Invalid,
        }
    }
}

impl From<vr::ETrackedControllerRole> for TrackedDeviceType {
    fn from(role: vr::ETrackedControllerRole) -> Self {
        match role {
            vr::ETrackedControllerRole::LeftHand => Self::LeftHand,
            vr::ETrackedControllerRole::RightHand => Self::RightHand,
            _ => Self::Unknown,
        }
    }
}

#[enum_dispatch]
pub trait TrackedDevice {
    fn get_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t>;

    fn get_base_device(&self) -> &BaseDevice;

    fn connected(&self) -> bool {
        self.get_base_device().connected.load(Ordering::Relaxed)
    }

    fn set_connected(&self, connected: bool) {
        self.get_base_device()
            .connected
            .store(connected, Ordering::Relaxed);
    }

    fn get_type(&self) -> TrackedDeviceType {
        self.get_base_device().device_type
    }

    fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile) {
        self.get_base_device()
            .interaction_profile
            .lock()
            .unwrap()
            .replace(profile);
    }

    fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile> {
        self.get_base_device()
            .interaction_profile
            .lock()
            .unwrap()
            .as_ref()
            .copied()
    }
}

pub struct BaseDevice {
    pub device_type: TrackedDeviceType,
    pub interaction_profile: Mutex<Option<&'static dyn InteractionProfile>>,
    pub profile_path: AtomicPath,
    pub connected: AtomicBool,
    pub previous_connected: AtomicBool,
}

impl BaseDevice {
    pub fn new(device_type: TrackedDeviceType) -> Self {
        assert!(
            device_type != TrackedDeviceType::Unknown,
            "Cannot create a device with an unknown type"
        );

        Self {
            device_type,
            interaction_profile: Mutex::new(None),
            profile_path: AtomicPath::new(),
            connected: AtomicBool::new(false),
            previous_connected: AtomicBool::new(false),
        }
    }
}
