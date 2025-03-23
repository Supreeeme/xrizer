use std::sync::atomic::Ordering;

use super::tracked_device::{BaseDevice, TrackedDevice, TrackedDeviceType};

use openvr::{self as vr, space_relation_to_openvr_pose};

use crate::{
    openxr_data::{OpenXrData, SessionData},
    tracy_span,
};

pub struct XrHMD {
    base: BaseDevice,
}

impl XrHMD {
    pub fn new() -> Self {
        let hmd = Self {
            base: BaseDevice::new(TrackedDeviceType::HMD),
        };

        hmd.base.connected.store(true, Ordering::Relaxed);

        hmd
    }
}

impl TrackedDevice for XrHMD {
    fn get_pose(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        tracy_span!("XrHMD::get_pose");

        let (location, velocity) = {
            session_data
                .view_space
                .relate(
                    session_data.get_space_for_origin(origin),
                    xr_data.display_time.get(),
                )
                .ok()?
        };

        Some(space_relation_to_openvr_pose(location, velocity))
    }

    fn get_base_device(&self) -> &BaseDevice {
        &self.base
    }
}
