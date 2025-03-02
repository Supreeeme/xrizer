use openxr as xr;
use std::f32::consts::{FRAC_PI_4, PI};
use std::sync::atomic::{AtomicBool, Ordering};
use log::error;
use openxr::{Haptic, HapticVibration};
use crate::input::{ExtraActionData};
use crate::openxr_data::SessionData;

#[derive(Debug, Clone, Copy)]
pub(super) enum DpadDirection {
    North,
    East,
    South,
    West,
    Center,
}

pub(super) struct DpadActions {
    pub xy: xr::Action<xr::Vector2f>,
    pub click_or_touch: Option<xr::Action<f32>>,
    pub haptic: Option<xr::Action<Haptic>>,
}

pub(super) struct DpadData {
    pub direction: DpadDirection,
    pub last_state: AtomicBool,
}

impl DpadData {
    const CENTER_ZONE: f32 = 0.5;

    // Thresholds for force-activated dpads, experimentally chosen to match SteamVR
    const DPAD_CLICK_THRESHOLD: f32 = 0.33;
    const DPAD_RELEASE_THRESHOLD: f32 = 0.2;
    fn state<G>(&self, extras: &ExtraActionData, session: &xr::Session<G>, subaction_path: xr::Path) -> xr::Result<Option<xr::ActionState<bool>>> {
        let Some(action) = extras.dpad_actions.as_ref() else {
            return Ok(None);
        };
        let parent_state = action.xy.state(session, xr::Path::NULL)?;
        let mut ret_state = xr::ActionState {
            current_state: false,
            last_change_time: parent_state.last_change_time, // TODO: this is wrong
            changed_since_last_sync: false,
            is_active: parent_state.is_active,
        };

        let last_active = self.last_state.load(Ordering::Relaxed);
        let active_threshold = if last_active { Self::DPAD_RELEASE_THRESHOLD } else { Self::DPAD_CLICK_THRESHOLD };

        let active = action
            .click_or_touch
            .as_ref()
            .map(|a| {
                // If this action isn't bound in the current interaction profile,
                // is_active will be false - in this case, it's probably a joystick touch dpad, in
                // which case we still want to read the current state.
                a.state(session, xr::Path::NULL)
                    .map(|s| !s.is_active || s.current_state > active_threshold)
            })
            .unwrap_or(Ok(true))?;

        if !active {
            self.last_state.store(false, Ordering::Relaxed);
            return Ok(None);
        }

        // convert to polar coordinates
        let xr::Vector2f { x, y } = parent_state.current_state;
        let radius = x.hypot(y);
        let angle = y.atan2(x);

        // pi/2 wedges, no overlap
        let in_bounds = match self.direction {
            DpadDirection::North => {
                radius >= Self::CENTER_ZONE && (FRAC_PI_4..=3.0 * FRAC_PI_4).contains(&angle)
            }
            DpadDirection::East => {
                radius >= Self::CENTER_ZONE && (-FRAC_PI_4..=FRAC_PI_4).contains(&angle)
            }
            DpadDirection::South => {
                radius >= Self::CENTER_ZONE && (-3.0 * FRAC_PI_4..=-FRAC_PI_4).contains(&angle)
            }
            // west section is disjoint with atan2
            DpadDirection::West => {
                radius >= Self::CENTER_ZONE
                    && ((3.0 * FRAC_PI_4..=PI).contains(&angle)
                        || (-PI..=-3.0 * FRAC_PI_4).contains(&angle))
            }
            DpadDirection::Center => radius < Self::CENTER_ZONE,
        };

        ret_state.current_state = in_bounds;
        if self
            .last_state
            .compare_exchange(!in_bounds, in_bounds, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            ret_state.changed_since_last_sync = true;
            if in_bounds {
                if let Some(haptic) = &action.haptic {
                    let haptic_event = HapticVibration::new()
                        .amplitude(0.25)
                        .duration(xr::Duration::MIN_HAPTIC)
                        .frequency(xr::FREQUENCY_UNSPECIFIED);
                    let _ = haptic.apply_feedback(session, subaction_path, &haptic_event)
                        .inspect_err(|e| error!("Couldn't activate dpad haptic: {e}"));
                }
            }
        }

        Ok(Some(ret_state))
    }
}

pub(super) struct GrabActions {
    pub force_action: xr::Action<f32>,
    pub value_action: xr::Action<f32>,
}

pub(super) struct GrabBindingData {
    hold_threshold: f32,
    release_threshold: f32,
    last_state: AtomicBool,
}

impl GrabBindingData {
    pub fn new(grab_threshold: Option<f32>, release_threshold: Option<f32>) -> Self {
        Self {
            hold_threshold: grab_threshold.unwrap_or(Self::DEFAULT_GRAB_THRESHOLD),
            release_threshold: release_threshold.unwrap_or(Self::DEFAULT_RELEASE_THRESHOLD),
            last_state: false.into(),
        }
    }

    // Default thresholds as set by SteamVR binding UI
    /// How much force to apply to begin a grab
    pub const DEFAULT_GRAB_THRESHOLD: f32 = 0.70;
    /// How much the value component needs to be to release the grab.
    pub const DEFAULT_RELEASE_THRESHOLD: f32 = 0.65;

    /// Returns None if the grab data is not active.
    fn grabbed<G>(
        &self,
        extra_action: &ExtraActionData,
        session: &xr::Session<G>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        // FIXME: the way this function calculates changed_since_last_sync is incorrect, as it will
        // always be false if this is called more than once between syncs. What should be done is
        // the state should be updated in UpdateActionState, but that may have other implications
        // I currently don't feel like thinking about, as this works and I haven't seen games grab action
        // state more than once beteween syncs.

        let Some(grabs) = &extra_action.grab_action else {
            return Ok(None);
        };

        let force_state = grabs.force_action.state(session, subaction_path)?;
        let value_state = grabs.value_action.state(session, subaction_path)?;
        if !force_state.is_active || !value_state.is_active {
            self.last_state.store(false, Ordering::Relaxed);
            Ok(None)
        } else {
            let prev_grabbed = self.last_state.load(Ordering::Relaxed);
            let value = if force_state.current_state > 0.0 { force_state.current_state + 1.0 } else { value_state.current_state };

            let grabbed = (prev_grabbed && value > self.release_threshold)
                || (!prev_grabbed && value >= self.hold_threshold);

            let changed_since_last_sync = grabbed != prev_grabbed;
            self.last_state.store(grabbed, Ordering::Relaxed);

            Ok(Some(xr::ActionState {
                current_state: grabbed,
                changed_since_last_sync,
                last_change_time: force_state.last_change_time,
                is_active: true,
            }))
        }
    }
}

#[derive(Default)]
pub(super) struct ToggleData {
    pub last_state: AtomicBool,
}

impl ToggleData {
    fn state<G>(
        &self,
        extra_action: &ExtraActionData,
        session: &xr::Session<G>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        let Some(action_to_read) = &extra_action.toggle_action else {
            return Ok(None)
        };
        let state = action_to_read.state(session, subaction_path)?;
        if !state.is_active {
            return Ok(None);
        }

        let s = self.last_state.load(Ordering::Relaxed);
        let current_state = if state.changed_since_last_sync && state.current_state {
            !s
        } else {
            s
        };

        let changed_since_last_sync = self.last_state
            .compare_exchange(!current_state, current_state, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok();

        Ok(Some(xr::ActionState {
            current_state,
            changed_since_last_sync,
            last_change_time: state.last_change_time,
            is_active: true,
        }))
    }
}

pub struct ThresholdBindingData {
    pub click_threshold: f32,
    pub release_threshold: f32,
    last_state: AtomicBool,
}

impl ThresholdBindingData {
    const DEFAULT_CLICK_THRESHOLD: f32 = 0.25;
    const DEFAULT_RELEASE_THRESHOLD: f32 = 0.20;

    pub fn new(click_threshold: Option<f32>, release_threshold: Option<f32>) -> Self {
        Self {
            click_threshold: click_threshold.unwrap_or(Self::DEFAULT_CLICK_THRESHOLD),
            release_threshold: release_threshold.unwrap_or(Self::DEFAULT_RELEASE_THRESHOLD),
            last_state: false.into(),
        }
    }

    fn state<G>(&self,
                extra_action: &ExtraActionData,
                session: &xr::Session<G>,
                subaction_path: xr::Path) -> xr::Result<Option<xr::ActionState<bool>>> {
        let Some(action_to_read) = &extra_action.analog_action else {
            return Ok(None)
        };
        let state = action_to_read.state(session, subaction_path)?;
        if !state.is_active {
            return Ok(None);
        }

        let s = self.last_state.load(Ordering::Relaxed);
        let threshold = if s { self.release_threshold } else { self.click_threshold };
        let current_state = state.current_state >= threshold;

        let changed_since_last_sync = self.last_state
            .compare_exchange(!current_state, current_state, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok();

        Ok(Some(xr::ActionState {
            current_state,
            changed_since_last_sync,
            last_change_time: state.last_change_time,
            is_active: true,
        }))
    }
}

pub enum BindingData {
    // For all cases where the action can be read directly, such as matching type or bool-to-float conversion,
    //  the xr::Action is read from ActionData
    // This can include actions where behavior is customized via OXR extensions

    Dpad(DpadData, xr::Path),
    Toggle(ToggleData, xr::Path),
    Grab(GrabBindingData, xr::Path),
    Threshold(ThresholdBindingData, xr::Path),
}

impl BindingData {
    pub fn state(&self, session: &SessionData, extra_data: &ExtraActionData, subaction_path: xr::Path) -> xr::Result<Option<xr::ActionState<bool>>> {
        assert_ne!(subaction_path, xr::Path::NULL);
        match self {
            BindingData::Dpad(dpad, x) if x == &subaction_path => { dpad.state(extra_data, &session.session, subaction_path) }
            BindingData::Toggle(toggle, x) if x == &subaction_path => { toggle.state(extra_data, &session.session, subaction_path) }
            BindingData::Grab(grab, x) if x == &subaction_path => { grab.grabbed(extra_data, &session.session, subaction_path) }
            BindingData::Threshold(threshold, x) if x == &subaction_path => { threshold.state(extra_data, &session.session, subaction_path) }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{tests::Fixture};
    use fakexr::UserPath::*;
    use openvr as vr;
    use crate::input::profiles::knuckles::Knuckles;
    use crate::input::profiles::vive_controller::ViveWands;

    macro_rules! get_toggle_action {
        ($fixture:expr, $handle:expr, $toggle_data:ident) => {
            let data = $fixture.input.openxr.session_data.get();
            let actions = data.input_data.get_loaded_actions().unwrap();
            let ExtraActionData { toggle_action, .. } =
                actions.try_get_extra($handle).unwrap();

            let $toggle_data = toggle_action.as_ref().unwrap();
        };
    }

    macro_rules! get_dpad_action {
        ($fixture:expr, $handle:expr, $dpad_data:ident) => {
            let data = $fixture.input.openxr.session_data.get();
            let actions = data.input_data.get_loaded_actions().unwrap();
            let ExtraActionData { dpad_actions, ..} =
                actions.try_get_extra($handle).unwrap();

            let $dpad_data = dpad_actions.as_ref().unwrap();
        };
    }

    macro_rules! get_grab_action {
        ($fixture:expr, $handle:expr, $grab_data:ident) => {
            let data = $fixture.input.openxr.session_data.get();
            let actions = data.input_data.get_loaded_actions().unwrap();
            let ExtraActionData { grab_action, .. } =
                actions.try_get_extra($handle).unwrap();

            let $grab_data = grab_action.as_ref().unwrap();
        };
    }

    #[test]
    fn dpad_input() {
        let f = Fixture::new();

        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");

        f.load_actions(c"actions_dpad.json");
        f.input.openxr.restart_session();

        get_dpad_action!(f, boolact, dpad_data);

        f.set_interaction_profile(&ViveWands, LeftHand);
        fakexr::set_action_state(
            dpad_data.xy.as_raw(),
            fakexr::ActionState::Vector2(0.0, 0.5),
            LeftHand,
        );
        fakexr::set_action_state(
            dpad_data.click_or_touch.as_ref().unwrap().as_raw(),
            fakexr::ActionState::Float(1.0),
            LeftHand,
        );

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(state.bState);
        assert!(state.bChanged);

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(state.bState);
        assert!(!state.bChanged);

        fakexr::set_action_state(
            dpad_data.xy.as_raw(),
            fakexr::ActionState::Vector2(0.5, 0.0),
            LeftHand,
        );
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(!state.bState);
        assert!(state.bChanged);
    }

    #[test]
    fn dpad_input_different_sets_have_different_actions() {
        let f = Fixture::new();

        let boolact_set1 = f.get_action_handle(c"/actions/set1/in/boolact");
        let boolact_set2 = f.get_action_handle(c"/actions/set2/in/boolact");

        f.load_actions(c"actions_dpad.json");

        get_dpad_action!(f, boolact_set1, set1_dpad);
        get_dpad_action!(f, boolact_set2, set2_dpad);

        assert_ne!(set1_dpad.xy.as_raw(), set2_dpad.xy.as_raw());
    }

    #[test]
    fn dpad_input_use_non_dpad_when_available() {
        let f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");

        f.load_actions(c"actions_dpad_mixed.json");
        f.input.openxr.restart_session();

        get_dpad_action!(f, boolact, _dpad);

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(!state.bState);
        assert!(!state.bActive);
        assert!(!state.bChanged);

        fakexr::set_action_state(
            f.get_action::<bool>(boolact),
            fakexr::ActionState::Bool(true),
            LeftHand,
        );
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bState);
        assert!(state.bActive);
        assert!(state.bChanged);
    }

    #[test]
    fn grab_binding() {
        let f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact2");
        f.load_actions(c"actions.json");
        get_grab_action!(f, boolact, grab_data);

        f.set_interaction_profile(&Knuckles, LeftHand);
        let value_state_check = |force, value, state, changed, line| {
            fakexr::set_action_state(
                grab_data.force_action.as_raw(),
                fakexr::ActionState::Float(force),
                LeftHand,
            );
            fakexr::set_action_state(
                grab_data.value_action.as_raw(),
                fakexr::ActionState::Float(value),
                LeftHand,
            );
            f.sync(vr::VRActiveActionSet_t {
                ulActionSet: set1,
                ..Default::default()
            });

            let s = f.get_bool_state(boolact).unwrap();
            assert_eq!(s.bState, state, "state failed (line {line})");
            assert!(s.bActive, "active failed (line {line})");
            assert_eq!(s.bChanged, changed, "changed failed (line {line})");
        };

        let grab = GrabBindingData::DEFAULT_GRAB_THRESHOLD;
        let release = GrabBindingData::DEFAULT_RELEASE_THRESHOLD;
        value_state_check(0.0, grab - 0.1, false, false, line!());
        value_state_check(0.0, grab + 0.1, true, true, line!());
        value_state_check(0.1, 0.0, true, false, line!());
        value_state_check(0.0, 1.0, true, false, line!());
        value_state_check(0.0, release, false, true, line!());
        value_state_check(0.0, grab - 0.1, false, false, line!());
    }

    #[test]
    fn grab_per_hand() {
        let f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");

        let left = f.get_input_source_handle(c"/user/hand/left");
        let right = f.get_input_source_handle(c"/user/hand/right");

        f.load_actions(c"actions_dpad_mixed.json");

        get_grab_action!(f, set1, grab_data);

        f.set_interaction_profile(&Knuckles, LeftHand);
        f.set_interaction_profile(&Knuckles, RightHand);

        let value_state_check = |force, value, hand, state, changed, line| {
            fakexr::set_action_state(
                grab_data.force_action.as_raw(),
                fakexr::ActionState::Float(force),
                hand,
            );
            fakexr::set_action_state(
                grab_data.value_action.as_raw(),
                fakexr::ActionState::Float(value),
                hand,
            );
            f.sync(vr::VRActiveActionSet_t {
                ulActionSet: set1,
                ..Default::default()
            });

            let restrict = match hand {
                LeftHand => left,
                RightHand => right,
            };
            let s = f.get_bool_state_hand(boolact, restrict).unwrap();
            assert_eq!(s.bState, state, "State wrong (line {line})");
            assert!(s.bActive, "Active wrong (line {line})");
            assert_eq!(s.bChanged, changed, "Changed wrong (line {line})");
        };

        let grab = GrabBindingData::DEFAULT_GRAB_THRESHOLD;
        let release = GrabBindingData::DEFAULT_RELEASE_THRESHOLD;
        value_state_check(0.0, grab - 0.1, LeftHand, false, false, line!());
        value_state_check(0.0, grab - 0.1, RightHand, false, false, line!());

        value_state_check(0.0, grab, LeftHand, true, true, line!());
        value_state_check(0.0, grab, RightHand, true, true, line!());

        value_state_check(0.0, release, LeftHand, false, true, line!());
        value_state_check(0.0, 1.0, RightHand, true, false, line!());
    }

    #[test]
    fn grab_binding_custom_threshold() {
        let f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");
        f.load_actions(c"actions.json");
        get_grab_action!(f, boolact, grab_data);

        f.set_interaction_profile(&Knuckles, RightHand);
        let value_state_check = |force, value, state, changed, line| {
            fakexr::set_action_state(
                grab_data.force_action.as_raw(),
                fakexr::ActionState::Float(force),
                RightHand,
            );
            fakexr::set_action_state(
                grab_data.value_action.as_raw(),
                fakexr::ActionState::Float(value),
                RightHand,
            );
            f.sync(vr::VRActiveActionSet_t {
                ulActionSet: set1,
                ..Default::default()
            });

            let s = f.get_bool_state(boolact).unwrap();
            assert_eq!(s.bState, state, "state failed (line {line})");
            assert!(s.bActive, "active failed (line {line})");
            assert_eq!(s.bChanged, changed, "changed failed (line {line})");
        };

        let grab = 0.16;
        let release = 0.15;
        value_state_check(0.0, 1.0, false, false, line!());
        value_state_check(grab + 0.01, 0.0, true, true, line!());
        value_state_check(grab - 0.001, 0.0, true, false, line!());
        value_state_check(release, 0.0, false, true, line!());
        value_state_check(0.0, 1.0, false, false, line!());
    }

    #[test]
    fn toggle_button() {
        let f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");
        f.load_actions(c"actions_toggle.json");

        get_toggle_action!(f, boolact, toggle_data);

        f.set_interaction_profile(&Knuckles, LeftHand);
        fakexr::set_action_state(
            toggle_data.as_raw(),
            fakexr::ActionState::Bool(true),
            LeftHand,
        );

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(state.bState);
        assert!(state.bChanged);

        fakexr::set_action_state(
            toggle_data.as_raw(),
            fakexr::ActionState::Bool(false),
            LeftHand,
        );

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(state.bState);
        assert!(!state.bChanged);

        fakexr::set_action_state(
            toggle_data.as_raw(),
            fakexr::ActionState::Bool(true),
            LeftHand,
        );

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(!state.bState);
        assert!(state.bChanged);

        // no change across sync point
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(!state.bState);
        assert!(!state.bChanged);
    }

    #[test]
    fn toggle_button_per_hand() {
        let f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");
        let left = f.get_input_source_handle(c"/user/hand/left");
        let right = f.get_input_source_handle(c"/user/hand/right");

        f.load_actions(c"actions_toggle.json");
        get_toggle_action!(f, boolact, toggle_data);

        let act = toggle_data.as_raw();

        f.set_interaction_profile(&Knuckles, LeftHand);
        f.set_interaction_profile(&Knuckles, RightHand);
        fakexr::set_action_state(act, false.into(), LeftHand);
        fakexr::set_action_state(act, false.into(), RightHand);
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s_left = f.get_bool_state_hand(boolact, left).unwrap();
        assert!(s_left.bActive);
        assert!(!s_left.bState);
        assert!(!s_left.bChanged);

        let s_right = f.get_bool_state_hand(boolact, right).unwrap();
        assert!(s_right.bActive);
        assert!(!s_right.bState);
        assert!(!s_right.bChanged);

        fakexr::set_action_state(act, true.into(), LeftHand);
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s_left = f.get_bool_state_hand(boolact, left).unwrap();
        assert!(s_left.bActive);
        assert!(s_left.bState);
        assert!(s_left.bChanged);

        let s_right = f.get_bool_state_hand(boolact, right).unwrap();
        assert!(s_right.bActive);
        assert!(!s_right.bState);
        assert!(!s_right.bChanged);

        fakexr::set_action_state(act, false.into(), LeftHand);
        fakexr::set_action_state(act, true.into(), RightHand);
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s_left = f.get_bool_state_hand(boolact, left).unwrap();
        assert!(s_left.bActive);
        assert!(s_left.bState);
        assert!(!s_left.bChanged);

        let s_right = f.get_bool_state_hand(boolact, right).unwrap();
        assert!(s_right.bActive);
        assert!(s_right.bState);
        assert!(s_right.bChanged);
    }
}
