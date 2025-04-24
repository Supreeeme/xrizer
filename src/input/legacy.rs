use super::{Input, InteractionProfile, Profiles};
use crate::openxr_data::{self, Hand, SessionData};
use glam::Quat;
use log::{debug, trace, warn};
use openvr as vr;
use openxr as xr;
use std::{
    ops::Deref,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        RwLock, RwLockReadGuard,
    },
};

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
fn button_mask_from_id(id: vr::EVRButtonId) -> u64 {
    1_u64 << (id as u32)
}

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

        let Some(hand) = self.device_index_to_hand(device_index) else {
            debug!("tried getting controller state, but device index {device_index} is invalid or not a controller!");
            return false;
        };

        let hand_path = self.get_subaction_path(hand);

        let data = self.openxr.session_data.get();

        let state = unsafe { state.as_mut() }.unwrap();
        *state = Default::default();

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
    a: xr::Action<bool>,
    trigger_click: xr::Action<bool>,
    squeeze_click: xr::Action<bool>,
    trigger: xr::Action<f32>,
    squeeze: xr::Action<f32>,
    // This can be a stick or a trackpad, so we'll just call it "xy"
    main_xy: xr::Action<xr::Vector2f>,
    main_xy_touch: xr::Action<bool>,
    main_xy_click: xr::Action<bool>,
}

pub(super) struct LegacyActionData {
    pub set: xr::ActionSet,
    pub left_spaces: HandSpaces,
    pub right_spaces: HandSpaces,
    pub actions: LegacyActions,
}

impl LegacyActionData {
    pub fn new(instance: &xr::Instance, left_hand: xr::Path, right_hand: xr::Path) -> Self {
        debug!("creating legacy actions");
        let leftright = [left_hand, right_hand];
        let create_spaces = |hand| {
            let hand_path = match hand {
                Hand::Left => left_hand,
                Hand::Right => right_hand,
            };
            HandSpaces {
                hand,
                hand_path,
                raw: RwLock::new(None),
            }
        };

        let left_spaces = create_spaces(Hand::Left);
        let right_spaces = create_spaces(Hand::Right);

        let set = instance
            .create_action_set("xrizer-legacy-set", "XRizer Legacy Set", 0)
            .unwrap();

        let actions = LegacyActions {
            grip_pose: set
                .create_action("grip-pose", "Grip Pose", &leftright)
                .unwrap(),
            aim_pose: set
                .create_action("aim-pose", "Aim Pose", &leftright)
                .unwrap(),
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
        };

        Self {
            set,
            left_spaces,
            right_spaces,
            actions,
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

    /// Based on the controller jsons in SteamVR, the "raw" pose
    /// This is stored as a space so we can locate hand joints relative to it for skeletal data.
    raw: RwLock<Option<xr::Space>>,
}

pub(super) struct SpaceReadGuard<'a>(RwLockReadGuard<'a, Option<xr::Space>>);
impl Deref for SpaceReadGuard<'_> {
    type Target = xr::Space;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl HandSpaces {
    pub fn try_get_or_init_raw(
        &self,
        hand_profile: &Option<&dyn InteractionProfile>,
        session_data: &SessionData,
        actions: &LegacyActions,
    ) -> Option<SpaceReadGuard> {
        {
            let raw = self.raw.read().unwrap();
            if raw.is_some() {
                return Some(SpaceReadGuard(raw));
            }
        }
        {
            let Some(profile) = hand_profile.as_ref() else {
                trace!("no hand profile, no raw space will be created");
                return None;
            };

            let offset = profile.offset_grip_pose(self.hand);
            let translation = offset.w_axis.truncate();
            let rotation = Quat::from_mat4(&offset);

            let offset_pose = xr::Posef {
                orientation: xr::Quaternionf {
                    x: rotation.x,
                    y: rotation.y,
                    z: rotation.z,
                    w: rotation.w,
                },
                position: xr::Vector3f {
                    x: translation.x,
                    y: translation.y,
                    z: translation.z,
                },
            };

            *self.raw.write().unwrap() = Some(
                actions
                    .grip_pose
                    .create_space(&session_data.session, self.hand_path, offset_pose)
                    .unwrap(),
            );
        }

        Some(SpaceReadGuard(self.raw.read().unwrap()))
    }

    pub fn reset_raw(&self) {
        *self.raw.write().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use crate::input::profiles::knuckles::Knuckles;
    use crate::input::tests::Fixture;
    use openvr as vr;

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
        let f = Fixture::new();
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
                .legacy_actions
                .get()
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
                {
                    state.ulButtonTouched
                }
            } else {
                {
                    state.ulButtonPressed
                }
            };

            match expect {
                true => {
                    let active_mask = ids
                        .iter()
                        .copied()
                        .fold(0, |val, id| val | super::button_mask_from_id(id));

                    assert_eq!(
                        mask, active_mask,
                        "Button not active - state: {:b} | button mask: {mask:b}",
                        mask
                    );
                }
                false => {
                    assert_eq!(mask, 0, "Button should be inactive - state: {:b}", mask);
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
}
