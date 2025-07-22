use crate::{
    clientcore::{Injected, Injector},
    input::Input,
    openxr_data::{Hand, RealOpenXrData, SessionData},
    tracy_span,
};
use glam::{Mat3, Quat, Vec3};
use log::{debug, error, trace, warn};
use openvr as vr;
use openxr as xr;
use std::ffi::CStr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Default)]
struct ConnectedHands {
    left: AtomicBool,
    right: AtomicBool,
}

#[derive(Copy, Clone)]
pub struct ViewData {
    pub flags: xr::ViewStateFlags,
    pub views: [xr::View; 2],
}

#[derive(Default)]
struct ViewCache {
    view: Option<ViewData>,
    local: Option<ViewData>,
    stage: Option<ViewData>,
}

impl ViewCache {
    fn get_views(
        &mut self,
        session: &SessionData,
        display_time: xr::Time,
        ty: xr::ReferenceSpaceType,
    ) -> ViewData {
        let data = match ty {
            xr::ReferenceSpaceType::VIEW => &mut self.view,
            xr::ReferenceSpaceType::LOCAL => &mut self.local,
            xr::ReferenceSpaceType::STAGE => &mut self.stage,
            other => panic!("unexpected reference space type: {other:?}"),
        };

        *data.get_or_insert_with(|| {
            let (flags, views) = session
                .session
                .locate_views(
                    xr::ViewConfigurationType::PRIMARY_STEREO,
                    display_time,
                    session.get_space_from_type(ty),
                )
                .expect("Couldn't locate views");

            ViewData {
                flags,
                views: views
                    .try_into()
                    .unwrap_or_else(|v: Vec<xr::View>| panic!("Expected 2 views, got {}", v.len())),
            }
        })
    }
}

#[derive(macros::InterfaceImpl)]
#[interface = "IVRSystem"]
#[versions(022, 021, 020, 019, 017, 016, 015, 014, 012, 009)]
pub struct System {
    openxr: Arc<RealOpenXrData>, // We don't need to test session restarting.
    input: Injected<Input<crate::compositor::Compositor>>,
    vtables: Vtables,
    last_connected_hands: ConnectedHands,
    views: Mutex<ViewCache>,
}

mod log_tags {
    pub const TRACKED_PROP: &str = "tracked_property";
}

impl System {
    pub fn new(openxr: Arc<RealOpenXrData>, injector: &Injector) -> Self {
        Self {
            openxr,
            input: injector.inject(),
            vtables: Default::default(),
            last_connected_hands: Default::default(),
            views: Mutex::default(),
        }
    }

    pub fn reset_views(&self) {
        std::mem::take(&mut *self.views.lock().unwrap());
        let session = self.openxr.session_data.get();
        let display_time = self.openxr.display_time.get();
        let mut views = self.views.lock().unwrap();
        views.get_views(&session, display_time, xr::ReferenceSpaceType::VIEW);
        views.get_views(
            &session,
            display_time,
            session.current_origin_as_reference_space(),
        );
    }

    pub fn get_views(&self, ty: xr::ReferenceSpaceType) -> ViewData {
        tracy_span!();
        let session = self.openxr.session_data.get();
        let mut views = self.views.lock().unwrap();
        views.get_views(&session, self.openxr.display_time.get(), ty)
    }
}

impl vr::IVRSystem022_Interface for System {
    fn GetRecommendedRenderTargetSize(&self, width: *mut u32, height: *mut u32) {
        let views = self
            .openxr
            .instance
            .enumerate_view_configuration_views(
                self.openxr.system_id,
                xr::ViewConfigurationType::PRIMARY_STEREO,
            )
            .unwrap();

        if !width.is_null() {
            unsafe { *width = views[0].recommended_image_rect_width };
        }

        if !height.is_null() {
            unsafe { *height = views[0].recommended_image_rect_height };
        }
    }
    fn GetProjectionMatrix(&self, eye: vr::EVREye, near_z: f32, far_z: f32) -> vr::HmdMatrix44_t {
        // https://github.com/ValveSoftware/openvr/wiki/IVRSystem::GetProjectionRaw
        let [mut left, mut right, mut up, mut down] = [0.0; 4];
        self.GetProjectionRaw(eye, &mut left, &mut right, &mut down, &mut up);

        let idx = 1.0 / (right - left);
        let idy = 1.0 / (up - down);
        let idz = 1.0 / (far_z - near_z);
        let sx = right + left;
        let sy = up + down;

        vr::HmdMatrix44_t {
            m: [
                [2.0 * idx, 0.0, sx * idx, 0.0],
                [0.0, 2.0 * idy, sy * idy, 0.0],
                [0.0, 0.0, -far_z * idz, -far_z * near_z * idz],
                [0.0, 0.0, -1.0, 0.0],
            ],
        }
    }
    fn GetProjectionRaw(
        &self,
        eye: vr::EVREye,
        left: *mut f32,
        right: *mut f32,
        top: *mut f32,
        bottom: *mut f32,
    ) {
        let ty = self
            .openxr
            .session_data
            .get()
            .current_origin_as_reference_space();
        let view = self.get_views(ty).views[eye as usize];

        // Top and bottom are flipped, for some reason
        unsafe {
            *left = view.fov.angle_left.tan();
            *right = view.fov.angle_right.tan();
            *bottom = view.fov.angle_up.tan();
            *top = view.fov.angle_down.tan();
        }
    }
    fn ComputeDistortion(
        &self,
        _: vr::EVREye,
        _: f32,
        _: f32,
        _: *mut vr::DistortionCoordinates_t,
    ) -> bool {
        crate::warn_unimplemented!("ComputeDistortion");
        false
    }
    fn GetEyeToHeadTransform(&self, eye: vr::EVREye) -> vr::HmdMatrix34_t {
        let views = self.get_views(xr::ReferenceSpaceType::VIEW).views;
        let view = views[eye as usize];
        let view_rot = view.pose.orientation;

        {
            tracy_span!("conversion");
            let rot = Mat3::from_quat(Quat::from_xyzw(
                view_rot.x, view_rot.y, view_rot.z, view_rot.w,
            ))
            .transpose();

            let gen_array = |translation, rot_axis: Vec3| {
                std::array::from_fn(|i| if i == 3 { translation } else { rot_axis[i] })
            };
            vr::HmdMatrix34_t {
                m: [
                    gen_array(view.pose.position.x, rot.x_axis),
                    gen_array(view.pose.position.y, rot.y_axis),
                    gen_array(view.pose.position.z, rot.z_axis),
                ],
            }
        }
    }
    fn GetTimeSinceLastVsync(&self, _: *mut f32, _: *mut u64) -> bool {
        crate::warn_unimplemented!("GetTimeSinceLastVsync");
        false
    }
    fn GetRuntimeVersion(&self) -> *const std::os::raw::c_char {
        static VERSION: &CStr = c"2.5.1";
        VERSION.as_ptr()
    }
    fn GetAppContainerFilePaths(&self, _: *mut std::os::raw::c_char, _: u32) -> u32 {
        todo!()
    }
    fn AcknowledgeQuit_Exiting(&self) {
        todo!()
    }
    fn PerformFirmwareUpdate(&self, _: vr::TrackedDeviceIndex_t) -> vr::EVRFirmwareError {
        todo!()
    }
    fn ShouldApplicationReduceRenderingWork(&self) -> bool {
        false
    }
    fn ShouldApplicationPause(&self) -> bool {
        false
    }
    fn IsSteamVRDrawingControllers(&self) -> bool {
        todo!()
    }
    fn IsInputAvailable(&self) -> bool {
        true
    }
    fn GetControllerAxisTypeNameFromEnum(
        &self,
        _: vr::EVRControllerAxisType,
    ) -> *const std::os::raw::c_char {
        crate::warn_unimplemented!("GetControllerAxisTypeNameFromEnum");
        static NAME: &CStr = c"Unknown";
        NAME.as_ptr()
    }
    fn GetButtonIdNameFromEnum(&self, _: vr::EVRButtonId) -> *const std::os::raw::c_char {
        crate::warn_unimplemented!("GetButtonIdNameFromEnum");
        static NAME: &CStr = c"Unknown";
        NAME.as_ptr()
    }
    fn TriggerHapticPulse(&self, _: vr::TrackedDeviceIndex_t, _: u32, _: std::os::raw::c_ushort) {
        crate::warn_unimplemented!("TriggerHapticPulse");
    }
    fn GetControllerStateWithPose(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        device_index: vr::TrackedDeviceIndex_t,
        state: *mut vr::VRControllerState_t,
        state_size: u32,
        pose: *mut vr::TrackedDevicePose_t,
    ) -> bool {
        if self.GetControllerState(device_index, state, state_size) {
            unsafe {
                *pose.as_mut().unwrap() = self
                    .input
                    .get()
                    .unwrap()
                    .get_controller_pose(Hand::try_from(device_index).unwrap(), Some(origin));
            }
            true
        } else {
            false
        }
    }
    fn GetControllerState(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        state: *mut vr::VRControllerState_t,
        state_size: u32,
    ) -> bool {
        self.input
            .force(|_| Input::new(self.openxr.clone()))
            .get_legacy_controller_state(device_index, state, state_size)
    }
    fn GetHiddenAreaMesh(
        &self,
        eye: vr::EVREye,
        ty: vr::EHiddenAreaMeshType,
    ) -> vr::HiddenAreaMesh_t {
        if !self.openxr.enabled_extensions.khr_visibility_mask {
            return Default::default();
        }

        debug!("GetHiddenAreaMesh: area mesh type: {ty:?}");
        let mask_ty = match ty {
            vr::EHiddenAreaMeshType::Standard => xr::VisibilityMaskTypeKHR::HIDDEN_TRIANGLE_MESH,
            vr::EHiddenAreaMeshType::Inverse => xr::VisibilityMaskTypeKHR::VISIBLE_TRIANGLE_MESH,
            vr::EHiddenAreaMeshType::LineLoop => xr::VisibilityMaskTypeKHR::LINE_LOOP,
            vr::EHiddenAreaMeshType::Max => {
                warn!("Unexpectedly got EHiddenAreaMeshType::Max - returning default area mesh");
                return Default::default();
            }
        };

        let session_data = self.openxr.session_data.get();
        let mask = session_data
            .session
            .get_visibility_mask_khr(
                xr::ViewConfigurationType::PRIMARY_STEREO,
                eye as u32,
                mask_ty,
            )
            .unwrap();

        trace!("openxr mask: {:#?} {:#?}", mask.indices, mask.vertices);

        let [mut left, mut right, mut top, mut bottom] = [0.0; 4];
        self.GetProjectionRaw(eye, &mut left, &mut right, &mut top, &mut bottom);

        // convert from indices + vertices to just vertices
        let vertices: Vec<_> = mask
            .indices
            .into_iter()
            .map(|i| {
                let v = mask.vertices[i as usize];

                // It is unclear to me why this scaling is necessary, but OpenComposite does it and
                // it seems to get games to use the mask correctly.
                let x_scaled = (v.x - left) / (right - left);
                let y_scaled = (v.y - top) / (bottom - top);
                vr::HmdVector2_t {
                    v: [x_scaled, y_scaled],
                }
            })
            .collect();

        trace!("vertices: {vertices:#?}");
        let count = vertices.len() / 3;
        // XXX: what are we supposed to do here? pVertexData is a random pointer and there's no
        // clear way for the application to deallocate it
        // fortunately it seems like applications don't call this often, so this leakage isn't a
        // huge deal.
        let vertices = Vec::leak(vertices).as_ptr();

        vr::HiddenAreaMesh_t {
            pVertexData: vertices,
            unTriangleCount: count as u32,
        }
    }
    fn GetEventTypeNameFromEnum(&self, _: vr::EVREventType) -> *const std::os::raw::c_char {
        todo!()
    }
    fn PollNextEventWithPose(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        event: *mut vr::VREvent_t,
        size: u32,
        pose: *mut vr::TrackedDevicePose_t,
    ) -> bool {
        for (current, prev, hand) in [
            (
                self.openxr.left_hand.connected(),
                &self.last_connected_hands.left,
                Hand::Left,
            ),
            (
                self.openxr.right_hand.connected(),
                &self.last_connected_hands.right,
                Hand::Right,
            ),
        ] {
            if prev
                .compare_exchange(!current, current, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                debug!(
                    "sending {hand:?} {}connected",
                    if current { "" } else { "not " }
                );

                // Since the VREvent_t struct can be a variable size, it seems a little dangerous to
                // create a reference to it, so we'll just operate through pointers.
                // The eventType, trackedDeviceIndex, and eventAgeSeconds fields have always existed.
                unsafe {
                    (&raw mut (*event).eventType).write(if current {
                        vr::EVREventType::TrackedDeviceActivated as u32
                    } else {
                        vr::EVREventType::TrackedDeviceDeactivated as u32
                    });

                    (&raw mut (*event).trackedDeviceIndex).write(hand as u32);
                    (&raw mut (*event).eventAgeSeconds).write(0.0);
                    if !pose.is_null() {
                        pose.write(
                            self.input
                                .force(|_| Input::new(self.openxr.clone()))
                                .get_controller_pose(hand, Some(origin)),
                        );
                    }
                }
                return true;
            }
        }

        self.input.get().is_some_and(|input| {
            let got_event = input.get_next_event(size, event);
            if got_event && !pose.is_null() {
                unsafe {
                    let index = (&raw const (*event).trackedDeviceIndex).read();
                    pose.write(input.get_controller_pose(Hand::try_from(index).unwrap(), None));
                }
            }
            got_event
        })
    }

    fn PollNextEvent(&self, event: *mut vr::VREvent_t, size: u32) -> bool {
        self.PollNextEventWithPose(
            vr::ETrackingUniverseOrigin::Seated,
            event,
            size,
            std::ptr::null_mut(),
        )
    }

    fn GetPropErrorNameFromEnum(
        &self,
        _: vr::ETrackedPropertyError,
    ) -> *const std::os::raw::c_char {
        c"Unknown error".as_ptr()
    }
    fn GetStringTrackedDeviceProperty(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        prop: vr::ETrackedDeviceProperty,
        value: *mut std::os::raw::c_char,
        size: u32,
        error: *mut vr::ETrackedPropertyError,
    ) -> u32 {
        debug!(target: log_tags::TRACKED_PROP, "requesting string property: {prop:?} ({device_index})");

        if !self.IsTrackedDeviceConnected(device_index) {
            if let Some(error) = unsafe { error.as_mut() } {
                *error = vr::ETrackedPropertyError::InvalidDevice;
            }
            return 0;
        }

        if let Some(error) = unsafe { error.as_mut() } {
            *error = vr::ETrackedPropertyError::Success;
        }

        let buf = if !value.is_null() && size > 0 {
            unsafe { std::slice::from_raw_parts_mut(value, size as usize) }
        } else {
            &mut []
        };

        let data = match device_index {
            vr::k_unTrackedDeviceIndex_Hmd => match prop {
                // The Unity OpenVR sample appears to have a hard requirement on these first three properties returning
                // something to even get the game to recognize the HMD's location. However, the value
                // itself doesn't appear to be that important.
                vr::ETrackedDeviceProperty::SerialNumber_String
                | vr::ETrackedDeviceProperty::ManufacturerName_String
                | vr::ETrackedDeviceProperty::ControllerType_String => Some(c"<unknown>"),
                _ => None,
            },
            x if Hand::try_from(x).is_ok() => self.input.get().and_then(|i| {
                i.get_controller_string_tracked_property(Hand::try_from(x).unwrap(), prop)
            }),
            _ => None,
        };

        let Some(data) = data else {
            if let Some(error) = unsafe { error.as_mut() } {
                *error = vr::ETrackedPropertyError::UnknownProperty;
            }
            return 0;
        };

        let data =
            unsafe { std::slice::from_raw_parts(data.as_ptr(), data.to_bytes_with_nul().len()) };
        if buf.len() < data.len() {
            if let Some(error) = unsafe { error.as_mut() } {
                *error = vr::ETrackedPropertyError::BufferTooSmall;
            }
        } else {
            buf[0..data.len()].copy_from_slice(data);
        }

        data.len() as u32
    }
    fn GetArrayTrackedDeviceProperty(
        &self,
        _: vr::TrackedDeviceIndex_t,
        _: vr::ETrackedDeviceProperty,
        _: vr::PropertyTypeTag_t,
        _: *mut std::os::raw::c_void,
        _: u32,
        _: *mut vr::ETrackedPropertyError,
    ) -> u32 {
        todo!()
    }
    fn GetMatrix34TrackedDeviceProperty(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        prop: vr::ETrackedDeviceProperty,
        err: *mut vr::ETrackedPropertyError,
    ) -> vr::HmdMatrix34_t {
        debug!(target: log_tags::TRACKED_PROP, "requesting matrix property: {prop:?} ({device_index})");
        if !self.IsTrackedDeviceConnected(device_index) {
            if let Some(err) = unsafe { err.as_mut() } {
                *err = vr::ETrackedPropertyError::InvalidDevice;
            }
            return Default::default();
        }

        if let Some(err) = unsafe { err.as_mut() } {
            *err = vr::ETrackedPropertyError::UnknownProperty;
        }
        Default::default()
    }
    fn GetUint64TrackedDeviceProperty(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        prop: vr::ETrackedDeviceProperty,
        err: *mut vr::ETrackedPropertyError,
    ) -> u64 {
        debug!(target: log_tags::TRACKED_PROP, "requesting uint64 property: {prop:?} ({device_index})");
        if !self.IsTrackedDeviceConnected(device_index) {
            if let Some(err) = unsafe { err.as_mut() } {
                *err = vr::ETrackedPropertyError::InvalidDevice;
            }
            return 0;
        }

        if let Some(err) = unsafe { err.as_mut() } {
            *err = vr::ETrackedPropertyError::Success;
        }

        match device_index {
            x if Hand::try_from(x).is_ok() => self.input.get().and_then(|input| {
                input.get_controller_uint_tracked_property(Hand::try_from(x).unwrap(), prop)
            }),
            _ => None,
        }
        .unwrap_or_else(|| {
            if let Some(err) = unsafe { err.as_mut() } {
                *err = vr::ETrackedPropertyError::UnknownProperty;
            }
            0
        })
    }
    fn GetInt32TrackedDeviceProperty(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        prop: vr::ETrackedDeviceProperty,
        err: *mut vr::ETrackedPropertyError,
    ) -> i32 {
        debug!(target: log_tags::TRACKED_PROP, "requesting int32 property: {prop:?} ({device_index})");
        if !self.IsTrackedDeviceConnected(device_index) {
            if let Some(err) = unsafe { err.as_mut() } {
                *err = vr::ETrackedPropertyError::InvalidDevice;
            }
            return 0;
        }

        if let Some(err) = unsafe { err.as_mut() } {
            *err = vr::ETrackedPropertyError::Success;
        }
        match device_index {
            x if Hand::try_from(x).is_ok() => self.input.get().and_then(|input| {
                input.get_controller_int_tracked_property(Hand::try_from(x).unwrap(), prop)
            }),
            _ => None,
        }
        .unwrap_or_else(|| {
            if let Some(err) = unsafe { err.as_mut() } {
                *err = vr::ETrackedPropertyError::UnknownProperty;
            }
            0
        })
    }
    fn GetFloatTrackedDeviceProperty(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        prop: vr::ETrackedDeviceProperty,
        error: *mut vr::ETrackedPropertyError,
    ) -> f32 {
        debug!(target: log_tags::TRACKED_PROP, "requesting float property: {prop:?} ({device_index})");
        if device_index != vr::k_unTrackedDeviceIndex_Hmd {
            if let Some(error) = unsafe { error.as_mut() } {
                *error = vr::ETrackedPropertyError::UnknownProperty;
            }
            return 0.0;
        }

        match prop {
            vr::ETrackedDeviceProperty::UserIpdMeters_Float => {
                let views = self.get_views(xr::ReferenceSpaceType::VIEW).views;
                views[1].pose.position.x - views[0].pose.position.x
            }
            vr::ETrackedDeviceProperty::DisplayFrequency_Float => 90.0,
            _ => {
                if let Some(error) = unsafe { error.as_mut() } {
                    *error = vr::ETrackedPropertyError::UnknownProperty;
                }
                0.0
            }
        }
    }
    fn GetBoolTrackedDeviceProperty(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        prop: vr::ETrackedDeviceProperty,
        err: *mut vr::ETrackedPropertyError,
    ) -> bool {
        debug!(target: log_tags::TRACKED_PROP, "requesting bool property: {prop:?} ({device_index})");
        if let Some(err) = unsafe { err.as_mut() } {
            *err = vr::ETrackedPropertyError::UnknownProperty;
        }
        false
    }

    fn IsTrackedDeviceConnected(&self, device_index: vr::TrackedDeviceIndex_t) -> bool {
        match device_index {
            vr::k_unTrackedDeviceIndex_Hmd => true,
            x if Hand::try_from(x).is_ok() => match Hand::try_from(x).unwrap() {
                Hand::Left => self.openxr.left_hand.connected(),
                Hand::Right => self.openxr.right_hand.connected(),
            },
            _ => false,
        }
    }

    fn GetTrackedDeviceClass(&self, index: vr::TrackedDeviceIndex_t) -> vr::ETrackedDeviceClass {
        match index {
            vr::k_unTrackedDeviceIndex_Hmd => vr::ETrackedDeviceClass::HMD,
            x if Hand::try_from(x).is_ok() => {
                if self.IsTrackedDeviceConnected(x) {
                    vr::ETrackedDeviceClass::Controller
                } else {
                    vr::ETrackedDeviceClass::Invalid
                }
            }
            _ => vr::ETrackedDeviceClass::Invalid,
        }
    }
    fn GetControllerRoleForTrackedDeviceIndex(
        &self,
        index: vr::TrackedDeviceIndex_t,
    ) -> vr::ETrackedControllerRole {
        match index {
            x if Hand::try_from(x).is_ok() => match Hand::try_from(x).unwrap() {
                Hand::Left => vr::ETrackedControllerRole::LeftHand,
                Hand::Right => vr::ETrackedControllerRole::RightHand,
            },
            _ => vr::ETrackedControllerRole::Invalid,
        }
    }
    fn GetTrackedDeviceIndexForControllerRole(
        &self,
        role: vr::ETrackedControllerRole,
    ) -> vr::TrackedDeviceIndex_t {
        match role {
            vr::ETrackedControllerRole::LeftHand => {
                if self.openxr.left_hand.connected() {
                    Hand::Left as u32
                } else {
                    vr::k_unTrackedDeviceIndexInvalid
                }
            }
            vr::ETrackedControllerRole::RightHand => {
                if self.openxr.right_hand.connected() {
                    Hand::Right as u32
                } else {
                    vr::k_unTrackedDeviceIndexInvalid
                }
            }
            _ => vr::k_unTrackedDeviceIndexInvalid,
        }
    }
    fn ApplyTransform(
        &self,
        _: *mut vr::TrackedDevicePose_t,
        _: *const vr::TrackedDevicePose_t,
        _: *const vr::HmdMatrix34_t,
    ) {
        todo!()
    }
    fn GetTrackedDeviceActivityLevel(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
    ) -> vr::EDeviceActivityLevel {
        match device_index {
            vr::k_unTrackedDeviceIndex_Hmd => vr::EDeviceActivityLevel::UserInteraction,
            x if Hand::try_from(x).is_ok() => {
                if self.IsTrackedDeviceConnected(x) {
                    vr::EDeviceActivityLevel::UserInteraction
                } else {
                    vr::EDeviceActivityLevel::Unknown
                }
            }
            _ => vr::EDeviceActivityLevel::Unknown,
        }
    }
    fn GetSortedTrackedDeviceIndicesOfClass(
        &self,
        _: vr::ETrackedDeviceClass,
        _: *mut vr::TrackedDeviceIndex_t,
        _: u32,
        _: vr::TrackedDeviceIndex_t,
    ) -> u32 {
        0
    }
    fn GetRawZeroPoseToStandingAbsoluteTrackingPose(&self) -> vr::HmdMatrix34_t {
        xr::Posef::IDENTITY.into()
    }
    fn GetSeatedZeroPoseToStandingAbsoluteTrackingPose(&self) -> vr::HmdMatrix34_t {
        xr::Posef::IDENTITY.into()
    }
    fn GetDeviceToAbsoluteTrackingPose(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        _seconds_to_photon_from_now: f32,
        pose_array: *mut vr::TrackedDevicePose_t,
        pose_count: u32,
    ) {
        self.input
            .force(|_| Input::new(self.openxr.clone()))
            .get_poses(
                unsafe { std::slice::from_raw_parts_mut(pose_array, pose_count as usize) },
                Some(origin),
            );
    }
    fn SetDisplayVisibility(&self, _: bool) -> bool {
        // Act as if we're limited to direct mode
        false
    }
    fn IsDisplayOnDesktop(&self) -> bool {
        // Direct mode
        false
    }
    fn GetOutputDevice(
        &self,
        device: *mut u64,
        texture_type: vr::ETextureType,
        instance: *mut vr::VkInstance_T,
    ) {
        if texture_type != vr::ETextureType::Vulkan {
            // Proton doesn't seem to properly translate this function, but it doesn't appear to
            // actually matter.
            log::error!("Unsupported texture type: {texture_type:?}");
            return;
        }

        unsafe {
            *device = self
                .openxr
                .instance
                .vulkan_graphics_device(self.openxr.system_id, instance as _)
                .expect("Failed to get vulkan physical device") as _;
        }
    }
    fn GetDXGIOutputInfo(&self, _: *mut i32) {
        todo!()
    }
    fn GetD3D9AdapterIndex(&self) -> i32 {
        todo!()
    }
}

impl vr::IVRSystem021On022 for System {
    fn ResetSeatedZeroPose(&self) {
        self.openxr
            .reset_tracking_space(vr::ETrackingUniverseOrigin::Seated);
    }
}

impl vr::IVRSystem020On021 for System {
    fn AcknowledgeQuit_UserPrompt(&self) {}
}

impl vr::IVRSystem019On020 for System {
    fn DriverDebugRequest(
        &self,
        _un_device_index: vr::TrackedDeviceIndex_t,
        _pch_request: *const std::os::raw::c_char,
        _pch_response_buffer: *mut std::os::raw::c_char,
        _un_response_buffer_size: u32,
    ) -> u32 {
        unimplemented!()
    }
}

impl vr::IVRSystem017On019 for System {
    fn IsInputFocusCapturedByAnotherProcess(&self) -> bool {
        false
    }
    fn ReleaseInputFocus(&self) {}
    fn CaptureInputFocus(&self) -> bool {
        true
    }
}

impl vr::IVRSystem016On017 for System {
    fn GetOutputDevice(&self, _device: *mut u64, _texture_type: vr::ETextureType) {
        // TODO: figure out what to pass for the instance...
        todo!()
    }
}

impl vr::IVRSystem014On015 for System {
    fn GetProjectionMatrix(
        &self,
        eye: vr::EVREye,
        near_z: f32,
        far_z: f32,
        _proj_type: vr::EGraphicsAPIConvention,
    ) -> vr::HmdMatrix44_t {
        // According to this bug: https://github.com/ValveSoftware/openvr/issues/70 the projection type
        // is straight up ignored in SteamVR anyway, lol. Bug for bug compat!

        <Self as vr::IVRSystem022_Interface>::GetProjectionMatrix(self, eye, near_z, far_z)
    }
}

impl vr::IVRSystem012On014 for System {
    fn ComputeDistortion(&self, eye: vr::EVREye, u: f32, v: f32) -> vr::DistortionCoordinates_t {
        let mut ret = vr::DistortionCoordinates_t::default();
        <Self as vr::IVRSystem022_Interface>::ComputeDistortion(self, eye, u, v, &mut ret);
        ret
    }

    fn GetHiddenAreaMesh(&self, eye: vr::EVREye) -> vr::HiddenAreaMesh_t {
        <Self as vr::IVRSystem022_Interface>::GetHiddenAreaMesh(
            self,
            eye,
            vr::EHiddenAreaMeshType::Standard,
        )
    }

    fn GetControllerState(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        state: *mut vr::VRControllerState_t,
    ) -> bool {
        <Self as vr::IVRSystem022_Interface>::GetControllerState(
            self,
            device_index,
            state,
            std::mem::size_of::<vr::VRControllerState_t>() as u32,
        )
    }

    fn GetControllerStateWithPose(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        device_index: vr::TrackedDeviceIndex_t,
        state: *mut vr::VRControllerState_t,
        device_pose: *mut vr::TrackedDevicePose_t,
    ) -> bool {
        <Self as vr::IVRSystem022_Interface>::GetControllerStateWithPose(
            self,
            origin,
            device_index,
            state,
            std::mem::size_of::<vr::VRControllerState_t>() as u32,
            device_pose,
        )
    }
}

impl vr::IVRSystem009On012 for System {
    fn PollNextEvent(&self, event: *mut vr::vr_0_9_12::VREvent_t) -> bool {
        self.PollNextEventWithPose(
            vr::ETrackingUniverseOrigin::Seated,
            event,
            std::ptr::null_mut(),
        )
    }

    fn PollNextEventWithPose(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        event: *mut vr::vr_0_9_12::VREvent_t,
        pose: *mut vr::TrackedDevicePose_t,
    ) -> bool {
        let mut e = vr::VREvent_t::default();
        let ret = <Self as vr::IVRSystem022_Interface>::PollNextEventWithPose(
            self,
            origin,
            &mut e,
            std::mem::size_of_val(&event) as u32,
            pose,
        );

        if ret && !event.is_null() {
            let event = unsafe { event.as_mut() }.unwrap();
            event.eventType = if let Ok(t) = vr::EVREventType::try_from(e.eventType) {
                t
            } else {
                error!("Unhandled event type for 0.9.12: {}", e.eventType);
                return false;
            };
            event.trackedDeviceIndex = e.trackedDeviceIndex;
            event.data = match e.eventType {
                x if x == vr::EVREventType::ButtonPress as u32
                    || x == vr::EVREventType::ButtonUnpress as u32
                    || x == vr::EVREventType::ButtonTouch as u32
                    || x == vr::EVREventType::ButtonUntouch as u32 =>
                {
                    vr::vr_0_9_12::VREvent_Data_t {
                        controller: unsafe { e.data.controller },
                    }
                }
                other => {
                    error!("Unhandled event type data for 0.9.12: {other:?}");
                    return false;
                }
            }
        }

        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clientcore::Injector;
    use std::ffi::CStr;
    use vr::IVRSystem022_Interface;

    #[test]
    fn unity_required_properties() {
        let xr = Arc::new(RealOpenXrData::new(&Injector::default()).unwrap());
        let injector = Injector::default();
        let system = System::new(xr, &injector);

        let test_prop = |property| {
            let mut err = vr::ETrackedPropertyError::Success;
            let len = system.GetStringTrackedDeviceProperty(
                vr::k_unTrackedDeviceIndex_Hmd,
                property,
                std::ptr::null_mut(),
                0,
                &mut err,
            );
            assert_eq!(err, vr::ETrackedPropertyError::BufferTooSmall);
            assert!(len > 0);
            let mut buf = vec![0; len as usize];

            let len = system.GetStringTrackedDeviceProperty(
                vr::k_unTrackedDeviceIndex_Hmd,
                property,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut err,
            );
            assert_eq!(err, vr::ETrackedPropertyError::Success);
            assert_eq!(len, buf.len() as u32);

            let slice = unsafe { std::slice::from_raw_parts(buf.as_ptr() as _, buf.len()) };
            CStr::from_bytes_with_nul(slice)
                .expect("Failed to convert returned buffer for {property:?} to CStr");
        };

        test_prop(vr::ETrackedDeviceProperty::SerialNumber_String);
        test_prop(vr::ETrackedDeviceProperty::ManufacturerName_String);
        test_prop(vr::ETrackedDeviceProperty::ControllerType_String);
    }
}
