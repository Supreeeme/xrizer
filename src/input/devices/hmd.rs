use openvr as vr;

use crate::openxr_data::{OpenXrData, SessionData};

use super::tracked_device::XrTrackedDevice;

impl XrTrackedDevice {
    pub fn get_hmd_pose(
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
