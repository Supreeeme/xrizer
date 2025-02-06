use std::{
    any::Any,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use openvr::{
    k_unTrackedDeviceIndexInvalid, ETrackingUniverseOrigin, TrackedDeviceIndex_t,
    TrackedDevicePose_t,
};

use crate::{
    input::InteractionProfile,
    openxr_data::{AtomicPath, Compositor, OpenXrData, SessionData},
};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TrackedDeviceType {
    HMD,
    LeftHand,
    RightHand,
    GenericTracker,
    Unknown,
}

impl TryFrom<TrackedDeviceIndex_t> for TrackedDeviceType {
    type Error = ();

    fn try_from(index: TrackedDeviceIndex_t) -> Result<Self, Self::Error> {
        match index {
            0 => Ok(TrackedDeviceType::HMD),
            1 => Ok(TrackedDeviceType::LeftHand),
            2 => Ok(TrackedDeviceType::RightHand),
            _ => Ok(TrackedDeviceType::GenericTracker),
        }
    }
}

impl TryFrom<&str> for TrackedDeviceType {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "/user/hand/left" => Ok(TrackedDeviceType::LeftHand),
            "/user/hand/right" => Ok(TrackedDeviceType::RightHand),
            _ => Ok(TrackedDeviceType::Unknown),
        }
    }
}

pub trait TrackedDevice<C: Compositor>: Sync + Send {
    fn get_pose(
        &self,
        origin: ETrackingUniverseOrigin,
        xr_data: &OpenXrData<C>,
        session_data: &SessionData,
        display_time: openxr::Time,
    ) -> Option<TrackedDevicePose_t>;
    fn get_type(&self) -> TrackedDeviceType;
    fn connected(&self) -> bool;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub struct XrTrackedDevice<C: Compositor> {
    device_index: TrackedDeviceIndex_t,
    pub device_type: TrackedDeviceType,
    pub interaction_profile: Mutex<Option<&'static dyn InteractionProfile>>,
    pub profile_path: AtomicPath,
    connected: AtomicBool,
    phantom: PhantomData<C>,
}

impl<C: Compositor> Default for XrTrackedDevice<C> {
    fn default() -> Self {
        Self {
            device_index: k_unTrackedDeviceIndexInvalid,
            device_type: TrackedDeviceType::Unknown,
            interaction_profile: Mutex::new(None),
            profile_path: AtomicPath::new(),
            connected: false.into(),
            phantom: PhantomData::default(),
        }
    }
}

impl<C: Compositor> XrTrackedDevice<C> {
    pub fn init(&mut self, device_index: TrackedDeviceIndex_t, device_type: TrackedDeviceType) {
        assert!(
            self.device_index == k_unTrackedDeviceIndexInvalid,
            "Cannot initialize tracked device twice - first with ID {}, then with ID {}",
            self.device_index,
            device_index
        );
        assert!(
            device_index != k_unTrackedDeviceIndexInvalid,
            "Cannot initialize tracked device with invalid ID k_unTrackedDeviceIndexInvalid"
        );
        assert!(
            device_type != TrackedDeviceType::Unknown,
            "Cannot initialize tracked device with unknown type TrackedDeviceType::Unknown"
        );

        self.device_index = device_index;
        self.device_type = device_type;
    }

    pub fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::Relaxed);
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile) {
        *self.interaction_profile.lock().unwrap() = Some(profile);
    }

    pub fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile> {
        self.interaction_profile.lock().unwrap().as_ref().copied()
    }
}
