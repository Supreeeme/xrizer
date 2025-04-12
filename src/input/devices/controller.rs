use openvr as vr;
use openxr as xr;

use crate::openxr_data::{Hand, OpenXrData, SessionData};

use super::tracked_device::{TrackedDeviceType, XrTrackedDevice};

use log::trace;

impl XrTrackedDevice {
    pub fn get_controller_pose(
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

    pub fn get_controller_subaction_path(&self) -> Option<xr::Path> {
        match self.get_type() {
            TrackedDeviceType::Controller { subaction_path, .. } => Some(subaction_path),
            _ => None,
        }
    }

    pub fn get_controller_hand(&self) -> Option<Hand> {
        match self.get_type() {
            TrackedDeviceType::Controller { hand, .. } => Some(hand),
            _ => None,
        }
    }
}
