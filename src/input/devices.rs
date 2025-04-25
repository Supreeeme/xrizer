use std::{
    ffi::CStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use openvr as vr;
use openxr as xr;

use crate::openxr_data::{self, AtomicPath, Hand, OpenXrData, SessionData};
use crate::tracy_span;
use log::trace;

use super::{profiles::MainAxisType, Input, InteractionProfile};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TrackedDeviceType {
    Hmd,
    Controller { hand: Hand },
}
pub struct XrTrackedDevice {
    device_type: TrackedDeviceType,
    interaction_profile: Mutex<Option<&'static dyn InteractionProfile>>,
    profile_path: AtomicPath,
    connected: AtomicBool,
    previous_connected: AtomicBool,
    pose_cache: Mutex<Option<vr::TrackedDevicePose_t>>,
}

pub struct TrackedDeviceCreateInfo {
    pub device_type: TrackedDeviceType,
    pub profile_path: Option<xr::Path>,
    pub interaction_profile: Option<&'static dyn InteractionProfile>,
}

impl XrTrackedDevice {
    pub fn new(info: TrackedDeviceCreateInfo) -> Self {
        let profile_path = AtomicPath::new();

        if let Some(path) = info.profile_path {
            profile_path.store(path);
        }

        Self {
            device_type: info.device_type,
            interaction_profile: Mutex::new(info.interaction_profile),
            profile_path,
            connected: if info.device_type == TrackedDeviceType::Hmd {
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
    pub(super) fn new() -> Self {
        Self {
            devices: vec![XrTrackedDevice::new(TrackedDeviceCreateInfo {
                device_type: TrackedDeviceType::Hmd,
                profile_path: None,
                interaction_profile: None,
            })],
        }
    }

    pub(super) fn get_device(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
    ) -> Option<&XrTrackedDevice> {
        self.devices.get(device_index as usize)
    }

    pub(super) fn push_device(
        &mut self,
        device: XrTrackedDevice,
    ) -> Result<vr::TrackedDeviceIndex_t, vr::EVRInputError> {
        let index = self.devices.len() as vr::TrackedDeviceIndex_t;

        if index >= vr::k_unMaxTrackedDeviceCount {
            return Err(vr::EVRInputError::MaxCapacityReached);
        }

        self.devices.push(device);

        Ok(index)
    }

    pub(super) fn get_hmd(&self) -> &XrTrackedDevice {
        unsafe { self.devices.get_unchecked(0) }
    }

    pub(super) fn get_controller(&self, hand: Hand) -> Option<&XrTrackedDevice> {
        self.get_device(self.get_controller_index(hand))
    }

    fn get_controller_index(&self, hand: Hand) -> vr::TrackedDeviceIndex_t {
        self.iter()
            .enumerate()
            .find(|(_, device)| device.get_controller_hand() == Some(hand))
            .map(|(i, _)| i as vr::TrackedDeviceIndex_t)
            .unwrap_or(vr::k_unTrackedDeviceIndexInvalid)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, XrTrackedDevice> {
        self.devices.iter()
    }
}

impl<C: openxr_data::Compositor> Input<C> {
    pub fn get_poses(
        &self,
        poses: &mut [vr::TrackedDevicePose_t],
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) {
        tracy_span!();
        let devices = self.devices.read().unwrap();
        let session_data = self.openxr.session_data.get();

        poses.iter_mut().enumerate().for_each(|(i, pose)| {
            let device = devices.get_device(i as u32);

            if let Some(device) = device {
                *pose = device
                    .get_pose(
                        &self.openxr,
                        &session_data,
                        origin.unwrap_or(session_data.current_origin),
                    )
                    .unwrap_or_default();
            }
        });
    }

    pub fn get_controller_pose(
        &self,
        hand: Hand,
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) -> Option<vr::TrackedDevicePose_t> {
        let controller_index = self.devices.read().ok()?.get_controller_index(hand);

        self.get_device_pose(controller_index, origin)
    }

    pub fn get_device_pose(
        &self,
        index: vr::TrackedDeviceIndex_t,
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) -> Option<vr::TrackedDevicePose_t> {
        tracy_span!();

        let session_data = self.openxr.session_data.get();

        self.devices.read().ok()?.get_device(index)?.get_pose(
            &self.openxr,
            &session_data,
            origin.unwrap_or(session_data.current_origin),
        )
    }

    pub fn is_device_connected(&self, index: vr::TrackedDeviceIndex_t) -> bool {
        let Some(devices) = self.devices.read().ok() else {
            return false;
        };

        let Some(device) = devices.get_device(index) else {
            return false;
        };

        device.connected()
    }

    pub fn device_index_to_device_type(
        &self,
        index: vr::TrackedDeviceIndex_t,
    ) -> Option<TrackedDeviceType> {
        let devices = self.devices.read().ok()?;
        let device = devices.get_device(index)?;

        Some(device.get_type())
    }

    pub fn device_index_to_hand(&self, index: vr::TrackedDeviceIndex_t) -> Option<Hand> {
        let devices = self.devices.read().ok()?;
        let device = devices.get_device(index)?;

        device.get_controller_hand()
    }

    pub fn get_controller_device_index(&self, hand: Hand) -> Option<vr::TrackedDeviceIndex_t> {
        let devices = self.devices.read().ok()?;
        let controller_index = devices.get_controller_index(hand);

        if controller_index == vr::k_unTrackedDeviceIndexInvalid {
            return None;
        }

        Some(controller_index)
    }

    fn get_profile_data(&self, hand: Hand) -> Option<&super::profiles::ProfileProperties> {
        let devices = self.devices.read().ok()?;
        let controller = devices.get_controller(hand)?;

        self.profile_map
            .get(&controller.get_profile_path())
            .map(|v| &**v)
    }

    pub fn get_controller_string_tracked_property(
        &self,
        hand: Hand,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<&'static CStr> {
        self.get_profile_data(hand).and_then(|data| {
            match property {
                // Audica likes to apply controller specific tweaks via this property
                vr::ETrackedDeviceProperty::ControllerType_String => {
                    Some(data.openvr_controller_type)
                }
                // I Expect You To Die 3 identifies controllers with this property -
                // why it couldn't just use ControllerType instead is beyond me...
                vr::ETrackedDeviceProperty::ModelNumber_String => Some(data.model),
                // Resonite won't recognize controllers without this
                vr::ETrackedDeviceProperty::RenderModelName_String => {
                    Some(*data.render_model_name.get(hand))
                }
                // Required for controllers to be acknowledged in I Expect You To Die 3
                vr::ETrackedDeviceProperty::SerialNumber_String
                | vr::ETrackedDeviceProperty::ManufacturerName_String => Some(c"<unknown>"),
                _ => None,
            }
        })
    }

    pub fn get_controller_int_tracked_property(
        &self,
        hand: Hand,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<i32> {
        self.get_profile_data(hand).and_then(|data| match property {
            vr::ETrackedDeviceProperty::Axis0Type_Int32 => match data.main_axis {
                MainAxisType::Thumbstick => Some(vr::EVRControllerAxisType::Joystick as _),
                MainAxisType::Trackpad => Some(vr::EVRControllerAxisType::TrackPad as _),
            },
            vr::ETrackedDeviceProperty::Axis1Type_Int32 => {
                Some(vr::EVRControllerAxisType::Trigger as _)
            }
            vr::ETrackedDeviceProperty::Axis2Type_Int32 => {
                // This is actually the grip, and gets recognized as such
                Some(vr::EVRControllerAxisType::Trigger as _)
            }
            // TODO: report knuckles trackpad?
            vr::ETrackedDeviceProperty::Axis3Type_Int32
            | vr::ETrackedDeviceProperty::Axis4Type_Int32 => {
                Some(vr::EVRControllerAxisType::None as _)
            }
            _ => None,
        })
    }
}
