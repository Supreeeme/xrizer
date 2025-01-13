mod action_manifest;
mod custom_bindings;
mod legacy;
mod profiles;
mod skeletal;

#[cfg(test)]
mod tests;

pub use profiles::{InteractionProfile, Profiles};

use crate::{
    openxr_data::{self, Hand, OpenXrData, SessionData},
    tracy_span, AtomicF32,
};
use custom_bindings::{BoolActionData, FloatActionData};
use legacy::{setup_legacy_bindings, LegacyActionData};
use log::{debug, info, trace, warn};
use openvr::{self as vr, space_relation_to_openvr_pose};
use openxr as xr;
use slotmap::{new_key_type, Key, KeyData, SecondaryMap, SlotMap};
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::mem::ManuallyDrop;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex, OnceLock, RwLock,
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
    cached_poses: Mutex<CachedSpaces>,
    legacy_packet_num: AtomicU32,
    skeletal_tracking_level: RwLock<vr::EVRSkeletalTrackingLevel>,
    profile_map: HashMap<xr::Path, &'static profiles::ProfileProperties>,
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
            loaded_actions_path: OnceLock::new(),
            left_hand_key,
            right_hand_key,
            cached_poses: Mutex::default(),
            legacy_packet_num: 0.into(),
            skeletal_tracking_level: RwLock::new(vr::EVRSkeletalTrackingLevel::Estimated),
            profile_map,
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
    legacy_actions: OnceLock<LegacyActionData>,
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
                &self.openxr,
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
        let (active_origin, hand) = match loaded.try_get_action(action) {
            Ok(ActionData::Pose { bindings }) => {
                let (mut hand, interaction_profile) = match subaction_path {
                    x if x == self.openxr.left_hand.subaction_path => (
                        Some(Hand::Left),
                        Some(self.openxr.left_hand.profile_path.load()),
                    ),
                    x if x == self.openxr.right_hand.subaction_path => (
                        Some(Hand::Right),
                        Some(self.openxr.right_hand.profile_path.load()),
                    ),
                    x if x == xr::Path::NULL => (None, None),
                    _ => unreachable!(),
                };

                let get_first_bound_hand_profile = || {
                    bindings
                        .get(&self.openxr.left_hand.profile_path.load())
                        .or_else(|| bindings.get(&self.openxr.right_hand.profile_path.load()))
                };

                let Some(bound) = interaction_profile
                    .and_then(|p| bindings.get(&p))
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

        let mut out = WriteOnDrop::new(action_data);
        get_action_from_handle!(self, handle, session_data, action);
        let subaction_path = get_subaction_path!(self, restrict_to_device, action_data);

        let (state, delta) = match action {
            ActionData::Vector1(data) => {
                let state = data.state(&session_data.session, subaction_path).unwrap();
                let delta = xr::Vector2f {
                    x: state.current_state - data.last_value.swap(state.current_state),
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
            activeOrigin: 0,
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

        let state = action.state(&session_data.session, subaction_path).unwrap();
        *out.value = vr::InputDigitalActionData_t {
            bActive: state.is_active,
            bState: state.current_state,
            activeOrigin: restrict_to_device, // TODO
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
            sync_sets.push(xr::ActiveActionSet::new(&legacy.set));
            self.legacy_packet_num.fetch_add(1, Ordering::Relaxed);
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
        tracy_span!();
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
        tracy_span!();
        let mut spaces = self.cached_poses.lock().unwrap();
        let data = self.openxr.session_data.get();
        spaces
            .get_pose_impl(
                &self.openxr,
                &data,
                self.openxr.display_time.get(),
                None,
                origin.unwrap_or(data.current_origin),
            )
            .unwrap()
    }

    /// Returns None if legacy actions haven't been set up yet.
    pub fn get_controller_pose(
        &self,
        hand: Hand,
        origin: Option<vr::ETrackingUniverseOrigin>,
    ) -> Option<vr::TrackedDevicePose_t> {
        tracy_span!();
        let mut spaces = self.cached_poses.lock().unwrap();
        let data = self.openxr.session_data.get();
        spaces.get_pose_impl(
            &self.openxr,
            &data,
            self.openxr.display_time.get(),
            Some(hand),
            origin.unwrap_or(data.current_origin),
        )
    }

    pub fn frame_start_update(&self) {
        tracy_span!();
        std::mem::take(&mut *self.cached_poses.lock().unwrap());
        let data = self.openxr.session_data.get();
        if let Some(loaded) = data.input_data.loaded_actions.get() {
            // If the game has loaded actions, we shouldn't need to sync the state because the game
            // should be doing it itself with UpdateActionState. However, some games (Tea for God)
            // don't actually call UpdateActionState if no controllers are reported as connected,
            // and interaction profiles are only updated after xrSyncActions is called. So here, we
            // do an action sync to try and get the runtime to update the interaction profile.
            let loaded = loaded.read().unwrap();
            if !self.openxr.left_hand.connected() || !self.openxr.right_hand.connected() {
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

                self.legacy_packet_num.fetch_add(1, Ordering::Relaxed);
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

    fn get_profile_data(&self, hand: Hand) -> Option<&profiles::ProfileProperties> {
        let hand = match hand {
            Hand::Left => &self.openxr.left_hand,
            Hand::Right => &self.openxr.right_hand,
        };
        let profile = hand.profile_path.load();
        self.profile_map.get(&profile).map(|v| &**v)
    }

    pub fn get_controller_string_tracked_property(
        &self,
        hand: Hand,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<&'static CStr> {
        self.get_profile_data(hand).and_then(|data| {
            match property {
                // Audica likes to apply controller specific tweaks via this property
                vr::ETrackedDeviceProperty::ControllerType_String => {
                    Some(data.openvr_controller_type)
                }
                // I Expect You To Die 3 identifies controllers with this property -
                // why it couldn't just use ControllerType instead is beyond me...
                vr::ETrackedDeviceProperty::ModelNumber_String => Some(data.model),
                // Resonite won't recognize controllers without this
                vr::ETrackedDeviceProperty::RenderModelName_String => {
                    Some(*data.render_model_name.get(hand))
                }
                // Required for controllers to be acknowledged in I Expect You To Die 3
                vr::ETrackedDeviceProperty::SerialNumber_String
                | vr::ETrackedDeviceProperty::ManufacturerName_String => Some(c"<unknown>"),
                _ => None,
            }
        })
    }

    pub fn get_controller_int_tracked_property(
        &self,
        hand: Hand,
        property: vr::ETrackedDeviceProperty,
    ) -> Option<i32> {
        self.get_profile_data(hand).and_then(|data| match property {
            vr::ETrackedDeviceProperty::Axis0Type_Int32 => {
                if data.has_joystick {
                    Some(vr::EVRControllerAxisType::Joystick as _)
                } else if data.has_trackpad {
                    Some(vr::EVRControllerAxisType::TrackPad as _)
                } else {
                    Some(vr::EVRControllerAxisType::None as _)
                }
            }
            vr::ETrackedDeviceProperty::Axis1Type_Int32 => {
                Some(vr::EVRControllerAxisType::Trigger as _)
            }
            vr::ETrackedDeviceProperty::Axis2Type_Int32 => {
                if data.has_joystick && data.has_trackpad {
                    Some(vr::EVRControllerAxisType::TrackPad as _)
                } else {
                    Some(vr::EVRControllerAxisType::None as _)
                }
            }
            vr::ETrackedDeviceProperty::Axis3Type_Int32
            | vr::ETrackedDeviceProperty::Axis4Type_Int32 => {
                Some(vr::EVRControllerAxisType::None as _)
            }
            _ => None,
        })
    }

    pub fn post_session_restart(&self, data: &SessionData) {
        // This function is called while a write lock is called on the session, and as such should
        // not use self.openxr.session_data.get().
        if let Some(path) = self.loaded_actions_path.get() {
            self.load_action_manifest(data, path).unwrap();
        }
    }
}

#[derive(Default)]
struct CachedSpaces {
    seated: CachedPoses,
    standing: CachedPoses,
}

#[derive(Default)]
struct CachedPoses {
    head: Option<vr::TrackedDevicePose_t>,
    left: Option<vr::TrackedDevicePose_t>,
    right: Option<vr::TrackedDevicePose_t>,
}

impl CachedSpaces {
    fn get_pose_impl(
        &mut self,
        xr_data: &OpenXrData<impl openxr_data::Compositor>,
        session_data: &SessionData,
        display_time: xr::Time,
        hand: Option<Hand>,
        origin: vr::ETrackingUniverseOrigin,
    ) -> Option<vr::TrackedDevicePose_t> {
        tracy_span!();
        let space = match origin {
            vr::ETrackingUniverseOrigin::Seated => &mut self.seated,
            vr::ETrackingUniverseOrigin::Standing => &mut self.standing,
            vr::ETrackingUniverseOrigin::RawAndUncalibrated => unreachable!(),
        };

        let pose = match hand {
            None => &mut space.head,
            Some(Hand::Left) => &mut space.left,
            Some(Hand::Right) => &mut space.right,
        };

        if let Some(pose) = pose {
            return Some(*pose);
        }

        let (loc, velo) = if let Some(hand) = hand {
            let legacy = session_data.input_data.legacy_actions.get()?;
            let spaces = match hand {
                Hand::Left => &legacy.left_spaces,
                Hand::Right => &legacy.right_spaces,
            };

            if let Some(raw) = spaces.try_get_or_init_raw(xr_data, session_data, &legacy.actions) {
                raw.relate(session_data.get_space_for_origin(origin), display_time)
                    .unwrap()
            } else {
                trace!("failed to get raw space, making empty pose");
                (xr::SpaceLocation::default(), xr::SpaceVelocity::default())
            }
        } else {
            session_data
                .view_space
                .relate(session_data.get_space_for_origin(origin), display_time)
                .unwrap()
        };

        let ret = space_relation_to_openvr_pose(loc, velo);
        Some(*pose.insert(ret))
    }
}

struct LoadedActions {
    sets: SecondaryMap<ActionSetKey, xr::ActionSet>,
    actions: SecondaryMap<ActionKey, ActionData>,
    info_set: xr::ActionSet,
    _info_action: xr::Action<bool>,
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
