use super::action_manifest::{ClickThresholdParams, GrabParameters};
use crate::input::{ActionData, ExtraActionData};
use crate::openxr_data::SessionData;
use crate::AtomicF32;
use log::error;
use openxr as xr;
use std::f32::consts::{FRAC_PI_4, PI};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use xr::{Haptic, HapticVibration};

mod marker {
    use openxr as xr;

    pub trait ActionsMarker {
        type T<U: xr::ActionTy>;
    }
    pub enum Actions {}
    pub enum Names {}

    impl ActionsMarker for Actions {
        type T<U: xr::ActionTy> = xr::Action<U>;
    }
    impl ActionsMarker for Names {
        type T<U: xr::ActionTy> = String;
    }

    pub type Action<T, M> = <M as ActionsMarker>::T<T>;
}
use marker::*;
pub(super) use marker::{Actions, Names};

pub(super) trait AsActionData {
    fn as_action_data(&self) -> Vec<ActionData>;
}

impl AsActionData for xr::Action<bool> {
    fn as_action_data(&self) -> Vec<ActionData> {
        vec![ActionData::Bool(self.clone())]
    }
}
impl AsActionData for xr::Action<f32> {
    fn as_action_data(&self) -> Vec<ActionData> {
        vec![ActionData::Vector1 {
            action: self.clone(),
            last_value: AtomicF32::new(0.),
        }]
    }
}
impl AsActionData for xr::Action<xr::Vector2f> {
    fn as_action_data(&self) -> Vec<ActionData> {
        vec![ActionData::Vector2 {
            action: self.clone(),
            last_value: (AtomicF32::new(0.), AtomicF32::new(0.)),
        }]
    }
}
impl AsActionData for () {
    fn as_action_data(&self) -> Vec<ActionData> {
        vec![]
    }
}

pub(super) trait AsIter {
    fn as_iter(&self) -> impl Iterator<Item = &str>;
    fn from_iter(it: impl IntoIterator<Item = String>) -> Self;
}

impl AsIter for String {
    fn as_iter(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.as_str())
    }

    fn from_iter(it: impl IntoIterator<Item = String>) -> Self {
        it.into_iter().next().unwrap()
    }
}

impl AsIter for () {
    fn as_iter(&self) -> impl Iterator<Item = &str> {
        std::iter::empty()
    }
    fn from_iter(_: impl IntoIterator<Item = String>) -> Self {}
}

pub(super) trait CustomBindingHelper:
    CustomBinding<ExtraActions<Actions>: AsActionData> + CustomBinding<ExtraActions<Names>: AsIter>
{
}

impl<T> CustomBindingHelper for T where
    T: CustomBinding<ExtraActions<Actions>: AsActionData>
        + CustomBinding<ExtraActions<Names>: AsIter>
{
}

pub(super) trait CustomBinding: Sized {
    type ExtraActions<M: ActionsMarker>;
    type BindingParams;

    fn extra_action_names(cleaned_action_name: &str) -> Self::ExtraActions<Names>;
    fn get_actions(
        extra_actions: &mut ExtraActionData,
    ) -> Option<&mut Option<Self::ExtraActions<Actions>>>;
    fn create_actions(
        action_names: &Self::ExtraActions<Names>,
        action_set: &xr::ActionSet,
        subaction_paths: &[xr::Path],
    ) -> Self::ExtraActions<Actions>;
    fn create_binding_data(params: Option<&Self::BindingParams>) -> BindingType;

    fn state(
        &self,
        actions: &Self::ExtraActions<Actions>,
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>>;
}

#[derive(Debug, Clone, Copy)]
pub(super) enum DpadDirection {
    North,
    East,
    South,
    West,
    Center,
}

#[derive(Clone)]
pub(super) struct DpadActions {
    pub xy: xr::Action<xr::Vector2f>,
    pub click_or_touch: Option<xr::Action<f32>>,
    pub haptic: Option<xr::Action<Haptic>>,
}

pub(super) struct DpadBindingParams {
    pub actions: DpadActions,
    pub direction: DpadDirection,
}

pub(super) struct DpadData {
    actions: DpadActions,
    direction: DpadDirection,
    last_state: AtomicBool,
    active: AtomicBool,
    changed: AtomicBool,
}

impl DpadData {
    const CENTER_ZONE: f32 = 0.5;

    // Thresholds for force-activated dpads, experimentally chosen to match SteamVR
    const DPAD_CLICK_THRESHOLD: f32 = 0.33;
    const DPAD_RELEASE_THRESHOLD: f32 = 0.2;
}

impl CustomBinding for DpadData {
    // The extra actions for the dpad are shared across all directions,
    // so we pass them in via the BindingParams.
    type ExtraActions<M: ActionsMarker> = ();
    type BindingParams = DpadBindingParams;
    fn extra_action_names(_: &str) -> Self::ExtraActions<Names> {}
    fn get_actions(_: &mut ExtraActionData) -> Option<&mut Option<Self::ExtraActions<Actions>>> {
        None
    }
    fn create_actions(
        _: &Self::ExtraActions<Names>,
        _: &xr::ActionSet,
        _: &[xr::Path],
    ) -> Self::ExtraActions<Actions> {
    }
    fn create_binding_data(params: Option<&Self::BindingParams>) -> BindingType {
        let DpadBindingParams { actions, direction } = params.unwrap();
        BindingType::Dpad(DpadData {
            actions: actions.clone(),
            direction: *direction,
            last_state: false.into(),
            active: false.into(),
            changed: false.into(),
        })
    }

    fn state(
        &self,
        _: &(),
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        let action = &self.actions;
        let parent_state = action.xy.state(session, subaction_path)?;
        let mut ret_state = xr::ActionState {
            current_state: false,
            last_change_time: parent_state.last_change_time, // TODO: this is wrong
            changed_since_last_sync: false,
            is_active: parent_state.is_active,
        };

        let last_active = self.last_state.load(Ordering::Relaxed);
        let active_threshold = if last_active {
            Self::DPAD_RELEASE_THRESHOLD
        } else {
            Self::DPAD_CLICK_THRESHOLD
        };

        let active = action
            .click_or_touch
            .as_ref()
            .map(|a| {
                // If this action isn't bound in the current interaction profile,
                // is_active will be false - in this case, it's probably a joystick touch dpad, in
                // which case we still want to read the current state.
                a.state(session, subaction_path)
                    .map(|s| !s.is_active || s.current_state > active_threshold)
            })
            .unwrap_or(Ok(true))?;

        if !active {
            let changed = self
                .last_state
                .compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok();
            self.changed.store(changed, Ordering::Relaxed);
            self.active.store(false, Ordering::Relaxed);
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
                    let _ = haptic
                        .apply_feedback(session, subaction_path, &haptic_event)
                        .inspect_err(|e| error!("Couldn't activate dpad haptic: {e}"));
                }
            }
        }

        self.changed
            .store(ret_state.changed_since_last_sync, Ordering::Relaxed);
        self.active.store(true, Ordering::Relaxed);

        Ok(Some(ret_state))
    }
}

pub(super) struct GrabActions<M: ActionsMarker> {
    pub force_action: Action<f32, M>,
    pub value_action: Action<f32, M>,
}

pub(super) struct GrabBindingData {
    hold_threshold: f32,
    release_threshold: f32,
    last_state: AtomicBool,
}

impl GrabBindingData {
    // Default thresholds as set by SteamVR binding UI
    /// How much force to apply to begin a grab
    const DEFAULT_GRAB_THRESHOLD: f32 = 0.70;
    /// How much the value component needs to be to release the grab.
    const DEFAULT_RELEASE_THRESHOLD: f32 = 0.65;

    pub fn new(grab_threshold: Option<f32>, release_threshold: Option<f32>) -> Self {
        Self {
            hold_threshold: grab_threshold.unwrap_or(Self::DEFAULT_GRAB_THRESHOLD),
            release_threshold: release_threshold.unwrap_or(Self::DEFAULT_RELEASE_THRESHOLD),
            last_state: false.into(),
        }
    }
}

impl AsActionData for GrabActions<Actions> {
    fn as_action_data(&self) -> Vec<ActionData> {
        vec![
            ActionData::Vector1 {
                action: self.force_action.clone(),
                last_value: AtomicF32::new(0.),
            },
            ActionData::Vector1 {
                action: self.value_action.clone(),
                last_value: AtomicF32::new(0.),
            },
        ]
    }
}

impl AsIter for GrabActions<Names> {
    fn as_iter(&self) -> impl Iterator<Item = &str> {
        [self.force_action.as_str(), self.value_action.as_str()].into_iter()
    }
    fn from_iter(it: impl IntoIterator<Item = String>) -> Self {
        let mut it = it.into_iter();
        let force_action = it.next().unwrap();
        let value_action = it.next().unwrap();
        Self {
            force_action,
            value_action,
        }
    }
}

impl CustomBinding for GrabBindingData {
    type ExtraActions<M: ActionsMarker> = GrabActions<M>;
    type BindingParams = GrabParameters;

    fn extra_action_names(cleaned_action_name: &str) -> Self::ExtraActions<Names> {
        GrabActions {
            force_action: [cleaned_action_name, "_grabactionf"].concat(),
            value_action: [cleaned_action_name, "_grabactionv"].concat(),
        }
    }

    fn get_actions(
        extra_actions: &mut ExtraActionData,
    ) -> Option<&mut Option<Self::ExtraActions<Actions>>> {
        Some(&mut extra_actions.grab_actions)
    }

    fn create_actions(
        action_names: &Self::ExtraActions<Names>,
        action_set: &xr::ActionSet,
        subaction_paths: &[xr::Path],
    ) -> Self::ExtraActions<Actions> {
        let GrabActions {
            force_action: force_name,
            value_action: value_name,
        } = action_names;
        let localized = format!("{force_name} grab action (force)");
        let force_action = action_set
            .create_action(force_name, &localized, subaction_paths)
            .unwrap();
        let localizedv = format!("{value_name} grab action (value)");
        let value_action = action_set
            .create_action(value_name, &localizedv, subaction_paths)
            .unwrap();

        GrabActions {
            force_action,
            value_action,
        }
    }

    fn create_binding_data(params: Option<&Self::BindingParams>) -> BindingType {
        BindingType::Grab(GrabBindingData::new(
            params
                .and_then(|x| x.value_hold_threshold.as_deref())
                .copied(),
            params
                .and_then(|x| x.value_release_threshold.as_deref())
                .copied(),
        ))
    }

    fn state(
        &self,
        grabs: &Self::ExtraActions<Actions>,
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        let force_state = grabs.force_action.state(session, subaction_path)?;
        let value_state = grabs.value_action.state(session, subaction_path)?;
        if !force_state.is_active || !value_state.is_active {
            self.last_state.store(false, Ordering::Relaxed);
            Ok(None)
        } else {
            let prev_grabbed = self.last_state.load(Ordering::Relaxed);
            let value = if force_state.current_state > 0.0 {
                force_state.current_state + 1.0
            } else {
                value_state.current_state
            };

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
    last_state: AtomicBool,
}

impl CustomBinding for ToggleData {
    type ExtraActions<M: ActionsMarker> = Action<bool, M>;
    type BindingParams = ();

    fn extra_action_names(cleaned_action_name: &str) -> Action<bool, Names> {
        [cleaned_action_name, "_tgl"].concat()
    }

    fn get_actions(
        extra_actions: &mut ExtraActionData,
    ) -> Option<&mut Option<Self::ExtraActions<Actions>>> {
        Some(&mut extra_actions.toggle_action)
    }

    fn create_actions(
        action_name: &String,
        action_set: &xr::ActionSet,
        subaction_paths: &[xr::Path],
    ) -> Self::ExtraActions<Actions> {
        action_set
            .create_action(
                action_name,
                &format!("{action_name} (toggle)"),
                subaction_paths,
            )
            .unwrap()
    }

    fn create_binding_data(_: Option<&()>) -> BindingType {
        BindingType::Toggle(ToggleData::default())
    }

    fn state(
        &self,
        action: &xr::Action<bool>,
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        let state = action.state(session, subaction_path)?;
        if !state.is_active {
            return Ok(None);
        }

        let s = self.last_state.load(Ordering::Relaxed);
        let current_state = if state.changed_since_last_sync && state.current_state {
            !s
        } else {
            s
        };

        let changed_since_last_sync = self
            .last_state
            .compare_exchange(
                !current_state,
                current_state,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok();

        Ok(Some(xr::ActionState {
            current_state,
            changed_since_last_sync,
            last_change_time: state.last_change_time,
            is_active: true,
        }))
    }
}

pub(super) struct ThresholdBindingData<T: ThresholdType> {
    click_threshold: f32,
    release_threshold: f32,
    last_state: AtomicBool,
    _marker: std::marker::PhantomData<T>,
}

pub(super) trait ThresholdType: Sized {
    type T: xr::ActionTy;
    const SUFFIX: &str;
    fn action(actions: &mut ExtraActionData) -> &mut Option<xr::Action<Self::T>>;
    fn binding_data(data: ThresholdBindingData<Self>) -> BindingType;
    fn state(
        action: &xr::Action<Self::T>,
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<xr::ActionState<f32>>;
}
pub(super) struct Vector2;
pub(super) struct Float;

impl ThresholdType for Vector2 {
    type T = xr::Vector2f;
    const SUFFIX: &str = "_asfloat2";
    fn action(actions: &mut ExtraActionData) -> &mut Option<xr::Action<Self::T>> {
        &mut actions.vector2_action
    }
    fn binding_data(data: ThresholdBindingData<Self>) -> BindingType {
        BindingType::ThresholdVec2(data)
    }
    fn state(
        action: &xr::Action<Self::T>,
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<xr::ActionState<f32>> {
        let state = action.state(session, subaction_path)?;
        Ok(xr::ActionState {
            is_active: state.is_active,
            changed_since_last_sync: state.changed_since_last_sync,
            last_change_time: state.last_change_time,
            current_state: state.current_state.x.hypot(state.current_state.y),
        })
    }
}

impl ThresholdType for Float {
    type T = f32;
    const SUFFIX: &str = "_asfloat";
    fn action(actions: &mut ExtraActionData) -> &mut Option<xr::Action<Self::T>> {
        &mut actions.analog_action
    }
    fn binding_data(data: ThresholdBindingData<Self>) -> BindingType {
        BindingType::ThresholdFloat(data)
    }
    fn state(
        action: &xr::Action<Self::T>,
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<xr::ActionState<f32>> {
        action.state(session, subaction_path)
    }
}

pub(super) type ThresholdBindingVector2 = ThresholdBindingData<Vector2>;
pub(super) type ThresholdBindingFloat = ThresholdBindingData<Float>;

pub(super) type ThresholdAction<T, M> = Action<<T as ThresholdType>::T, M>;

impl<T: ThresholdType> ThresholdBindingData<T> {
    const DEFAULT_CLICK_THRESHOLD: f32 = 0.25;
    const DEFAULT_RELEASE_THRESHOLD: f32 = 0.20;

    pub fn new(click_threshold: Option<f32>, release_threshold: Option<f32>) -> Self {
        Self {
            click_threshold: click_threshold.unwrap_or(Self::DEFAULT_CLICK_THRESHOLD),
            release_threshold: release_threshold.unwrap_or(Self::DEFAULT_RELEASE_THRESHOLD),
            last_state: false.into(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: ThresholdType> CustomBinding for ThresholdBindingData<T> {
    type ExtraActions<M: ActionsMarker> = ThresholdAction<T, M>;
    type BindingParams = ClickThresholdParams;

    fn extra_action_names(cleaned_action_name: &str) -> Self::ExtraActions<Names> {
        [cleaned_action_name, T::SUFFIX].concat()
    }

    fn get_actions(
        extra_actions: &mut ExtraActionData,
    ) -> Option<&mut Option<Self::ExtraActions<Actions>>> {
        Some(T::action(extra_actions))
    }

    fn create_actions(
        action_name: &ThresholdAction<T, Names>,
        action_set: &xr::ActionSet,
        subaction_paths: &[xr::Path],
    ) -> Self::ExtraActions<Actions> {
        action_set
            .create_action(
                action_name,
                &format!("{action_name} ({})", T::SUFFIX),
                subaction_paths,
            )
            .unwrap()
    }

    fn create_binding_data(params: Option<&Self::BindingParams>) -> BindingType {
        T::binding_data(ThresholdBindingData::new(
            params
                .and_then(|x| x.click_activate_threshold.as_deref())
                .copied(),
            params
                .and_then(|x| x.click_deactivate_threshold.as_deref())
                .copied(),
        ))
    }

    fn state(
        &self,
        action: &Self::ExtraActions<Actions>,
        session: &xr::Session<xr::AnyGraphics>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        let state = T::state(action, session, subaction_path)?;
        if !state.is_active {
            return Ok(None);
        }

        let s = self.last_state.load(Ordering::Relaxed);
        let threshold = if s {
            self.release_threshold
        } else {
            self.click_threshold
        };
        let current_state = state.current_state >= threshold;

        let changed_since_last_sync = self
            .last_state
            .compare_exchange(
                !current_state,
                current_state,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok();

        Ok(Some(xr::ActionState {
            current_state,
            changed_since_last_sync,
            last_change_time: state.last_change_time,
            is_active: true,
        }))
    }
}

enum BindingState {
    Unsynced,
    Synced(Option<xr::ActionState<bool>>),
}

pub struct BindingData {
    pub ty: BindingType,
    pub hand: xr::Path,
    last_state: Mutex<BindingState>,
}

impl BindingData {
    pub fn new(ty: BindingType, hand: xr::Path) -> Self {
        Self {
            ty,
            hand,
            last_state: Mutex::new(BindingState::Unsynced),
        }
    }
}

pub enum BindingType {
    // For all cases where the action can be read directly, such as matching type or bool-to-float conversion,
    //  the xr::Action is read from ActionData
    // This can include actions where behavior is customized via OXR extensions
    Dpad(DpadData),
    Toggle(ToggleData),
    Grab(GrabBindingData),
    ThresholdFloat(ThresholdBindingFloat),
    ThresholdVec2(ThresholdBindingVector2),
}

impl BindingData {
    pub fn unsync(&self) {
        *self.last_state.lock().unwrap() = BindingState::Unsynced;
    }

    pub fn state(
        &self,
        session: &SessionData,
        extra_data: &ExtraActionData,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        assert_ne!(subaction_path, xr::Path::NULL);
        macro_rules! get_state {
            ($data:ident, $action_name:ident) => {{
                let Some(action) = extra_data.$action_name.as_ref() else {
                    return Ok(None);
                };
                $data.state(action, &session.session, subaction_path)
            }};
        }

        if self.hand != subaction_path {
            return Ok(None);
        }

        let mut last_state = self.last_state.lock().unwrap();
        if let BindingState::Synced(state) = *last_state {
            return Ok(state);
        }

        let state = match &self.ty {
            BindingType::Dpad(dpad) => dpad.state(&(), &session.session, subaction_path),
            BindingType::Toggle(toggle) => {
                get_state!(toggle, toggle_action)
            }
            BindingType::Grab(grab) => {
                get_state!(grab, grab_actions)
            }
            BindingType::ThresholdFloat(threshold) => {
                get_state!(threshold, analog_action)
            }
            BindingType::ThresholdVec2(threshold) => {
                get_state!(threshold, vector2_action)
            }
        }?;

        *last_state = BindingState::Synced(state);
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::profiles::knuckles::Knuckles;
    use crate::input::profiles::oculus_touch::Touch;
    use crate::input::profiles::vive_controller::ViveWands;
    use crate::input::tests::{ExtraActionType, Fixture};
    use crate::input::InteractionProfile;
    use fakexr::ActionState;
    use fakexr::UserPath::*;
    use openvr as vr;
    use slotmap::Key;

    macro_rules! get_toggle_action {
        ($fixture:expr, $handle:expr, $toggle_data:ident) => {
            let input = $fixture.input.clone();
            let data = input.openxr.session_data.get();
            let actions = data.input_data.get_loaded_actions().unwrap();
            let ExtraActionData { toggle_action, .. } = actions.try_get_extra($handle).unwrap();

            let $toggle_data = toggle_action.as_ref().unwrap();
        };
    }

    macro_rules! get_analog_action {
        ($fixture:expr, $handle:expr, $analog_data:ident) => {
            let input = $fixture.input.clone();
            let data = input.openxr.session_data.get();
            let actions = data.input_data.get_loaded_actions().unwrap();
            let ExtraActionData { analog_action, .. } = actions.try_get_extra($handle).unwrap();

            let $analog_data = analog_action.as_ref().unwrap();
        };
    }

    macro_rules! get_dpad_action {
        ($fixture:expr, $handle:expr, $dpad_data:ident, $profile:ident) => {
            let input = $fixture.input.clone();
            let data = input.openxr.session_data.get();
            let actions = data.input_data.get_loaded_actions().unwrap();
            let path = $fixture
                .input
                .openxr
                .instance
                .string_to_path($profile.profile_path())
                .unwrap();
            let bindings = actions.try_get_bindings($handle, path).unwrap();

            let bindings: Vec<&DpadData> = bindings
                .iter()
                .filter_map(|x| match x {
                    BindingData {
                        ty: BindingType::Dpad(a),
                        ..
                    } => Some(a),
                    _ => None,
                })
                .collect();
            if bindings.len() != 1 {
                panic!("Got {} dpad bindings when one was expected", bindings.len());
            }

            let $dpad_data = &bindings[0].actions;
        };
    }

    macro_rules! get_grab_action {
        ($fixture:expr, $handle:expr, $grab_data:ident) => {
            let input = $fixture.input.clone();
            let data = input.openxr.session_data.get();
            let actions = data.input_data.get_loaded_actions().unwrap();
            let ExtraActionData { grab_actions, .. } = actions.try_get_extra($handle).unwrap();

            let $grab_data = grab_actions.as_ref().unwrap();
        };
    }

    #[test]
    fn dpad_input() {
        let mut f = Fixture::new();

        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");

        f.load_actions(c"actions_dpad.json");
        f.input.openxr.restart_session();

        get_dpad_action!(f, boolact, dpad_data, ViveWands);

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

        get_dpad_action!(f, boolact_set1, set1_dpad, ViveWands);
        get_dpad_action!(f, boolact_set2, set2_dpad, ViveWands);

        assert_ne!(set1_dpad.xy.as_raw(), set2_dpad.xy.as_raw());
    }

    #[test]
    fn dpad_input_use_non_dpad_when_available() {
        let mut f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");

        f.load_actions(c"actions_dpad_mixed.json");
        f.input.openxr.restart_session();

        get_dpad_action!(f, boolact, _dpad, ViveWands);

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
    fn dpad_cross_profile_actions() {
        let mut f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");

        f.load_actions(c"actions_dpad_multi.json");
        f.input.openxr.restart_session();

        get_dpad_action!(f, boolact, dpad_data_vive, ViveWands);
        get_dpad_action!(f, boolact, dpad_data_knuckles, Knuckles);

        // These bindings are on different dpads (trackpad vs thumbstick)
        assert_ne!(dpad_data_vive.xy.as_raw(), dpad_data_knuckles.xy.as_raw());

        f.set_interaction_profile(&ViveWands, LeftHand);
        fakexr::set_action_state(
            dpad_data_vive.xy.as_raw(),
            fakexr::ActionState::Vector2(0.0, 0.5),
            LeftHand,
        );
        fakexr::set_action_state(
            dpad_data_vive.click_or_touch.as_ref().unwrap().as_raw(),
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

        f.set_interaction_profile(&Knuckles, LeftHand);
        fakexr::set_action_state(
            dpad_data_knuckles.xy.as_raw(),
            fakexr::ActionState::Vector2(0.0, 0.0),
            LeftHand,
        );
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        // Any input on touchpad shouldn't trigger thumbstick dpad
        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(!state.bState);
        assert!(!state.bChanged);

        fakexr::set_action_state(
            dpad_data_knuckles.xy.as_raw(),
            fakexr::ActionState::Vector2(0.0, 0.5),
            LeftHand,
        );
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        // Verify thumbstick deflection is sufficient
        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(state.bState);
        assert!(state.bChanged);

        f.set_interaction_profile(&ViveWands, LeftHand);
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        // Verify action state stickiness across interaction profiles that this test assumes
        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(state.bState);
        assert!(!state.bChanged);

        fakexr::set_action_state(
            dpad_data_vive.xy.as_raw(),
            fakexr::ActionState::Vector2(0.0, 0.0),
            LeftHand,
        );

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        // Verify dpad deactivation on sliding input to center
        let state = f.get_bool_state(boolact).unwrap();
        assert!(state.bActive);
        assert!(!state.bState);
        assert!(state.bChanged);
    }

    #[test]
    fn dpad_input_same_action_on_different_inputs() {
        let mut f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");
        f.load_actions(c"actions_dpad_two_inputs.json");

        f.set_interaction_profile(&Touch, LeftHand);
        let input = f.input.clone();
        let data = input.openxr.session_data.get();
        let actions = data.input_data.get_loaded_actions().unwrap();
        let path = f
            .input
            .openxr
            .instance
            .string_to_path(Touch.profile_path())
            .unwrap();
        let bindings = actions.try_get_bindings(boolact, path).unwrap();

        let bindings: Vec<(&DpadData, xr::Path)> = bindings
            .iter()
            .filter_map(|x| match x {
                BindingData {
                    hand,
                    ty: BindingType::Dpad(a),
                    ..
                } => Some((a, *hand)),
                _ => None,
            })
            .collect();

        assert_eq!(bindings.len(), 2);
        let left_binding = bindings
            .iter()
            .find_map(|(data, path)| {
                (*path == f.input.openxr.left_hand.subaction_path).then_some(*data)
            })
            .unwrap();
        let right_binding = bindings
            .iter()
            .find_map(|(data, path)| {
                (*path == f.input.openxr.right_hand.subaction_path).then_some(*data)
            })
            .unwrap();
        assert!(!std::ptr::eq(left_binding, right_binding));

        fakexr::set_action_state(
            left_binding.actions.xy.as_raw(),
            ActionState::Vector2(1.0, 0.0),
            LeftHand,
        );

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s = f.get_bool_state(boolact).unwrap();
        assert!(s.bActive);
        assert!(s.bState);
        assert!(s.bChanged);

        let s = f
            .get_bool_state_hand(boolact, f.input.left_hand_key.data().as_ffi())
            .unwrap();
        assert!(s.bActive);
        assert!(s.bState);
        assert!(s.bChanged);

        let s = f
            .get_bool_state_hand(boolact, f.input.right_hand_key.data().as_ffi())
            .unwrap();
        assert!(!s.bActive);
        assert!(!s.bState);
        assert!(!s.bChanged);

        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s = f.get_bool_state(boolact).unwrap();
        assert!(s.bActive);
        assert!(s.bState);
        assert!(!s.bChanged);

        let s = f
            .get_bool_state_hand(boolact, f.input.left_hand_key.data().as_ffi())
            .unwrap();
        assert!(s.bActive);
        assert!(s.bState);
        assert!(!s.bChanged);
    }

    #[test]
    fn grab_binding() {
        let mut f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact2");
        f.load_actions(c"actions.json");
        get_grab_action!(f, boolact, grab_data);

        f.set_interaction_profile(&Knuckles, LeftHand);
        let mut value_state_check = |force, value, state, changed, line| {
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
        let mut f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");

        let left = f.get_input_source_handle(c"/user/hand/left");
        let right = f.get_input_source_handle(c"/user/hand/right");

        f.load_actions(c"actions_dpad_mixed.json");

        get_grab_action!(f, set1, grab_data);

        f.set_interaction_profile(&Knuckles, LeftHand);
        f.set_interaction_profile(&Knuckles, RightHand);

        let mut value_state_check = |force, value, hand, state, changed, line| {
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
        let mut f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact");
        f.load_actions(c"actions.json");
        get_grab_action!(f, boolact, grab_data);

        f.set_interaction_profile(&Knuckles, RightHand);
        let mut value_state_check = |force, value, state, changed, line| {
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
        let mut f = Fixture::new();
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
        let mut f = Fixture::new();
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

    #[test]
    fn grip_touch_from_pull_oculus() {
        let mut f = Fixture::new();
        let set1 = f.get_action_set_handle(c"/actions/set1");
        let boolact = f.get_action_handle(c"/actions/set1/in/boolact2");
        let left = f.get_input_source_handle(c"/user/hand/left");

        f.load_actions(c"actions.json");
        get_analog_action!(f, boolact, analog_data);

        let act = analog_data.as_raw();
        f.verify_extra_bindings(
            Touch.profile_path(),
            c"/actions/set1/in/boolact2",
            ExtraActionType::Analog,
            ["/user/hand/left/input/squeeze/value".into()],
        );

        f.set_interaction_profile(&Touch, LeftHand);
        fakexr::set_action_state(act, ActionState::Float(0.0), LeftHand);
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s_left = f.get_bool_state_hand(boolact, left).unwrap();
        assert!(s_left.bActive);
        assert!(!s_left.bState);
        assert!(!s_left.bChanged);

        fakexr::set_action_state(act, ActionState::Float(0.01), LeftHand);
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s_left = f.get_bool_state_hand(boolact, left).unwrap();
        assert!(s_left.bActive);
        assert!(s_left.bState);
        assert!(s_left.bChanged);

        fakexr::set_action_state(act, ActionState::Float(0.0), LeftHand);
        f.sync(vr::VRActiveActionSet_t {
            ulActionSet: set1,
            ..Default::default()
        });

        let s_left = f.get_bool_state_hand(boolact, left).unwrap();
        assert!(s_left.bActive);
        assert!(!s_left.bState);
        assert!(s_left.bChanged);
    }

    #[test]
    fn trigger_no_touch_from_pull_oculus() {
        let f = Fixture::new();

        f.load_actions(c"actions.json");
        f.verify_no_extra_bindings(
            Touch.profile_path(),
            c"/actions/set1/in/boolact3",
            ExtraActionType::Analog,
        );
    }
}
