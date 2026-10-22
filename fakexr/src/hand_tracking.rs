use std::cell::Cell;

thread_local! {
    static EXTENSIONS: Cell<(bool, bool)> = const { Cell::new((true, true)) };
}

pub(super) fn enabled_extensions() -> (bool, bool) {
    EXTENSIONS.get()
}

pub fn with_hand_tracking_extensions<T>(
    data_source: bool,
    motion_range: bool,
    f: impl FnOnce() -> T,
) -> T {
    struct Restore((bool, bool));
    impl Drop for Restore {
        fn drop(&mut self) {
            EXTENSIONS.set(self.0);
        }
    }
    let _restore = Restore(EXTENSIONS.replace((data_source, motion_range)));
    f()
}

use super::*;

pub(crate) struct HandTracker {
    session: Arc<Session>,
    hand: UserPath,
    sources: Vec<xr::HandTrackingDataSourceEXT>,
}

impl_handle!(HandTracker, xr::HandTrackerEXT);

pub fn set_controller_active(session: xr::Session, hand: UserPath, active: bool) {
    get_hand_data(hand, &session.to_handle().unwrap())
        .controller_active
        .store(active);
}

pub fn set_hand_tracking(
    session: xr::Session,
    hand: UserPath,
    source: Option<xr::HandTrackingDataSourceEXT>,
) {
    get_hand_data(hand, &session.to_handle().unwrap())
        .hand_tracking_source
        .store(source);
}

pub(super) extern "system" fn create_hand_tracker_e_x_t(
    session: xr::Session,
    info: *const xr::HandTrackerCreateInfoEXT,
    tracker: *mut xr::HandTrackerEXT,
) -> xr::Result {
    let session = get_handle!(session);
    let info = unsafe { &*info };
    let mut sources = Vec::new();
    if !info.next.is_null() {
        let source = unsafe { &*(info.next as *const xr::HandTrackingDataSourceInfoEXT) };
        assert_eq!(source.ty, xr::HandTrackingDataSourceInfoEXT::TYPE);
        sources = unsafe {
            std::slice::from_raw_parts(
                source.requested_data_sources,
                source.requested_data_source_count as usize,
            )
        }
        .to_vec();
        assert_eq!(
            sources,
            [
                xr::HandTrackingDataSourceEXT::UNOBSTRUCTED,
                xr::HandTrackingDataSourceEXT::CONTROLLER
            ]
        );
    }
    let hand = match info.hand {
        xr::HandEXT::LEFT => UserPath::LeftHand,
        xr::HandEXT::RIGHT => UserPath::RightHand,
        _ => return xr::Result::ERROR_VALIDATION_FAILURE,
    };
    unsafe {
        *tracker = Arc::new(HandTracker {
            session,
            hand,
            sources,
        })
        .to_xr()
    };
    xr::Result::SUCCESS
}

pub(super) extern "system" fn destroy_hand_tracker_e_x_t(
    tracker: xr::HandTrackerEXT,
) -> xr::Result {
    destroy_handle(tracker)
}

pub(super) extern "system" fn locate_hand_joints_e_x_t(
    tracker: xr::HandTrackerEXT,
    info: *const xr::HandJointsLocateInfoEXT,
    locations: *mut xr::HandJointLocationsEXT,
) -> xr::Result {
    let tracker = get_handle!(tracker);
    let info = unsafe { &*info };
    let locations = unsafe { &mut *locations };
    let mut range = xr::HandJointsMotionRangeEXT::UNOBSTRUCTED;
    if !info.next.is_null() {
        let range_info = unsafe { &*(info.next as *const xr::HandJointsMotionRangeInfoEXT) };
        assert_eq!(range_info.ty, xr::HandJointsMotionRangeInfoEXT::TYPE);
        range = range_info.hand_joints_motion_range;
    }
    let source = get_hand_data(tracker.hand, &tracker.session)
        .hand_tracking_source
        .load();
    locations.is_active = source.is_some().into();
    if !locations.next.is_null() {
        assert!(
            !tracker.sources.is_empty(),
            "Source state requires source info at creation"
        );
        let state = unsafe { &mut *(locations.next as *mut xr::HandTrackingDataSourceStateEXT) };
        assert_eq!(state.ty, xr::HandTrackingDataSourceStateEXT::TYPE);
        state.is_active = source.is_some().into();
        if let Some(source) = source {
            state.data_source = source;
        }
    }
    if source.is_some() {
        assert_eq!(locations.joint_count as usize, xr::HAND_JOINT_COUNT_EXT);
        let joints = unsafe {
            std::slice::from_raw_parts_mut(
                locations.joint_locations,
                locations.joint_count as usize,
            )
        };
        for (i, joint) in joints.iter_mut().enumerate() {
            *joint = xr::HandJointLocationEXT {
                location_flags: xr::SpaceLocationFlags::POSITION_VALID
                    | xr::SpaceLocationFlags::ORIENTATION_VALID,
                pose: xr::Posef {
                    orientation: xr::Quaternionf {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 1.0,
                    },
                    position: xr::Vector3f {
                        x: i as f32 * 0.01,
                        y: if source == Some(xr::HandTrackingDataSourceEXT::CONTROLLER)
                            && range == xr::HandJointsMotionRangeEXT::CONFORMING_TO_CONTROLLER
                        {
                            0.05
                        } else {
                            0.0
                        },
                        z: 0.0,
                    },
                },
                radius: 0.01,
            };
        }
    }
    xr::Result::SUCCESS
}
