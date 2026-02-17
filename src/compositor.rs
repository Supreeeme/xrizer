use crate::{
    AtomicF64,
    clientcore::{Injected, Injector},
    graphics_backends::{GraphicsBackend, SupportedBackend, supported_backends_enum},
    input::Input,
    openxr_data::{self, FrameStream, OpenXrData, SessionCreateInfo, SessionData},
    overlay::OverlayMan,
    system::System,
    tracy_span,
};

use log::{debug, info, trace, warn};
use openvr as vr;
use openxr as xr;
use std::mem::offset_of;
use std::sync::{
    Arc, Mutex, Once,
    atomic::{AtomicU32, Ordering},
};
use std::time::Instant;
use std::{ffi::c_char, ops::Deref};

#[derive(Default)]
pub struct CompositorSessionData(Mutex<Option<DynFrameController>>);

#[derive(macros::InterfaceImpl)]
#[interface = "IVRCompositor"]
#[versions(
    029, 028, 027, 026, 022, 021, 020, 019, 018, 016, 015, 014, 012, 011, 009
)]
pub struct Compositor {
    vtables: Vtables,
    openxr: Arc<OpenXrData<Self>>,
    input: Injected<Input<Self>>,
    system: Injected<System>,
    /// Stores the backend data in between session restarts.
    tmp_backend: Mutex<Option<AnyTempBackendData>>,
    overlays: Injected<OverlayMan>,
    metrics: FrameMetrics,
    timing_mode: Mutex<vr::EVRCompositorTimingMode>,
    frame_state: Mutex<FrameState>,
    focused: Once,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FrameState {
    Waited,
    Begun,
    Submitted,
}

impl FrameState {
    fn advance_to(&mut self, new: Self) -> bool {
        let old = *self;
        let allowed: bool = match old {
            Self::Waited => new == Self::Begun,
            Self::Begun => new != Self::Begun,
            Self::Submitted => new == Self::Waited,
        };
        if allowed {
            *self = new;
            trace!("advanced frame state from {old:?} to {new:?}");
        } else {
            warn!("Tried to advance frame state from {old:?} to {new:?}, which is not allowed");
        }
        allowed
    }
}

struct FrameMetrics {
    system_start: Instant,
    index: AtomicU32,
    time: AtomicF64,
}

struct TempBackendData<G: GraphicsBackend> {
    backend: G,
    swapchain_create_info: Option<xr::SwapchainCreateInfo<G::Api>>,
}
supported_backends_enum!(enum AnyTempBackendData: TempBackendData);

impl Compositor {
    pub fn new(openxr: Arc<OpenXrData<Self>>, injector: &Injector) -> Self {
        Self {
            vtables: Default::default(),
            openxr,
            input: injector.inject(),
            system: injector.inject(),
            tmp_backend: Mutex::default(),
            overlays: injector.inject(),
            metrics: FrameMetrics {
                system_start: Instant::now(),
                index: 0.into(),
                time: 0.0.into(),
            },
            timing_mode: vr::EVRCompositorTimingMode::Implicit.into(),
            frame_state: FrameState::Submitted.into(),
            focused: Once::new(),
        }
    }

    fn maybe_wait_frame(&self, session_data: &SessionData) {
        tracy_span!();
        let mut frame_lock = { session_data.comp_data.0.lock().unwrap() };
        self.frame_state
            .lock()
            .unwrap()
            .advance_to(FrameState::Waited);
        let Some(ctrl) = frame_lock.as_mut() else {
            debug!("no frame controller - not starting frame");
            return;
        };

        #[macros::any_graphics(DynFrameController)]
        fn wait_frame<G: GraphicsBackend + 'static>(
            ctrl: &mut FrameController<G>,
        ) -> (xr::Time, i64) {
            ctrl.wait_frame()
        }

        let (display_time, display_period) = ctrl.with_any_graphics_mut::<wait_frame>(());
        self.openxr.display_time.set(display_time);
        self.openxr
            .display_period_nanos
            .store(display_period, Ordering::Relaxed);
    }

    fn maybe_begin_frame(&self, session_data: &SessionData) {
        tracy_span!();
        let mut frame_lock = { session_data.comp_data.0.lock().unwrap() };
        if !self
            .frame_state
            .lock()
            .unwrap()
            .advance_to(FrameState::Begun)
        {
            debug!("not starting frame - already begun or submitted");
            return;
        };
        let Some(ctrl) = frame_lock.as_mut() else {
            debug!("no frame controller - not starting frame");
            return;
        };

        #[macros::any_graphics(DynFrameController)]
        fn begin_frame<G: GraphicsBackend + 'static>(ctrl: &mut FrameController<G>) {
            ctrl.begin_frame()
        }

        ctrl.with_any_graphics_mut::<begin_frame>(());
    }

    pub fn initialize_real_session(
        &self,
        texture: &vr::Texture_t,
        bounds: vr::VRTextureBounds_t,
    ) -> Result<(), vr::EVRCompositorError> {
        let backend =
            SupportedBackend::new(texture, bounds).ok_or(vr::EVRCompositorError::InvalidTexture)?;

        #[macros::any_graphics(SupportedBackend)]
        fn swapchain_info<G: GraphicsBackend>(
            backend: G,
            texture: &vr::Texture_t,
            bounds: vr::VRTextureBounds_t,
        ) -> Result<AnyTempBackendData, vr::EVRCompositorError>
        where
            AnyTempBackendData: From<TempBackendData<G>>,
        {
            let b_texture =
                G::get_texture(texture).ok_or(vr::EVRCompositorError::InvalidTexture)?;
            let info = backend.swapchain_info_for_texture(b_texture, bounds, texture.eColorSpace);
            Ok(TempBackendData {
                backend,
                swapchain_create_info: Some(info),
            }
            .into())
        }

        let tmp_backend = backend
            .with_any_graphics_owned::<swapchain_info>((texture, bounds))
            .inspect_err(|_| debug!("received invalid texture handle, not restarting session"))?;
        *self.tmp_backend.lock().unwrap() = Some(tmp_backend);

        info!("Creating real backend for texture type {:?}", texture.eType);
        self.openxr.restart_session();
        Ok(())
    }
}

fn fill_vk_extensions_buffer(extensions: String, buffer: *mut c_char, buffer_size: u32) -> u32 {
    let bytes = unsafe {
        std::slice::from_raw_parts(extensions.as_ptr() as *const c_char, extensions.len())
    };

    if !buffer.is_null() && buffer_size as usize > bytes.len() {
        let buffer = unsafe { std::slice::from_raw_parts_mut(buffer, buffer_size as usize) };
        buffer[0..bytes.len()].copy_from_slice(bytes);
        buffer[bytes.len()] = 0;
    }

    bytes.len() as u32 + 1
}

impl openxr_data::Compositor for Compositor {
    fn get_session_create_info(&self, data: CompositorSessionData) -> SessionCreateInfo {
        #[macros::any_graphics(AnyTempBackendData)]
        fn info<G: GraphicsBackend>(data: &TempBackendData<G>) -> SessionCreateInfo
        where
            SessionCreateInfo: From<openxr_data::CreateInfo<G::Api>>,
        {
            SessionCreateInfo::from_info::<G::Api>(data.backend.session_create_info())
        }

        self.tmp_backend
            .lock()
            .unwrap()
            .get_or_insert_with(|| {
                #[macros::any_graphics(DynFrameController)]
                fn take<G: GraphicsBackend>(ctrl: FrameController<G>) -> AnyTempBackendData
                where
                    AnyTempBackendData: From<TempBackendData<G>>,
                {
                    TempBackendData {
                        backend: ctrl.backend,
                        swapchain_create_info: ctrl.swapchain_data.map(|d| d.info),
                    }
                    .into()
                }
                data.0
                    .lock()
                    .unwrap()
                    .take()
                    .expect("one of tmp backend or frame controller should be setup")
                    .with_any_graphics_owned::<take>(())
            })
            .with_any_graphics::<info>(())
    }

    fn post_session_restart(
        &self,
        session_data: &SessionData,
        waiter: xr::FrameWaiter,
        stream: FrameStream,
    ) {
        // This function is called while a write lock is called on the session, and as such should
        // not use self.openxr.session_data.get().

        let backend_data = self
            .tmp_backend
            .lock()
            .unwrap()
            .take()
            .expect("tmp backend should be set before post_session_restart is called");

        #[macros::any_graphics(AnyTempBackendData)]
        fn new_frame_controller<G: GraphicsBackend + 'static>(
            data: TempBackendData<G>,
            session_data: &SessionData,
            waiter: xr::FrameWaiter,
            stream: FrameStream,
        ) -> DynFrameController
        where
            for<'a> &'a openxr_data::GraphicalSession:
                TryInto<&'a openxr_data::Session<G::Api>, Error: std::fmt::Display>,
            FrameStream: TryInto<xr::FrameStream<G::Api>>,
            DynFrameController: From<FrameController<G>>,
            <G::Api as xr::Graphics>::Format: PartialEq + std::fmt::Debug,
        {
            FrameController::new(
                session_data,
                waiter,
                stream.try_into().unwrap_or_else(|_| unreachable!()),
                data.backend,
                data.swapchain_create_info,
            )
            .into()
        }

        *session_data.comp_data.0.lock().unwrap() = Some(
            backend_data.with_any_graphics_owned::<new_frame_controller>((
                session_data,
                waiter,
                stream,
            )),
        );

        let old_state = std::mem::replace(
            &mut *self.frame_state.lock().unwrap(),
            FrameState::Submitted,
        );

        trace!("returning to {old_state:?} frame state");
        match old_state {
            FrameState::Submitted => {}
            FrameState::Waited => self.maybe_wait_frame(session_data),
            FrameState::Begun => {
                self.maybe_wait_frame(session_data);
                self.maybe_begin_frame(session_data);
            }
        }
    }
}

#[allow(non_snake_case)]
impl vr::IVRCompositor029_Interface for Compositor {
    fn GetPosesForFrame(
        &self,
        _unPosePredictionID: u32,
        _pPoseArray: *mut vr::TrackedDevicePose_t,
        _unPoseArrayCount: u32,
    ) -> vr::EVRCompositorError {
        todo!()
    }
    fn GetLastPosePredictionIDs(
        &self,
        _pRenderPosePredictionID: *mut u32,
        _pGamePosePredictionID: *mut u32,
    ) -> vr::EVRCompositorError {
        crate::warn_unimplemented!("GetLastPosePredictionIDs");
        vr::EVRCompositorError::None
    }
    fn GetCompositorBenchmarkResults(
        &self,
        _pBenchmarkResults: *mut vr::Compositor_BenchmarkResults,
        _nSizeOfBenchmarkResults: u32,
    ) -> bool {
        crate::warn_unimplemented!("GetCompositorBenchmarkResults");
        false
    }
    fn ClearStageOverride(&self) {}
    fn SetStageOverride_Async(
        &self,
        _pchRenderModelPath: *const std::ffi::c_char,
        _pTransform: *const vr::HmdMatrix34_t,
        _pRenderSettings: *const vr::Compositor_StageRenderSettings,
        _nSizeOfRenderSettings: u32,
    ) -> vr::EVRCompositorError {
        crate::warn_unimplemented!("SetStageOverride_Async");
        vr::EVRCompositorError::None
    }
    fn IsCurrentSceneFocusAppLoading(&self) -> bool {
        false
    }
    fn IsMotionSmoothingSupported(&self) -> bool {
        todo!()
    }
    fn IsMotionSmoothingEnabled(&self) -> bool {
        todo!()
    }
    fn SubmitExplicitTimingData(&self) -> vr::EVRCompositorError {
        if *self.timing_mode.lock().unwrap() == vr::EVRCompositorTimingMode::Implicit {
            return vr::EVRCompositorError::RequestFailed;
        }

        if !self.focused.is_completed() {
            // SteamVR doesn't seem to return an error in this case...
            return vr::EVRCompositorError::None;
        }

        let session_data = self.openxr.session_data.get();
        self.maybe_begin_frame(&session_data);
        vr::EVRCompositorError::None
    }
    fn SetExplicitTimingMode(&self, timing_mode: vr::EVRCompositorTimingMode) {
        debug!("Setting timing mode to {timing_mode:?}");
        *self.timing_mode.lock().unwrap() = timing_mode;
    }

    fn GetVulkanDeviceExtensionsRequired(
        &self,
        _physical_device: *mut vr::VkPhysicalDevice_T,
        buffer: *mut std::ffi::c_char,
        buffer_size: u32,
    ) -> u32 {
        let exts = self
            .openxr
            .instance
            .vulkan_legacy_device_extensions(self.openxr.system_id)
            .unwrap();
        log::debug!("required device extensions: {exts}");
        fill_vk_extensions_buffer(exts, buffer, buffer_size)
    }

    fn GetVulkanInstanceExtensionsRequired(
        &self,
        buffer: *mut std::ffi::c_char,
        buffer_size: u32,
    ) -> u32 {
        let exts = self
            .openxr
            .instance
            .vulkan_legacy_instance_extensions(self.openxr.system_id)
            .unwrap();
        log::debug!("required instance extensions: {exts}");
        fill_vk_extensions_buffer(exts, buffer, buffer_size)
    }

    fn UnlockGLSharedTextureForAccess(&self, _glSharedTextureHandle: vr::glSharedTextureHandle_t) {
        todo!()
    }
    fn LockGLSharedTextureForAccess(&self, _glSharedTextureHandle: vr::glSharedTextureHandle_t) {
        todo!()
    }
    fn ReleaseSharedGLTexture(
        &self,
        _glTextureId: vr::glUInt_t,
        _glSharedTextureHandle: vr::glSharedTextureHandle_t,
    ) -> bool {
        todo!()
    }
    fn GetMirrorTextureGL(
        &self,
        _eEye: vr::EVREye,
        _pglTextureId: *mut vr::glUInt_t,
        _pglSharedTextureHandle: *mut vr::glSharedTextureHandle_t,
    ) -> vr::EVRCompositorError {
        todo!()
    }
    fn ReleaseMirrorTextureD3D11(&self, _pD3D11ShaderResourceView: *mut std::ffi::c_void) {
        todo!()
    }
    fn GetMirrorTextureD3D11(
        &self,
        _eEye: vr::EVREye,
        _pD3D11DeviceOrResource: *mut std::ffi::c_void,
        _ppD3D11ShaderResourceView: *mut *mut std::ffi::c_void,
    ) -> vr::EVRCompositorError {
        crate::warn_unimplemented!("GetMirrorTextureD3D11");
        vr::EVRCompositorError::IncompatibleVersion
    }
    fn SuspendRendering(&self, bSuspend: bool) {
        #[macros::any_graphics(DynFrameController)]
        fn set_suspend_render<G: GraphicsBackend + 'static>(
            ctrl: &mut FrameController<G>,
            app_suspend_render: bool,
        ) {
            ctrl.app_suspend_render = app_suspend_render;
        }

        self.openxr
            .session_data
            .get()
            .comp_data
            .0
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|ctrl| ctrl.with_any_graphics_mut::<set_suspend_render>(bSuspend));
    }
    fn ForceReconnectProcess(&self) {
        todo!()
    }
    fn ForceInterleavedReprojectionOn(&self, _: bool) {
        crate::warn_unimplemented!("ForceInterleavedReprojectionOn");
    }
    fn ShouldAppRenderWithLowResources(&self) -> bool {
        // TODO
        false
    }
    fn CompositorDumpImages(&self) {
        todo!()
    }
    fn IsMirrorWindowVisible(&self) -> bool {
        todo!()
    }
    fn HideMirrorWindow(&self) {
        todo!()
    }
    fn ShowMirrorWindow(&self) {
        todo!()
    }
    fn CanRenderScene(&self) -> bool {
        true
    }
    fn GetLastFrameRenderer(&self) -> u32 {
        todo!()
    }
    fn GetCurrentSceneFocusProcess(&self) -> u32 {
        todo!()
    }
    fn IsFullscreen(&self) -> bool {
        true
    }
    fn CompositorQuit(&self) {
        todo!()
    }
    fn CompositorGoToBack(&self) {
        crate::warn_unimplemented!("CompositorGoToBack");
    }
    fn CompositorBringToFront(&self) {
        crate::warn_unimplemented!("CompositorBringToFront");
    }
    fn ClearSkyboxOverride(&self) {
        if let Some(overlays) = self.overlays.get() {
            overlays.clear_skybox();
        }
    }
    fn SetSkyboxOverride(
        &self,
        pTextures: *const vr::Texture_t,
        unTextureCount: u32,
    ) -> vr::EVRCompositorError {
        let overlays = self
            .overlays
            .force(|injector| OverlayMan::new(self.openxr.clone(), injector));
        if pTextures.is_null() {
            return vr::EVRCompositorError::RequestFailed;
        }
        match unTextureCount {
            1..=2 => {
                if !self
                    .openxr
                    .enabled_extensions
                    .khr_composition_layer_equirect2
                {
                    log::info!("Could not set skybox: khr_composition_layer_equirect2 unsupported");
                    return vr::EVRCompositorError::None;
                }
                log::debug!("Setting new equirect skybox");
            }
            6 => {
                log::debug!("Setting new box skybox");
            }
            _ => {
                log::warn!("Invalid number of skybox textures: {unTextureCount}");
                return vr::EVRCompositorError::RequestFailed;
            }
        }

        let textures = unsafe { std::slice::from_raw_parts(pTextures, unTextureCount as _) };
        if let Err(e) = overlays.set_skybox(textures) {
            e
        } else {
            vr::EVRCompositorError::None
        }
    }
    fn GetCurrentGridAlpha(&self) -> f32 {
        0.0
    }
    fn FadeGrid(&self, _fSeconds: f32, bFadeGridIn: bool) {
        #[macros::any_graphics(DynFrameController)]
        fn set_fade_grid<G: GraphicsBackend + 'static>(
            ctrl: &mut FrameController<G>,
            app_fade_grid: bool,
        ) {
            ctrl.app_fade_grid = app_fade_grid;
        }

        self.openxr
            .session_data
            .get()
            .comp_data
            .0
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|ctrl| ctrl.with_any_graphics_mut::<set_fade_grid>(bFadeGridIn));
    }
    fn GetCurrentFadeColor(&self, _bBackground: bool) -> vr::HmdColor_t {
        todo!()
    }
    fn FadeToColor(
        &self,
        _fSeconds: f32,
        _fRed: f32,
        _fGreen: f32,
        _fBlue: f32,
        _fAlpha: f32,
        _bBackground: bool,
    ) {
        crate::warn_unimplemented!("FadeToColor");
    }
    fn GetCumulativeStats(
        &self,
        _pStats: *mut vr::Compositor_CumulativeStats,
        _nStatsSizeInBytes: u32,
    ) {
        todo!()
    }
    fn GetFrameTimeRemaining(&self) -> f32 {
        crate::warn_unimplemented!("GetFrameTimeRemaining");
        0.0
    }
    fn GetFrameTimings(&self, _pTiming: *mut vr::Compositor_FrameTiming, _nFrames: u32) -> u32 {
        todo!()
    }
    fn GetFrameTiming(&self, timing: *mut vr::Compositor_FrameTiming, _frames_ago: u32) -> bool {
        if timing.is_null() || !timing.is_aligned() {
            return false;
        }

        let size = unsafe { (&raw const (*timing).m_nSize).read() } as usize;
        fn ptr_size<T>(_: *const T) -> usize {
            std::mem::size_of::<T>()
        }
        if size
            < offset_of!(vr::Compositor_FrameTiming, m_HmdPose)
                + ptr_size(unsafe { &raw const (*timing).m_HmdPose })
        {
            return false;
        }
        // We're using raw pointers here because the Compositor_FrameTiming struct can be a
        // varaible size, so we don't want to create a reference to a struct with an incorrect
        // (to us) size, because that would be Undefined Behavior.
        macro_rules! set {
            ($member:ident, $value:expr) => {{
                let ptr = &raw mut (*timing).$member;
                ptr.write_unaligned($value)
            }};
        }

        unsafe {
            // TODO: These values are copy/pasted from OpenComposite, determine if real values are
            // necessary/better
            set!(m_nFrameIndex, self.metrics.index.load(Ordering::Relaxed));
            set!(m_nNumFramePresents, 1);
            set!(m_nNumMisPresented, 0);
            set!(m_nReprojectionFlags, 0);
            set!(m_flSystemTimeInSeconds, self.metrics.time.load());
            set!(m_flPreSubmitGpuMs, 8.0);
            set!(m_flPostSubmitGpuMs, 1.0);
            set!(m_flTotalRenderGpuMs, 9.0);

            set!(m_flCompositorRenderGpuMs, 1.5);
            set!(m_flCompositorRenderCpuMs, 3.0);
            set!(m_flCompositorIdleCpuMs, 0.1);

            set!(
                m_flClientFrameIntervalMs,
                1000.0 / self.openxr.get_refresh_rate()
            );
            set!(m_flPresentCallCpuMs, 0.0);
            set!(m_flWaitForPresentCpuMs, 0.0);
            set!(m_flSubmitFrameMs, 0.0);

            set!(m_flWaitGetPosesCalledMs, 0.0);
            set!(m_flNewPosesReadyMs, 0.0);
            set!(m_flNewFrameReadyMs, 0.0); // second call to IVRCompositor::Submit
            set!(m_flCompositorUpdateStartMs, 0.0);
            set!(m_flCompositorUpdateEndMs, 0.0);
            set!(m_flCompositorRenderStartMs, 0.0);
        }

        true
    }
    fn PostPresentHandoff(&self) {
        #[macros::any_graphics(DynFrameController)]
        fn end_frame<G: GraphicsBackend + 'static>(
            ctrl: &mut FrameController<G>,
            session_data: &SessionData,
            system: &System,
            display_time: xr::Time,
            overlays: Option<&OverlayMan>,
        ) where
            for<'b> &'b crate::overlay::AnySwapchainMap:
                TryInto<&'b crate::overlay::SwapchainMap<G::Api>, Error: std::fmt::Display>,
        {
            ctrl.end_frame(session_data, system, display_time, overlays)
        }

        let session_data = self.openxr.session_data.get();
        let mut frame_lock = session_data.comp_data.0.lock().unwrap();
        let Some(ctrl) = frame_lock.as_mut() else {
            debug!("no frame controller - not presenting frame");
            return;
        };

        if *self.frame_state.lock().unwrap() != FrameState::Begun {
            return;
        }

        trace!("presenting frame");
        let system = self.system.force(|i| System::new(self.openxr.clone(), i));
        let display_time = self.openxr.display_time.get();
        let overlays = self.overlays.get();

        ctrl.with_any_graphics_mut::<end_frame>((
            &session_data,
            &system,
            display_time,
            overlays.as_deref(),
        ));

        self.frame_state
            .lock()
            .unwrap()
            .advance_to(FrameState::Submitted);

        self.metrics.index.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .time
            .store(self.metrics.system_start.elapsed().as_secs_f64());
        #[cfg(feature = "tracing")]
        {
            tracy_client::frame_mark();
        }
    }
    fn ClearLastSubmittedFrame(&self) {
        crate::warn_unimplemented!("ClearLastSubmittedFrame");
    }
    fn SubmitWithArrayIndex(
        &self,
        _eEye: vr::EVREye,
        _pTexture: *const vr::Texture_t,
        _unTextureArrayIndex: u32,
        _pBounds: *const vr::VRTextureBounds_t,
        _nSubmitFlags: vr::EVRSubmitFlags,
    ) -> vr::EVRCompositorError {
        todo!()
    }

    fn GetSubmitTexture(
        &self,
        _: *mut vr::Texture_t,
        __bindgen_vtable: *mut bool,
        _: vr::EVRCompositorTextureUsage,
        _: *const vr::Texture_t,
        _: *const vr::VRTextureBounds_t,
        _: vr::EVRSubmitFlags,
    ) -> vr::EVRCompositorError {
        crate::warn_unimplemented!("GetSubmitTexture");
        vr::EVRCompositorError::IncompatibleVersion
    }

    fn Submit(
        &self,
        eye: vr::EVREye,
        texture: *const vr::Texture_t,
        bounds: *const vr::VRTextureBounds_t,
        submit_flags: vr::EVRSubmitFlags,
    ) -> vr::EVRCompositorError {
        let bounds = unsafe { bounds.as_ref() }
            .copied()
            .unwrap_or(vr::VRTextureBounds_t {
                uMin: 0.0,
                vMin: 0.0,
                uMax: 1.0,
                vMax: 1.0,
            });

        // Superhot passes crazy bounds on startup.
        if !bounds.valid() {
            return vr::EVRCompositorError::InvalidBounds;
        }

        let Some(texture) = (unsafe { texture.as_ref() }) else {
            return vr::EVRCompositorError::InvalidTexture;
        };

        if !self.focused.is_completed() {
            return vr::EVRCompositorError::DoNotHaveFocus;
        }

        let mut session_lock = self.openxr.session_data.get();
        let mut frame_lock = session_lock.comp_data.0.lock().unwrap();

        let ctrl = match frame_lock.as_mut() {
            Some(ctrl) => ctrl,
            None => {
                drop(frame_lock);
                drop(session_lock);

                if let Err(e) = self.initialize_real_session(texture, bounds) {
                    return e;
                }
                info!("Received game texture, restarted session with new data");

                session_lock = self.openxr.session_data.get();
                frame_lock = session_lock.comp_data.0.lock().unwrap();
                frame_lock.as_mut().unwrap()
            }
        };

        #[macros::any_graphics(DynFrameController)]
        fn submit<G: GraphicsBackend + 'static>(
            ctrl: &mut FrameController<G>,
            session_data: &SessionData,
            eye: vr::EVREye,
            texture: &vr::Texture_t,
            bounds: vr::VRTextureBounds_t,
            flags: vr::EVRSubmitFlags,
        ) -> xr::Result<(), vr::EVRCompositorError>
        where
            for<'d> &'d openxr_data::GraphicalSession:
                TryInto<&'d openxr_data::Session<G::Api>, Error: std::fmt::Display>,
            <G::Api as xr::Graphics>::Format: Eq + std::fmt::Debug,
        {
            let real_texture =
                G::get_texture(texture).ok_or(vr::EVRCompositorError::InvalidTexture)?;
            ctrl.submit_impl(
                session_data,
                eye,
                real_texture,
                texture.eColorSpace,
                bounds,
                flags,
            )
        }

        if let Err(e) = ctrl.with_any_graphics_mut::<submit>((
            &session_lock,
            eye,
            texture,
            bounds,
            submit_flags,
        )) {
            return e;
        }
        vr::EVRCompositorError::None
    }

    fn GetLastPoseForTrackedDeviceIndex(
        &self,
        device_index: vr::TrackedDeviceIndex_t,
        output_pose: *mut vr::TrackedDevicePose_t,
        output_game_pose: *mut vr::TrackedDevicePose_t,
    ) -> vr::EVRCompositorError {
        if output_pose.is_null() && output_game_pose.is_null() {
            return vr::EVRCompositorError::RequestFailed;
        }

        let input = self.input.force(|_| Input::new(self.openxr.clone()));

        let Some(pose) = input.get_device_pose(device_index, None) else {
            return vr::EVRCompositorError::RequestFailed;
        };

        if !output_pose.is_null() {
            unsafe { output_pose.write(pose) };
        }

        if !output_game_pose.is_null() {
            unsafe { output_game_pose.write(pose) };
        }

        vr::EVRCompositorError::None
    }
    fn GetLastPoses(
        &self,
        render_pose_array: *mut vr::TrackedDevicePose_t,
        render_pose_count: u32,
        game_pose_array: *mut vr::TrackedDevicePose_t,
        game_pose_count: u32,
    ) -> vr::EVRCompositorError {
        tracy_span!("GetLastPoses impl");
        if render_pose_count == 0 {
            return vr::EVRCompositorError::None;
        }
        let render_poses = unsafe {
            std::slice::from_raw_parts_mut(render_pose_array, render_pose_count as usize)
        };
        self.input
            .force(|_| Input::new(self.openxr.clone()))
            .get_poses(render_poses, None);

        // Not entirely sure how the game poses are supposed to differ from the render poses,
        // but a lot of games use the game pose array for controller positions.
        if game_pose_count > 0 {
            let game_poses = unsafe {
                std::slice::from_raw_parts_mut(game_pose_array, game_pose_count as usize)
            };
            assert!(game_poses.len() <= render_poses.len());
            game_poses.copy_from_slice(&render_poses[0..game_poses.len()]);
        }

        vr::EVRCompositorError::None
    }

    fn WaitGetPoses(
        &self,
        render_pose_array: *mut vr::TrackedDevicePose_t,
        render_pose_count: u32,
        game_pose_array: *mut vr::TrackedDevicePose_t,
        game_pose_count: u32,
    ) -> vr::EVRCompositorError {
        tracy_span!("WaitGetPoses impl");
        // This should be called every frame - we must regularly poll events
        self.openxr.poll_events();
        self.focused.call_once(|| {});
        {
            let session_data = self.openxr.session_data.get();
            let timing_mode = *self.timing_mode.lock().unwrap();
            if matches!(
                timing_mode,
                vr::EVRCompositorTimingMode::Implicit
                    | vr::EVRCompositorTimingMode::Explicit_RuntimePerformsPostPresentHandoff
            ) && *self.frame_state.lock().unwrap() == FrameState::Begun
            {
                self.PostPresentHandoff();
            }

            if *self.frame_state.lock().unwrap() == FrameState::Waited {
                // discard frame
                self.maybe_begin_frame(&session_data);
            }
            self.maybe_wait_frame(&session_data);

            if timing_mode == vr::EVRCompositorTimingMode::Implicit {
                self.maybe_begin_frame(&session_data);
            }
        }
        if let Some(system) = self.system.get() {
            system.reset_views();
        }
        if let Some(input) = self.input.get() {
            input.frame_start_update();
        }

        self.GetLastPoses(
            render_pose_array,
            render_pose_count,
            game_pose_array,
            game_pose_count,
        )
    }

    fn GetTrackingSpace(&self) -> vr::ETrackingUniverseOrigin {
        self.openxr.get_tracking_space()
    }

    fn SetTrackingSpace(&self, origin: vr::ETrackingUniverseOrigin) {
        self.openxr.set_tracking_space(origin);
    }
}

impl vr::IVRCompositor021On022 for Compositor {
    fn SetExplicitTimingMode(&self, explicit: bool) {
        let mode = if explicit {
            vr::EVRCompositorTimingMode::Explicit_ApplicationPerformsPostPresentHandoff
        } else {
            vr::EVRCompositorTimingMode::Implicit
        };

        <Self as vr::IVRCompositor028_Interface>::SetExplicitTimingMode(self, mode);
    }
}

impl vr::IVRCompositor016On018 for Compositor {
    fn GetFrameTiming(
        &self,
        _timing: *mut vr::vr_1_0_3::Compositor_FrameTiming,
        _frames_ago: u32,
    ) -> bool {
        crate::warn_unimplemented!("GetFrameTiming (v1.0.3)");
        false
    }
}

impl vr::IVRCompositor015On016 for Compositor {
    fn GetFrameTiming(
        &self,
        _timing: *mut openvr::vr_1_0_1::Compositor_FrameTiming,
        _frames_ago: u32,
    ) -> bool {
        crate::warn_unimplemented!("GetFrameTiming (v1.0.1)");
        false
    }
    fn RequestScreenshot(
        &self,
        _screenshot_type: vr::EVRScreenshotType,
        _dest_file_name: *const std::ffi::c_char,
        _vr_dest_file_name: *const std::ffi::c_char,
    ) -> vr::EVRCompositorError {
        todo!("RequestScreenshot (v1.0.1)");
    }
    fn GetCurrentScreenshotType(&self) -> vr::EVRScreenshotType {
        todo!("GetCurrentScreenshotType (v1.0.1)");
    }
}

impl vr::IVRCompositor014On015 for Compositor {
    fn GetFrameTiming(
        &self,
        _timing: *mut vr::vr_0_9_20::Compositor_FrameTiming,
        _frames_ago: u32,
    ) -> bool {
        crate::warn_unimplemented!("GetFrameTiming (v0.9.20)");
        false
    }
}

impl vr::IVRCompositor012On014 for Compositor {
    fn GetFrameTiming(
        &self,
        _timing: *mut openvr::vr_0_9_17::Compositor_FrameTiming,
        _frames_ago: u32,
    ) -> bool {
        crate::warn_unimplemented!("GetFrameTiming (v0.9.17)");
        false
    }
}

impl vr::IVRCompositor011On012 for Compositor {
    fn GetFrameTiming(
        &self,
        _timing: *mut openvr::vr_0_9_15::Compositor_FrameTiming,
        _frames_ago: u32,
    ) -> bool {
        crate::warn_unimplemented!("GetFrameTiming (v0.9.15)");
        false
    }
}

impl vr::IVRCompositor009On011 for Compositor {
    fn GetFrameTiming(
        &self,
        _timing: *mut vr::vr_0_9_12::Compositor_FrameTiming,
        _frames_ago: u32,
    ) -> bool {
        crate::warn_unimplemented!("GetFrameTiming (v0.9.12)");
        false
    }
}

#[derive(Copy, Clone, Default)]
struct SubmittedEye {
    extent: xr::Extent2Di,
    flip_vertically: bool,
}

struct SwapchainData<G: xr::Graphics> {
    swapchain: xr::Swapchain<G>,
    info: xr::SwapchainCreateInfo<G>,
    initial_format: G::Format,
}

struct FrameController<G: GraphicsBackend> {
    stream: xr::FrameStream<G::Api>,
    waiter: xr::FrameWaiter,
    swapchain_data: Option<SwapchainData<G::Api>>,
    image_index: usize,
    image_acquired: bool,
    should_render: bool,
    app_suspend_render: bool,
    app_fade_grid: bool,
    eyes_submitted: [Option<SubmittedEye>; 2],
    submitting_null: bool,
    backend: G,
}
supported_backends_enum!(enum DynFrameController: FrameController);

impl<G: GraphicsBackend> FrameController<G> {
    fn init_swapchain(
        session_data: &SessionData,
        create_info: &mut xr::SwapchainCreateInfo<G::Api>,
        backend: &mut G,
    ) -> (xr::Swapchain<G::Api>, <G::Api as xr::Graphics>::Format)
    where
        for<'a> &'a openxr_data::GraphicalSession:
            TryInto<&'a openxr_data::Session<G::Api>, Error: std::fmt::Display>,
        <G::Api as xr::Graphics>::Format: PartialEq + std::fmt::Debug,
    {
        assert!(
            is_valid_swapchain_info(create_info),
            "Recreating swapchain with invalid dimensions {}x{}",
            create_info.width,
            create_info.height
        );

        let initial_format = create_info.format;
        session_data.check_format::<G>(create_info);

        let swapchain = session_data
            .create_swapchain(create_info)
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to create swapchain: {err} (info: {:#?})",
                    [
                        ("create_flags", format!("{:?}", create_info.create_flags)),
                        ("width", create_info.width.to_string()),
                        ("height", create_info.height.to_string()),
                        ("sample_count", create_info.sample_count.to_string())
                    ]
                )
            });

        let images = swapchain
            .enumerate_images()
            .expect("Failed to enumerate swapchain images");

        backend.store_swapchain_images(images, create_info.format);
        debug!(
            "Created new swapchain: {}x{}, format = {:?}",
            create_info.width, create_info.height, create_info.format
        );

        (swapchain, initial_format)
    }

    fn new(
        session_data: &SessionData,
        waiter: xr::FrameWaiter,
        stream: xr::FrameStream<G::Api>,
        mut backend: G,
        create_info: Option<xr::SwapchainCreateInfo<G::Api>>,
    ) -> Self
    where
        for<'a> &'a openxr_data::GraphicalSession:
            TryInto<&'a openxr_data::Session<G::Api>, Error: std::fmt::Display>,
        <G::Api as xr::Graphics>::Format: PartialEq + std::fmt::Debug,
    {
        let swapchain_data = if let Some(mut info) = create_info {
            is_valid_swapchain_info(&info).then(|| {
                let (swapchain, initial_format) =
                    Self::init_swapchain(session_data, &mut info, &mut backend);
                SwapchainData {
                    swapchain,
                    info,
                    initial_format,
                }
            })
        } else {
            None
        };

        Self {
            stream,
            waiter,
            swapchain_data,
            image_index: 0,
            image_acquired: false,
            should_render: false,
            app_suspend_render: false,
            app_fade_grid: false,
            eyes_submitted: Default::default(),
            submitting_null: false,
            backend,
        }
    }

    fn recreate_swapchain(
        &mut self,
        session_data: &SessionData,
        mut create_info: xr::SwapchainCreateInfo<G::Api>,
    ) where
        for<'a> &'a openxr_data::GraphicalSession:
            TryInto<&'a openxr_data::Session<G::Api>, Error: std::fmt::Display>,
        <G::Api as xr::Graphics>::Format: PartialEq + std::fmt::Debug,
    {
        let (swapchain, initial_format) =
            Self::init_swapchain(session_data, &mut create_info, &mut self.backend);

        self.swapchain_data = Some(SwapchainData {
            swapchain,
            info: create_info,
            initial_format,
        });
        self.acquire_swapchain_image();
        self.eyes_submitted = Default::default();
    }

    fn acquire_swapchain_image(&mut self) {
        let swapchain = &mut self
            .swapchain_data
            .as_mut()
            .expect("Can't acquire swapchain image with no swapchain!")
            .swapchain;

        self.image_index = swapchain
            .acquire_image()
            .expect("Failed to acquire swapchain image") as usize;

        trace!("waiting image");
        {
            tracy_span!("wait swapchain image");
            swapchain
                .wait_image(xr::Duration::INFINITE)
                .expect("Failed to wait for swapchain image");
        }

        self.image_acquired = true;
    }

    fn wait_frame(&mut self) -> (xr::Time, i64) {
        let frame_state = {
            tracy_span!("wait frame");
            self.waiter.wait().unwrap()
        };
        self.should_render = frame_state.should_render && !self.app_suspend_render;
        (
            frame_state.predicted_display_time,
            frame_state.predicted_display_period.as_nanos(),
        )
    }

    fn begin_frame(&mut self) {
        if self.image_acquired {
            tracy_span!("release old swapchain image");
            self.swapchain_data
                .as_mut()
                .expect("Image is acquired, yet we have no swapchain?")
                .swapchain
                .release_image()
                .unwrap();
        }

        if self.swapchain_data.is_some() {
            self.acquire_swapchain_image();
        }

        {
            tracy_span!("begin frame");
            self.stream.begin().expect("Couldn't begin frame");
        }
        self.eyes_submitted = [None; 2];
        self.submitting_null = false;
        trace!("frame begin");
    }

    fn submit_impl(
        &mut self,
        session_data: &SessionData,
        eye: vr::EVREye,
        texture: G::OpenVrTexture,
        color_space: vr::EColorSpace,
        bounds: vr::VRTextureBounds_t,
        submit_flags: vr::EVRSubmitFlags,
    ) -> Result<(), vr::EVRCompositorError>
    where
        <G::Api as xr::Graphics>::Format: Eq,
        for<'b> &'b openxr_data::GraphicalSession:
            TryInto<&'b openxr_data::Session<G::Api>, Error: std::fmt::Display>,
        <G::Api as xr::Graphics>::Format: PartialEq + std::fmt::Debug,
    {
        // No Man's Sky does this.
        if self.eyes_submitted[eye as usize].is_some() {
            return Err(vr::EVRCompositorError::AlreadySubmitted);
        }

        self.eyes_submitted[eye as usize] = if self.should_render {
            // Make sure our image dimensions haven't changed.
            let new_info = self
                .backend
                .swapchain_info_for_texture(texture, bounds, color_space);

            is_valid_swapchain_info(&new_info)
                .then(|| {
                    assert!(
                        !self.submitting_null,
                        "App submitted a null texture and a normal texture in the same frame"
                    );

                    if !self.swapchain_data.as_ref().is_some_and(|data| {
                        is_usable_swapchain(&data.info, data.initial_format, &new_info)
                    }) {
                        info!("recreating swapchain (for {eye:?})");
                        self.recreate_swapchain(session_data, new_info);
                    }

                    SubmittedEye {
                        extent: self.backend.copy_texture_to_swapchain(
                            eye,
                            texture,
                            color_space,
                            bounds,
                            self.image_index,
                            submit_flags,
                        ),
                        flip_vertically: bounds.vertically_flipped(),
                    }
                })
                .or_else(|| {
                    trace!("submitting null this frame");
                    self.submitting_null = true;
                    Some(Default::default())
                })
        } else {
            Some(Default::default())
        };

        trace!("submitted {eye:?}");
        if self.eyes_submitted.iter().all(|eye| eye.is_some()) {
            let mut swapchain_data = self.swapchain_data.as_mut();
            if let Some(data) = &mut swapchain_data {
                trace!("releasing image");
                data.swapchain.release_image().unwrap();
            }
            self.image_acquired = false;
        }

        Ok(())
    }

    fn end_frame(
        &mut self,
        session_data: &SessionData,
        system: &System,
        display_time: xr::Time,
        overlays: Option<&OverlayMan>,
    ) where
        for<'b> &'b crate::overlay::AnySwapchainMap:
            TryInto<&'b crate::overlay::SwapchainMap<G::Api>, Error: std::fmt::Display>,
    {
        let mut proj_layer_views = Vec::new();

        if self.should_render
            && !self.submitting_null
            && self.eyes_submitted.iter().all(|eye| eye.is_some())
        {
            let swapchain_data = self
                .swapchain_data
                .as_ref()
                .expect("Swapchain data unexpectedly invalid on submit");

            let crate::system::ViewData { flags, views } =
                system.get_views(session_data.current_origin_as_reference_space());
            proj_layer_views = views
                .into_iter()
                .enumerate()
                .map(|(eye_index, view)| {
                    let pose = xr::Posef {
                        orientation: if flags.contains(xr::ViewStateFlags::ORIENTATION_VALID) {
                            view.pose.orientation
                        } else {
                            xr::Quaternionf::IDENTITY
                        },
                        position: if flags.contains(xr::ViewStateFlags::POSITION_VALID) {
                            view.pose.position
                        } else {
                            xr::Vector3f::default()
                        },
                    };

                    let SubmittedEye {
                        extent,
                        flip_vertically,
                    } = self.eyes_submitted[eye_index]
                        .unwrap_or_else(|| panic!("Eye {eye_index} has not been submitted!"));
                    let mut fov = view.fov;
                    if flip_vertically {
                        std::mem::swap(&mut fov.angle_up, &mut fov.angle_down);
                    }

                    let sub_image = xr::SwapchainSubImage::new()
                        .swapchain(&swapchain_data.swapchain)
                        .image_array_index(eye_index as u32)
                        .image_rect(xr::Rect2Di {
                            extent,
                            offset: xr::Offset2Di::default(),
                        });

                    xr::CompositionLayerProjectionView::new()
                        .fov(fov)
                        .pose(pose)
                        .sub_image(sub_image)
                })
                .collect()
        }

        let mut proj_layer = None;
        if !proj_layer_views.is_empty() {
            trace!("projection layer present");
            proj_layer = Some(
                xr::CompositionLayerProjection::new()
                    .space(session_data.tracking_space())
                    .views(&proj_layer_views),
            );
        }

        let mut layers: Vec<&xr::CompositionLayerBase<_>> = Vec::new();
        if let Some(l) = proj_layer.as_ref() {
            layers.push(l);
        }
        let overlay_layers;
        if let Some(overlay_man) = overlays {
            overlay_layers = overlay_man.get_layers(session_data, self.app_fade_grid);
            layers.extend(overlay_layers.iter().map(Deref::deref));
        }

        self.stream
            .end(display_time, xr::EnvironmentBlendMode::OPAQUE, &layers)
            .unwrap();

        trace!("frame submitted");
    }
}

pub fn is_usable_swapchain<G: xr::Graphics>(
    current: &xr::SwapchainCreateInfo<G>,
    creation_format: G::Format,
    new: &xr::SwapchainCreateInfo<G>,
) -> bool
where
    G::Format: Eq,
{
    creation_format == new.format
        && current.width >= new.width
        && current.height >= new.height
        && current.array_size == new.array_size
        && current.sample_count == new.sample_count
}

fn is_valid_swapchain_info<G: xr::Graphics>(info: &xr::SwapchainCreateInfo<G>) -> bool {
    info.width > 0 && info.height > 0
}

#[cfg(test)]
pub use tests::FakeGraphicsData;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics_backends::{GraphicsBackend, VulkanData};
    use std::cell::Cell;
    use std::ffi::CStr;
    use std::mem::MaybeUninit;
    use std::thread_local;
    use vr::EVRCompositorError::*;
    use vr::IVRCompositor028_Interface;

    pub struct FakeGraphicsData {
        vk: Arc<VulkanData>,
        swapchain_format: Option<u32>,
    }
    thread_local! {
        static SWAPCHAIN_WIDTH: Cell<u32> = const { Cell::new(10) };
        static SWAPCHAIN_HEIGHT: Cell<u32> = const { Cell::new(10) };
        static SWAPCHAIN_FORMAT: Cell<u32> = const { Cell::new(0) };
    }

    pub enum FakeApi {}
    impl xr::Graphics for FakeApi {
        type Requirements = xr::vulkan::Requirements;
        type SessionCreateInfo = xr::vulkan::SessionCreateInfo;
        type Format = <xr::Vulkan as xr::Graphics>::Format;
        type SwapchainImage = <xr::Vulkan as xr::Graphics>::SwapchainImage;

        fn raise_format(x: i64) -> Self::Format {
            xr::Vulkan::raise_format(x)
        }

        fn lower_format(x: Self::Format) -> i64 {
            xr::Vulkan::lower_format(x)
        }

        fn requirements(
            instance: &openxr::Instance,
            system: openxr::SystemId,
        ) -> openxr::Result<Self::Requirements> {
            xr::Vulkan::requirements(instance, system)
        }

        unsafe fn create_session(
            instance: &openxr::Instance,
            system: openxr::SystemId,
            info: &Self::SessionCreateInfo,
        ) -> openxr::Result<openxr::sys::Session> {
            unsafe { xr::Vulkan::create_session(instance, system, info) }
        }

        fn enumerate_swapchain_images(
            _: &openxr::Swapchain<Self>,
        ) -> openxr::Result<Vec<Self::SwapchainImage>> {
            Ok(Vec::new())
        }
    }

    impl GraphicsBackend for FakeGraphicsData {
        type Api = FakeApi;
        type OpenVrTexture = <VulkanData as GraphicsBackend>::OpenVrTexture;
        type NiceFormat = <VulkanData as GraphicsBackend>::NiceFormat;

        fn to_nice_format(format: <Self::Api as openxr::Graphics>::Format) -> Self::NiceFormat {
            VulkanData::to_nice_format(format)
        }
        fn session_create_info(&self) -> <Self::Api as openxr::Graphics>::SessionCreateInfo {
            self.vk.session_create_info()
        }

        fn get_texture(texture: &openvr::Texture_t) -> Option<Self::OpenVrTexture> {
            VulkanData::get_texture(texture)
        }

        fn swapchain_info_for_texture(
            &self,
            _: Self::OpenVrTexture,
            _: openvr::VRTextureBounds_t,
            _: openvr::EColorSpace,
        ) -> openxr::SwapchainCreateInfo<Self::Api> {
            xr::SwapchainCreateInfo {
                create_flags: xr::SwapchainCreateFlags::EMPTY,
                usage_flags: xr::SwapchainUsageFlags::EMPTY,
                format: SWAPCHAIN_FORMAT.get(),
                sample_count: 1,
                width: SWAPCHAIN_WIDTH.get(),
                height: SWAPCHAIN_HEIGHT.get(),
                face_count: 1,
                array_size: 2,
                mip_count: 1,
            }
        }

        fn store_swapchain_images(
            &mut self,
            _images: Vec<<Self::Api as openxr::Graphics>::SwapchainImage>,
            format: u32,
        ) {
            self.swapchain_format.replace(format);
        }

        fn copy_texture_to_swapchain(
            &self,
            _eye: openvr::EVREye,
            _texture: Self::OpenVrTexture,
            _color_space: vr::EColorSpace,
            _bounds: openvr::VRTextureBounds_t,
            _image_index: usize,
            _submit_flags: openvr::EVRSubmitFlags,
        ) -> openxr::Extent2Di {
            xr::Extent2Di::default()
        }

        fn copy_overlay_to_swapchain(
            &mut self,
            _texture: Self::OpenVrTexture,
            _bounds: openvr::VRTextureBounds_t,
            _image_index: usize,
        ) -> openxr::Extent2Di {
            xr::Extent2Di::default()
        }
    }

    impl FakeGraphicsData {
        fn texture(data: &Arc<VulkanData>) -> vr::Texture_t {
            vr::Texture_t {
                eType: vr::ETextureType::Reserved,
                handle: Arc::into_raw(data.clone()) as _,
                eColorSpace: vr::EColorSpace::Auto,
            }
        }

        pub fn new(texture: &vr::Texture_t) -> Self {
            assert_eq!(texture.eType, vr::ETextureType::Reserved);
            let ptr = texture.handle as *const VulkanData;
            let vk = unsafe {
                Arc::increment_strong_count(ptr);
                Arc::from_raw(ptr)
            };
            Self {
                vk,
                swapchain_format: Option::None,
            }
        }
    }

    struct Fixture {
        comp: Arc<Compositor>,
        vk: Arc<VulkanData>,
    }

    impl Fixture {
        fn new() -> Self {
            let xr = Arc::new(OpenXrData::new(&Injector::default()).unwrap());
            let vk = Arc::new(VulkanData::new_temporary(&xr.instance, xr.system_id));
            let comp = Arc::new(Compositor::new(xr.clone(), &Injector::default()));
            xr.compositor.set(Arc::downgrade(&comp));
            crate::init_logging();

            Self { comp, vk }
        }

        fn wait_get_poses(&self) -> vr::EVRCompositorError {
            self.comp
                .WaitGetPoses(std::ptr::null_mut(), 0, std::ptr::null_mut(), 0)
        }

        fn submit(&self, eye: vr::EVREye) -> vr::EVRCompositorError {
            self.comp.Submit(
                eye,
                &FakeGraphicsData::texture(&self.vk),
                std::ptr::null(),
                vr::EVRSubmitFlags::Default,
            )
        }

        fn ensure_real_session(&self, explicit: bool) {
            // synchronize session
            assert_eq!(self.wait_get_poses(), None);
            if explicit {
                assert_eq!(self.comp.SubmitExplicitTimingData(), None);
            }
            assert_eq!(self.submit(vr::EVREye::Left), None);
            assert_eq!(self.submit(vr::EVREye::Right), None);
            if explicit {
                self.comp.PostPresentHandoff();
            } else {
                assert_eq!(self.wait_get_poses(), None);
            }

            let data = self.comp.openxr.session_data.get();
            let lock = data.comp_data.0.lock().unwrap();
            let DynFrameController::Fake(ctrl) = lock.as_ref().unwrap() else {
                panic!("Frame controller was not set up or not faked!");
            };
            assert!(!ctrl.should_render);
        }

        #[track_caller]
        fn check_frame_state(&self, state: fakexr::FrameState) {
            let session = self.comp.openxr.session_data.get().session.as_raw();
            assert_eq!(fakexr::session_frame_state(session), state);
        }
    }

    #[test]
    fn bad_bounds() {
        let f = Fixture::new();

        let bad_bounds = [
            // bound greater than 1 (Superhot does this)
            vr::VRTextureBounds_t {
                vMin: 0.0,
                vMax: 1.1,
                uMin: 0.0,
                uMax: 1.0,
            },
            // bound greater than 1
            vr::VRTextureBounds_t {
                vMin: 0.0,
                vMax: 1.0,
                uMin: 0.0,
                uMax: 1.1,
            },
            // negative bound
            vr::VRTextureBounds_t {
                vMin: -0.1,
                vMax: 1.0,
                uMin: 0.0,
                uMax: 1.0,
            },
            // min/max equal
            vr::VRTextureBounds_t {
                vMin: 1.0,
                vMax: 1.0,
                uMin: 0.0,
                uMax: 1.0,
            },
        ];

        for bound in bad_bounds {
            assert_eq!(
                f.comp.Submit(
                    vr::EVREye::Left,
                    &FakeGraphicsData::texture(&f.vk),
                    &bound,
                    vr::EVRSubmitFlags::Default,
                ),
                InvalidBounds,
                "Bound didn't return InvalidBounds: {bound:?}"
            );
        }
    }

    #[test]
    fn allow_flipped_bounds() {
        let Fixture { comp, .. } = Fixture::new();

        assert_ne!(
            comp.Submit(
                vr::EVREye::Left,
                std::ptr::null(),
                &vr::VRTextureBounds_t {
                    uMin: 0.0,
                    vMin: 1.0,
                    uMax: 1.0,
                    vMax: 0.0
                },
                vr::EVRSubmitFlags::Default
            ),
            InvalidBounds
        );
    }

    #[test]
    fn error_on_multiple_same_eye_submit() {
        let f = Fixture::new();

        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);
        assert_eq!(f.submit(vr::EVREye::Left), AlreadySubmitted);

        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.submit(vr::EVREye::Left), None);
    }

    #[test]
    fn allow_waitgetposes_without_submit() {
        let f = Fixture::new();

        // Enable frame controller
        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.submit(vr::EVREye::Left), None);

        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.wait_get_poses(), None);
    }

    #[test]
    fn recreate_swapchain() {
        let f = Fixture::new();
        f.ensure_real_session(false);

        let get_swapchain_width = || {
            let data = f.comp.openxr.session_data.get();
            let lock = data.comp_data.0.lock().unwrap();
            let DynFrameController::Fake(ctrl) = lock.as_ref().unwrap() else {
                panic!("Frame controller was not set up or not faked!");
            };
            ctrl.swapchain_data
                .as_ref()
                .expect("swapchain info missing")
                .info
                .width
        };

        let old_width = get_swapchain_width();
        SWAPCHAIN_WIDTH.set(40);
        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);
        let new_width = get_swapchain_width();
        assert_ne!(old_width, new_width);

        assert_eq!(f.wait_get_poses(), None);
        SWAPCHAIN_WIDTH.set(20);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        let newer_width = get_swapchain_width();
        assert_eq!(newer_width, new_width);
    }

    #[test]
    fn get_frame_timing() {
        let f = Fixture::new();
        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);

        let mut timing = MaybeUninit::new(vr::Compositor_FrameTiming::default());
        unsafe {
            (&raw mut (*timing.as_mut_ptr()).m_nSize)
                .write(std::mem::size_of::<vr::Compositor_FrameTiming>() as u32);
        }
        assert!(f.comp.GetFrameTiming(timing.as_mut_ptr(), 1));
        let small_size = std::mem::offset_of!(vr::Compositor_FrameTiming, m_HmdPose)
            + std::mem::size_of::<vr::TrackedDevicePose_t>();
        unsafe {
            (&raw mut (*timing.as_mut_ptr()).m_nSize).write(small_size as u32);
        }
        assert!(f.comp.GetFrameTiming(timing.as_mut_ptr(), 1));
        unsafe {
            (&raw mut (*timing.as_mut_ptr()).m_nSize).write(0);
        }
        assert!(!f.comp.GetFrameTiming(timing.as_mut_ptr(), 1));
    }

    #[test]
    fn zero_dims_texture() {
        let f = Fixture::new();
        SWAPCHAIN_WIDTH.set(0);
        SWAPCHAIN_HEIGHT.set(0);

        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);

        assert_eq!(f.wait_get_poses(), None);
        {
            let data = f.comp.openxr.session_data.get();
            let lock = data.comp_data.0.lock().unwrap();
            let DynFrameController::Fake(ctrl) = lock.as_ref().unwrap() else {
                panic!("Frame controller was not set up or not faked!");
            };
            assert!(ctrl.swapchain_data.is_none());
            assert!(!ctrl.should_render);
        }
        SWAPCHAIN_WIDTH.set(10);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);

        assert_eq!(f.wait_get_poses(), None);
        {
            let data = f.comp.openxr.session_data.get();
            let lock = data.comp_data.0.lock().unwrap();
            let DynFrameController::Fake(ctrl) = lock.as_ref().unwrap() else {
                panic!("Frame controller was not set up or not faked!");
            };
            assert!(ctrl.swapchain_data.is_none());
            assert!(ctrl.should_render);
        }
        SWAPCHAIN_HEIGHT.set(10);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);

        assert_eq!(f.wait_get_poses(), None);
        {
            let data = f.comp.openxr.session_data.get();
            let lock = data.comp_data.0.lock().unwrap();
            let DynFrameController::Fake(ctrl) = lock.as_ref().unwrap() else {
                panic!("Frame controller was not set up or not faked!");
            };
            assert!(ctrl.swapchain_data.is_some());
            assert!(ctrl.should_render);
        }
    }

    #[test]
    fn vulkan_extensions() {
        let f = Fixture::new();

        fn tst(func: impl Fn(*mut c_char, u32) -> u32, dbg: &str) {
            // Normal flow
            let size = func(std::ptr::null_mut(), 0);
            let mut exts = vec![0; size as usize];
            func(exts.as_mut_ptr(), exts.len() as u32);

            let data = unsafe { CStr::from_ptr(exts.as_ptr()) };
            assert_eq!(data, c"VK_foo VK_bar", "{dbg}");

            // Oversized buffer
            let mut exts = vec![0; size as usize * 2];
            func(exts.as_mut_ptr(), exts.len() as u32);

            let data = unsafe { CStr::from_ptr(exts.as_ptr()) };
            assert_eq!(data, c"VK_foo VK_bar", "{dbg}");

            // Undersized buffer - should not crash
            let mut exts = vec![0];
            func(exts.as_mut_ptr(), exts.len() as u32);
        }

        tst(
            |buf, size| f.comp.GetVulkanInstanceExtensionsRequired(buf, size),
            "instance exts",
        );
        tst(
            |buf, size| {
                f.comp
                    .GetVulkanDeviceExtensionsRequired(std::ptr::null_mut(), buf, size)
            },
            "device exts",
        );
    }

    #[test]
    fn unsupported_format() {
        let f = Fixture::new();
        SWAPCHAIN_FORMAT.set(1);
        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);
        let data = f.comp.openxr.session_data.get();
        let lock = data.comp_data.0.lock().unwrap();
        let DynFrameController::Fake(ctrl) = lock.as_ref().unwrap() else {
            panic!("Frame controller was not set up or not faked!");
        };
        let data = ctrl
            .swapchain_data
            .as_ref()
            .expect("Swapchain data is missing");
        assert_eq!(data.initial_format, 1);
        assert_eq!(data.info.format, 0);
    }

    #[test]
    fn explicit_timing() {
        let f = Fixture::new();
        f.ensure_real_session(false);

        f.comp.SetExplicitTimingMode(
            vr::EVRCompositorTimingMode::Explicit_ApplicationPerformsPostPresentHandoff,
        );
        assert_eq!(f.wait_get_poses(), None);
        f.check_frame_state(fakexr::FrameState::Waited);

        assert_eq!(f.comp.SubmitExplicitTimingData(), None);
        f.check_frame_state(fakexr::FrameState::Begun);

        assert_eq!(f.submit(vr::EVREye::Left), None);
        f.check_frame_state(fakexr::FrameState::Begun);
        assert_eq!(f.submit(vr::EVREye::Right), None);
        f.check_frame_state(fakexr::FrameState::Begun);

        f.comp.PostPresentHandoff();
        f.check_frame_state(fakexr::FrameState::Ended);
    }

    #[test]
    fn explicit_timing_no_submit() {
        let f = Fixture::new();
        f.ensure_real_session(false);

        f.comp.SetExplicitTimingMode(
            vr::EVRCompositorTimingMode::Explicit_ApplicationPerformsPostPresentHandoff,
        );
        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(f.comp.SubmitExplicitTimingData(), None);
        f.comp.PostPresentHandoff();
        f.check_frame_state(fakexr::FrameState::Ended);
    }

    #[test]
    fn explicit_timing_multiple_waitgetposes() {
        let f = Fixture::new();
        f.ensure_real_session(false);
        f.comp.SetExplicitTimingMode(
            vr::EVRCompositorTimingMode::Explicit_ApplicationPerformsPostPresentHandoff,
        );

        assert_eq!(f.wait_get_poses(), None);
        f.check_frame_state(fakexr::FrameState::Waited);
        assert_eq!(f.wait_get_poses(), None);
        f.check_frame_state(fakexr::FrameState::Waited);
    }

    #[test]
    fn explicit_timing_multiple_submit_explicit_timing_data() {
        let f = Fixture::new();
        f.ensure_real_session(false);
        f.comp.SetExplicitTimingMode(
            vr::EVRCompositorTimingMode::Explicit_ApplicationPerformsPostPresentHandoff,
        );

        assert_eq!(f.comp.SubmitExplicitTimingData(), None);
        f.check_frame_state(fakexr::FrameState::Begun);
        assert_eq!(f.comp.SubmitExplicitTimingData(), None);
        f.check_frame_state(fakexr::FrameState::Begun);
    }

    #[test]
    fn explicit_timing_session_restart_after_waitgetposes() {
        let f = Fixture::new();
        f.comp.SetExplicitTimingMode(
            vr::EVRCompositorTimingMode::Explicit_ApplicationPerformsPostPresentHandoff,
        );

        f.ensure_real_session(true);
        f.comp.openxr.restart_session();
        assert_eq!(f.wait_get_poses(), None);

        f.check_frame_state(fakexr::FrameState::Waited);
    }

    #[test]
    fn explicit_timing_unfocused() {
        let f = Fixture::new();
        f.comp.SetExplicitTimingMode(
            vr::EVRCompositorTimingMode::Explicit_ApplicationPerformsPostPresentHandoff,
        );

        f.comp.SubmitExplicitTimingData();
        assert_eq!(f.submit(vr::EVREye::Left), DoNotHaveFocus);
        assert_eq!(f.submit(vr::EVREye::Right), DoNotHaveFocus);

        assert_eq!(f.wait_get_poses(), None);
        f.comp.SubmitExplicitTimingData();
        assert_eq!(f.submit(vr::EVREye::Left), None);
        assert_eq!(f.submit(vr::EVREye::Right), None);
    }

    #[test]
    fn submit_overlay_without_projection_layer() {
        use crate::overlay::OverlayMan;
        use vr::IVROverlay027_Interface;

        let f = Fixture::new();
        let overlays = Arc::new(OverlayMan::new(f.comp.openxr.clone(), &Injector::default()));
        f.comp.overlays.set(Arc::downgrade(&overlays));
        overlays.compositor.set(Arc::downgrade(&f.comp));

        let mut overlay = 0;
        assert_eq!(
            overlays.CreateOverlay(
                c"test_overlay".as_ptr(),
                c"TestOverlay".as_ptr(),
                &mut overlay
            ),
            vr::EVROverlayError::None
        );

        assert_eq!(f.wait_get_poses(), None);
        assert_eq!(
            overlays.SetOverlayTexture(overlay, &FakeGraphicsData::texture(&f.vk)),
            vr::EVROverlayError::None
        );
        f.check_frame_state(fakexr::FrameState::Begun);
        assert_eq!(overlays.ShowOverlay(overlay), vr::EVROverlayError::None);
        f.comp.PostPresentHandoff();
        f.check_frame_state(fakexr::FrameState::Ended);
    }
}
