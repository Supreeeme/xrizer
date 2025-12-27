mod action_manifest;
mod custom_bindings;
mod devices;
mod legacy;
mod profiles;
mod skeletal;

#[cfg(test)]
mod tests;

pub use devices::TrackedDeviceType;
pub use profiles::{InteractionProfile, Profiles};

use devices::{ControllerData, SubactionPaths, TrackedDevice, TrackedDeviceList};
use skeletal::FingerState;
use skeletal::HandSkeletonBone;
use skeletal::SkeletalInputActionData;

use crate::{
    AtomicF32,
    openxr_data::{self, Hand, OpenXrData, SessionData},
    tracy_span,
};
use custom_bindings::{BindingData, GrabActions};
use glam::Quat;
use legacy::LegacyActionData;
use log::{debug, info, trace, warn};
use openvr as vr;
use openxr as xr;
use slotmap::{Key, KeyData, SecondaryMap, SlotMap, new_key_type};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{CStr, CString, c_char, c_void};
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard};

new_key_type! {
    struct InputSourceKey;
    struct ActionKey;
    struct ActionSetKey;
}

#[derive(macros::InterfaceImpl)]
#[interface = "IVRInput"]
#[versions(010, 007, 006, 005, 004)]
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
    skeletal_bone_cache:
        [Mutex<Option<[vr::VRBoneTransform_t; HandSkeletonBone::Count as usize]>>; 2],
    estimated_finger_state: [Mutex<FingerState>; 2],
    subaction_paths: SubactionPaths,
    events: Mutex<VecDeque<InputEvent>>,
    loading_actions: AtomicBool,
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
        let mut map = SlotMap::with_key();
        let left_hand_key = map.insert(c"/user/hand/left".into());
        let right_hand_key = map.insert(c"/user/hand/right".into());
        let subaction_paths = SubactionPaths::new(&openxr.instance);
        let pose_data = PoseData::new(
            &openxr.instance,
            subaction_paths.left,
            subaction_paths.right,
        );
        openxr
            .session_data
            .get()
            .input_data
            .pose_data
            .set(pose_data)
            .unwrap_or_else(|_| panic!("PoseData already setup"));

        Self {
            openxr,
            vtables: Default::default(),
            input_source_map: RwLock::new(map),
            action_map: Default::default(),
            set_map: Default::default(),
            loaded_actions_path: OnceLock::new(),
            left_hand_key,
            right_hand_key,
            legacy_state: Default::default(),
            skeletal_tracking_level: RwLock::new(vr::EVRSkeletalTrackingLevel::Estimated),
            skeletal_bone_cache: Default::default(),
            estimated_finger_state: [
                Mutex::new(FingerState::new()),
                Mutex::new(FingerState::new()),
            ],
            subaction_paths,
            events: Mutex::default(),
            loading_actions: false.into(),
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
        let LoadedActions::Manifest(loaded_actions) = session.input_data.actions.get()? else {
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

pub struct InputSessionData {
    actions: OnceLock<LoadedActions>,
    estimated_skeleton_actions: OnceLock<SkeletalInputActionData>,
    pose_data: OnceLock<PoseData>,
    devices: RwLock<TrackedDeviceList>,
}

impl InputSessionData {
    pub(crate) fn new(session: &xr::Session<xr::AnyGraphics>) -> Self {
        Self {
            actions: OnceLock::new(),
            estimated_skeleton_actions: OnceLock::new(),
            pose_data: OnceLock::new(),
            devices: RwLock::new(TrackedDeviceList::new(session)),
        }
    }

    #[inline]
    fn get_loaded_actions(&self) -> Option<&ManifestLoadedActions> {
        match self.actions.get()? {
            LoadedActions::Manifest(m) => Some(m),
            _ => None,
        }
    }
    #[inline]
    fn get_legacy_actions(&self) -> Option<&LegacyActionData> {
        match self.actions.get()? {
            LoadedActions::Legacy(l) => Some(l),
            _ => None,
        }
    }

    pub(crate) fn interaction_profile_changed(&self) {
        if let Some(data) = self.pose_data.get() {
            // If the interaction profile changes the offsets must be updated too
            // Delete the current raw spaces so they can be recreated later
            data.reset_spaces();
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
    Skeleton(Hand),
    Haptic(xr::Action<xr::Haptic>),
}

#[derive(Default)]
struct ExtraActionData {
    toggle_action: Option<xr::Action<bool>>,
    analog_action: Option<xr::Action<f32>>,
    double_action: Option<xr::Action<bool>>,
    vector2_action: Option<xr::Action<xr::Vector2f>>,
    grab_actions: Option<GrabActions<custom_bindings::Actions>>,
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
    /// "All tracked devices also get two pose components registered regardless of what render model they use: /pose/raw and /pose/tip"
    /// "By default, both are set to the unaltered pose of the device."
    /// ~https://github.com/ValveSoftware/openvr/wiki/Input-Profiles#pose-components
    Raw,
    /// "If you provide /pose/tip in your rendermodel you should set it to the position and rotation that are appropriate for pointing (i.e. with a laser pointer) with your controller."
    /// ~https://github.com/ValveSoftware/openvr/wiki/Input-Profiles#pose-components
    Tip,
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
        crate::warn_unimplemented!("OpenBindingUI");
        vr::EVRInputError::None
    }
    fn IsUsingLegacyInput(&self) -> bool {
        return self
            .openxr
            .session_data
            .get()
            .input_data
            .get_legacy_actions()
            .is_some();
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
                return vr::EVRInputError::InvalidDevice;
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
        _: *const c_void,
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
        _: *mut c_void,
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
        if transform_array_count < skeletal::HandSkeletonBone::Count as u32 {
            return vr::EVRInputError::BufferTooSmall;
        }
        let out_transforms = unsafe {
            std::slice::from_raw_parts_mut(transform_array, transform_array_count as usize)
        };

        get_action_from_handle!(self, handle, session_data, action);
        let ActionData::Skeleton(hand) = action else {
            return vr::EVRInputError::WrongType;
        };

        let Some(transforms) =
            self.get_bones_from_hand_tracking(&session_data, transform_space, *hand)
        else {
            return vr::EVRInputError::NoData;
        };
        out_transforms.copy_from_slice(&transforms[0..out_transforms.len()]);
        vr::EVRInputError::None
    }
    fn GetSkeletalTrackingLevel(
        &self,
        action: vr::VRActionHandle_t,
        level: *mut vr::EVRSkeletalTrackingLevel,
    ) -> vr::EVRInputError {
        get_action_from_handle!(self, action, data, action);
        let ActionData::Skeleton(hand) = action else {
            return vr::EVRInputError::WrongType;
        };

        let Some(index) = self.get_controller_device_index(*hand) else {
            return vr::EVRInputError::InvalidDevice;
        };

        let controller_type = self.get_device_string_tracked_property(
            index,
            vr::ETrackedDeviceProperty::ControllerType_String,
        );

        unsafe {
            // Make sure knuckles are always Partial
            // TODO: Remove in favor of using XR_EXT_hand_tracking_data_source
            if controller_type.as_deref() == Some(c"knuckles") {
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
        let ActionData::Skeleton(hand) = action else {
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
        crate::warn_unimplemented!("SetDominantHand");
        vr::EVRInputError::None
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

        get_action_from_handle!(self, action, session_data, action);
        let ActionData::Skeleton(hand) = action else {
            return vr::EVRInputError::WrongType;
        };

        let origin = match hand {
            Hand::Left => self.left_hand_key.data().as_ffi(),
            Hand::Right => self.right_hand_key.data().as_ffi(),
        };
        unsafe {
            std::ptr::addr_of_mut!((*action_data).bActive).write(
                self.get_bones_from_hand_tracking(
                    &session_data,
                    vr::EVRSkeletalTransformSpace::Parent,
                    *hand,
                )
                .is_some(),
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
        let devices = data.input_data.devices.read().unwrap();
        let get_hand = |hand| {
            devices
                .get_controller(hand)
                .map(|h| (hand, h.profile_path))
                .unzip()
        };
        let (active_origin, hand) = match loaded.try_get_action(action) {
            Ok(ActionData::Pose) => {
                let (mut hand, interaction_profile) = match subaction_path {
                    x if x == self.get_subaction_path(Hand::Left) => get_hand(Hand::Left),
                    x if x == self.get_subaction_path(Hand::Right) => get_hand(Hand::Right),
                    x if x == xr::Path::NULL => (None, None),
                    _ => unreachable!(),
                };

                let get_hand_pose =
                    |hand: &TrackedDevice| loaded.try_get_pose(action, hand.profile_path).ok();

                let get_first_bound_hand_profile = || {
                    devices
                        .get_controller(Hand::Left)
                        .and_then(get_hand_pose)
                        .or_else(|| devices.get_controller(Hand::Right).and_then(get_hand_pose))
                };

                let Some(bound) = interaction_profile
                    .and_then(|p| loaded.try_get_pose(action, p).ok())
                    .or_else(get_first_bound_hand_profile)
                else {
                    match hand {
                        Some(hand) => {
                            trace!(
                                "action has no bindings for the {hand:?} hand's interaction profile"
                            );
                        }
                        None => {
                            trace!("action has no bindings for either hand's interaction profile");
                        }
                    }

                    // We may have no interaction profile with hand tracking, try to get synthesised pose
                    let pose = hand.and_then(|hand| {
                        devices
                            .get_controller(hand)
                            .and_then(|d| d.get_pose(&self.openxr, &data, origin))
                    });

                    if let Some(pose) = pose {
                        unsafe {
                            action_data.write(vr::InputPoseActionData_t {
                                bActive: true,
                                activeOrigin: 0,
                                pose,
                            });
                        }
                        return vr::EVRInputError::None;
                    } else {
                        no_data!()
                    }
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
                    trace!("action has no bindings for the hand {hand:?}");
                    no_data!()
                };

                let hand = hand.unwrap();
                let origin = origin.unwrap_or_else(|| match hand {
                    Hand::Left => self.left_hand_key.data().as_ffi(),
                    Hand::Right => self.right_hand_key.data().as_ffi(),
                });

                match ty {
                    BoundPoseType::Raw | BoundPoseType::Gdc2015 => (origin, hand),
                    BoundPoseType::Tip => {
                        // ToDo: Check if render model has a tip pose otherwise use raw pose
                        // For now, just use the raw pose
                        (origin, hand)
                    }
                }
            }
            Ok(ActionData::Skeleton(hand)) => {
                if subaction_path != xr::Path::NULL {
                    return vr::EVRInputError::InvalidDevice;
                }
                (0, *hand)
            }
            Ok(_) => return vr::EVRInputError::WrongType,
            Err(e) => return e,
        };

        drop(devices);
        drop(data);

        unsafe {
            let pose = self
                .get_controller_pose(hand, Some(origin))
                .unwrap_or_default();
            action_data.write(vr::InputPoseActionData_t {
                bActive: true,
                activeOrigin: active_origin,
                pose,
            });
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
                    && binding_state.is_active
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
            && binding_state.is_active
            && (binding_state.current_state && !state.current_state || !state.is_active)
        {
            state = binding_state;
            active_hand = binding_source;
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
        let mut sync_sets = Vec::with_capacity(active_sets.len() + 3);
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

            let skeletal_input = data.input_data.estimated_skeleton_actions.get().unwrap();
            sync_sets.push(xr::ActiveActionSet::new(
                &data.input_data.pose_data.get().unwrap().set,
            ));
            sync_sets.push(xr::ActiveActionSet::new(&skeletal_input.set));
            sync_sets.push(xr::ActiveActionSet::new(&actions.haptic_set));
            self.legacy_state.on_action_sync();
        }

        {
            tracy_span!("xrSyncActions");
            data.session.sync_actions(&sync_sets).unwrap();
        }

        let devices = data.input_data.devices.read().unwrap();
        let left_profile = devices
            .get_controller(Hand::Left)
            .map(|dev| dev.profile_path);
        let right_profile = devices
            .get_controller(Hand::Right)
            .map(|dev| dev.profile_path);
        for key in &actions.actions_with_custom_bindings {
            let unsync_custom_bindings = |key, profile| {
                if profile == xr::Path::NULL {
                    return;
                }

                let Some(bindings) = actions
                    .per_profile_bindings
                    .get(&profile)
                    .and_then(|map| map.get(key))
                else {
                    return;
                };

                for binding in bindings {
                    binding.unsync();
                }
            };

            match (left_profile, right_profile) {
                (Some(profile), None) | (None, Some(profile)) => {
                    unsync_custom_bindings(*key, profile)
                }
                (Some(left), Some(right)) => {
                    unsync_custom_bindings(*key, left);
                    if left != right {
                        unsync_custom_bindings(*key, right);
                    }
                }
                (None, None) => {}
            }
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
        self.loading_actions.store(true, Ordering::Relaxed);
        let mut data = self.openxr.session_data.get();
        if data.input_data.get_legacy_actions().is_some() {
            drop(data);
            self.openxr.restart_session();
            data = self.openxr.session_data.get();
        }

        let ret = match self.load_action_manifest(&data, path) {
            Ok(_) => vr::EVRInputError::None,
            Err(e) => e,
        };

        self.loading_actions.store(false, Ordering::Relaxed);
        ret
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

impl<C: openxr_data::Compositor> vr::IVRInput004On005 for Input<C> {
    #[inline]
    fn DecompressSkeletalBoneData(
        &self,
        _compressed_buffer: *mut c_void,
        _compressed_buffer_size: u32,
        _transform_space: *mut vr::EVRSkeletalTransformSpace,
        _transform_array: *mut vr::VRBoneTransform_t,
        _transform_array_count: u32,
    ) -> vr::EVRInputError {
        todo!()
    }

    #[inline]
    fn GetOriginLocalizedName(
        &self,
        origin: vr::VRInputValueHandle_t,
        name_array: *mut c_char,
        name_array_size: u32,
    ) -> vr::EVRInputError {
        <Self as vr::IVRInput010_Interface>::GetOriginLocalizedName(
            self,
            origin,
            name_array,
            name_array_size,
            0,
        )
    }

    #[inline]
    fn GetSkeletalActionData(
        &self,
        action: vr::VRActionHandle_t,
        action_data: *mut vr::InputSkeletalActionData_t,
        action_data_size: u32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        // restrict_to_device not supported for skeletal actions
        if restrict_to_device != vr::k_ulInvalidInputValueHandle {
            return vr::EVRInputError::NoData;
        }

        <Self as vr::IVRInput010_Interface>::GetSkeletalActionData(
            self,
            action,
            action_data,
            action_data_size,
        )
    }

    #[inline]
    fn GetSkeletalBoneData(
        &self,
        action: vr::VRActionHandle_t,
        transform_space: vr::EVRSkeletalTransformSpace,
        motion_range: vr::EVRSkeletalMotionRange,
        transform_array: *mut vr::VRBoneTransform_t,
        transform_array_count: u32,
        restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        // restrict_to_device not supported for skeletal actions
        if restrict_to_device != vr::k_ulInvalidInputValueHandle {
            return vr::EVRInputError::NoData;
        }

        <Self as vr::IVRInput010_Interface>::GetSkeletalBoneData(
            self,
            action,
            transform_space,
            motion_range,
            transform_array,
            transform_array_count,
        )
    }

    #[inline]
    fn GetSkeletalBoneDataCompressed(
        &self,
        _action: vr::VRActionHandle_t,
        _transform_space: vr::EVRSkeletalTransformSpace,
        _motion_range: vr::EVRSkeletalMotionRange,
        _compressed_data: *mut c_void,
        _compressed_size: u32,
        _required_compressed_size: *mut u32,
        _restrict_to_device: vr::VRInputValueHandle_t,
    ) -> vr::EVRInputError {
        todo!()
    }
}

impl<C: openxr_data::Compositor> Input<C> {
    pub fn interaction_profile_changed(&self, session_data: &SessionData) {
        let mut devices = session_data.input_data.devices.write().unwrap();

        let mut devices_to_create = vec![];

        for hand in [Hand::Left, Hand::Right] {
            let mut controller = devices.get_controller_mut(hand);
            let subaction_path = self.get_subaction_path(hand);

            let profile_path = session_data
                .session
                .current_interaction_profile(subaction_path)
                .unwrap();

            if let Some(controller) = controller.as_mut() {
                controller.profile_path = profile_path;
            }

            let profile_name = match profile_path {
                xr::Path::NULL => {
                    if let Some(controller) = controller.as_mut() {
                        controller.connected = false;
                    }
                    "<null>".to_owned()
                }
                path => {
                    if let Some(controller) = controller.as_mut() {
                        controller.connected = true;
                    }
                    self.openxr.instance.path_to_string(path).unwrap()
                }
            };

            let profile = Profiles::get().profile_from_name(&profile_name);

            if let Some(p) = profile {
                if let Some(controller) = controller.as_mut() {
                    controller.interaction_profile = Some(p);
                } else {
                    let hand_tracker = session_data
                        .session
                        .create_hand_tracker(hand.into())
                        .inspect_err(|e| {
                            if !matches!(
                                *e,
                                xr::sys::Result::ERROR_EXTENSION_NOT_PRESENT
                                    | xr::sys::Result::ERROR_FEATURE_UNSUPPORTED
                            ) {
                                log::debug!("Failed to create hand tracker for hand {hand:?}: {e}");
                            }
                        })
                        .ok();
                    devices_to_create.push((
                        TrackedDeviceType::Controller(ControllerData {
                            hand,
                            hand_tracker,
                            skeleton_cache: Mutex::new(Default::default()),
                            pose_is_synthesised: false.into(),
                        }),
                        Some(profile_path),
                        Some(p),
                    ));
                }
            };

            session_data.input_data.interaction_profile_changed();

            info!(
                "{} interaction profile changed: {}",
                self.openxr
                    .instance
                    .path_to_string(self.get_subaction_path(hand))
                    .unwrap(),
                profile_name
            )
        }

        for (device_type, profile_path, interaction_profile) in devices_to_create {
            let mut device = TrackedDevice::new(device_type, profile_path, interaction_profile);
            device.connected = true;

            devices.push_device(device).unwrap_or_else(|e| {
                panic!("Failed to create new controller: {:?}", e);
            });
        }

        #[cfg(feature = "monado")]
        devices
            .create_monado_generic_trackers(&self.openxr, session_data)
            .unwrap();
    }

    pub fn frame_start_update(&self) {
        tracy_span!();
        let data = self.openxr.session_data.get();
        let devices = data.input_data.devices.read().unwrap();

        for cache in self.skeletal_bone_cache.as_ref() {
            std::mem::take(&mut *cache.lock().unwrap());
        }

        for device in devices.iter() {
            device.clear_pose_cache();
        }

        let left_hand = devices.get_controller(Hand::Left);
        let right_hand = devices.get_controller(Hand::Right);

        let input_data = &data.input_data;
        if let Some(loaded) = input_data.get_loaded_actions() {
            // If the game has loaded actions, we shouldn't need to sync the state because the game
            // should be doing it itself with UpdateActionState. However, some games (Tea for God)
            // don't actually call UpdateActionState if no controllers are reported as connected,
            // and interaction profiles are only updated after xrSyncActions is called. So here, we
            // do an action sync to try and get the runtime to update the interaction profile.
            if (left_hand.is_none_or(|hand| !hand.is_connected(&self.openxr, &data)))
                && (right_hand.is_none_or(|hand| !hand.is_connected(&self.openxr, &data)))
            {
                debug!("no controllers connected - syncing info set");
                data.session
                    .sync_actions(&[xr::ActiveActionSet::new(&loaded.info_set)])
                    .unwrap();
            }
            return;
        }

        match input_data.get_legacy_actions() {
            Some(actions) => {
                data.session
                    .sync_actions(&[
                        xr::ActiveActionSet::new(&actions.set),
                        xr::ActiveActionSet::new(&input_data.pose_data.get().unwrap().set),
                    ])
                    .unwrap();

                self.legacy_state.on_action_sync();
            }
            None => {
                if self.loading_actions.load(Ordering::Relaxed) {
                    return;
                }

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
                self.setup_legacy_actions();
            }
        }
    }

    pub fn post_session_restart(&self, data: &SessionData) {
        // This function is called while a write lock is called on the session, and as such should
        // not use self.openxr.session_data.get().
        data.input_data
            .pose_data
            .set(PoseData::new(
                &self.openxr.instance,
                self.subaction_paths.left,
                self.subaction_paths.right,
            ))
            .unwrap_or_else(|_| panic!("PoseData already setup"));
        if let Some(path) = self.loaded_actions_path.get() {
            let _ = self.load_action_manifest(data, path);
        }
    }

    pub fn get_next_event(&self, size: u32, out: *mut vr::VREvent_t) -> bool {
        const FUNC: &str = "get_next_event";
        if out.is_null() {
            warn!("{FUNC}: Got null event pointer.");
            return false;
        }
        let data = self.openxr.session_data.get();
        let mut devices = data.input_data.devices.write().unwrap();

        for (i, device) in devices.iter_mut().enumerate() {
            let current = device.connected;

            if device.has_connected_changed() {
                debug!(
                    "sending {:?} {}connected",
                    device.get_type(),
                    if current { "" } else { "not " }
                );

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
                warn!(
                    "{FUNC}: Provided event struct size ({size}) is smaller than required ({MIN_CONTROLLER_EVENT_SIZE})."
                );
                return false;
            }
            // VREvent_t can be different sizes depending on the OpenVR version,
            // so we use raw pointers to avoid creating a reference, because if the
            // size doesn't match our VREvent_t's size, we are in UB land
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

enum LoadedActions {
    Legacy(LegacyActionData),
    Manifest(ManifestLoadedActions),
}

struct ManifestLoadedActions {
    sets: SecondaryMap<ActionSetKey, xr::ActionSet>,
    actions: SecondaryMap<ActionKey, ActionData>,
    extra_actions: SecondaryMap<ActionKey, ExtraActionData>,
    actions_with_custom_bindings: HashSet<ActionKey>,
    per_profile_pose_bindings: HashMap<xr::Path, SecondaryMap<ActionKey, BoundPose>>,
    per_profile_bindings: HashMap<xr::Path, SecondaryMap<ActionKey, Vec<BindingData>>>,
    info_set: xr::ActionSet,
    _info_action: xr::Action<bool>,
    haptic_set: xr::ActionSet,
    haptic_action: xr::Action<xr::Haptic>,
}

impl ManifestLoadedActions {
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
            .inspect_err(|_| trace!("didn't find action for {key:?}"))
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

struct PoseData {
    set: xr::ActionSet,
    grip: xr::Action<xr::Posef>,
    left_space: HandSpace,
    right_space: HandSpace,
}

impl PoseData {
    fn new(instance: &xr::Instance, left_path: xr::Path, right_path: xr::Path) -> Self {
        let set = instance
            .create_action_set("xrizer-pose-data", "xrizer pose data", 0)
            .unwrap();
        let grip = set
            .create_action("grip-pose", "Grip Pose", &[left_path, right_path])
            .unwrap();
        Self {
            set,
            grip,
            left_space: HandSpace {
                hand: Hand::Left,
                hand_path: left_path,
                raw: RwLock::default(),
            },
            right_space: HandSpace {
                hand: Hand::Right,
                hand_path: right_path,
                raw: RwLock::default(),
            },
        }
    }
    fn reset_spaces(&self) {
        self.left_space.reset_raw();
        self.right_space.reset_raw();
    }
}

struct HandSpace {
    hand: Hand,
    hand_path: xr::Path,

    /// Based on the controller jsons in SteamVR, the "raw" pose
    /// This is stored as a space so we can locate hand joints relative to it for skeletal data.
    raw: RwLock<Option<xr::Space>>,
}

struct SpaceReadGuard<'a>(RwLockReadGuard<'a, Option<xr::Space>>);
impl Deref for SpaceReadGuard<'_> {
    type Target = xr::Space;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl HandSpace {
    pub fn try_get_or_init_raw(
        &self,
        hand_profile: &Option<&dyn InteractionProfile>,
        session_data: &SessionData,
        pose_data: &PoseData,
    ) -> Option<SpaceReadGuard<'_>> {
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
                pose_data
                    .grip
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
