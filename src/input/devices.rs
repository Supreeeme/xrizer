use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::sync::Mutex;

use openvr as vr;
use openxr as xr;
use openxr_mndx_xdev_space::{SessionXDevExtensionMNDX, XDev, XR_MNDX_XDEV_SPACE_EXTENSION_NAME};

use crate::input::profiles::vive_tracker::ViveTracker;
use crate::openxr_data::{self, Hand, OpenXrData, SessionData};
use crate::tracy_span;
use log::trace;

use super::{profiles::MainAxisType, Input, InteractionProfile};

pub enum TrackedDeviceType {
    Hmd,
    Controller { hand: Hand },
    GenericTracker { space: xr::Space, serial: CString },
}

impl PartialEq for TrackedDeviceType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TrackedDeviceType::Hmd, TrackedDeviceType::Hmd) => true,
            (
                TrackedDeviceType::Controller { hand: hand_a },
                TrackedDeviceType::Controller { hand: hand_b },
            ) => hand_a == hand_b,
            (
                TrackedDeviceType::GenericTracker {
                    serial: serial1, ..
                },
                TrackedDeviceType::GenericTracker {
                    serial: serial2, ..
                },
            ) => serial1.as_ref() == serial2.as_ref(),
            _ => false,
        }
    }
}

impl Display for TrackedDeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackedDeviceType::Hmd => write!(f, "HMD"),
            TrackedDeviceType::Controller { hand } => write!(f, "Controller ({:?})", hand),
            TrackedDeviceType::GenericTracker { serial, .. } => {
                write!(f, "Generic Tracker ({})", serial.to_string_lossy())
            }
        }
    }
}

pub struct TrackedDevice {
    device_type: TrackedDeviceType,
    pub interaction_profile: Option<&'static dyn InteractionProfile>,
    pub profile_path: xr::Path,
    pub connected: bool,
    pub previous_connected: bool,
    pose_cache: Mutex<Option<vr::TrackedDevicePose_t>>,
}

fn get_hmd_pose(
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

fn get_controller_pose(
    xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
    session_data: &SessionData,
    controller: &TrackedDevice,
    origin: vr::ETrackingUniverseOrigin,
) -> Option<vr::TrackedDevicePose_t> {
    let pose_data = session_data.input_data.pose_data.get()?;

    let spaces = match controller.get_controller_hand().unwrap() {
        Hand::Left => &pose_data.left_space,
        Hand::Right => &pose_data.right_space,
    };

    let (location, velocity) = if let Some(raw) =
        spaces.try_get_or_init_raw(&controller.interaction_profile, session_data, pose_data)
    {
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

fn get_generic_tracker_pose(
    xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
    session_data: &SessionData,
    tracker: &TrackedDevice,
    origin: vr::ETrackingUniverseOrigin,
) -> Option<vr::TrackedDevicePose_t> {
    let space = match tracker.get_type() {
        TrackedDeviceType::GenericTracker { space, .. } => Some(space),
        _ => return None,
    };

    let (location, velocity) = space?
        .relate(
            session_data.get_space_for_origin(origin),
            xr_data.display_time.get(),
        )
        .ok()?;

    Some(vr::space_relation_to_openvr_pose(location, velocity))
}

impl TrackedDevice {
    pub(super) fn new(
        device_type: TrackedDeviceType,
        profile_path: Option<xr::Path>,
        interaction_profile: Option<&'static dyn InteractionProfile>,
    ) -> Self {
        Self {
            interaction_profile,
            profile_path: profile_path.unwrap_or(xr::Path::NULL),
            connected: device_type == TrackedDeviceType::Hmd,
            device_type,
            previous_connected: false,
            pose_cache: Mutex::new(None),
        }
    }

    pub fn get_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        let mut pose_cache = self.pose_cache.lock().unwrap();
        if let Some(pose) = *pose_cache {
            return Some(pose);
        }

        *pose_cache = match self.device_type {
            TrackedDeviceType::Hmd => get_hmd_pose(xr_data, session_data, origin),
            TrackedDeviceType::Controller { .. } => {
                get_controller_pose(xr_data, session_data, self, origin)
            }
            TrackedDeviceType::GenericTracker { .. } => {
                get_generic_tracker_pose(xr_data, session_data, self, origin)
            }
        };

        *pose_cache
    }

    pub fn clear_pose_cache(&self) {
        std::mem::take(&mut *self.pose_cache.lock().unwrap());
    }

    pub fn has_connected_changed(&mut self) -> bool {
        if self.previous_connected != self.connected {
            self.previous_connected = self.connected;
            true
        } else {
            false
        }
    }

    pub fn get_type(&self) -> &TrackedDeviceType {
        &self.device_type
    }

    pub fn get_controller_hand(&self) -> Option<Hand> {
        match self.device_type {
            TrackedDeviceType::Controller { hand, .. } => Some(hand),
            _ => None,
        }
    }

    fn get_string_property(&self, property: vr::ETrackedDeviceProperty) -> Option<&CStr> {
        let hand = match self.device_type {
            TrackedDeviceType::Controller { hand } => hand,
            _ => Hand::Left,
        };

        let data = self.interaction_profile.as_ref()?.properties();

        match property {
            // Audica likes to apply controller specific tweaks via this property
            vr::ETrackedDeviceProperty::ControllerType_String => Some(data.openvr_controller_type),
            // I Expect You To Die 3 identifies controllers with this property -
            // why it couldn't just use ControllerType instead is beyond me...
            // Because some controllers have different model names for each hand......
            vr::ETrackedDeviceProperty::ModelNumber_String => Some(*data.model.get(hand)),
            // Resonite won't recognize controllers without this
            vr::ETrackedDeviceProperty::RenderModelName_String => {
                Some(*data.render_model_name.get(hand))
            }
            vr::ETrackedDeviceProperty::RegisteredDeviceType_String => {
                Some(*data.registered_device_type.get(hand))
            }
            vr::ETrackedDeviceProperty::TrackingSystemName_String => {
                Some(data.tracking_system_name)
            }
            // Required for controllers to be acknowledged in I Expect You To Die 3
            vr::ETrackedDeviceProperty::SerialNumber_String => match self.get_type() {
                TrackedDeviceType::Controller { .. } => Some(*data.serial_number.get(hand)),
                TrackedDeviceType::GenericTracker { serial, .. } => Some(serial.as_c_str()),
                _ => None,
            },
            vr::ETrackedDeviceProperty::ManufacturerName_String => Some(data.manufacturer_name),
            _ => None,
        }
    }

    fn get_int_property(&self, property: vr::ETrackedDeviceProperty) -> Option<i32> {
        match self.device_type {
            TrackedDeviceType::Controller { .. } => {
                let profile = self.interaction_profile?;
                let data = profile.properties();

                match property {
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
                }
            }
            _ => None,
        }
    }

    fn get_uint_property(&self, property: vr::ETrackedDeviceProperty) -> Option<u64> {
        match self.device_type {
            TrackedDeviceType::Controller { .. } => {
                let profile = self.interaction_profile?;
                let data = profile.properties();

                match property {
                    vr::ETrackedDeviceProperty::SupportedButtons_Uint64 => {
                        Some(data.legacy_buttons_mask)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
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

pub struct TrackedDeviceList {
    devices: Vec<TrackedDevice>,
}

impl Default for TrackedDeviceList {
    fn default() -> Self {
        Self {
            devices: vec![TrackedDevice::new(TrackedDeviceType::Hmd, None, None)],
        }
    }
}

impl TrackedDeviceList {
    pub(super) fn get_device(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
    ) -> Option<&TrackedDevice> {
        self.devices.get(device_index as usize)
    }
    pub(super) fn get_device_mut(
        &mut self,
        device_index: vr::TrackedDeviceIndex_t,
    ) -> Option<&mut TrackedDevice> {
        self.devices.get_mut(device_index as usize)
    }

    pub(super) fn push_device(
        &mut self,
        device: TrackedDevice,
    ) -> Result<vr::TrackedDeviceIndex_t, vr::EVRInputError> {
        let index = self.devices.len() as vr::TrackedDeviceIndex_t;

        if index >= vr::k_unMaxTrackedDeviceCount {
            return Err(vr::EVRInputError::MaxCapacityReached);
        }

        self.devices.push(device);

        Ok(index)
    }

    pub(super) fn get_hmd(&self) -> &TrackedDevice {
        self.devices.first().unwrap()
    }

    pub(super) fn get_controller(&self, hand: Hand) -> Option<&TrackedDevice> {
        self.get_device(self.get_controller_index(hand)?)
    }
    pub(super) fn get_controller_mut(&mut self, hand: Hand) -> Option<&mut TrackedDevice> {
        self.get_device_mut(self.get_controller_index(hand)?)
    }

    fn get_controller_index(&self, hand: Hand) -> Option<vr::TrackedDeviceIndex_t> {
        self.iter()
            .enumerate()
            .find(|(_, device)| device.get_controller_hand() == Some(hand))
            .map(|(i, _)| i as vr::TrackedDeviceIndex_t)
    }

    pub(super) fn create_monado_generic_trackers(
        &mut self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
    ) -> xr::Result<()> {
        if !xr_data
            .enabled_extensions
            .other
            .contains(&XR_MNDX_XDEV_SPACE_EXTENSION_NAME.to_string())
        {
            return Ok(());
        }

        self.devices.retain(|device| {
            !matches!(device.device_type, TrackedDeviceType::GenericTracker { .. })
        });

        let max_generic_trackers = vr::k_unMaxTrackedDeviceCount as usize - self.devices.len();

        let mut xdevs: Vec<XDev> = session_data
            .session
            .get_xdev_list()?
            .enumerate_xdevs()?
            .into_iter()
            .filter(|xdev| {
                xdev.can_create_space() && xdev.name().to_lowercase().contains("tracker")
            })
            .collect();

        xdevs.truncate(max_generic_trackers);

        let trackers = xdevs.into_iter().map(|xdev| {
            let serial = CString::new(xdev.serial()).unwrap();
            let space = xdev.create_space(xr::Posef::IDENTITY).unwrap();
            let mut tracker = TrackedDevice::new(
                TrackedDeviceType::GenericTracker { serial, space },
                None,
                Some(&ViveTracker),
            );
            tracker.connected = true;
            tracker
        });
        self.devices.extend(trackers);

        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &TrackedDevice> {
        self.devices.iter()
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TrackedDevice> {
        self.devices.iter_mut()
    }
}

impl<C: openxr_data::Compositor> Input<C> {
    pub fn get_poses(
        &self,
        poses: &mut [vr::TrackedDevicePose_t],
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) {
        tracy_span!();
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();

        for (i, pose) in poses.iter_mut().enumerate() {
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
        }
    }

    pub fn get_controller_pose(
        &self,
        hand: Hand,
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) -> Option<vr::TrackedDevicePose_t> {
        let session_data = self.openxr.session_data.get();
        let controller_index = session_data
            .input_data
            .devices
            .read()
            .unwrap()
            .get_controller_index(hand)?;

        self.get_device_pose(controller_index, origin)
    }

    pub fn get_device_pose(
        &self,
        index: vr::TrackedDeviceIndex_t,
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) -> Option<vr::TrackedDevicePose_t> {
        tracy_span!();

        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();

        devices.get_device(index)?.get_pose(
            &self.openxr,
            &session_data,
            origin.unwrap_or(session_data.current_origin),
        )
    }

    pub fn is_device_connected(&self, index: vr::TrackedDeviceIndex_t) -> bool {
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();

        devices.get_device(index).is_some_and(|d| d.connected)
    }

    pub fn device_index_to_tracked_device_class(
        &self,
        index: vr::TrackedDeviceIndex_t,
    ) -> Option<vr::ETrackedDeviceClass> {
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();
        let device = devices.get_device(index)?;

        match device.get_type() {
            TrackedDeviceType::Hmd => Some(vr::ETrackedDeviceClass::HMD),
            TrackedDeviceType::Controller { .. } => Some(vr::ETrackedDeviceClass::Controller),
            TrackedDeviceType::GenericTracker { .. } => {
                Some(vr::ETrackedDeviceClass::GenericTracker)
            }
        }
    }

    pub fn device_index_to_hand(&self, index: vr::TrackedDeviceIndex_t) -> Option<Hand> {
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();
        let device = devices.get_device(index)?;

        device.get_controller_hand()
    }

    pub fn get_controller_device_index(&self, hand: Hand) -> Option<vr::TrackedDeviceIndex_t> {
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();

        devices.get_controller_index(hand)
    }

    pub fn get_device_string_tracked_property(
        &self,
        index: vr::TrackedDeviceIndex_t,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<CString> {
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();
        let device = devices.get_device(index)?;

        device.get_string_property(property).map(|s| s.to_owned())
    }

    pub fn get_device_int_tracked_property(
        &self,
        index: vr::TrackedDeviceIndex_t,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<i32> {
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();
        let device = devices.get_device(index)?;

        device.get_int_property(property)
    }

    pub fn get_device_uint_tracked_property(
        &self,
        index: vr::TrackedDeviceIndex_t,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<u64> {
        let session_data = self.openxr.session_data.get();
        let devices = session_data.input_data.devices.read().unwrap();
        let device = devices.get_device(index)?;

        device.get_uint_property(property)
    }
}
