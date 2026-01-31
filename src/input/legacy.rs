use super::{Input, PoseData, Profiles, WriteOnDrop};
use crate::{
    input::{ActionData, LoadedActions, ManifestLoadedActions},
    openxr_data::{self},
};
use log::{debug, trace, warn};
use openvr as vr;
use openxr as xr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Default)]
pub(super) struct LegacyState {
    packet_num: AtomicU32,
    got_state_this_frame: [AtomicBool; 2],
}

impl LegacyState {
    pub fn on_action_sync(&self) {
        self.packet_num.fetch_add(1, Ordering::Relaxed);
        for state in &self.got_state_this_frame {
            state.store(false, Ordering::Relaxed);
        }
    }
}

// Adapted from openvr.h
pub const fn button_mask_from_id(id: vr::EVRButtonId) -> u64 {
    1_u64 << (id as u32)
}
#[macro_export]
macro_rules! button_mask_from_ids {
    ($($x:expr), * $(,)?) => {
        0u64 $(| button_mask_from_id($x))*
    };
}

impl<C: openxr_data::Compositor> Input<C> {
    pub fn setup_legacy_actions(&self) {
        debug!("setting up legacy actions");

        let session_data = self.openxr.session_data.get();
        let session = &session_data.session;
        let legacy = LegacyActionData::new(
            &self.openxr.instance,
            self.subaction_paths.left,
            self.subaction_paths.right,
        );
        let input_data = &session_data.input_data;

        for profile in Profiles::get().profiles_iter() {
            const fn constrain<F>(f: F) -> F
            where
                F: for<'a> Fn(&'a str) -> xr::Path,
            {
                f
            }
            let stp = constrain(|s| self.openxr.instance.string_to_path(s).unwrap());
            let bindings = profile.legacy_bindings(&stp);
            let profile = stp(profile.profile_path());
            self.openxr
                .instance
                .suggest_interaction_profile_bindings(
                    profile,
                    &bindings
                        .into_iter(&legacy.actions, input_data.pose_data.get().unwrap())
                        .collect::<Vec<_>>(),
                )
                .unwrap();
        }

        let pose_set = &input_data.pose_data.get().unwrap().set;

        session
            .attach_action_sets(&[&legacy.set, pose_set])
            .unwrap();
        session
            .sync_actions(&[
                xr::ActiveActionSet::new(&legacy.set),
                xr::ActiveActionSet::new(pose_set),
            ])
            .unwrap();

        input_data
            .actions
            .set(LoadedActions::Legacy(legacy))
            .unwrap_or_else(|_| panic!("Actions unexpectedly set up"));
    }

    pub fn legacy_haptic(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        _axis_id: u32, // TODO: what is this for?
        duration_us: std::ffi::c_ushort,
    ) {
        let Some(hand) = self.device_index_to_hand(device_index) else {
            debug!("tried triggering haptic on invalid device index: {device_index}");
            return;
        };
        let hand_path = self.get_subaction_path(hand);

        let data = self.openxr.session_data.get();
        if let Some(manifest_actions) = data.input_data.get_loaded_actions() {
            // Game provided action manifest but also calls the legacy action's pulse method.
            self.legacy_haptic_via_manifest(manifest_actions, hand_path, duration_us);
            return;
        }

        let Some(legacy) = data.input_data.get_legacy_actions() else {
            debug!("tried triggering haptic, but legacy actions aren't ready");
            return;
        };

        let duration_nanos = std::time::Duration::from_micros(duration_us as u64).as_nanos();

        debug!(
            "triggering legacy haptic for {duration_us} microseconds ({} seconds/{} milliseconds)",
            std::time::Duration::from_micros(duration_us as _).as_secs_f32(),
            std::time::Duration::from_micros(duration_us as _).as_millis()
        );

        if let Err(e) = legacy.actions.haptic.apply_feedback(
            &data.session,
            hand_path,
            &xr::HapticVibration::new()
                .amplitude(1.0)
                .frequency(xr::FREQUENCY_UNSPECIFIED)
                .duration(xr::Duration::from_nanos(duration_nanos as i64)),
        ) {
            warn!("Failed to trigger haptic: {e:?}");
        }
    }

    /// Trigger a full amplitude vibration on the given path via a Manifest Action.
    ///
    /// This is necessary for the legacy input system to handle because applications may call
    /// legacy-input haptic interface functions while providing manifest files.
    fn legacy_haptic_via_manifest(
        &self,
        manifest_actions: &ManifestLoadedActions,
        hand_path: xr::Path,
        duration_us: ::std::ffi::c_ushort,
    ) {
        trace!("triggered legacy haptic while using action manifest");
        let Some(haptic_action) =
            manifest_actions
                .actions
                .iter()
                .find_map(|(_k, action)| match action {
                    ActionData::Haptic(haptic_action) => Some(haptic_action),
                    _ => None,
                })
        else {
            debug!("triggered legacy haptic with loaded actions, but no haptic action found.");
            return;
        };
        haptic_action
            .apply_feedback(
                &self.openxr.session_data.get().session,
                hand_path,
                &xr::HapticVibration::new()
                    .amplitude(1.0)
                    .frequency(xr::FREQUENCY_UNSPECIFIED)
                    .duration(xr::Duration::from_nanos(i64::from(duration_us) * 1000)),
            )
            .unwrap();
    }

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

        if state.is_null() {
            return false;
        }

        let mut state = WriteOnDrop::new(state);
        let state = &mut state.value;

        let data = self.openxr.session_data.get();
        if data.input_data.get_loaded_actions().is_some() {
            debug!("not returning legacy controller state due to loaded actions");
            return false;
        }

        let Some(legacy) = data.input_data.get_legacy_actions() else {
            debug!("tried getting controller state, but legacy actions aren't ready");
            return false;
        };
        let actions = &legacy.actions;

        let Some(hand) = self.device_index_to_hand(device_index) else {
            debug!(
                "tried getting controller state, but device index {device_index} is invalid or not a controller!"
            );
            return false;
        };

        let hand_path = self.get_subaction_path(hand);

        let data = self.openxr.session_data.get();

        state.unPacketNum = self.legacy_state.packet_num.load(Ordering::Relaxed);

        // Only send the input event if we haven't already.
        let mut events = self.legacy_state.got_state_this_frame[hand as usize - 1]
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
            .then(|| self.events.lock().unwrap());

        let mut read_button =
            |id, click_action: &xr::Action<bool>, touch_action: Option<&xr::Action<bool>>| {
                let touch_state = touch_action.map(|a| a.state(&data.session, hand_path).unwrap());
                let touched = touch_state.is_some_and(|s| s.current_state);
                state.ulButtonTouched |= button_mask_from_id(id) & (touched as u64 * u64::MAX);

                let click_state = click_action.state(&data.session, hand_path).unwrap();
                let pressed = click_state.current_state;
                state.ulButtonPressed |= button_mask_from_id(id) & (pressed as u64 * u64::MAX);

                if let Some(events) = &mut events {
                    if touch_state.is_some_and(|s| s.changed_since_last_sync) {
                        events.push_back(super::InputEvent {
                            ty: if touched {
                                vr::EVREventType::ButtonTouch
                            } else {
                                vr::EVREventType::ButtonUntouch
                            },
                            index: device_index,
                            data: vr::VREvent_Controller_t { button: id as u32 },
                        });
                    }
                    if click_state.changed_since_last_sync {
                        events.push_back(super::InputEvent {
                            ty: if pressed {
                                vr::EVREventType::ButtonPress
                            } else {
                                vr::EVREventType::ButtonUnpress
                            },
                            index: device_index,
                            data: vr::VREvent_Controller_t { button: id as u32 },
                        });
                    }
                }
            };

        read_button(
            vr::EVRButtonId::Axis0,
            &actions.main_xy_click,
            Some(&actions.main_xy_touch),
        );
        read_button(
            vr::EVRButtonId::SteamVR_Trigger,
            &actions.trigger_click,
            None,
        );
        read_button(vr::EVRButtonId::ApplicationMenu, &actions.app_menu, None);
        read_button(vr::EVRButtonId::A, &actions.a, None);
        read_button(vr::EVRButtonId::Grip, &actions.squeeze_click, None);
        read_button(vr::EVRButtonId::Axis2, &actions.squeeze_click, None);

        let j = actions.main_xy.state(&data.session, hand_path).unwrap();
        state.rAxis[0] = vr::VRControllerAxis_t {
            x: j.current_state.x,
            y: j.current_state.y,
        };

        let t = actions.trigger.state(&data.session, hand_path).unwrap();
        state.rAxis[1] = vr::VRControllerAxis_t {
            x: t.current_state,
            y: 0.0,
        };

        let s = actions.squeeze.state(&data.session, hand_path).unwrap();
        state.rAxis[2] = vr::VRControllerAxis_t {
            x: s.current_state,
            y: 0.0,
        };

        true
    }
}

mod marker {
    use openxr as xr;
    // Some type magic to parameterize our legacy actions to act as actions or bindings
    pub trait ActionsMarker {
        type T<U: xr::ActionTy>;
    }
    pub struct Actions;
    pub struct Bindings {
        // This pose is handled separately, in the PoseData struct,
        // so we don't use an action for it, but we still need the binding.
        pub grip_pose: Vec<xr::Path>,
    }
    impl ActionsMarker for Actions {
        type T<U: xr::ActionTy> = xr::Action<U>;
    }
    impl ActionsMarker for Bindings {
        type T<U: xr::ActionTy> = Vec<xr::Path>;
    }

    pub type Action<T, M> = <M as ActionsMarker>::T<T>;
}
pub(super) use marker::Bindings;
use marker::*;

////////////////////////
// Whenever a field is added to this struct, it also needs to be added to LegacyBindings::into_iter below
///////////////////////
#[allow(private_interfaces, private_bounds)]
pub(super) struct Legacy<M: ActionsMarker> {
    pub app_menu: Action<bool, M>,
    pub a: Action<bool, M>,
    pub trigger_click: Action<bool, M>,
    pub squeeze_click: Action<bool, M>,
    pub trigger: Action<f32, M>,
    pub squeeze: Action<f32, M>,
    // This can be a stick or a trackpad, so we'll just call it "xy"
    pub main_xy: Action<xr::Vector2f, M>,
    pub main_xy_touch: Action<bool, M>,
    pub main_xy_click: Action<bool, M>,
    pub haptic: Action<xr::Haptic, M>,
    pub extra: M,
}

pub(super) type LegacyActions = Legacy<Actions>;
pub(super) type LegacyBindings = Legacy<Bindings>;

impl LegacyBindings {
    fn into_iter<'a>(
        self,
        actions: &'a LegacyActions,
        pose_data: &'a PoseData,
    ) -> impl Iterator<Item = xr::Binding<'a>> {
        macro_rules! bindings {
            ($begin:expr, $($field:ident),+$(,)?) => {
                $begin $(
                    .chain(
                        self.$field.into_iter().map(|path| xr::Binding::new(&actions.$field, path))
                    )
                )+
            }
        }

        // TODO: figure out how to automatically derive this...
        bindings![
            self.extra
                .grip_pose
                .into_iter()
                .map(|path| xr::Binding::new(&pose_data.grip, path)),
            app_menu,
            a,
            trigger_click,
            squeeze_click,
            trigger,
            squeeze,
            main_xy,
            main_xy_touch,
            main_xy_click,
            haptic,
        ]
    }
}

pub(super) struct LegacyActionData {
    pub set: xr::ActionSet,
    actions: LegacyActions,
}

impl LegacyActionData {
    pub fn new(instance: &xr::Instance, left_hand: xr::Path, right_hand: xr::Path) -> Self {
        debug!("creating legacy actions");
        let leftright = [left_hand, right_hand];

        let set = instance
            .create_action_set("xrizer-legacy-set", "XRizer Legacy Set", 0)
            .unwrap();

        let actions = LegacyActions {
            trigger_click: set
                .create_action("trigger-click", "Trigger Click", &leftright)
                .unwrap(),
            trigger: set.create_action("trigger", "Trigger", &leftright).unwrap(),
            squeeze: set.create_action("squeeze", "Squeeze", &leftright).unwrap(),
            app_menu: set
                .create_action("app-menu", "Application Menu", &leftright)
                .unwrap(),
            a: set.create_action("a", "A Button", &leftright).unwrap(),
            squeeze_click: set
                .create_action("grip-click", "Grip Click", &leftright)
                .unwrap(),
            main_xy: set
                .create_action("main-joystick", "Main Joystick/Trackpad", &leftright)
                .unwrap(),
            main_xy_click: set
                .create_action("main-joystick-click", "Main Joystick Click", &leftright)
                .unwrap(),
            main_xy_touch: set
                .create_action("main-joystick-touch", "Main Joystick Touch", &leftright)
                .unwrap(),
            haptic: set.create_action("haptic", "Haptic", &leftright).unwrap(),
            extra: Actions,
        };

        Self { set, actions }
    }
}

#[cfg(test)]
mod tests {
    use crate::input::ActionData;
    use crate::input::profiles::{knuckles::Knuckles, simple_controller::SimpleController};
    use crate::input::tests::{Fixture, compare_pose};
    use crate::openxr_data::Hand;
    use openvr as vr;
    use openxr as xr;

    #[repr(C)]
    #[derive(Default)]
    struct MyEvent {
        ty: u32,
        index: vr::TrackedDeviceIndex_t,
        age: f32,
        data: EventData,
    }

    // A small version of the VREvent_Data_t union - writing to this should not cause UB!
    #[repr(C)]
    union EventData {
        controller: vr::VREvent_Controller_t,
    }

    impl Default for EventData {
        fn default() -> Self {
            Self {
                controller: Default::default(),
            }
        }
    }

    const _: () = {
        use std::mem::offset_of;

        macro_rules! verify_offset {
            ($real:ident, $fake:ident) => {
                assert!(offset_of!(vr::VREvent_t, $real) == offset_of!(MyEvent, $fake));
            };
        }
        verify_offset!(eventType, ty);
        verify_offset!(trackedDeviceIndex, index);
        verify_offset!(eventAgeSeconds, age);
        verify_offset!(data, data);
    };

    #[test]
    fn no_legacy_input_before_session_setup() {
        let fixture = Fixture::new();

        let got_input = fixture.input.get_legacy_controller_state(
            1,
            &mut vr::VRControllerState_t::default(),
            std::mem::size_of::<vr::VRControllerState_t>() as _,
        );
        assert!(!got_input);

        fixture.input.frame_start_update();
        let got_input = fixture.input.get_legacy_controller_state(
            1,
            &mut vr::VRControllerState_t::default(),
            std::mem::size_of::<vr::VRControllerState_t>() as _,
        );
        assert!(!got_input);
    }

    fn legacy_input(
        get_action: impl FnOnce(&super::LegacyActions) -> openxr::sys::Action,
        ids: &[vr::EVRButtonId],
        touch: bool,
    ) {
        use fakexr::UserPath::*;
        let mut f = Fixture::new();
        f.input.openxr.restart_session();

        f.set_interaction_profile(&Knuckles, LeftHand);
        f.set_interaction_profile(&Knuckles, RightHand);
        f.input.frame_start_update();
        f.input.openxr.poll_events();
        let action = get_action(
            &f.input
                .openxr
                .session_data
                .get()
                .input_data
                .get_legacy_actions()
                .unwrap()
                .actions,
        );

        let get_state = |hand: fakexr::UserPath| {
            let mut state = vr::VRControllerState_t::default();
            assert!(f.input.get_legacy_controller_state(
                match hand {
                    LeftHand => 1,
                    RightHand => 2,
                },
                &mut state,
                std::mem::size_of_val(&state) as u32
            ));
            state
        };

        let get_event = || {
            let mut event = MyEvent::default();
            f.input
                .get_next_event(
                    std::mem::size_of_val(&event) as u32,
                    &mut event as *mut _ as *mut vr::VREvent_t,
                )
                .then_some(event)
        };

        let expect_event =
            |msg| get_event().unwrap_or_else(|| panic!("Expected to get an event ({msg})"));
        let expect_no_event = |msg| {
            let event = get_event();
            assert!(
                event.is_none(),
                "Got unexpected event: {} ({msg})",
                event.unwrap().ty
            );
        };

        let update_action_state = |left_state, right_state| {
            fakexr::set_action_state(action, fakexr::ActionState::Bool(left_state), LeftHand);
            fakexr::set_action_state(action, fakexr::ActionState::Bool(right_state), RightHand);
            f.input.frame_start_update();
        };

        let expect_press = |state: &vr::VRControllerState_t, expect: bool| {
            // The braces around state.ulButtonPressed are to force create a copy, because
            // VRControllerState_t is a packed struct and references to unaligned fields are undefined.
            let mask = if touch {
                state.ulButtonTouched
            } else {
                state.ulButtonPressed
            };

            match expect {
                true => {
                    let active_mask = ids
                        .iter()
                        .copied()
                        .fold(0, |val, id| val | super::button_mask_from_id(id));

                    assert_eq!(
                        mask, active_mask,
                        "Button not active - state: {mask:b} | button mask: {mask:b}"
                    );
                }
                false => {
                    assert_eq!(mask, 0, "Button should be inactive - state: {mask:b}");
                }
            }
        };

        let (active_event, inactive_event) = if touch {
            (
                vr::EVREventType::ButtonTouch as u32,
                vr::EVREventType::ButtonUntouch as u32,
            )
        } else {
            (
                vr::EVREventType::ButtonPress as u32,
                vr::EVREventType::ButtonUnpress as u32,
            )
        };

        let hands = [LeftHand, RightHand];

        while let Some(event) = get_event() {
            assert_eq!(event.ty, vr::EVREventType::TrackedDeviceActivated as u32);
        }

        for hand in hands {
            let state = get_state(hand);
            expect_press(&state, false);
            expect_no_event(format!("{hand:?}"));
        }

        // State change to true
        update_action_state(true, true);

        for (idx, hand) in hands.iter().copied().enumerate() {
            let idx = idx as u32 + 1;
            let state = get_state(hand);
            expect_press(&state, true);

            for id in ids {
                let event = expect_event(format!("{hand:?}"));
                assert_eq!(event.ty, active_event, "{hand:?}");
                assert_eq!(event.index, idx, "{hand:?}");
                assert_eq!(
                    unsafe { event.data.controller }.button,
                    *id as u32,
                    "{hand:?}"
                );
            }
        }

        // No frame update - no change
        for hand in hands {
            let state = get_state(hand);
            expect_press(&state, true);
            expect_no_event(format!("{hand:?}"));
        }

        // Frame update but no change
        f.input.frame_start_update();
        for hand in hands {
            let state = get_state(hand);
            expect_press(&state, true);
            expect_no_event(format!("{hand:?}"));
        }

        // State change to false
        update_action_state(false, false);

        for (idx, hand) in hands.iter().copied().enumerate() {
            let idx = idx as u32 + 1;
            let state = get_state(hand);
            expect_press(&state, false);

            for id in ids {
                let event = expect_event(format!("{id:?}"));
                assert_eq!(event.ty, inactive_event, "{hand:?}");
                assert_eq!(event.index, idx, "{hand:?}");
                assert_eq!(
                    unsafe { event.data.controller }.button,
                    *id as u32,
                    "{hand:?}"
                );
            }
        }

        // State change one hand
        update_action_state(true, false);

        let state = get_state(LeftHand);
        expect_press(&state, true);
        for id in ids {
            let event = expect_event(format!("{id:?}"));
            assert_eq!(event.ty, active_event, "{id:?}");
            assert_eq!(event.index, 1, "{id:?}");
            assert_eq!(
                unsafe { event.data.controller }.button,
                *id as u32,
                "{id:?}"
            );
        }

        let state = get_state(RightHand);
        expect_press(&state, false);
        expect_no_event("RightHand".to_string());
    }

    macro_rules! test_button {
        ($click:ident, $id:path $(| $other_id:path)*) => {
            paste::paste! {
                #[test]
                fn [<button_ $click>]() {
                    legacy_input(|actions| actions.$click.as_raw(), &[$id $(, $other_id)*], false);
                }
            }
        };
        ($click:ident, $id:path $(| $other_id:path)*, $touch:ident) => {
            test_button!($click, $id $(| $other_id)*);
            paste::paste! {
                #[test]
                fn [<button_ $touch>]() {
                    legacy_input(|actions| actions.$touch.as_raw(), &[$id $(, $other_id)*], true);
                }
            }
        };
    }

    test_button!(main_xy_click, vr::EVRButtonId::Axis0, main_xy_touch);
    test_button!(trigger_click, vr::EVRButtonId::SteamVR_Trigger);
    test_button!(app_menu, vr::EVRButtonId::ApplicationMenu);
    test_button!(
        squeeze_click,
        vr::EVRButtonId::Grip | vr::EVRButtonId::Axis2
    );
    test_button!(a, vr::EVRButtonId::A);

    #[test]
    fn no_legacy_input_with_manifest() {
        let mut f = Fixture::new();

        f.input.openxr.restart_session();

        f.set_interaction_profile(&SimpleController, fakexr::UserPath::LeftHand);
        f.set_interaction_profile(&SimpleController, fakexr::UserPath::RightHand);
        f.input.frame_start_update();
        f.input.openxr.poll_events();

        let mut state = vr::VRControllerState_t::default();
        assert!(f.input.get_legacy_controller_state(
            1,
            &mut state,
            std::mem::size_of_val(&state) as u32
        ));

        f.load_actions(c"actions.json");
        f.input.openxr.poll_events();
        f.input.frame_start_update();
        assert!(!f.input.get_legacy_controller_state(
            1,
            &mut state,
            std::mem::size_of_val(&state) as u32
        ));
    }

    #[test]
    fn poses_updated() {
        use fakexr::UserPath::*;
        let mut f = Fixture::new();
        f.input.openxr.restart_session();
        f.set_interaction_profile(&SimpleController, LeftHand);
        f.set_interaction_profile(&SimpleController, RightHand);
        f.input.frame_start_update();
        f.input.openxr.poll_events();

        fakexr::set_grip(f.raw_session(), LeftHand, xr::Posef::IDENTITY);
        fakexr::set_grip(f.raw_session(), RightHand, xr::Posef::IDENTITY);
        f.input.frame_start_update();

        let seated_origin = vr::ETrackingUniverseOrigin::Seated;
        let left_pose = f.input.get_controller_pose(Hand::Left, Some(seated_origin));
        compare_pose(
            xr::Posef::IDENTITY,
            left_pose.unwrap().mDeviceToAbsoluteTracking.into(),
        );
        compare_pose(
            xr::Posef::IDENTITY,
            f.input
                .get_controller_pose(Hand::Right, Some(seated_origin))
                .unwrap()
                .mDeviceToAbsoluteTracking
                .into(),
        );

        let new_pose = xr::Posef {
            position: xr::Vector3f {
                x: 0.5,
                y: 0.5,
                z: 0.5,
            },
            orientation: xr::Quaternionf::IDENTITY,
        };

        fakexr::set_grip(f.raw_session(), LeftHand, new_pose);
        fakexr::set_grip(f.raw_session(), RightHand, new_pose);
        f.input.frame_start_update();
        compare_pose(
            new_pose,
            f.input
                .get_controller_pose(Hand::Left, Some(seated_origin))
                .unwrap()
                .mDeviceToAbsoluteTracking
                .into(),
        );
        compare_pose(
            new_pose,
            f.input
                .get_controller_pose(Hand::Right, Some(seated_origin))
                .unwrap()
                .mDeviceToAbsoluteTracking
                .into(),
        );
    }

    #[test]
    fn init_controller_state_on_failure() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");
        f.input.frame_start_update();

        let mut state = std::mem::MaybeUninit::<vr::VRControllerState_t>::uninit();
        assert!(!f.input.get_legacy_controller_state(
            0,
            state.as_mut_ptr(),
            std::mem::size_of_val(&state) as u32
        ));

        let state = unsafe { state.assume_init() };
        assert_eq!({ state.ulButtonPressed }, 0);
    }

    #[test]
    fn legacy_haptic() {
        let mut f = Fixture::new();
        f.input.openxr.restart_session();
        f.set_interaction_profile(&SimpleController, fakexr::UserPath::LeftHand);
        f.set_interaction_profile(&SimpleController, fakexr::UserPath::RightHand);
        f.input.openxr.poll_events();
        f.input.frame_start_update();

        f.input.openxr.poll_events();
        f.input.frame_start_update();
        let haptic = f
            .input
            .openxr
            .session_data
            .get()
            .input_data
            .get_legacy_actions()
            .unwrap()
            .actions
            .haptic
            .as_raw();

        assert!(!fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::LeftHand
        ));
        assert!(!fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::RightHand
        ));

        f.input.legacy_haptic(1, 0, 3000);
        assert!(fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::LeftHand
        ));

        f.input.legacy_haptic(2, 0, 3000);
        assert!(fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::RightHand
        ));
    }

    #[test]
    fn legacy_haptic_with_action_manifest() {
        let mut f = Fixture::new();
        f.load_actions(c"actions.json");
        f.input.openxr.restart_session();
        f.set_interaction_profile(&SimpleController, fakexr::UserPath::LeftHand);
        f.set_interaction_profile(&SimpleController, fakexr::UserPath::RightHand);
        f.input.openxr.poll_events();
        f.input.frame_start_update();

        f.input.openxr.poll_events();
        f.input.frame_start_update();

        let haptic = f
            .input
            .openxr
            .session_data
            .get()
            .input_data
            .get_loaded_actions()
            .unwrap()
            .actions
            .iter()
            .find_map(|(_k, action)| match action {
                ActionData::Haptic(haptic_action) => Some(haptic_action),
                _ => None,
            })
            .unwrap()
            .as_raw();

        assert!(!fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::LeftHand
        ));
        assert!(!fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::RightHand
        ));

        f.input.legacy_haptic(1, 0, 3000);
        assert!(fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::LeftHand
        ));

        f.input.legacy_haptic(2, 0, 3000);
        assert!(fakexr::is_haptic_activated(
            haptic,
            fakexr::UserPath::RightHand
        ));
    }
}
