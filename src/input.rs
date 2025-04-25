mod action_manifest;
mod custom_bindings;
mod legacy;
mod profiles;
mod skeletal;

pub mod devices;

#[cfg(test)]
mod tests;

use devices::SubactionPaths;
use devices::TrackedDeviceCreateInfo;
use devices::TrackedDeviceList;
use devices::TrackedDeviceType;
use devices::XrTrackedDevice;
pub use profiles::{InteractionProfile, Profiles};
use skeletal::FingerState;
use skeletal::SkeletalInputActionData;

use crate::openxr_data::HandPath;
use crate::{
    openxr_data::{self, Hand, OpenXrData, SessionData},
    tracy_span, AtomicF32,
};
use custom_bindings::{BindingData, GrabActions};
use legacy::{setup_legacy_bindings, LegacyActionData};
use log::{debug, info, trace, warn};
use openvr as vr;
use openxr as xr;
use slotmap::{new_key_type, Key, KeyData, SecondaryMap, SlotMap};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::{c_char, CStr, CString};
use std::mem::ManuallyDrop;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

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
    legacy_state: legacy::LegacyState,
    skeletal_tracking_level: RwLock<vr::EVRSkeletalTrackingLevel>,
    profile_map: HashMap<xr::Path, &'static profiles::ProfileProperties>,
    estimated_finger_state: [Mutex<FingerState>; 2],
    subaction_paths: SubactionPaths,
    events: Mutex<VecDeque<InputEvent>>,
    devices: RwLock<TrackedDeviceList>,
}

struct InputEvent {
    ty: vr::EVREventType,
    index: vr::TrackedDeviceIndex_t,
    data: vr::VREvent_Controller_t,
}

#[derive(Debug)]
struct Action {
    path: String,
}

struct WriteOnDrop<T> {
    value: ManuallyDrop<T>,
    ptr: *mut T,
}

impl<T: Default> WriteOnDrop<T> {
    fn new(ptr: *mut T) -> Self {
        Self {
            value: Default::default(),
            ptr,
        }
    }
}

impl<T> Drop for WriteOnDrop<T> {
    fn drop(&mut self) {
        unsafe {
            let val = ManuallyDrop::take(&mut self.value);
            self.ptr.write(val);
        }
    }
}

impl<C: openxr_data::Compositor> Input<C> {
    pub fn new(openxr: Arc<OpenXrData<C>>) -> Self {
        let devices = RwLock::new(TrackedDeviceList::new());
        let mut map = SlotMap::with_key();
        let left_hand_key = map.insert(c"/user/hand/left".into());
        let right_hand_key = map.insert(c"/user/hand/right".into());
        let subaction_paths = SubactionPaths::new(&openxr.instance);
        let profile_map = Profiles::get()
            .profiles_iter()
            .map(|profile| {
                (
                    openxr
                        .instance
                        .string_to_path(profile.profile_path())
                        .unwrap(),
                    profile.properties(),
                )
            })
            .collect();

        Self {
            openxr,
            vtables: Default::default(),
            input_source_map: RwLock::new(map),
            action_map: Default::default(),
            set_map: Default::default(),
            devices,
            loaded_actions_path: OnceLock::new(),
            left_hand_key,
            right_hand_key,
            legacy_state: Default::default(),
            skeletal_tracking_level: RwLock::new(vr::EVRSkeletalTrackingLevel::Estimated),
            profile_map,
            estimated_finger_state: [
                Mutex::new(FingerState::new()),
                Mutex::new(FingerState::new()),
            ],
            subaction_paths,
            events: Mutex::default(),
        }
    }

    fn get_subaction_path(&self, hand: Hand) -> xr::Path {
        match hand {
            Hand::Left => self.subaction_paths.left,
            Hand::Right => self.subaction_paths.right,
        }
    }

    fn subaction_path_from_handle(&self, handle: vr::VRInputValueHandle_t) -> Option<xr::Path> {
        if handle == vr::k_ulInvalidInputValueHandle {
            Some(xr::Path::NULL)
        } else {
            match InputSourceKey::from(KeyData::from_ffi(handle)) {
                x if x == self.left_hand_key => Some(self.get_subaction_path(Hand::Left)),
                x if x == self.right_hand_key => Some(self.get_subaction_path(Hand::Right)),
                _ => None,
            }
        }
    }

    fn state_from_bindings_left_right(
        &self,
        action: vr::VRActionHandle_t,
    ) -> Option<(xr::ActionState<bool>, vr::VRInputValueHandle_t)> {
        debug_assert!(self.left_hand_key.0.as_ffi() != 0);
        debug_assert!(self.right_hand_key.0.as_ffi() != 0);
        let left_state = self.state_from_bindings(action, self.left_hand_key.0.as_ffi());

        match left_state {
            None => self.state_from_bindings(action, self.right_hand_key.0.as_ffi()),
            Some((left, _)) => {
                if left.is_active && left.current_state {
                    return left_state;
                }
                let right_state = self.state_from_bindings(action, self.right_hand_key.0.as_ffi());
                match right_state {
                    None => left_state,
                    Some((right, _)) => {
                        if right.is_active && right.current_state {
                            return right_state;
                        }
                        if left.is_active {
                            return left_state;
                        }
                        right_state
                    }
                }
            }
        }
    }

    fn state_from_bindings(
        &self,
        action: vr::VRActionHandle_t,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> Option<(xr::ActionState<bool>, vr::VRInputValueHandle_t)> {
        let subaction = self.subaction_path_from_handle(restrict_to_device)?;
        if subaction == xr::Path::NULL {
            return self.state_from_bindings_left_right(action);
        }

        let session = self.openxr.session_data.get();
        let Ok(loaded_actions) = session.input_data.loaded_actions.get()?.read() else {
            return None;
        };

        let interaction_profile = session
            .session
            .current_interaction_profile(subaction)
            .ok()?;
        let bindings = loaded_actions
            .try_get_bindings(action, interaction_profile)
            .ok()?;
        let extra_data = loaded_actions.try_get_extra(action).ok()?;

        let mut best_state: Option<xr::ActionState<bool>> = None;

        for x in bindings.iter() {
            let Ok(Some(state)) = x.state(&session, extra_data, subaction) else {
                continue;
            };

            if state.is_active
                && (!best_state.is_some_and(|x| x.is_active)
                    || state.current_state && !best_state.is_some_and(|x| x.current_state))
            {
                best_state = Some(state);
                if state.current_state {
                    break;
                }
            }
        }

        best_state.map(|x| (x, restrict_to_device))
    }
}

#[derive(Default)]
pub struct InputSessionData {
    loaded_actions: OnceLock<RwLock<LoadedActions>>,
    legacy_actions: OnceLock<LegacyActionData>,
    estimated_skeleton_actions: OnceLock<SkeletalInputActionData>,
}

impl InputSessionData {
    #[inline]
    fn get_loaded_actions(&self) -> Option<std::sync::RwLockReadGuard<'_, LoadedActions>> {
        self.loaded_actions.get().map(|l| l.read().unwrap())
    }

    pub(crate) fn interaction_profile_changed(&self) {
        if let Some(legacy) = self.legacy_actions.get() {
            // If the interaction profile changes the offsets must be updated too
            // Delete the current raw spaces so they can be recreated later
            legacy.left_spaces.reset_raw();
            legacy.right_spaces.reset_raw();
        }
    }
}
enum ActionData {
    Bool(xr::Action<bool>),
    Vector1 {
        action: xr::Action<f32>,
        last_value: AtomicF32,
    },
    Vector2 {
        action: xr::Action<xr::Vector2f>,
        last_value: (AtomicF32, AtomicF32),
    },
    Pose,
    Skeleton {
        hand: Hand,
        hand_tracker: Option<xr::HandTracker>,
    },
    Haptic(xr::Action<xr::Haptic>),
}

#[derive(Default)]
struct ExtraActionData {
    pub toggle_action: Option<xr::Action<bool>>,
    pub analog_action: Option<xr::Action<f32>>,
    pub vector2_action: Option<xr::Action<xr::Vector2f>>,
    pub grab_action: Option<GrabActions>,
}

#[derive(Debug, Default)]
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

macro_rules! get_action_from_handle {
    ($self:expr, $handle:expr, $session_data:ident, $action:ident) => {
        get_action_from_handle!($self, $handle, $session_data, $action, loaded)
    };

    ($self:expr, $handle:expr, $session_data:ident, $action:ident, $loaded:ident) => {
        let $session_data = $self.openxr.session_data.get();
        let Some($loaded) = $session_data.input_data.get_loaded_actions() else {
            return vr::EVRInputError::InvalidHandle;
        };

        let $action = match $loaded.try_get_action($handle) {
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
        crate::warn_unimplemented!("GetSkeletalSummaryData");
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
        action: vr::VRActionHandle_t,
        level: *mut vr::EVRSkeletalTrackingLevel,
    ) -> vr::EVRInputError {
        get_action_from_handle!(self, action, data, action);
        let ActionData::Skeleton { hand, .. } = action else {
            return vr::EVRInputError::WrongType;
        };

        let controller_type = self.get_controller_string_tracked_property(
            *hand,
            vr::ETrackedDeviceProperty::ControllerType_String,
        );

        unsafe {
            // Make sure knuckles are always Partial
            // TODO: Remove in favor of using XR_EXT_hand_tracking_data_source
            if controller_type == Some(c"knuckles") {
                *level = vr::EVRSkeletalTrackingLevel::Partial;
            } else {
                *level = *self.skeletal_tracking_level.read().unwrap();
            }
        }
        vr::EVRInputError::None
    }
    fn GetSkeletalReferenceTransforms(
        &self,
        handle: vr::VRActionHandle_t,
        space: vr::EVRSkeletalTransformSpace,
        pose: vr::EVRSkeletalReferencePose,
        transform_array: *mut vr::VRBoneTransform_t,
        transform_array_count: u32,
    ) -> vr::EVRInputError {
        // As far as I'm aware this is only/mainly used by HL:A
        // For some reason it is required to position the wrist bone at all times, at least when it comes to Quest controllers

        assert_eq!(
            transform_array_count,
            skeletal::HandSkeletonBone::Count as u32
        );
        let transforms = unsafe {
            std::slice::from_raw_parts_mut(transform_array, transform_array_count as usize)
        };

        get_action_from_handle!(self, handle, session_data, action);
        let ActionData::Skeleton { hand, .. } = action else {
            return vr::EVRInputError::WrongType;
        };

        self.get_reference_transforms(*hand, space, pose, transforms);
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
                    .actions
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
            let action_map = self.action_map.read().unwrap();
            let action_key = ActionKey::from(KeyData::from_ffi(action));
            let input_map = self.input_source_map.read().unwrap();
            let input_key = InputSourceKey::from(KeyData::from_ffi(restrict_to_device));
            trace!(
                "getting pose for {:?} (restrict: {:?})",
                action_map.get(action_key).map(|a| &a.path),
                input_map.get(input_key)
            );
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
        let devices = self.devices.read().unwrap();

        let left_hand = devices.get_controller(Hand::Left);
        let right_hand = devices.get_controller(Hand::Right);

        let (active_origin, hand) = match loaded.try_get_action(action) {
            Ok(ActionData::Pose) => {
                let (mut hand, interaction_profile) = match subaction_path {
                    x if x == self.get_subaction_path(Hand::Left) => {
                        if let Some(left_hand) = left_hand {
                            (Some(Hand::Left), Some(left_hand.get_profile_path()))
                        } else {
                            (None, None)
                        }
                    }
                    x if x == self.get_subaction_path(Hand::Right) => {
                        if let Some(right_hand) = right_hand {
                            (Some(Hand::Right), Some(right_hand.get_profile_path()))
                        } else {
                            (None, None)
                        }
                    }
                    x if x == xr::Path::NULL => (None, None),
                    _ => unreachable!(),
                };

                let get_first_bound_hand_profile = || {
                    if let Some(left_hand) = left_hand {
                        loaded
                            .try_get_pose(action, left_hand.get_profile_path())
                            .ok()
                    } else if let Some(right_hand) = right_hand {
                        loaded
                            .try_get_pose(action, right_hand.get_profile_path())
                            .ok()
                    } else {
                        None
                    }
                };

                let Some(bound) = interaction_profile
                    .and_then(|p| loaded.try_get_pose(action, p).ok())
                    .or_else(get_first_bound_hand_profile)
                else {
                    match hand {
                        Some(hand) => {
                            trace!("action has no bindings for the {hand:?} hand's interaction profile");
                        }
                        None => {
                            trace!("action has no bindings for either hand's interaction profile");
                        }
                    }

                    no_data!()
                };

                let origin = hand.is_some().then_some(restrict_to_device);
                let pose_type = match hand {
                    Some(Hand::Left) => bound.left,
                    Some(Hand::Right) => bound.right,
                    None => {
                        hand = Some(Hand::Left);
                        bound.left.or_else(|| {
                            hand = Some(Hand::Right);
                            bound.right
                        })
                    }
                };

                let Some(ty) = pose_type else {
                    trace!("action has no bindings for the hand {:?}", hand);
                    no_data!()
                };

                let hand = hand.unwrap();
                let origin = origin.unwrap_or_else(|| match hand {
                    Hand::Left => self.left_hand_key.data().as_ffi(),
                    Hand::Right => self.right_hand_key.data().as_ffi(),
                });

                match ty {
                    BoundPoseType::Raw | BoundPoseType::Gdc2015 => (origin, hand),
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
            if let Some(pose) = self.get_controller_pose(hand, Some(origin)) {
                action_data.write(vr::InputPoseActionData_t {
                    bActive: true,
                    activeOrigin: active_origin,
                    pose,
                });
            } else {
                action_data.write(vr::InputPoseActionData_t {
                    bActive: false,
                    ..Default::default()
                });
            }
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

        let mut out = WriteOnDrop::new(action_data);
        get_action_from_handle!(self, handle, session_data, action, loaded);
        let subaction_path = get_subaction_path!(self, restrict_to_device, action_data);

        let mut active_hand = restrict_to_device;
        let (state, delta) = match action {
            ActionData::Vector1 { action, last_value } => {
                let mut state = action.state(&session_data.session, subaction_path).unwrap();

                // It's generally not clear how SteamVR handles float actions with multiple bindings;
                //   so emulate OpenXR, which takes maximum among active actions
                if let Some((binding_state, binding_source)) =
                    self.state_from_bindings(handle, restrict_to_device)
                {
                    if binding_state.is_active
                        && (binding_state.current_state && state.current_state != 1.0
                            || !state.is_active)
                    {
                        state = xr::ActionState {
                            current_state: if binding_state.current_state {
                                1.0
                            } else {
                                0.0
                            },
                            is_active: binding_state.is_active,
                            changed_since_last_sync: binding_state.changed_since_last_sync,
                            last_change_time: binding_state.last_change_time,
                        };
                        active_hand = binding_source;
                    }
                }

                let delta = xr::Vector2f {
                    x: state.current_state - last_value.swap(state.current_state),
                    y: 0.0,
                };
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
                    x: state.current_state.x - last_value.0.swap(state.current_state.x),
                    y: state.current_state.y - last_value.1.swap(state.current_state.y),
                };
                (state, delta)
            }
            _ => return vr::EVRInputError::WrongType,
        };

        *out.value = vr::InputAnalogActionData_t {
            bActive: state.is_active,
            activeOrigin: active_hand,
            x: state.current_state.x,
            deltaX: delta.x,
            y: state.current_state.y,
            deltaY: delta.y,
            ..Default::default()
        };

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

        let mut out = WriteOnDrop::new(action_data);

        get_action_from_handle!(self, handle, session_data, action);
        let subaction_path = get_subaction_path!(self, restrict_to_device, action_data);
        let ActionData::Bool(action) = &action else {
            return vr::EVRInputError::WrongType;
        };

        let mut state = action.state(&session_data.session, subaction_path).unwrap();

        let mut active_hand = restrict_to_device;
        if let Some((binding_state, binding_source)) =
            self.state_from_bindings(handle, restrict_to_device)
        {
            if binding_state.is_active
                && (binding_state.current_state && !state.current_state || !state.is_active)
            {
                state = binding_state;
                active_hand = binding_source;
            }
        }

        *out.value = vr::InputDigitalActionData_t {
            bActive: state.is_active,
            bState: state.current_state,
            activeOrigin: active_hand,
            bChanged: state.changed_since_last_sync,
            fUpdateTime: 0.0, // TODO
        };

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

        let set_map = self.set_map.read().unwrap();
        let mut sync_sets = Vec::with_capacity(active_sets.len() + 1);
        {
            tracy_span!("UpdateActionState generate active sets");
            for set in active_sets {
                let key = ActionSetKey::from(KeyData::from_ffi(set.ulActionSet));
                let name = set_map.get(key);
                let Some(set) = actions.sets.get(key) else {
                    debug!("Application passed invalid action set key: {key:?} ({name:?})");
                    return vr::EVRInputError::InvalidHandle;
                };
                debug!("Activating set {}", name.unwrap());
                sync_sets.push(set.into());
            }

            let legacy = data.input_data.legacy_actions.get().unwrap();
            let skeletal_input = data.input_data.estimated_skeleton_actions.get().unwrap();
            sync_sets.push(xr::ActiveActionSet::new(&legacy.set));
            sync_sets.push(xr::ActiveActionSet::new(&skeletal_input.set));
            self.legacy_state.on_action_sync();
        }

        {
            tracy_span!("xrSyncActions");
            data.session.sync_actions(&sync_sets).unwrap();
        }

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
        if path.is_null() {
            return vr::EVRInputError::InvalidParam;
        }
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
    pub fn interaction_profile_changed(&self) {
        let session = self.openxr.session_data.get();
        let mut devices = self.devices.write().unwrap();

        let mut devices_to_create = vec![];

        for hand in [Hand::Left, Hand::Right] {
            let hmd = devices.get_hmd();
            let controller = devices.get_controller(hand);
            let subaction_path = self.get_subaction_path(hand);

            let profile_path = session
                .session
                .current_interaction_profile(subaction_path)
                .unwrap();

            if let Some(controller) = controller {
                controller.set_profile_path(profile_path);
            }

            let profile_name = match profile_path {
                xr::Path::NULL => {
                    if let Some(controller) = controller {
                        controller.set_connected(false);
                    }
                    "<null>".to_owned()
                }
                path => {
                    if let Some(controller) = controller {
                        controller.set_connected(true);
                    }
                    self.openxr.instance.path_to_string(path).unwrap()
                }
            };

            let profile = Profiles::get().profile_from_name(&profile_name);

            if let Some(p) = profile {
                if let Some(controller) = controller {
                    controller.set_interaction_profile(p);
                } else {
                    devices_to_create.push(TrackedDeviceCreateInfo {
                        device_type: TrackedDeviceType::Controller { hand },
                        profile_path: Some(profile_path),
                        interaction_profile: Some(p),
                    });
                }
                hmd.set_interaction_profile(p);
            };

            session.input_data.interaction_profile_changed();

            info!(
                "{} interaction profile changed: {}",
                HandPath::from(hand),
                profile_name
            )
        }

        for device_info in devices_to_create {
            let device = XrTrackedDevice::new(device_info);
            device.set_connected(true);

            devices.push_device(device).unwrap_or_else(|e| {
                panic!("Failed to create new controller: {:?}", e);
            });
        }
    }

    pub fn frame_start_update(&self) {
        tracy_span!();
        let data = self.openxr.session_data.get();
        let devices = self.devices.read().unwrap();

        devices.iter().for_each(|device| {
            device.clear_pose_cache();
        });

        let left_hand = devices.get_controller(Hand::Left);
        let right_hand = devices.get_controller(Hand::Right);

        if let Some(loaded) = data.input_data.loaded_actions.get() {
            // If the game has loaded actions, we shouldn't need to sync the state because the game
            // should be doing it itself with UpdateActionState. However, some games (Tea for God)
            // don't actually call UpdateActionState if no controllers are reported as connected,
            // and interaction profiles are only updated after xrSyncActions is called. So here, we
            // do an action sync to try and get the runtime to update the interaction profile.
            let loaded = loaded.read().unwrap();
            if (left_hand.is_none() || !left_hand.unwrap().connected())
                && (right_hand.is_none() || !right_hand.unwrap().connected())
            {
                debug!("no controllers connected - syncing info set");
                data.session
                    .sync_actions(&[xr::ActiveActionSet::new(&loaded.info_set)])
                    .unwrap();
            }
            return;
        }

        match data.input_data.legacy_actions.get() {
            Some(actions) => {
                data.session
                    .sync_actions(&[xr::ActiveActionSet::new(&actions.set)])
                    .unwrap();

                self.legacy_state.on_action_sync();
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
                let legacy = LegacyActionData::new(
                    &self.openxr.instance,
                    self.get_subaction_path(Hand::Left),
                    self.get_subaction_path(Hand::Right),
                );
                setup_legacy_bindings(&self.openxr.instance, &data.session, &legacy);
                data.input_data
                    .legacy_actions
                    .set(legacy)
                    .unwrap_or_else(|_| unreachable!());
            }
        }
    }

    pub fn post_session_restart(&self, data: &SessionData) {
        // This function is called while a write lock is called on the session, and as such should
        // not use self.openxr.session_data.get().
        if let Some(path) = self.loaded_actions_path.get() {
            self.load_action_manifest(data, path).unwrap();
        }
    }

    pub fn get_next_event(&self, size: u32, out: *mut vr::VREvent_t) -> bool {
        const FUNC: &str = "get_next_event";
        if out.is_null() {
            warn!("{FUNC}: Got null event pointer.");
            return false;
        }

        let devices = self.devices.read().unwrap();

        for (i, device) in devices.iter().enumerate() {
            let current = device.connected();

            if device.compare_exchange_connected().is_ok() {
                debug!(
                    "sending {:?} {}connected",
                    device.get_type(),
                    if current { "" } else { "not " }
                );

                // VREvent_t can be different sizes depending on the OpenVR version,
                // so we use raw pointers to avoid creating a reference, because if the
                // size doesn't match our VREvent_t's size, we are in UB land
                self.events.lock().unwrap().push_back(InputEvent {
                    ty: if current {
                        vr::EVREventType::TrackedDeviceActivated
                    } else {
                        vr::EVREventType::TrackedDeviceDeactivated
                    },
                    index: i as vr::TrackedDeviceIndex_t,
                    data: Default::default(),
                });
            }
        }

        if let Some(event) = self.events.lock().unwrap().pop_front() {
            const MIN_CONTROLLER_EVENT_SIZE: usize = std::mem::offset_of!(vr::VREvent_t, data)
                + std::mem::size_of::<vr::VREvent_Controller_t>();
            if size < MIN_CONTROLLER_EVENT_SIZE as u32 {
                warn!("{FUNC}: Provided event struct size ({size}) is smaller than required ({MIN_CONTROLLER_EVENT_SIZE}).");
                return false;
            }

            unsafe {
                (&raw mut (*out).eventType).write(event.ty as u32);
                (&raw mut (*out).trackedDeviceIndex).write(event.index);
                (&raw mut (*out).eventAgeSeconds).write(0.0);
                (&raw mut (*out).data.controller).write(event.data);
            }
            true
        } else {
            false
        }
    }
}

struct LoadedActions {
    sets: SecondaryMap<ActionSetKey, xr::ActionSet>,
    actions: SecondaryMap<ActionKey, ActionData>,
    extra_actions: SecondaryMap<ActionKey, ExtraActionData>,
    per_profile_pose_bindings: HashMap<xr::Path, SecondaryMap<ActionKey, BoundPose>>,
    per_profile_bindings: HashMap<xr::Path, SecondaryMap<ActionKey, Vec<BindingData>>>,
    info_set: xr::ActionSet,
    _info_action: xr::Action<bool>,
}

impl LoadedActions {
    fn try_get_bindings(
        &self,
        handle: vr::VRActionHandle_t,
        interaction_profile: xr::Path,
    ) -> Result<&Vec<BindingData>, vr::EVRInputError> {
        let key = ActionKey::from(KeyData::from_ffi(handle));
        self.per_profile_bindings
            .get(&interaction_profile)
            .ok_or(vr::EVRInputError::InvalidHandle)?
            .get(key)
            .ok_or(vr::EVRInputError::InvalidHandle)
    }

    fn try_get_action(
        &self,
        handle: vr::VRActionHandle_t,
    ) -> Result<&ActionData, vr::EVRInputError> {
        let key = ActionKey::from(KeyData::from_ffi(handle));
        self.actions
            .get(key)
            .ok_or(vr::EVRInputError::InvalidHandle)
    }

    fn try_get_extra(
        &self,
        handle: vr::VRActionHandle_t,
    ) -> Result<&ExtraActionData, vr::EVRInputError> {
        let key = ActionKey::from(KeyData::from_ffi(handle));
        self.extra_actions
            .get(key)
            .ok_or(vr::EVRInputError::InvalidHandle)
    }

    fn try_get_pose(
        &self,
        handle: vr::VRActionHandle_t,
        interaction_profile: xr::Path,
    ) -> Result<&BoundPose, vr::EVRInputError> {
        let key = ActionKey::from(KeyData::from_ffi(handle));
        self.per_profile_pose_bindings
            .get(&interaction_profile)
            .ok_or(vr::EVRInputError::InvalidHandle)?
            .get(key)
            .ok_or(vr::EVRInputError::InvalidHandle)
    }
}
