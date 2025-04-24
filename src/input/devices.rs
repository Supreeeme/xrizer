use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};

use openvr as vr;
use openxr as xr;

use crate::openxr_data::{AtomicPath, Hand, OpenXrData, SessionData};
use log::trace;

use super::InteractionProfile;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TrackedDeviceType {
    Hmd,
    Controller { hand: Hand },
}

impl TryFrom<vr::TrackedDeviceIndex_t> for TrackedDeviceType {
    type Error = ();

    fn try_from(value: vr::TrackedDeviceIndex_t) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Hmd),
            1 => Ok(Self::Controller { hand: Hand::Left }),
            2 => Ok(Self::Controller { hand: Hand::Right }),
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
    pose_cache: Mutex<Option<vr::TrackedDevicePose_t>>,
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
            pose_cache: Mutex::new(None),
        }
    }

    pub fn get_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        let mut pose_cache = self.pose_cache.lock().ok()?;
        if let Some(pose) = *pose_cache {
            return Some(pose);
        }

        *pose_cache = match self.device_type {
            TrackedDeviceType::Hmd => self.get_hmd_pose(xr_data, session_data, origin),
            TrackedDeviceType::Controller { .. } => {
                self.get_controller_pose(xr_data, session_data, origin)
            }
        };

        *pose_cache
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

    pub fn clear_pose_cache(&self) {
        std::mem::take(&mut *self.pose_cache.lock().unwrap());
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

    // Controllers
    fn get_controller_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        let legacy_actions = session_data.input_data.legacy_actions.get()?;

        let spaces = match self.get_controller_hand()? {
            Hand::Left => &legacy_actions.left_spaces,
            Hand::Right => &legacy_actions.right_spaces,
        };

        let (location, velocity) = if let Some(raw) = spaces.try_get_or_init_raw(
            &self.get_interaction_profile(),
            session_data,
            &legacy_actions.actions,
        ) {
            raw.relate(
                session_data.get_space_for_origin(origin),
                xr_data.display_time.get(),
            )
            .ok()?
        } else {
            trace!("Failed to get raw space, returning empty pose");
            (xr::SpaceLocation::default(), xr::SpaceVelocity::default())
        };

        Some(vr::space_relation_to_openvr_pose(location, velocity))
    }

    pub fn get_controller_hand(&self) -> Option<Hand> {
        match self.get_type() {
            TrackedDeviceType::Controller { hand, .. } => Some(hand),
            _ => None,
        }
    }

    // HMD
    fn get_hmd_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        let (location, velocity) = {
            session_data
                .view_space
                .relate(
                    session_data.get_space_for_origin(origin),
                    xr_data.display_time.get(),
                )
                .ok()?
        };

        Some(vr::space_relation_to_openvr_pose(location, velocity))
    }
}

pub struct TrackedDeviceList {
    devices: Vec<XrTrackedDevice>,
}

pub struct SubactionPaths {
    pub left: xr::Path,
    pub right: xr::Path,
}

impl SubactionPaths {
    pub fn new(instance: &xr::Instance) -> Self {
        let left = instance
            .string_to_path("/user/hand/left")
            .expect("Failed to convert string to path");
        let right = instance
            .string_to_path("/user/hand/right")
            .expect("Failed to convert string to path");

        Self { left, right }
    }
}

impl TrackedDeviceList {
    pub fn new() -> Self {
        Self {
            devices: vec![
                XrTrackedDevice::new(TrackedDeviceType::Hmd),
                XrTrackedDevice::new(TrackedDeviceType::Controller { hand: Hand::Left }),
                XrTrackedDevice::new(TrackedDeviceType::Controller { hand: Hand::Right }),
            ],
        }
    }

    pub fn get_device(&self, device_index: vr::TrackedDeviceIndex_t) -> Option<&XrTrackedDevice> {
        self.devices.get(device_index as usize)
    }

    pub fn get_hmd(&self) -> &XrTrackedDevice {
        unsafe { self.devices.get_unchecked(0) }
    }

    pub fn get_controller(&self, hand: Hand) -> &XrTrackedDevice {
        unsafe { self.devices.get_unchecked(hand as usize) }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, XrTrackedDevice> {
        self.devices.iter()
    }
}