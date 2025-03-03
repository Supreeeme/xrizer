use super::{Input, Profiles};
use crate::openxr_data::{self, Hand, OpenXrData, SessionData};
use log::{debug, trace, warn};
use openvr as vr;
use openxr as xr;
use std::sync::{atomic::Ordering, OnceLock};

impl<C: openxr_data::Compositor> Input<C> {
    pub fn get_legacy_controller_state(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        state: *mut vr::VRControllerState_t,
        state_size: u32,
    ) -> bool {
        if state_size as usize != std::mem::size_of::<vr::VRControllerState_t>() {
            warn!(
                "Got an unexpected size for VRControllerState_t (expected {}, got {state_size})",
                std::mem::size_of::<vr::VRControllerState_t>()
            );
            return false;
        }

        let data = self.openxr.session_data.get();
        let Some(legacy) = data.input_data.legacy_actions.get() else {
            debug!("tried getting controller state, but legacy actions aren't ready");
            return false;
        };
        let actions = &legacy.actions;

        let Ok(hand) = Hand::try_from(device_index) else {
            debug!("requested controller state for invalid device index: {device_index}");
            return false;
        };

        let hand_path = match hand {
            Hand::Left => self.openxr.left_hand.subaction_path,
            Hand::Right => self.openxr.right_hand.subaction_path,
        };

        let data = self.openxr.session_data.get();

        // Adapted from openvr.h
        fn button_mask_from_id(id: vr::EVRButtonId) -> u64 {
            1_u64 << (id as u32)
        }

        let state = unsafe { state.as_mut() }.unwrap();
        *state = Default::default();

        state.unPacketNum = self.legacy_packet_num.load(Ordering::Relaxed);

        let mut read_button = |id, action: &xr::Action<bool>| {
            let val = action
                .state(&data.session, hand_path)
                .unwrap()
                .current_state as u64
                * u64::MAX;
            state.ulButtonPressed |= button_mask_from_id(id) & val;
        };

        read_button(vr::EVRButtonId::SteamVR_Trigger, &actions.trigger_click);
        read_button(vr::EVRButtonId::ApplicationMenu, &actions.app_menu);

        let t = actions.trigger.state(&data.session, hand_path).unwrap();
        state.rAxis[1] = vr::VRControllerAxis_t {
            x: t.current_state,
            y: 0.0,
        };

        true
    }
}

macro_rules! legacy_actions_and_bindings {
    ($($field:ident: $ty:ty),+$(,)?) => {
        pub(super) struct LegacyActions {
            $(pub $field: $ty),+
        }
        pub(super) struct LegacyBindings {
            $(pub $field: Vec<xr::Path>),+
        }
        impl LegacyBindings {
            pub fn binding_iter(self, actions: &LegacyActions) -> impl Iterator<Item = xr::Binding<'_>> {
                std::iter::empty()
                $(
                    .chain(
                        self.$field.into_iter().map(|binding| xr::Binding::new(&actions.$field, binding))
                    )
                )+
            }
        }
    }
}

legacy_actions_and_bindings! {
    grip_pose: xr::Action<xr::Posef>,
    aim_pose: xr::Action<xr::Posef>,
    app_menu: xr::Action<bool>,
    trigger_click: xr::Action<bool>,
    trigger: xr::Action<f32>,
    squeeze: xr::Action<f32>,
}

pub(super) struct LegacyActionData {
    pub set: xr::ActionSet,
    pub left_spaces: HandSpaces,
    pub right_spaces: HandSpaces,
    pub actions: LegacyActions,
}

impl LegacyActionData {
    pub fn new<'a>(
        instance: &'a xr::Instance,
        session: &'a xr::Session<xr::AnyGraphics>,
        left_hand: xr::Path,
        right_hand: xr::Path,
    ) -> Self {
        debug!("creating legacy actions");
        let leftright = [left_hand, right_hand];
        let set = instance
            .create_action_set("xrizer-legacy-set", "XRizer Legacy Set", 0)
            .unwrap();
        let grip_pose = set
            .create_action("grip-pose", "Grip Pose", &leftright)
            .unwrap();
        let aim_pose = set
            .create_action("aim-pose", "Aim Pose", &leftright)
            .unwrap();
        let trigger_click = set
            .create_action("trigger-click", "Trigger Click", &leftright)
            .unwrap();
        let trigger = set.create_action("trigger", "Trigger", &leftright).unwrap();
        let squeeze = set.create_action("squeeze", "Squeeze", &leftright).unwrap();
        let app_menu = set
            .create_action("app-menu", "Application Menu", &leftright)
            .unwrap();

        let create_spaces = |hand| {
            let hand_path = match hand {
                Hand::Left => left_hand,
                Hand::Right => right_hand,
            };
            HandSpaces {
                hand,
                hand_path,
                grip: grip_pose
                    .create_space(session, hand_path, xr::Posef::IDENTITY)
                    .unwrap(),
                aim: aim_pose
                    .create_space(session, hand_path, xr::Posef::IDENTITY)
                    .unwrap(),
                raw: OnceLock::new(),
            }
        };

        let left_spaces = create_spaces(Hand::Left);
        let right_spaces = create_spaces(Hand::Right);

        Self {
            set,
            left_spaces,
            right_spaces,
            actions: LegacyActions {
                grip_pose,
                aim_pose,
                app_menu,
                trigger_click,
                trigger,
                squeeze,
            },
        }
    }
}

pub fn setup_legacy_bindings(
    instance: &xr::Instance,
    session: &xr::Session<xr::AnyGraphics>,
    legacy: &LegacyActionData,
) {
    debug!("setting up legacy bindings");

    let actions = &legacy.actions;
    for profile in Profiles::get().profiles_iter() {
        const fn constrain<F>(f: F) -> F
        where
            F: for<'a> Fn(&'a str) -> xr::Path,
        {
            f
        }
        let stp = constrain(|s| instance.string_to_path(s).unwrap());
        let bindings = profile.legacy_bindings(&stp);
        let profile = stp(profile.profile_path());
        instance
            .suggest_interaction_profile_bindings(
                profile,
                &bindings.binding_iter(actions).collect::<Vec<_>>(),
            )
            .unwrap();
    }

    session.attach_action_sets(&[&legacy.set]).unwrap();
    session
        .sync_actions(&[xr::ActiveActionSet::new(&legacy.set)])
        .unwrap();
}

pub(super) struct HandSpaces {
    hand: Hand,
    hand_path: xr::Path,
    grip: xr::Space,
    aim: xr::Space,

    /// Based on the controller jsons in SteamVR, the "raw" pose
    /// (which seems to be equivalent to the pose returned by WaitGetPoses)
    /// is actually the grip pose, but in the same position as the aim pose.
    /// Using this pose instead of the grip fixes strange controller rotation in
    /// I Expect You To Die 3.
    /// This is stored as a space so we can locate hand joints relative to it for skeletal data.
    raw: OnceLock<xr::Space>,
}

impl HandSpaces {
    pub fn try_get_or_init_raw(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        actions: &LegacyActions,
        time: xr::Time,
    ) -> Option<&xr::Space> {
        if let Some(raw) = self.raw.get() {
            return Some(raw);
        }

        // This offset between grip and aim poses should be static,
        // so it should be fine to only grab it once.
        let aim_loc = self.aim.locate(&self.grip, time).unwrap();
        if !aim_loc.location_flags.contains(
            xr::SpaceLocationFlags::POSITION_VALID | xr::SpaceLocationFlags::ORIENTATION_VALID,
        ) {
            trace!("couldn't locate aim pose, no raw space will be created");
            return None;
        }

        let hand_profile = match self.hand {
            Hand::Right => &xr_data.right_hand.profile,
            Hand::Left => &xr_data.left_hand.profile,
        };

        let hand_profile = hand_profile.lock().unwrap();
        let Some(profile) = hand_profile.as_ref() else {
            trace!("no hand profile, no raw space will be created");
            return None;
        };

        self.raw
            .set(
                actions
                    .grip_pose
                    .create_space(
                        &session_data.session,
                        self.hand_path,
                        profile.offset_grip_pose(xr::Posef {
                            orientation: xr::Quaternionf::IDENTITY,
                            position: aim_loc.pose.position,
                        }),
                    )
                    .unwrap(),
            )
            .unwrap_or_else(|_| unreachable!());

        self.raw.get()
    }
}

#[cfg(test)]
mod tests {
    use crate::input::{
        profiles::vive_controller::ViveWands,
        tests::{compare_pose, Fixture},
    };
    use fakexr::UserPath::*;
    use glam::Quat;
    use openvr as vr;
    use openxr as xr;
    use std::f32::consts::FRAC_PI_4;

    #[test]
    fn raw_pose_is_grip_at_aim() {
        let f = Fixture::new();

        let set1 = f.get_action_set_handle(c"/actions/set1");
        let pose_handle = f.get_action_handle(c"/actions/set1/in/pose");
        let left_hand = f.get_input_source_handle(c"/user/hand/left");
        f.load_actions(c"actions.json");
        f.set_interaction_profile(&ViveWands, LeftHand);

        let grip_rot = Quat::from_rotation_x(-FRAC_PI_4);
        let grip = xr::Posef {
            position: xr::Vector3f {
                x: 0.5,
                y: 0.5,
                z: 0.5,
            },
            orientation: xr::Quaternionf {
                x: grip_rot.x,
                y: grip_rot.y,
                z: grip_rot.z,
                w: grip_rot.w,
            },
        };

        fakexr::set_grip(f.raw_session(), LeftHand, grip);

        let aim = xr::Posef {
            position: xr::Vector3f {
                x: 0.7,
                y: 0.6,
                z: 1.0,
            },
            orientation: xr::Quaternionf::IDENTITY,
        };

        fakexr::set_aim(f.raw_session(), LeftHand, aim);

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });
        let data = f.get_pose(pose_handle, left_hand).unwrap();

        assert!(data.bActive);
        assert_eq!(data.activeOrigin, left_hand);

        let pose = data.pose;
        assert!(pose.bDeviceIsConnected);
        assert!(pose.bPoseIsValid);
        assert_eq!(pose.eTrackingResult, vr::ETrackingResult::Running_OK);

        compare_pose(
            xr::Posef {
                position: aim.position,
                orientation: grip.orientation,
            },
            pose.mDeviceToAbsoluteTracking.into(),
        );
    }
}
