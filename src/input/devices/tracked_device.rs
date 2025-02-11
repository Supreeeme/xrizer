use std::{
    any::Any,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use openvr::{
    k_unTrackedDeviceIndexInvalid, ETrackedDeviceClass, ETrackedDeviceProperty,
    ETrackedPropertyError, ETrackingUniverseOrigin, TrackedDeviceIndex_t, TrackedDevicePose_t,
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

impl From<TrackedDeviceIndex_t> for TrackedDeviceType {
    fn from(index: TrackedDeviceIndex_t) -> Self {
        match index {
            0 => TrackedDeviceType::HMD,
            1 => TrackedDeviceType::LeftHand,
            2 => TrackedDeviceType::RightHand,
            _ => TrackedDeviceType::GenericTracker,
        }
    }
}

impl From<&str> for TrackedDeviceType {
    fn from(value: &str) -> Self {
        match value {
            "/user/hand/left" => TrackedDeviceType::LeftHand,
            "/user/hand/right" => TrackedDeviceType::RightHand,
            _ => TrackedDeviceType::Unknown,
        }
    }
}

impl Into<ETrackedDeviceClass> for TrackedDeviceType {
    fn into(self) -> ETrackedDeviceClass {
        match self {
            TrackedDeviceType::HMD => ETrackedDeviceClass::HMD,
            TrackedDeviceType::LeftHand | TrackedDeviceType::RightHand => {
                ETrackedDeviceClass::Controller
            }
            TrackedDeviceType::GenericTracker => ETrackedDeviceClass::GenericTracker,
            _ => ETrackedDeviceClass::Invalid,
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
    fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile);
    fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile>;

    fn get_bool_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> bool;
    fn get_float_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
        system: &crate::system::System,
    ) -> f32;
    fn get_int32_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> i32;
    fn get_uint64_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> u64;

    fn get_string_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> &str;

    fn get_device(&self) -> &XrTrackedDevice<C>;

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
#[macro_export]
macro_rules! string_prop {
    () => {
        if prop == $in {}
    };
}
#[macro_export]
macro_rules! prop {
    ($match:expr, $prop:ident, $in:expr) => {
        if $prop == $match {
            return $in;
        }
    };
}
#[macro_export]
macro_rules! set_property_error {
    ($err:ident, $error:expr) => {
        if let Some(err) = unsafe { $err.as_mut() } {
            *err = $error;
        }
    };
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
}

// Implementation for default behavior for devices to fall back on.
impl<C: Compositor> TrackedDevice<C> for XrTrackedDevice<C> {
    fn get_pose(
        &self,
        _origin: ETrackingUniverseOrigin,
        _xr_data: &OpenXrData<C>,
        _session_data: &SessionData,
        _display_time: openxr::Time,
    ) -> Option<TrackedDevicePose_t> {
        todo!()
    }

    fn get_type(&self) -> TrackedDeviceType {
        todo!()
    }

    fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn set_interaction_profile(&self, profile: &'static dyn InteractionProfile) {
        *self.interaction_profile.lock().unwrap() = Some(profile);
    }

    fn get_interaction_profile(&self) -> Option<&'static dyn InteractionProfile> {
        self.interaction_profile.lock().unwrap().as_ref().copied()
    }

    fn get_bool_property(
        &self,
        _prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> bool {
        set_property_error!(err, ETrackedPropertyError::UnknownProperty);

        false
    }

    fn get_float_property(
        &self,
        _prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
        _system: &crate::system::System,
    ) -> f32 {
        set_property_error!(err, ETrackedPropertyError::UnknownProperty);
        0.0
    }

    fn get_int32_property(
        &self,
        _prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> i32 {
        set_property_error!(err, ETrackedPropertyError::UnknownProperty);
        0
    }

    fn get_uint64_property(
        &self,
        _prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> u64 {
        set_property_error!(err, ETrackedPropertyError::UnknownProperty);
        0
    }

    fn get_string_property(
        &self,
        prop: ETrackedDeviceProperty,
        err: *mut ETrackedPropertyError,
    ) -> &str {
        prop!(
            ETrackedDeviceProperty::TrackingSystemName_String,
            prop,
            "oculus"
        );
        prop!(
            ETrackedDeviceProperty::ManufacturerName_String,
            prop,
            "Oculus"
        );
        prop!(
            ETrackedDeviceProperty::SerialNumber_String,
            prop,
            "<unknown>"
        );
        prop!(
            ETrackedDeviceProperty::RenderModelName_String,
            prop,
            "<unknown>"
        );

        // used by Firebird The Unfinished - see https://gitlab.com/znixian/OpenOVR/-/issues/58
        // Copied from SteamVR
        prop!(ETrackedDeviceProperty::DriverVersion_String, prop, "1.32.0");

        // From docs:
        // input profile to use for this device in the input system. Will default to tracking system
        // name if this isn't provided
        prop!(
            ETrackedDeviceProperty::InputProfilePath_String,
            prop,
            self.get_string_property(ETrackedDeviceProperty::TrackingSystemName_String, err)
        );

        set_property_error!(err, ETrackedPropertyError::UnknownProperty);
        ""
    }

    fn get_device(&self) -> &XrTrackedDevice<C> {
        self
    }

    fn as_any(&self) -> &dyn Any {
        todo!()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        todo!()
    }
}
