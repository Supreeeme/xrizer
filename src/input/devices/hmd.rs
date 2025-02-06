use std::marker::PhantomData;

use openvr::{space_relation_to_openvr_pose, TrackedDevicePose_t};

use crate::openxr_data::{Compositor, OpenXrData, SessionData};

use super::tracked_device::{TrackedDevice, TrackedDeviceType, XrTrackedDevice};

pub struct XrHMD<C: Compositor> {
    pub device: XrTrackedDevice<C>,
    phantom: PhantomData<C>,
}

impl<C: Compositor> XrHMD<C> {
    pub fn new() -> Self {
        let mut hmd = Self {
            device: XrTrackedDevice::<C>::default(),
            phantom: PhantomData::default(),
        };

        hmd.device.init(0, TrackedDeviceType::HMD);
        hmd.device.set_connected(true);

        hmd
    }
}

impl<C: Compositor> TrackedDevice<C> for XrHMD<C> {
    fn get_pose(
        &self,
        origin: openvr::ETrackingUniverseOrigin,
        _xr_data: &OpenXrData<C>,
        session_data: &SessionData,
        display_time: openxr::Time,
    ) -> Option<TrackedDevicePose_t> {
        let (hmd_location, hmd_velocity) = {
            session_data
                .view_space
                .relate(session_data.get_space_for_origin(origin), display_time)
                .unwrap()
        };

        Some(space_relation_to_openvr_pose(hmd_location, hmd_velocity))
    }

    fn get_type(&self) -> TrackedDeviceType {
        self.device.device_type
    }

    fn connected(&self) -> bool {
        self.device.is_connected()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
