mod action_manifest;
mod knuckles;
mod simple_controller;
mod skeletal;
mod vive_controller;

#[cfg(test)]
mod tests;

use crate::{
    convert::space_relation_to_openvr_pose,
    openxr_data::{self, Hand, OpenXrData, SessionData},
    vr,
};
use action_manifest::InteractionProfile;
use log::{debug, info, trace, warn};
use openxr as xr;
use slotmap::{new_key_type, Key, KeyData, SecondaryMap, SlotMap};
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_4, PI};
use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, OnceLock, RwLock,
};

new_key_type! {
    struct InputSourceKey;
    struct ActionKey;
    struct ActionSetKey;
}

#[derive(macros::InterfaceImpl)]
#[interface = "IVRInput"]
#[versions(010, 007, 006, 005)]
pub struct Input<C: openxr_data::Compositor> {
    openxr: Arc<OpenXrData<C>>,
    vtables: Vtables<C>,
    input_source_map: RwLock<SlotMap<InputSourceKey, CString>>,
    left_hand_key: InputSourceKey,
    right_hand_key: InputSourceKey,
    action_map: RwLock<SlotMap<ActionKey, Action>>,
    set_map: RwLock<SlotMap<ActionSetKey, String>>,
    loaded_actions_path: OnceLock<PathBuf>,
}

impl<C: openxr_data::Compositor> Input<C> {
    pub fn new(openxr: Arc<OpenXrData<C>>) -> Self {
        let mut map = SlotMap::with_key();
        let left_hand_key = map.insert(c"/user/hand/left".into());
        let right_hand_key = map.insert(c"/user/hand/right".into());
        Self {
            openxr,
            vtables: Default::default(),
            input_source_map: RwLock::new(map),
            action_map: Default::default(),
            set_map: Default::default(),
            loaded_actions_path: OnceLock::new(),
            left_hand_key,
            right_hand_key,
        }
    }

    fn subaction_path_from_handle(&self, handle: vr::VRInputValueHandle_t) -> Option<xr::Path> {
        if handle == vr::k_ulInvalidInputValueHandle {
            Some(xr::Path::NULL)
        } else {
            match InputSourceKey::from(KeyData::from_ffi(handle)) {
                x if x == self.left_hand_key => Some(self.openxr.left_hand.subaction_path),
                x if x == self.right_hand_key => Some(self.openxr.right_hand.subaction_path),
                _ => None,
            }
        }
    }
}

#[derive(Default)]
pub struct InputSessionData {
    loaded_actions: OnceLock<RwLock<LoadedActions>>,
    legacy_actions: OnceLock<LegacyActions>,
}

impl InputSessionData {
    #[inline]
    fn get_loaded_actions(&self) -> Option<std::sync::RwLockReadGuard<'_, LoadedActions>> {
        self.loaded_actions.get().map(|l| l.read().unwrap())
    }
}

enum ActionData {
    Bool(BoolActionData),
    Vector1(FloatActionData),
    Vector2 {
        action: xr::Action<xr::Vector2f>,
        last_value: (AtomicF32, AtomicF32),
    },
    Pose {
        /// Maps an interaction profile path to whatever kind of pose was bound for this action for
        /// that profile.
        bindings: HashMap<xr::Path, BoundPose>,
    },
    Skeleton {
        hand: Hand,
        hand_tracker: Option<xr::HandTracker>,
    },
    Haptic(xr::Action<xr::Haptic>),
}

#[derive(Debug)]
struct BoundPose {
    left: Option<BoundPoseType>,
    right: Option<BoundPoseType>,
}

#[derive(Clone, Copy, Debug)]
enum BoundPoseType {
    /// Equivalent to what is returned by WaitGetPoses, this appears to be the same or close to
    /// OpenXR's grip pose in the same position as the aim pose.
    Raw,
    /// Not sure why games still use this, but having it be equivalent to raw seems to work fine.
    Gdc2015,
}

#[derive(Debug)]
enum DpadDirection {
    North,
    East,
    South,
    West,
    Center,
}

struct BoolActionData {
    action: xr::Action<bool>,
    dpad_data: Option<DpadData>,
    grab_data: Option<GrabBindingData>,
}

impl BoolActionData {
    fn new(action: xr::Action<bool>) -> Self {
        Self {
            action,
            dpad_data: None,
            grab_data: None,
        }
    }

    fn state<G>(
        &self,
        session: &xr::Session<G>,
        subaction_path: xr::Path,
    ) -> xr::Result<xr::ActionState<bool>> {
        // First, we try the normal boolean action
        // We may have dpad data, but some controller types may not have been bound to dpad inputs,
        // so we need to try the regular action first.
        let mut state = self.action.state(session, subaction_path)?;

        if state.is_active && state.current_state {
            return Ok(state);
        }

        // state.is_active being false implies there's nothing bound to the action, so then we try
        // our dpad input, if available.

        if let Some(data) = &self.dpad_data {
            if let Some(s) = data.state(session)? {
                state = s;
                if s.current_state {
                    return Ok(s);
                }
            }
        }

        if let Some(data) = &self.grab_data {
            if let Some(state) = data.grabbed(session, subaction_path)? {
                return Ok(state);
            }
        }

        Ok(state)
    }
}

struct FloatActionData {
    action: xr::Action<f32>,
    last_value: AtomicF32,
    grab_data: Option<GrabBindingData>,
}

impl FloatActionData {
    fn new(action: xr::Action<f32>) -> Self {
        Self {
            action,
            last_value: Default::default(),
            grab_data: None,
        }
    }

    fn state<G>(
        &self,
        session: &xr::Session<G>,
        subaction_path: xr::Path,
    ) -> xr::Result<xr::ActionState<f32>> {
        let state = self.action.state(session, subaction_path)?;
        if state.is_active {
            return Ok(state);
        }

        if let Some(data) = &self.grab_data {
            if data.grabbed(session, subaction_path)?.is_some() {
                todo!("handle grab bindings for float actions");
            }
        }

        Ok(state)
    }
}

struct DpadData {
    parent: xr::Action<xr::Vector2f>,
    click_or_touch: Option<xr::Action<bool>>,
    direction: DpadDirection,
    last_state: AtomicBool,
}

impl DpadData {
    const CENTER_ZONE: f32 = 0.5;
    fn state<G>(&self, session: &xr::Session<G>) -> xr::Result<Option<xr::ActionState<bool>>> {
        let parent_state = self.parent.state(session, xr::Path::NULL)?;
        let mut ret_state = xr::ActionState {
            current_state: false,
            last_change_time: parent_state.last_change_time, // TODO: this is wrong
            changed_since_last_sync: false,
            is_active: parent_state.is_active,
        };

        let active = self
            .click_or_touch
            .as_ref()
            .map(|a| {
                // If this action isn't bound in the current interaction profile,
                // is_active will be false - in this case, it's probably a joystick touch dpad, in
                // which case we still want to read the current state.
                a.state(session, xr::Path::NULL)
                    .map(|s| !s.is_active || s.current_state)
            })
            .unwrap_or(Ok(true))?;

        if !active {
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
        }

        Ok(Some(ret_state))
    }
}

struct GrabBindingData {
    force_action: xr::Action<f32>,
    value_action: xr::Action<f32>,
    last_state: [(xr::Path, AtomicBool); 2],
}

impl GrabBindingData {
    fn new(force: xr::Action<f32>, value: xr::Action<f32>, paths: [xr::Path; 2]) -> Self {
        assert!(paths.iter().copied().all(|p| p != xr::Path::NULL));
        Self {
            force_action: force,
            value_action: value,
            last_state: paths.map(|p| (p, false.into())),
        }
    }

    // These values were determined empirically.

    /// How much force to apply to begin a grab
    const GRAB_THRESHOLD: f32 = 0.10;
    /// How much the value component needs to be to release the grab.
    const RELEASE_THRESHOLD: f32 = 0.35;

    /// Returns None if the grab data is not active.
    fn grabbed<G>(
        &self,
        session: &xr::Session<G>,
        subaction_path: xr::Path,
    ) -> xr::Result<Option<xr::ActionState<bool>>> {
        // FIXME: the way this function calculates changed_since_last_sync is incorrect, as it will
        // always be false if this is called more than once between syncs. What should be done is
        // the state should be updated in UpdateActionState, but that may have other implications
        // I currently don't feel like thinking about, as this works and I haven't seen games grab action
        // state more than once beteween syncs.
        let force_state = self.force_action.state(session, subaction_path)?;
        let value_state = self.value_action.state(session, subaction_path)?;
        if !force_state.is_active || !value_state.is_active {
            Ok(None)
        } else {
            let (grabbed, changed_since_last_sync) = match &self.last_state {
                [(path, old_state), _] | [_, (path, old_state)] if *path == subaction_path => {
                    let s = old_state.load(Ordering::Relaxed);
                    let grabbed = (!s && force_state.current_state >= Self::GRAB_THRESHOLD)
                        || (s && value_state.current_state > Self::RELEASE_THRESHOLD);
                    let changed = old_state
                        .compare_exchange(!grabbed, grabbed, Ordering::Relaxed, Ordering::Relaxed)
                        .is_ok();
                    (grabbed, changed)
                }
                [(_, old_state1), (_, old_state2)] if subaction_path == xr::Path::NULL => {
                    let s =
                        old_state1.load(Ordering::Relaxed) || old_state2.load(Ordering::Relaxed);
                    let grabbed = (!s && force_state.current_state >= Self::GRAB_THRESHOLD)
                        || (s && value_state.current_state > Self::RELEASE_THRESHOLD);
                    let cmpex = |state: &AtomicBool| {
                        state
                            .compare_exchange(
                                !grabbed,
                                grabbed,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                    };
                    let changed1 = cmpex(old_state1);
                    let changed2 = cmpex(old_state2);
                    (grabbed, changed1 || changed2)
                }
                _ => unreachable!(),
            };

            Ok(Some(xr::ActionState {
                current_state: grabbed,
                changed_since_last_sync,
                last_change_time: force_state.last_change_time,
                is_active: true,
            }))
        }
    }
}

macro_rules! get_action_from_handle {
    ($self:expr, $handle:expr, $session_data:ident, $action:ident) => {
        let $session_data = $self.openxr.session_data.get();
        let Some(loaded) = $session_data.input_data.get_loaded_actions() else {
            return vr::EVRInputError::InvalidHandle;
        };

        let $action = match loaded.try_get_action($handle) {
            Ok(action) => action,
            Err(e) => return e,
        };
    };
}

macro_rules! get_subaction_path {
    ($self:expr, $restrict:expr, $data:expr) => {
        match $self.subaction_path_from_handle($restrict) {
            Some(p) => p,
            None => {
                unsafe {
                    $data.write(Default::default());
                }
                return vr::EVRInputError::None;
            }
        }
    };
}

#[derive(Debug)]
struct Action {
    path: String,
}

impl<C: openxr_data::Compositor> vr::IVRInput010_Interface for Input<C> {
    fn GetBindingVariant(
        &self,
        _: vr::VRInputValueHandle_t,
        _: *mut c_char,
        _: u32,
    ) -> vr::EVRInputError {
        crate::warn_unimplemented!("GetBindingVariant");
        vr::EVRInputError::None
    }
    fn OpenBindingUI(
        &self,
        _: *const c_char,
        _: vr::VRActionSetHandle_t,
        _: vr::VRInputValueHandle_t,
        _: bool,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn IsUsingLegacyInput(&self) -> bool {
        todo!()
    }
    fn GetComponentStateForBinding(
        &self,
        _: *const c_char,
        _: *const c_char,
        _: *const vr::InputBindingInfo_t,
        _: u32,
        _: u32,
        _: *mut vr::RenderModel_ComponentState_t,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn ShowBindingsForActionSet(
        &self,
        _: *mut vr::VRActiveActionSet_t,
        _: u32,
        _: u32,
        _: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn ShowActionOrigins(
        &self,
        _: vr::VRActionSetHandle_t,
        _: vr::VRActionHandle_t,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn GetActionBindingInfo(
        &self,
        _: vr::VRActionHandle_t,
        _: *mut vr::InputBindingInfo_t,
        _: u32,
        _: u32,
        returned_binding_info_count: *mut u32,
    ) -> vr::EVRInputError {
        crate::warn_unimplemented!("GetActionBindingInfo");
        if !returned_binding_info_count.is_null() {
            unsafe { *returned_binding_info_count = 0 };
        }
        vr::EVRInputError::None
    }
    fn GetOriginTrackedDeviceInfo(
        &self,
        handle: vr::VRInputValueHandle_t,
        info: *mut vr::InputOriginInfo_t,
        info_size: u32,
    ) -> vr::EVRInputError {
        assert_eq!(
            info_size as usize,
            std::mem::size_of::<vr::InputOriginInfo_t>()
        );

        let key = InputSourceKey::from(KeyData::from_ffi(handle));
        let map = self.input_source_map.read().unwrap();
        if !map.contains_key(key) {
            return vr::EVRInputError::InvalidHandle;
        }

        // Superhot needs this device index to render controllers.
        let index = match key {
            x if x == self.left_hand_key => Hand::Left as u32,
            x if x == self.right_hand_key => Hand::Right as u32,
            _ => {
                unsafe {
                    info.write(Default::default());
                }
                return vr::EVRInputError::None;
            }
        };

        unsafe {
            *info.as_mut().unwrap() = vr::InputOriginInfo_t {
                devicePath: handle,
                trackedDeviceIndex: index,
                rchRenderModelComponentName: [0; 128],
            };
        }
        vr::EVRInputError::None
    }
    fn GetOriginLocalizedName(
        &self,
        _: vr::VRInputValueHandle_t,
        _: *mut c_char,
        _: u32,
        _: i32,
    ) -> vr::EVRInputError {
        crate::warn_unimplemented!("GetOriginLocalizedName");
        vr::EVRInputError::None
    }
    fn GetActionOrigins(
        &self,
        _: vr::VRActionSetHandle_t,
        _: vr::VRActionHandle_t,
        _: *mut vr::VRInputValueHandle_t,
        _: u32,
    ) -> vr::EVRInputError {
        crate::warn_unimplemented!("GetActionOrigins");
        vr::EVRInputError::None
    }
    fn TriggerHapticVibrationAction(
        &self,
        action: vr::VRActionHandle_t,
        start_seconds_from_now: f32,
        duration_seconds: f32,
        frequency: f32,
        amplitude: f32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        get_action_from_handle!(self, action, session_data, action);
        let Some(subaction_path) = self.subaction_path_from_handle(restrict_to_device) else {
            return vr::EVRInputError::None;
        };

        let ActionData::Haptic(action) = action else {
            return vr::EVRInputError::WrongType;
        };

        if start_seconds_from_now > 0.0 {
            warn!("start_seconds_from_now: {start_seconds_from_now}")
        }

        action
            .apply_feedback(
                &session_data.session,
                subaction_path,
                &xr::HapticVibration::new()
                    .amplitude(amplitude.clamp(0.0, 1.0))
                    .frequency(frequency)
                    .duration(xr::Duration::from_nanos((duration_seconds * 1e9) as _)),
            )
            .unwrap();

        vr::EVRInputError::None
    }
    fn DecompressSkeletalBoneData(
        &self,
        _: *const std::os::raw::c_void,
        _: u32,
        _: vr::EVRSkeletalTransformSpace,
        _: *mut vr::VRBoneTransform_t,
        _: u32,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn GetSkeletalBoneDataCompressed(
        &self,
        _: vr::VRActionHandle_t,
        _: vr::EVRSkeletalMotionRange,
        _: *mut std::os::raw::c_void,
        _: u32,
        _: *mut u32,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn GetSkeletalSummaryData(
        &self,
        action: vr::VRActionHandle_t,
        _: vr::EVRSummaryType,
        data: *mut vr::VRSkeletalSummaryData_t,
    ) -> vr::EVRInputError {
        get_action_from_handle!(self, action, session_data, _action);
        unsafe {
            data.write(vr::VRSkeletalSummaryData_t {
                flFingerSplay: [0.2; 4],
                flFingerCurl: [0.0; 5],
            })
        }
        vr::EVRInputError::None
    }
    fn GetSkeletalBoneData(
        &self,
        handle: vr::VRActionHandle_t,
        transform_space: vr::EVRSkeletalTransformSpace,
        _motion_range: vr::EVRSkeletalMotionRange,
        transform_array: *mut vr::VRBoneTransform_t,
        transform_array_count: u32,
    ) -> vr::EVRInputError {
        assert_eq!(
            transform_array_count,
            skeletal::HandSkeletonBone::Count as u32
        );
        let transforms = unsafe {
            std::slice::from_raw_parts_mut(transform_array, transform_array_count as usize)
        };

        get_action_from_handle!(self, handle, session_data, action);
        let ActionData::Skeleton { hand, hand_tracker } = action else {
            return vr::EVRInputError::WrongType;
        };
        if let Some(hand_tracker) = hand_tracker.as_ref() {
            self.get_bones_from_hand_tracking(
                &session_data,
                transform_space,
                hand_tracker,
                *hand,
                transforms,
            )
        } else {
            self.get_estimated_bones(&session_data, transform_space, *hand, transforms);
        }

        vr::EVRInputError::None
    }
    fn GetSkeletalTrackingLevel(
        &self,
        _: vr::VRActionHandle_t,
        level: *mut vr::EVRSkeletalTrackingLevel,
    ) -> vr::EVRInputError {
        unsafe {
            *level = vr::EVRSkeletalTrackingLevel::Partial;
        }
        vr::EVRInputError::None
    }
    fn GetSkeletalReferenceTransforms(
        &self,
        _: vr::VRActionHandle_t,
        _: vr::EVRSkeletalTransformSpace,
        _: vr::EVRSkeletalReferencePose,
        _: *mut vr::VRBoneTransform_t,
        _: u32,
    ) -> vr::EVRInputError {
        crate::warn_unimplemented!("GetSkeletalReferenceTransforms");
        vr::EVRInputError::None
    }
    fn GetBoneName(
        &self,
        _: vr::VRActionHandle_t,
        _: vr::BoneIndex_t,
        _: *mut c_char,
        _: u32,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn GetBoneHierarchy(
        &self,
        _: vr::VRActionHandle_t,
        _: *mut vr::BoneIndex_t,
        _: u32,
    ) -> vr::EVRInputError {
        todo!()
    }
    fn GetBoneCount(&self, handle: vr::VRActionHandle_t, count: *mut u32) -> vr::EVRInputError {
        get_action_from_handle!(self, handle, session_data, action);
        if !matches!(action, ActionData::Skeleton { .. }) {
            return vr::EVRInputError::WrongType;
        }

        let Some(count) = (unsafe { count.as_mut() }) else {
            return vr::EVRInputError::InvalidParam;
        };
        *count = skeletal::HandSkeletonBone::Count as u32;

        vr::EVRInputError::None
    }
    fn SetDominantHand(&self, _: vr::ETrackedControllerRole) -> vr::EVRInputError {
        todo!()
    }
    fn GetDominantHand(&self, _: *mut vr::ETrackedControllerRole) -> vr::EVRInputError {
        crate::warn_unimplemented!("GetDominantHand");
        vr::EVRInputError::None
    }
    fn GetSkeletalActionData(
        &self,
        action: vr::VRActionHandle_t,
        action_data: *mut vr::InputSkeletalActionData_t,
        _action_data_size: u32,
    ) -> vr::EVRInputError {
        //assert_eq!(
        //    action_data_size as usize,
        //    std::mem::size_of::<vr::InputSkeletalActionData_t>()
        //);

        let data = self.openxr.session_data.get();
        let Some(loaded) = data.input_data.get_loaded_actions() else {
            return vr::EVRInputError::InvalidHandle;
        };
        let origin = match loaded.try_get_action(action) {
            Ok(ActionData::Skeleton { hand, .. }) => match hand {
                Hand::Left => self.left_hand_key.data().as_ffi(),
                Hand::Right => self.right_hand_key.data().as_ffi(),
            },
            Ok(_) => return vr::EVRInputError::WrongType,
            Err(e) => return e,
        };
        let legacy = data.input_data.legacy_actions.get().unwrap();
        unsafe {
            std::ptr::addr_of_mut!((*action_data).bActive).write(
                legacy
                    .grip_pose
                    .is_active(&data.session, xr::Path::NULL)
                    .unwrap(),
            );
            std::ptr::addr_of_mut!((*action_data).activeOrigin).write(origin);
        }
        vr::EVRInputError::None
    }
    fn GetPoseActionDataForNextFrame(
        &self,
        action: vr::VRActionHandle_t,
        origin: vr::ETrackingUniverseOrigin,
        action_data: *mut vr::InputPoseActionData_t,
        action_data_size: u32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        assert_eq!(
            action_data_size as usize,
            std::mem::size_of::<vr::InputPoseActionData_t>()
        );

        if log::log_enabled!(log::Level::Trace) {
            let map = self.action_map.read().unwrap();
            let key = ActionKey::from(KeyData::from_ffi(action));
            trace!("getting pose for {}", map[key].path);
        }

        let data = self.openxr.session_data.get();
        let Some(loaded) = data.input_data.get_loaded_actions() else {
            return vr::EVRInputError::InvalidHandle;
        };

        macro_rules! no_data {
            () => {{
                unsafe {
                    action_data.write(Default::default());
                }
                return vr::EVRInputError::None;
            }};
        }
        let subaction_path = get_subaction_path!(self, restrict_to_device, action_data);
        let (active_origin, hand) = match loaded.try_get_action(action) {
            Ok(ActionData::Pose { bindings }) => {
                let (hand, hand_info, active_origin) = match subaction_path {
                    x if x == self.openxr.left_hand.subaction_path => (
                        Hand::Left,
                        &self.openxr.left_hand,
                        self.left_hand_key.data().as_ffi(),
                    ),
                    x if x == self.openxr.right_hand.subaction_path => (
                        Hand::Right,
                        &self.openxr.right_hand,
                        self.right_hand_key.data().as_ffi(),
                    ),
                    _ => unreachable!(),
                };
                let Some(bound) = bindings.get(&hand_info.interaction_profile.load()) else {
                    trace!(
                        "action has no bindings for the interaction profile {:?}",
                        hand_info.interaction_profile.load()
                    );
                    no_data!()
                };

                let pose_type = match hand {
                    Hand::Left => bound.left,
                    Hand::Right => bound.right,
                };
                let Some(ty) = pose_type else {
                    trace!("action has no bindings for the hand {hand:?}");
                    no_data!()
                };

                match ty {
                    BoundPoseType::Raw | BoundPoseType::Gdc2015 => (active_origin, hand),
                }
            }
            Ok(ActionData::Skeleton { hand, .. }) => {
                if subaction_path != xr::Path::NULL {
                    return vr::EVRInputError::InvalidDevice;
                }
                (0, *hand)
            }
            Ok(_) => return vr::EVRInputError::WrongType,
            Err(e) => return e,
        };

        drop(loaded);
        drop(data);
        unsafe {
            action_data.write(vr::InputPoseActionData_t {
                bActive: true,
                activeOrigin: active_origin,
                pose: self.get_controller_pose(hand, Some(origin)).expect("wtf"),
            })
        }

        vr::EVRInputError::None
    }

    fn GetPoseActionDataRelativeToNow(
        &self,
        action: vr::VRActionHandle_t,
        origin: vr::ETrackingUniverseOrigin,
        _seconds_from_now: f32,
        action_data: *mut vr::InputPoseActionData_t,
        action_data_size: u32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        self.GetPoseActionDataForNextFrame(
            action,
            origin,
            action_data,
            action_data_size,
            restrict_to_device,
        )
    }

    fn GetAnalogActionData(
        &self,
        handle: vr::VRActionHandle_t,
        action_data: *mut vr::InputAnalogActionData_t,
        action_data_size: u32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        assert_eq!(
            action_data_size as usize,
            std::mem::size_of::<vr::InputAnalogActionData_t>()
        );

        get_action_from_handle!(self, handle, session_data, action);
        let subaction_path = get_subaction_path!(self, restrict_to_device, action_data);

        let (state, delta) = match action {
            ActionData::Vector1(data) => {
                let state = data.state(&session_data.session, subaction_path).unwrap();
                let delta = xr::Vector2f {
                    x: state.current_state - data.last_value.load(),
                    y: 0.0,
                };
                data.last_value.store(state.current_state);
                (
                    xr::ActionState::<xr::Vector2f> {
                        current_state: xr::Vector2f {
                            x: state.current_state,
                            y: 0.0,
                        },
                        changed_since_last_sync: state.changed_since_last_sync,
                        last_change_time: state.last_change_time,
                        is_active: state.is_active,
                    },
                    delta,
                )
            }
            ActionData::Vector2 { action, last_value } => {
                let state = action.state(&session_data.session, subaction_path).unwrap();
                let delta = xr::Vector2f {
                    x: state.current_state.x - last_value.0.load(),
                    y: state.current_state.y - last_value.1.load(),
                };
                last_value.0.store(state.current_state.x);
                last_value.1.store(state.current_state.y);
                (state, delta)
            }
            _ => return vr::EVRInputError::WrongType,
        };
        unsafe {
            action_data.write(vr::InputAnalogActionData_t {
                bActive: state.is_active,
                activeOrigin: 0,
                x: state.current_state.x,
                deltaX: delta.x,
                y: state.current_state.y,
                deltaY: delta.y,
                ..Default::default()
            });
        }

        vr::EVRInputError::None
    }

    fn GetDigitalActionData(
        &self,
        handle: vr::VRActionHandle_t,
        action_data: *mut vr::InputDigitalActionData_t,
        action_data_size: u32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        assert_eq!(
            action_data_size as usize,
            std::mem::size_of::<vr::InputDigitalActionData_t>()
        );

        let data = self.openxr.session_data.get();
        let Some(loaded) = data.input_data.get_loaded_actions() else {
            return vr::EVRInputError::InvalidHandle;
        };

        let subaction_path = get_subaction_path!(self, restrict_to_device, action_data);
        let action = match loaded.try_get_action(handle) {
            Ok(action) => {
                let ActionData::Bool(action) = &action else {
                    return vr::EVRInputError::WrongType;
                };
                action
            }
            Err(e) => return e,
        };

        let state = action.state(&data.session, subaction_path).unwrap();
        unsafe {
            action_data.write(vr::InputDigitalActionData_t {
                bActive: state.is_active,
                bState: state.current_state,
                activeOrigin: restrict_to_device, // TODO
                bChanged: state.changed_since_last_sync,
                fUpdateTime: 0.0, // TODO
            });
        }

        vr::EVRInputError::None
    }

    fn UpdateActionState(
        &self,
        active_sets: *mut vr::VRActiveActionSet_t,
        active_set_size: u32,
        active_set_count: u32,
    ) -> vr::EVRInputError {
        assert_eq!(
            active_set_size as usize,
            std::mem::size_of::<vr::VRActiveActionSet_t>()
        );
        // alyx
        if active_set_count == 0 {
            return vr::EVRInputError::NoActiveActionSet;
        }

        let active_sets =
            unsafe { std::slice::from_raw_parts(active_sets, active_set_count as usize) };

        if active_sets
            .iter()
            .any(|set| set.ulRestrictedToDevice != vr::k_ulInvalidInputValueHandle)
        {
            crate::warn_once!("Per device action set restriction is not implemented yet.");
        }

        let data = self.openxr.session_data.get();
        let Some(actions) = data.input_data.get_loaded_actions() else {
            return vr::EVRInputError::InvalidParam;
        };

        let mut sync_sets = Vec::with_capacity(active_sets.len() + 1);
        for set in active_sets {
            let key = ActionSetKey::from(KeyData::from_ffi(set.ulActionSet));
            let m = self.set_map.read().unwrap();
            let name = m.get(key);
            let Some(set) = actions.sets.get(key) else {
                debug!("Application passed invalid action set key: {key:?} ({name:?})");
                return vr::EVRInputError::InvalidHandle;
            };
            debug!("Activating set {}", name.unwrap());
            sync_sets.push(set.into());
        }

        let legacy = data.input_data.legacy_actions.get().unwrap();
        sync_sets.push(xr::ActiveActionSet::new(&legacy.set));
        legacy.packet_num.fetch_add(1, Ordering::Relaxed);

        data.session.sync_actions(&sync_sets).unwrap();

        vr::EVRInputError::None
    }

    fn GetInputSourceHandle(
        &self,
        input_source_path: *const c_char,
        handle: *mut vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        let path = unsafe { CStr::from_ptr(input_source_path) };

        let ret = {
            let guard = self.input_source_map.read().unwrap();
            match guard.iter().find(|(_, src)| src.as_c_str() == path) {
                Some((key, _)) => key.data().as_ffi(),
                None => {
                    drop(guard);
                    let mut guard = self.input_source_map.write().unwrap();
                    let key = guard.insert(path.into());
                    key.data().as_ffi()
                }
            }
        };
        if let Some(handle) = unsafe { handle.as_mut() } {
            debug!("requested handle for path {path:?}: {ret}");
            *handle = ret;
            vr::EVRInputError::None
        } else {
            vr::EVRInputError::InvalidParam
        }
    }

    fn GetActionHandle(
        &self,
        action_name: *const c_char,
        handle: *mut vr::VRActionHandle_t,
    ) -> vr::EVRInputError {
        let name = unsafe { CStr::from_ptr(action_name) }
            .to_string_lossy()
            .to_lowercase();
        let guard = self.action_map.read().unwrap();
        let val = match guard.iter().find(|(_, action)| action.path == name) {
            Some((key, _)) => key.data().as_ffi(),
            None => {
                drop(guard);
                let mut guard = self.action_map.write().unwrap();
                let key = guard.insert(Action { path: name });
                key.data().as_ffi()
            }
        };

        if let Some(handle) = unsafe { handle.as_mut() } {
            *handle = val;
            vr::EVRInputError::None
        } else {
            vr::EVRInputError::InvalidParam
        }
    }

    fn GetActionSetHandle(
        &self,
        action_set_name: *const c_char,
        handle: *mut vr::VRActionSetHandle_t,
    ) -> vr::EVRInputError {
        let name = unsafe { CStr::from_ptr(action_set_name) }
            .to_string_lossy()
            .to_lowercase();
        let guard = self.set_map.read().unwrap();
        let val = match guard.iter().find(|(_, set)| **set == name) {
            Some((key, _)) => key.data().as_ffi(),
            None => {
                drop(guard);
                let mut guard = self.set_map.write().unwrap();
                let key = guard.insert(name);
                key.data().as_ffi()
            }
        };

        if let Some(handle) = unsafe { handle.as_mut() } {
            *handle = val;
            vr::EVRInputError::None
        } else {
            vr::EVRInputError::InvalidParam
        }
    }

    fn SetActionManifestPath(&self, path: *const c_char) -> vr::EVRInputError {
        let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
        let path = std::path::Path::new(&*path);
        info!("loading action manifest from {path:?}");

        // We need to restart the session if the legacy actions have already been attached.
        let mut data = self.openxr.session_data.get();
        if data.input_data.legacy_actions.get().is_some() {
            drop(data);
            self.openxr.restart_session();
            data = self.openxr.session_data.get();
        }
        match self.load_action_manifest(&data, path) {
            Ok(_) => vr::EVRInputError::None,
            Err(e) => e,
        }
    }
}

impl<C: openxr_data::Compositor> vr::IVRInput005On006 for Input<C> {
    #[inline]
    fn GetSkeletalSummaryData(
        &self,
        action: vr::VRActionHandle_t,
        summary_data: *mut vr::VRSkeletalSummaryData_t,
    ) -> vr::EVRInputError {
        <Self as vr::IVRInput010_Interface>::GetSkeletalSummaryData(
            self,
            action,
            vr::EVRSummaryType::FromAnimation,
            summary_data,
        )
    }

    #[inline]
    fn GetPoseActionData(
        &self,
        action: vr::VRActionHandle_t,
        origin: vr::ETrackingUniverseOrigin,
        seconds_from_now: f32,
        action_data: *mut vr::InputPoseActionData_t,
        action_data_size: u32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        <Self as vr::IVRInput010_Interface>::GetPoseActionDataRelativeToNow(
            self,
            action,
            origin,
            seconds_from_now,
            action_data,
            action_data_size,
            restrict_to_device,
        )
    }
}

impl<C: openxr_data::Compositor> Input<C> {
    pub fn get_poses(
        &self,
        poses: &mut [vr::TrackedDevicePose_t],
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) {
        poses[0] = self.get_hmd_pose(origin);

        if poses.len() > Hand::Left as usize {
            poses[Hand::Left as usize] = self
                .get_controller_pose(Hand::Left, origin)
                .unwrap_or_default();
        }
        if poses.len() > Hand::Right as usize {
            poses[Hand::Right as usize] = self
                .get_controller_pose(Hand::Right, origin)
                .unwrap_or_default();
        }
    }

    fn get_hmd_pose(&self, origin: Option<vr::ETrackingUniverseOrigin>) -> vr::TrackedDevicePose_t {
        let data = self.openxr.session_data.get();
        let (hmd_location, hmd_velocity) = {
            data.view_space
                .relate(
                    data.get_space_for_origin(origin.unwrap_or(data.current_origin)),
                    self.openxr.display_time.get(),
                )
                .unwrap()
        };

        space_relation_to_openvr_pose(hmd_location, hmd_velocity)
    }

    /// Returns None if legacy actions haven't been set up yet.
    pub fn get_controller_pose(
        &self,
        hand: Hand,
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) -> Option<vr::TrackedDevicePose_t> {
        let data = self.openxr.session_data.get();
        let actions = data.input_data.legacy_actions.get()?;
        let spaces = match hand {
            Hand::Left => &actions.left_spaces,
            Hand::Right => &actions.right_spaces,
        };

        let (loc, velo) = if let Some(raw) =
            spaces.try_get_or_init_raw(&data, actions, self.openxr.display_time.get())
        {
            raw.relate(
                data.get_space_for_origin(origin.unwrap_or(data.current_origin)),
                self.openxr.display_time.get(),
            )
            .unwrap()
        } else {
            trace!("failed to get raw space, making empty pose");
            (xr::SpaceLocation::default(), xr::SpaceVelocity::default())
        };

        Some(space_relation_to_openvr_pose(loc, velo))
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

        let data = self.openxr.session_data.get();
        let Some(actions) = data.input_data.legacy_actions.get() else {
            debug!("tried getting controller state, but legacy actions aren't ready");
            return false;
        };

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

        state.unPacketNum = actions.packet_num.load(Ordering::Relaxed);

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

    pub fn frame_start_update(&self) {
        let data = self.openxr.session_data.get();
        // If the game has loaded actions, we don't need to sync the state because the game should
        // be doing it itself (with UpdateActionState)
        if data.input_data.loaded_actions.get().is_some() {
            return;
        }

        match data.input_data.legacy_actions.get() {
            Some(actions) => {
                data.session
                    .sync_actions(&[xr::ActiveActionSet::new(&actions.set)])
                    .unwrap();

                actions.packet_num.fetch_add(1, Ordering::Relaxed);
            }
            None => {
                // If we haven't created our legacy actions yet but we're getting our per frame
                // update, go ahead and create them
                // This will force us to have to restart the session when we get an action
                // manifest, but that's fine, because in the event an action manifest is never
                // loaded (legacy input), having the legacy actions loaded and synced enables us to
                // determine if controllers are actually connected, and some games avoid getting
                // controller state unless they are reported as actually connected.

                // Make sure we're using the real session already
                // This avoids a scenario where we could go:
                // 1. attach legacy inputs
                // 2. restart session to attach action manifest
                // 3. restart to use real session
                if !data.is_real_session() {
                    debug!(
                        "Couldn't set up legacy actions because we're not in the real session yet."
                    );
                    return;
                }
                let legacy = LegacyActions::new(
                    &self.openxr.instance,
                    &data.session,
                    self.openxr.left_hand.subaction_path,
                    self.openxr.right_hand.subaction_path,
                );
                setup_legacy_bindings(&self.openxr.instance, &data.session, &legacy);
                data.input_data
                    .legacy_actions
                    .set(legacy)
                    .unwrap_or_else(|_| unreachable!());
            }
        }
    }

    pub fn get_controller_string_tracked_property(
        &self,
        hand: Hand,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<&'static CStr> {
        struct ProfileData {
            controller_type: &'static CStr,
            model_number: &'static CStr,
        }
        static PROFILE_MAP: OnceLock<HashMap<xr::Path, ProfileData>> = OnceLock::new();
        let get_profile_data = || {
            let map = PROFILE_MAP.get_or_init(|| {
                let instance = &self.openxr.instance;
                let mut map = HashMap::new();
                let out = &mut map;
                action_manifest::for_each_profile! {<'a>(
                    instance: &'a xr::Instance,
                    out: &'a mut HashMap<xr::Path, ProfileData>
                ) {
                    out.insert(
                        instance.string_to_path(P::PROFILE_PATH).unwrap(),
                        ProfileData {
                            controller_type: P::OPENVR_CONTROLLER_TYPE,
                            model_number: P::MODEL,
                        }
                    );
                }}
                map
            });
            let hand = match hand {
                Hand::Left => &self.openxr.left_hand,
                Hand::Right => &self.openxr.right_hand,
            };
            let profile = hand.interaction_profile.load();
            map.get(&profile)
        };

        match property {
            // Audica likes to apply controller specific tweaks via this property
            vr::ETrackedDeviceProperty::ControllerType_String => {
                get_profile_data().map(|data| data.controller_type)
            }
            // I Expect You To Die 3 identifies controllers with this property -
            // why it couldn't just use ControllerType instead is beyond me...
            vr::ETrackedDeviceProperty::ModelNumber_String => {
                get_profile_data().map(|data| data.model_number)
            }
            // Required for controllers to be acknowledged in I Expect You To Die 3
            vr::ETrackedDeviceProperty::SerialNumber_String
            | vr::ETrackedDeviceProperty::ManufacturerName_String => Some(c"<unknown>"),
            _ => None,
        }
    }

    pub fn post_session_restart(&self, data: &SessionData) {
        // This function is called while a write lock is called on the session, and as such should
        // not use self.openxr.session_data.get().
        if let Some(path) = self.loaded_actions_path.get() {
            self.load_action_manifest(data, path).unwrap();
        }
    }
}

fn setup_legacy_bindings(
    instance: &xr::Instance,
    session: &xr::Session<xr::vulkan::Vulkan>,
    actions: &LegacyActions,
) {
    debug!("setting up legacy bindings");

    action_manifest::for_each_profile! {<'a>(
        instance: &'a xr::Instance,
        actions: &'a LegacyActions
    ) {
        const fn constrain<F>(f: F) -> F
            where F: for<'a> Fn(&'a str) -> xr::Path
        {
            f
        }
        let stp = constrain(|s| instance.string_to_path(s).unwrap());
        let bindings = P::legacy_bindings(stp, actions);
        let profile = stp(P::PROFILE_PATH);
        instance
            .suggest_interaction_profile_bindings(profile, &bindings)
            .unwrap();
    }}

    session.attach_action_sets(&[&actions.set]).unwrap();
    session
        .sync_actions(&[xr::ActiveActionSet::new(&actions.set)])
        .unwrap();
}

struct LoadedActions {
    sets: SecondaryMap<ActionSetKey, xr::ActionSet>,
    actions: SecondaryMap<ActionKey, ActionData>,
}

impl LoadedActions {
    fn try_get_action(
        &self,
        handle: vr::VRActionHandle_t,
    ) -> Result<&ActionData, vr::EVRInputError> {
        let key = ActionKey::from(KeyData::from_ffi(handle));
        self.actions
            .get(key)
            .ok_or(vr::EVRInputError::InvalidHandle)
    }
}

struct HandSpaces {
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
    fn try_get_or_init_raw(
        &self,
        data: &SessionData,
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

        self.raw
            .set(
                actions
                    .grip_pose
                    .create_space(
                        &data.session,
                        self.hand_path,
                        xr::Posef {
                            orientation: xr::Quaternionf::IDENTITY,
                            position: aim_loc.pose.position,
                        },
                    )
                    .unwrap(),
            )
            .unwrap_or_else(|_| unreachable!());

        self.raw.get()
    }
}

struct LegacyActions {
    set: xr::ActionSet,
    grip_pose: xr::Action<xr::Posef>,
    aim_pose: xr::Action<xr::Posef>,
    app_menu: xr::Action<bool>,
    trigger_click: xr::Action<bool>,
    trigger: xr::Action<f32>,
    squeeze: xr::Action<f32>,
    packet_num: AtomicU32,
    left_spaces: HandSpaces,
    right_spaces: HandSpaces,
}

impl LegacyActions {
    fn new<'a>(
        instance: &'a xr::Instance,
        session: &'a xr::Session<xr::vulkan::Vulkan>,
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

        let create_spaces = |hand| HandSpaces {
            hand_path: hand,
            grip: grip_pose
                .create_space(session, hand, xr::Posef::IDENTITY)
                .unwrap(),
            aim: aim_pose
                .create_space(session, hand, xr::Posef::IDENTITY)
                .unwrap(),
            raw: OnceLock::new(),
        };

        let left_spaces = create_spaces(left_hand);
        let right_spaces = create_spaces(right_hand);

        Self {
            set,
            grip_pose,
            aim_pose,
            app_menu,
            trigger_click,
            trigger,
            squeeze,
            packet_num: 0.into(),
            left_spaces,
            right_spaces,
        }
    }
}

#[derive(Default)]
struct AtomicF32(AtomicU32);
impl AtomicF32 {
    fn new(value: f32) -> Self {
        Self(value.to_bits().into())
    }

    fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn store(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed)
    }
}

impl From<f32> for AtomicF32 {
    fn from(value: f32) -> Self {
        Self::new(value)
    }
}
