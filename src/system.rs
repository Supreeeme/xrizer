use crate::{
    clientcore::{Injected, Injector},
    input::Input,
    openxr_data::{Hand, RealOpenXrData, SessionData},
    overlay::OverlayMan,
    tracy_span,
};
use glam::{Mat3, Quat, Vec3};
use log::{debug, error, trace, warn};
use openvr as vr;
use openxr as xr;
use std::ffi::{CStr, CString};
use std::sync::{Arc, Mutex};

#[derive(Copy, Clone)]
pub struct ViewData {
    pub flags: xr::ViewStateFlags,
    pub views: [xr::View; 2],
}

#[derive(Copy, Clone)]
struct ViewDataViewSpace {
    data: ViewData,
    original_orientations: [Quat; 2],
}

#[derive(Default)]
struct ViewCache {
    view: Option<ViewDataViewSpace>,
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
        match ty {
            xr::ReferenceSpaceType::VIEW => {
                self.view
                    .get_or_insert_with(|| Self::get_views_view_space(session, display_time))
                    .data
            }
            xr::ReferenceSpaceType::LOCAL | xr::ReferenceSpaceType::STAGE => {
                let view = match ty {
                    xr::ReferenceSpaceType::LOCAL => &mut self.local,
                    xr::ReferenceSpaceType::STAGE => &mut self.stage,
                    _ => unreachable!(),
                };

                *view.get_or_insert_with(|| {
                    let view_rots = self
                        .view
                        .get_or_insert_with(|| Self::get_views_view_space(session, display_time))
                        .original_orientations;

                    Self::get_views_other_space(session, display_time, ty, view_rots)
                })
            }
            other => panic!("unexpected reference space type: {other:?}"),
        }
    }

    fn get_views_view_space(session: &SessionData, display_time: xr::Time) -> ViewDataViewSpace {
        let (flags, mut views) = session
            .session
            .locate_views(
                xr::ViewConfigurationType::PRIMARY_STEREO,
                display_time,
                session.get_space_from_type(xr::ReferenceSpaceType::VIEW),
            )
            .expect("Couldn't locate views");

        let original_orientations = views
            .iter_mut()
            .map(
                |xr::View {
                     pose: xr::Posef { orientation: o, .. },
                     ..
                 }| {
                    let ret = Quat::from_xyzw(o.x, o.y, o.z, o.w).inverse();
                    *o = xr::Quaternionf::IDENTITY; // parallel views
                    ret
                },
            )
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        ViewDataViewSpace {
            data: ViewData {
                flags,
                views: views
                    .try_into()
                    .unwrap_or_else(|v: Vec<xr::View>| panic!("Expected 2 views, got {}", v.len())),
            },
            original_orientations,
        }
    }

    fn get_views_other_space(
        session: &SessionData,
        display_time: xr::Time,
        ty: xr::ReferenceSpaceType,
        view_data_orientations_inverse: [Quat; 2],
    ) -> ViewData {
        let (flags, mut views) = session
            .session
            .locate_views(
                xr::ViewConfigurationType::PRIMARY_STEREO,
                display_time,
                session.get_space_from_type(ty),
            )
            .expect("Couldn't locate views");

        for (
            xr::View {
                pose: xr::Posef {
                    orientation: rot, ..
                },
                ..
            },
            view_rot,
        ) in views.iter_mut().zip(view_data_orientations_inverse)
        {
            let quat = Quat::from_xyzw(rot.x, rot.y, rot.z, rot.w);
            // rotate the inverse of the view space view rotation by this space's
            // view orientation to remove the canting from the displays in this space
            let adjusted_rot = quat * view_rot;
            *rot = xr::Quaternionf {
                x: adjusted_rot.x,
                y: adjusted_rot.y,
                z: adjusted_rot.z,
                w: adjusted_rot.w,
            };
        }

        ViewData {
            flags,
            views: views
                .try_into()
                .unwrap_or_else(|v: Vec<xr::View>| panic!("Expected 2 views, got {}", v.len())),
        }
    }
}

#[derive(macros::InterfaceImpl)]
#[interface = "IVRSystem"]
#[versions(023, 022, 021, 020, 019, 017, 016, 015, 014, 012, 011, 009)]
pub struct System {
    openxr: Arc<RealOpenXrData>, // We don't need to test session restarting.
    input: Injected<Input<crate::compositor::Compositor>>,
    overlay: Injected<OverlayMan>,
    vtables: Vtables,
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
            overlay: injector.inject(),
            vtables: Default::default(),
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

impl vr::IVRSystem023_Interface for System {
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
    fn TriggerHapticPulse(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        axis_id: u32,
        duration_us: std::ffi::c_ushort,
    ) {
        self.input
            .force(|_| Input::new(self.openxr.clone()))
            .legacy_haptic(device_index, axis_id, duration_us);
    }
    fn GetControllerStateWithPose(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        device_index: vr::TrackedDeviceIndex_t,
        state: *mut vr::VRControllerState_t,
        state_size: u32,
        pose: *mut vr::TrackedDevicePose_t,
    ) -> bool {
        let input = self.input.force(|_| Input::new(self.openxr.clone()));

        let Some(hand) = input.device_index_to_hand(device_index) else {
            return false;
        };

        if self.GetControllerState(device_index, state, state_size) {
            unsafe {
                *pose.as_mut().unwrap() = self
                    .input
                    .get()
                    .unwrap()
                    .get_controller_pose(hand, Some(origin))
                    .unwrap_or_default();
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

    fn PollNextEventWithPoseAndOverlays(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        event: *mut vr::VREvent_t,
        size: u32,
        pose: *mut vr::TrackedDevicePose_t,
        overlay_handle: *mut vr::VROverlayHandle_t,
    ) -> bool {
        if self.PollNextEventWithPose(origin, event, size, pose) {
            return true;
        }
        let Some(overlay) = self.overlay.get() else {
            return false;
        };

        if let Some(handle) = overlay.get_next_overlay_event(event) {
            if !overlay_handle.is_null() {
                unsafe {
                    overlay_handle.write(handle);
                }
            }
            true
        } else {
            false
        }
    }

    fn PollNextEventWithPose(
        &self,
        origin: vr::ETrackingUniverseOrigin,
        event: *mut vr::VREvent_t,
        size: u32,
        pose: *mut vr::TrackedDevicePose_t,
    ) -> bool {
        let Some(input) = self.input.get() else {
            return false;
        };

        let got_event = input.get_next_event(size, event);
        if got_event && !pose.is_null() {
            unsafe {
                let index = (&raw const (*event).trackedDeviceIndex).read();
                pose.write(input.get_device_pose(index, Some(origin)).unwrap());
            }
        }
        got_event
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
                | vr::ETrackedDeviceProperty::ControllerType_String => {
                    Some(CString::new("<unknown>").unwrap())
                }
                _ => None,
            },
            _ => self
                .input
                .get()
                .and_then(|input| input.get_device_string_tracked_property(device_index, prop)),
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

        self.input
            .get()
            .and_then(|input| input.get_device_uint_tracked_property(device_index, prop))
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
        self.input
            .get()
            .and_then(|input| input.get_device_int_tracked_property(device_index, prop))
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
            vr::ETrackedDeviceProperty::DisplayFrequency_Float => self.openxr.get_refresh_rate(),
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
            _ => self
                .input
                .get()
                .is_some_and(|input| input.is_device_connected(device_index)),
        }
    }

    fn GetTrackedDeviceClass(&self, index: vr::TrackedDeviceIndex_t) -> vr::ETrackedDeviceClass {
        match index {
            vr::k_unTrackedDeviceIndex_Hmd => vr::ETrackedDeviceClass::HMD,
            _ => self
                .input
                .get()
                .and_then(|input| input.device_index_to_tracked_device_class(index))
                .unwrap_or(vr::ETrackedDeviceClass::Invalid),
        }
    }

    fn GetControllerRoleForTrackedDeviceIndex(
        &self,
        index: vr::TrackedDeviceIndex_t,
    ) -> vr::ETrackedControllerRole {
        let Some(input) = self.input.get() else {
            return vr::ETrackedControllerRole::Invalid;
        };
        input
            .device_index_to_hand(index)
            .map_or(vr::ETrackedControllerRole::Invalid, |hand| hand.into())
    }

    fn GetTrackedDeviceIndexForControllerRole(
        &self,
        role: vr::ETrackedControllerRole,
    ) -> vr::TrackedDeviceIndex_t {
        let Some(input) = self.input.get() else {
            return vr::k_unTrackedDeviceIndexInvalid;
        };

        Hand::try_from(role).map_or(vr::k_unTrackedDeviceIndexInvalid, |hand| {
            input
                .get_controller_device_index(hand)
                .unwrap_or(vr::k_unTrackedDeviceIndexInvalid)
        })
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
            x if self
                .input
                .get()
                .is_some_and(|input| input.device_index_to_hand(x).is_some()) =>
            {
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

impl vr::IVRSystem011On012 for System {
    fn PerformanceTestEnableCapture(&self, _: bool) {
        todo!()
    }

    fn PerformanceTestReportFidelityLevelChange(&self, _: i32) {
        todo!()
    }
}

impl vr::IVRSystem009On011 for System {
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
    use crate::{clientcore::Injector, openxr_data::OpenXrData};
    use std::ffi::CStr;
    use vr::IVRSystem022_Interface;

    #[test]
    fn unity_required_properties() {
        let xr = Arc::new(OpenXrData::new(&Injector::default()).unwrap());
        let injector = Injector::default();
        let input = Arc::new(Input::new(xr.clone()));
        let system = System::new(xr, &injector);

        system.input.set(Arc::downgrade(&input));

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
