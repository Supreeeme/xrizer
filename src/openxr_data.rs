use crate::{
    clientcore::{Injected, Injector},
    graphics_backends::{GraphicsBackend, VulkanData, supported_apis_enum},
};
use derive_more::Deref;
use glam::f32::{Quat, Vec3};
use log::{info, warn};
use openvr as vr;
use openxr as xr;
use std::mem::ManuallyDrop;
use std::sync::{
    RwLock,
    atomic::{AtomicI64, Ordering},
};
use std::time::Duration;

#[cfg(feature = "monado")]
use openxr_mndx_xdev_space::XR_MNDX_XDEV_SPACE_EXTENSION_NAME;

pub trait Compositor: vr::InterfaceImpl {
    fn post_session_restart(
        &self,
        session: &SessionData,
        waiter: xr::FrameWaiter,
        stream: FrameStream,
    );

    fn get_session_create_info(
        &self,
        data: crate::compositor::CompositorSessionData,
    ) -> SessionCreateInfo;

    #[cfg(test)]
    fn on_restart(&self) {}
}

pub type RealOpenXrData = OpenXrData<crate::compositor::Compositor>;
pub struct OpenXrData<C: Compositor> {
    _entry: xr::Entry,
    pub instance: xr::Instance,
    pub system_id: xr::SystemId,
    pub session_data: SessionReadGuard,
    pub display_time: AtomicXrTime,
    pub display_period_nanos: AtomicI64,
    pub enabled_extensions: xr::ExtensionSet,

    /// should only be externally accessed for testing
    pub(crate) input: Injected<crate::input::Input<C>>,
    pub(crate) compositor: Injected<C>,
}

impl<C: Compositor> Drop for OpenXrData<C> {
    fn drop(&mut self) {
        let mut data = unsafe { ManuallyDrop::take(&mut *self.session_data.0.get_mut().unwrap()) };
        self.end_session(&mut data);
    }
}

#[derive(Debug)]
#[allow(dead_code)] // Results aren't used, but they're printed
#[allow(clippy::enum_variant_names)]
pub enum InitError {
    EnumeratingExtensionsFailed(xr::sys::Result),
    InstanceCreationFailed(xr::sys::Result),
    SystemCreationFailed(xr::sys::Result),
    SessionCreationFailed(SessionCreationError),
}

impl From<SessionCreationError> for InitError {
    fn from(value: SessionCreationError) -> Self {
        Self::SessionCreationFailed(value)
    }
}

fn get_app_name() -> Option<String> {
    let exe = std::fs::read_link("/proc/self/exe")
        .inspect_err(|e| warn!("Couldn't get app name from /proc/self/exe: {e}"))
        .ok()?;

    let basename = exe.file_name().unwrap();
    if basename == "wine64-preloader" || basename == "wine-preloader" {
        fn extract_wine_exe_name() -> Option<String> {
            let exe_path = std::env::args().next()?;
            // The Windows path separator is \ (instead of /) so we can't use Path.
            // We just want the basename anyway, so we'll just grab the last piece.
            let exe_name = exe_path.rsplit_once('\\')?.1;
            Some(
                exe_name
                    .strip_suffix(".exe")
                    .unwrap_or(exe_name)
                    .to_string(),
            )
        }
        if let Some(name) = extract_wine_exe_name() {
            return Some(name);
        }
    }

    Some(basename.to_string_lossy().into_owned())
}

fn make_version() -> u32 {
    env!("CARGO_PKG_VERSION_MAJOR").parse::<u32>().unwrap_or(0) * 1000000
        + env!("CARGO_PKG_VERSION_MINOR").parse::<u32>().unwrap_or(0) * 1000
        + env!("CARGO_PKG_VERSION_PATCH").parse::<u32>().unwrap_or(1)
}

impl<C: Compositor> OpenXrData<C> {
    pub fn new(injector: &Injector) -> Result<Self, InitError> {
        #[cfg(not(test))]
        let entry = xr::Entry::linked();

        #[cfg(test)]
        let entry =
            unsafe { xr::Entry::from_get_instance_proc_addr(fakexr::get_instance_proc_addr) }
                .unwrap();

        let supported_exts = entry
            .enumerate_extensions()
            .map_err(InitError::EnumeratingExtensionsFailed)?;
        let mut exts = xr::ExtensionSet::default();
        exts.khr_vulkan_enable = supported_exts.khr_vulkan_enable;
        exts.khr_opengl_enable = supported_exts.khr_opengl_enable;
        exts.khr_convert_timespec_time = supported_exts.khr_convert_timespec_time;
        exts.ext_hand_tracking = supported_exts.ext_hand_tracking;
        exts.khr_visibility_mask = supported_exts.khr_visibility_mask;
        exts.khr_composition_layer_cylinder = supported_exts.khr_composition_layer_cylinder;
        exts.khr_composition_layer_equirect2 = supported_exts.khr_composition_layer_equirect2;
        exts.khr_composition_layer_color_scale_bias =
            supported_exts.khr_composition_layer_color_scale_bias;
        exts.htc_vive_focus3_controller_interaction =
            supported_exts.htc_vive_focus3_controller_interaction;
        exts.fb_display_refresh_rate = supported_exts.fb_display_refresh_rate;

        // Extension that enables simple full body tracking support via generic tracked devices.
        // Available only in the Monado OpenXR runtime.
        #[cfg(feature = "monado")]
        if supported_exts
            .other
            .contains(&XR_MNDX_XDEV_SPACE_EXTENSION_NAME.to_string())
        {
            exts.other
                .push(XR_MNDX_XDEV_SPACE_EXTENSION_NAME.to_string());
        }

        let instance = entry
            .create_instance(
                &xr::ApplicationInfo {
                    application_name: get_app_name().as_deref().unwrap_or("XRizer"),
                    application_version: 0,
                    engine_name: "XRizer",
                    engine_version: make_version(),
                    ..Default::default()
                },
                &exts,
                &[],
            )
            .map_err(InitError::InstanceCreationFailed)?;

        let system_id = instance
            .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
            .map_err(InitError::SystemCreationFailed)?;

        let session_data = SessionReadGuard(RwLock::new(ManuallyDrop::new(
            SessionData::new(
                &instance,
                system_id,
                vr::ETrackingUniverseOrigin::Standing,
                None,
            )?
            .0,
        )));

        let display_time = if exts.khr_convert_timespec_time {
            instance.now().map(|t| t.as_nanos()).unwrap_or(1i64)
        } else {
            1i64
        };

        Ok(Self {
            _entry: entry,
            instance,
            system_id,
            session_data,
            display_time: AtomicXrTime(display_time.into()), // This will get replaced on the first WaitGetPoses
            display_period_nanos: 11111111.into(), // This will get replaced on the first WaitGetPoses
            enabled_extensions: exts,
            input: injector.inject(),
            compositor: injector.inject(),
        })
    }

    pub fn poll_events(&self) {
        let data = self.session_data.get();
        if let Some(state) = self.poll_events_impl(&data) {
            drop(data);
            self.session_data.0.write().unwrap().state = state;
        }
    }

    fn poll_events_impl(&self, session_data: &SessionData) -> Option<xr::SessionState> {
        let mut buf = xr::EventDataBuffer::new();
        let mut state = None;
        while let Some(event) = self.instance.poll_event(&mut buf).unwrap() {
            match event {
                xr::Event::SessionStateChanged(event) => {
                    state = Some(event.state());
                    info!("OpenXR session state changed: {:?}", event.state());
                }
                xr::Event::InteractionProfileChanged(_) => {
                    if let Some(input) = self.input.get() {
                        input.interaction_profile_changed(session_data);
                    }
                }
                _ => {
                    info!("unknown event");
                }
            }
        }

        state
    }

    pub fn restart_session(&self) {
        let mut session_guard = self.session_data.0.write().unwrap();
        self.end_session(&mut session_guard);

        let origin = session_guard.current_origin;
        let comp = self
            .compositor
            .get()
            .expect("Session is being restarted, but compositor has not been set up!");

        let info = comp.get_session_create_info(std::mem::take(&mut session_guard.comp_data));

        // We need to destroy the old session before creating the new one.
        let _ = unsafe { ManuallyDrop::take(&mut *session_guard) };

        let (session, waiter, stream) =
            SessionData::new(&self.instance, self.system_id, origin, Some(&info))
                .expect("Failed to initalize new session");

        comp.post_session_restart(&session, waiter, stream);

        if let Some(input) = self.input.get() {
            input.post_session_restart(&session);
        }

        *session_guard = ManuallyDrop::new(session);
    }

    pub fn set_tracking_space(&self, space: vr::ETrackingUniverseOrigin) {
        self.session_data.0.write().unwrap().current_origin = space;
    }

    pub fn get_tracking_space(&self) -> vr::ETrackingUniverseOrigin {
        self.session_data.get().current_origin
    }

    pub fn reset_tracking_space(&self, origin: vr::ETrackingUniverseOrigin) {
        let mut guard = self.session_data.0.write().unwrap();
        let SessionData {
            session,
            view_space,
            local_space_reference,
            local_space_adjusted,
            stage_space_reference,
            stage_space_adjusted,
            ..
        } = &mut **guard;

        let reset_space = |ref_space, adjusted_space: &mut xr::Space, ty| {
            let xr::Posef {
                position,
                orientation,
            } = view_space
                .locate(ref_space, self.display_time.get())
                .unwrap()
                .pose;

            // Only set the rotation around the y axis
            let (twist, _) = swing_twist_decomposition(
                Quat::from_xyzw(orientation.x, orientation.y, orientation.z, orientation.w),
                Vec3::Y,
            )
            .unwrap_or_else(|| {
                warn!("Couldn't decompose rotation - using identity");
                (Quat::IDENTITY, Quat::IDENTITY)
            });

            *adjusted_space = session
                .create_reference_space(
                    ty,
                    xr::Posef {
                        position,
                        orientation: xr::Quaternionf {
                            x: twist.x,
                            y: twist.y,
                            z: twist.z,
                            w: twist.w,
                        },
                    },
                )
                .unwrap();
        };

        match origin {
            vr::ETrackingUniverseOrigin::RawAndUncalibrated => unimplemented!(),
            vr::ETrackingUniverseOrigin::Standing => reset_space(
                stage_space_reference,
                stage_space_adjusted,
                xr::ReferenceSpaceType::STAGE,
            ),
            vr::ETrackingUniverseOrigin::Seated => reset_space(
                local_space_reference,
                local_space_adjusted,
                xr::ReferenceSpaceType::LOCAL,
            ),
        };
    }

    pub fn get_refresh_rate(&self) -> f32 {
        let get_fallback_rate = || {
            Duration::from_nanos(
                self.display_period_nanos
                    .load(Ordering::Relaxed)
                    .try_into()
                    .unwrap(),
            )
            .as_secs_f32()
        };
        if !self.enabled_extensions.fb_display_refresh_rate {
            return get_fallback_rate();
        }

        self.session_data
            .get()
            .session
            .get_display_refresh_rate()
            .unwrap_or_else(|_| get_fallback_rate())
    }

    fn end_session(&self, session_data: &mut SessionData) {
        session_data.session.request_exit().unwrap();
        let mut state = session_data.state;
        while state != xr::SessionState::STOPPING {
            if let Some(s) = self.poll_events_impl(session_data) {
                state = s;
            }
        }
        #[cfg(test)]
        if let Some(comp) = self.compositor.get() {
            comp.on_restart();
        }
        session_data.session.end().unwrap();
        while state != xr::SessionState::EXITING {
            if let Some(s) = self.poll_events_impl(session_data) {
                state = s;
            }
        }
    }
}

pub struct AtomicXrTime(AtomicI64);

impl AtomicXrTime {
    #[inline]
    pub fn set(&self, time: xr::Time) {
        self.0.store(time.as_nanos(), Ordering::Relaxed);
    }

    #[inline]
    pub fn get(&self) -> xr::Time {
        xr::Time::from_nanos(self.0.load(Ordering::Relaxed))
    }
}

pub struct SessionReadGuard(RwLock<ManuallyDrop<SessionData>>);
impl SessionReadGuard {
    pub fn get(&self) -> std::sync::RwLockReadGuard<'_, ManuallyDrop<SessionData>> {
        self.0.read().unwrap()
    }
}

pub struct Session<G: xr::Graphics> {
    session: xr::Session<G>,
    swapchain_formats: Vec<G::Format>,
}
supported_apis_enum!(pub enum GraphicalSession: Session);
impl std::fmt::Display for GraphicalSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphicalSession::Vulkan(_) => f.write_str("GraphicalSession::Vulkan"),
            GraphicalSession::OpenGL(_) => f.write_str("GraphicalSession::OpenGL"),
            #[cfg(test)]
            GraphicalSession::Fake(_) => f.write_str("GraphicalSession::Fake"),
        }
    }
}
supported_apis_enum!(pub enum FrameStream: xr::FrameStream);

// Implementing From results in a "conflicting implementations" error: https://github.com/rust-lang/rust/issues/85576
#[repr(transparent)]
#[derive(Deref)]
pub struct CreateInfo<G: xr::Graphics>(pub G::SessionCreateInfo);
supported_apis_enum!(pub enum SessionCreateInfo: CreateInfo);

impl SessionCreateInfo {
    pub fn from_info<G: xr::Graphics>(info: G::SessionCreateInfo) -> Self
    where
        Self: From<CreateInfo<G>>,
    {
        Self::from(CreateInfo(info))
    }
}

pub struct SessionData {
    pub session: xr::Session<xr::AnyGraphics>,
    session_graphics: GraphicalSession,
    pub state: xr::SessionState,
    pub view_space: xr::Space,
    // The "reference" space is always equivalent to the reference space with an identity offset.
    // The "adjusted" space may have an offset, set by reset_tracking_space.
    // The adjusted spaces should be used for locating things - the reference spaces are only
    // needed for reset_tracking_space
    local_space_reference: xr::Space,
    local_space_adjusted: xr::Space,
    stage_space_reference: xr::Space,
    stage_space_adjusted: xr::Space,
    pub current_origin: vr::ETrackingUniverseOrigin,

    pub input_data: crate::input::InputSessionData,
    pub comp_data: crate::compositor::CompositorSessionData,
    pub overlay_data: crate::overlay::OverlaySessionData,
    /// OpenXR requires graphics information before creating a session, but OpenVR clients don't
    /// have to provide that information until they actually submit a frame. Yet, we need some
    /// information only available behind a session (i.e., calling xrLocateViews for
    /// GetProjectionMatrix), so we will create a session with fake graphics info to appease OpenXR,
    /// that will be replaced with a real one after the application calls IVRSystem::Submit.
    /// When we're using the real session, this will be None.
    /// Note that it also important that this comes after all members which internally use a xr::Session
    /// \- structs are dropped in declaration order, and if we drop our temporary Vulkan data
    /// before the session, the runtime will likely be very unhappy.
    temp_vulkan: Option<VulkanData>,
}

#[derive(Debug)]
#[allow(dead_code)] // Results aren't used, but they're printed
#[allow(clippy::enum_variant_names)]
pub enum SessionCreationError {
    SessionCreationFailed(xr::sys::Result),
    PollEventFailed(xr::sys::Result),
    BeginSessionFailed(xr::sys::Result),
}

impl SessionData {
    fn new(
        instance: &xr::Instance,
        system_id: xr::SystemId,
        current_origin: vr::ETrackingUniverseOrigin,
        create_info: Option<&SessionCreateInfo>,
    ) -> Result<(Self, xr::FrameWaiter, FrameStream), SessionCreationError> {
        let info;
        let (temp_vulkan, info) = if let Some(info) = create_info {
            if let SessionCreateInfo::Vulkan(info) = info {
                // Monado seems to (incorrectly) give validation errors unless we call this.
                let pd =
                    unsafe { instance.vulkan_graphics_device(system_id, info.instance) }.unwrap();
                assert_eq!(pd, info.physical_device);
            }
            (None, info)
        } else {
            let vk = VulkanData::new_temporary(instance, system_id);
            info = SessionCreateInfo::from_info::<xr::Vulkan>(vk.session_create_info());
            (Some(vk), &info)
        };

        #[macros::any_graphics(SessionCreateInfo)]
        fn create_session<G: xr::Graphics>(
            info: &CreateInfo<G>,
            instance: &xr::Instance,
            system_id: xr::SystemId,
        ) -> xr::Result<(
            xr::Session<xr::AnyGraphics>,
            GraphicalSession,
            xr::FrameWaiter,
            FrameStream,
        )>
        where
            GraphicalSession: From<Session<G>>,
            FrameStream: From<xr::FrameStream<G>>,
        {
            info!(
                "Creating OpenXR session with graphics API {}",
                std::any::type_name::<G>()
            );
            // required to call
            let _ = instance.graphics_requirements::<G>(system_id).unwrap();

            unsafe { instance.create_session(system_id, &info.0) }.map(|(session, w, s)| {
                let swapchain_formats = session
                    .enumerate_swapchain_formats()
                    .expect("Couldn't enumerate session swapchain formats!");
                (
                    session.clone().into_any_graphics(),
                    Session {
                        session,
                        swapchain_formats,
                    }
                    .into(),
                    w,
                    s.into(),
                )
            })
        }

        let (session, session_graphics, waiter, stream) = info
            .with_any_graphics::<create_session>((instance, system_id))
            .map_err(SessionCreationError::SessionCreationFailed)?;

        info!("New session created!");
        let view_space = session
            .create_reference_space(xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY)
            .unwrap();
        let [local_space_reference, local_space_adjusted] = std::array::from_fn(|_| {
            session
                .create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)
                .unwrap()
        });
        let [stage_space_reference, stage_space_adjusted] = std::array::from_fn(|_| {
            session
                .create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)
                .unwrap()
        });

        let mut buf = xr::EventDataBuffer::new();
        loop {
            if let Some(xr::Event::SessionStateChanged(state)) = instance
                .poll_event(&mut buf)
                .map_err(SessionCreationError::PollEventFailed)?
                && state.state() == xr::SessionState::READY
            {
                break;
            }
        }

        info!(
            "OpenXR session state changed: {:?}",
            xr::SessionState::READY
        );
        session
            .begin(xr::ViewConfigurationType::PRIMARY_STEREO)
            .map_err(SessionCreationError::BeginSessionFailed)?;
        info!("Began OpenXR session.");

        Ok((
            SessionData {
                temp_vulkan,
                session,
                session_graphics,
                state: xr::SessionState::READY,
                view_space,
                local_space_reference,
                local_space_adjusted,
                stage_space_reference,
                stage_space_adjusted,
                input_data: Default::default(),
                comp_data: Default::default(),
                overlay_data: Default::default(),
                current_origin,
            },
            waiter,
            stream,
        ))
    }

    pub fn create_swapchain<G: xr::Graphics>(
        &self,
        info: &xr::SwapchainCreateInfo<G>,
    ) -> xr::Result<xr::Swapchain<G>>
    where
        for<'a> &'a GraphicalSession: TryInto<&'a Session<G>, Error: std::fmt::Display>,
    {
        (&self.session_graphics)
            .try_into()
            .unwrap_or_else(|e| {
                panic!(
                    "Session was not using API {}: {e}",
                    std::any::type_name::<G>()
                )
            })
            .session
            .create_swapchain(info)
    }

    pub fn check_format<G: GraphicsBackend>(&self, info: &mut xr::SwapchainCreateInfo<G::Api>)
    where
        for<'a> &'a GraphicalSession: TryInto<&'a Session<G::Api>, Error: std::fmt::Display>,
        <G::Api as xr::Graphics>::Format: PartialEq,
    {
        let formats = &(&self.session_graphics)
            .try_into()
            .unwrap_or_else(|_| {
                panic!(
                    "Expected session API {}, but current session is using {}!",
                    std::any::type_name::<G>(),
                    self.session_graphics,
                )
            })
            .swapchain_formats;

        if !formats.contains(&info.format) {
            let new_format = formats[0];
            warn!(
                "Requested to init swapchain with unsupported format {:?} - instead using {:?}",
                G::to_nice_format(info.format),
                G::to_nice_format(new_format)
            );
            info.format = new_format;
        }
    }

    pub fn tracking_space(&self) -> &xr::Space {
        self.get_space_for_origin(self.current_origin)
    }

    #[inline]
    pub fn get_space_for_origin(&self, origin: vr::ETrackingUniverseOrigin) -> &xr::Space {
        match origin {
            vr::ETrackingUniverseOrigin::Seated => &self.local_space_adjusted,
            vr::ETrackingUniverseOrigin::Standing => &self.stage_space_adjusted,
            vr::ETrackingUniverseOrigin::RawAndUncalibrated => unreachable!(),
        }
    }

    #[inline]
    pub fn get_space_from_type(&self, ty: xr::ReferenceSpaceType) -> &xr::Space {
        match ty {
            xr::ReferenceSpaceType::VIEW => &self.view_space,
            xr::ReferenceSpaceType::LOCAL => &self.local_space_adjusted,
            xr::ReferenceSpaceType::STAGE => &self.stage_space_adjusted,
            other => panic!("Unsupported reference space type: {other:?}"),
        }
    }

    #[inline]
    pub fn current_origin_as_reference_space(&self) -> xr::ReferenceSpaceType {
        match self.current_origin {
            vr::ETrackingUniverseOrigin::Seated => xr::ReferenceSpaceType::LOCAL,
            vr::ETrackingUniverseOrigin::Standing => xr::ReferenceSpaceType::STAGE,
            vr::ETrackingUniverseOrigin::RawAndUncalibrated => unreachable!(),
        }
    }

    /// Returns true if this session is not using a temporary graphics setup.
    #[inline]
    pub fn is_real_session(&self) -> bool {
        self.temp_vulkan.is_none()
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Hand {
    Left = 1,
    Right,
}

impl TryFrom<vr::ETrackedControllerRole> for Hand {
    type Error = ();
    #[inline]
    fn try_from(value: vr::ETrackedControllerRole) -> Result<Self, Self::Error> {
        match value {
            vr::ETrackedControllerRole::LeftHand => Ok(Hand::Left),
            vr::ETrackedControllerRole::RightHand => Ok(Hand::Right),
            _ => Err(()),
        }
    }
}

impl From<Hand> for vr::ETrackedControllerRole {
    fn from(hand: Hand) -> Self {
        match hand {
            Hand::Left => vr::ETrackedControllerRole::LeftHand,
            Hand::Right => vr::ETrackedControllerRole::RightHand,
        }
    }
}

/// Taken from: https://github.com/bitshifter/glam-rs/issues/536
/// Decompose the rotation on to 2 parts.
///
/// 1. Twist - rotation around the "direction" vector
/// 2. Swing - rotation around axis that is perpendicular to "direction" vector
///
/// The rotation can be composed back by
/// `rotation = swing * twist`.
/// Order matters!
///
/// has singularity in case of swing_rotation close to 180 degrees rotation.
/// if the input quaternion is of non-unit length, the outputs are non-unit as well
/// otherwise, outputs are both unit
fn swing_twist_decomposition(rotation: Quat, axis: Vec3) -> Option<(Quat, Quat)> {
    let rotation_axis = rotation.xyz();
    let projection = rotation_axis.project_onto(axis);

    let twist = {
        let maybe_flipped_twist = Quat::from_vec4(projection.extend(rotation.w));
        if rotation_axis.dot(projection) < 0.0 {
            -maybe_flipped_twist
        } else {
            maybe_flipped_twist
        }
    };

    if twist.length_squared() != 0.0 {
        let swing = rotation * twist.conjugate();
        Some((twist.normalize(), swing))
    } else {
        None
    }
}

#[cfg(test)]
pub use tests::FakeCompositor;

#[cfg(test)]
mod tests {
    use super::{FrameStream, GraphicsBackend, OpenXrData, SessionCreateInfo};
    use crate::clientcore::Injector;
    use openxr as xr;
    use std::ffi::CStr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier, Mutex};

    pub struct FakeCompositor {
        backend: crate::graphics_backends::VulkanData,
        barrier: Mutex<Option<Arc<Barrier>>>,
        restart_complete: AtomicBool,
    }
    impl FakeCompositor {
        pub fn new(xr: &OpenXrData<Self>) -> Self {
            Self::new_with_barrier(xr, None)
        }

        fn new_with_barrier(xr: &OpenXrData<Self>, barrier: Option<Arc<Barrier>>) -> Self {
            Self {
                backend: crate::graphics_backends::VulkanData::new_temporary(
                    &xr.instance,
                    xr.system_id,
                ),
                barrier: barrier.into(),
                restart_complete: false.into(),
            }
        }
    }
    impl openvr::InterfaceImpl for FakeCompositor {
        fn get_version(_: &CStr) -> Option<Box<dyn FnOnce(&Arc<Self>) -> *mut std::ffi::c_void>> {
            None
        }
        fn supported_versions() -> &'static [&'static CStr] {
            &[]
        }
    }
    impl super::Compositor for FakeCompositor {
        fn get_session_create_info(
            &self,
            _: crate::compositor::CompositorSessionData,
        ) -> SessionCreateInfo {
            SessionCreateInfo::from_info::<xr::Vulkan>(self.backend.session_create_info())
        }

        fn post_session_restart(
            &self,
            _: &crate::openxr_data::SessionData,
            _: openxr::FrameWaiter,
            _: FrameStream,
        ) {
            self.restart_complete.store(true, Ordering::Relaxed);
        }

        fn on_restart(&self) {
            if let Some(barrier) = self.barrier.lock().unwrap().take() {
                barrier.wait();
            }
        }
    }

    #[test]
    fn session_restart_contended() {
        crate::init_logging();
        let data = Arc::new(OpenXrData::<FakeCompositor>::new(&Injector::default()).unwrap());
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let comp = Arc::new(FakeCompositor::new_with_barrier(
            &data,
            Some(barrier.clone()),
        ));
        data.compositor.set(Arc::downgrade(&comp));

        std::thread::scope(|scope| {
            {
                let barrier = barrier.clone();
                let data = data.clone();
                let comp = comp.clone();
                scope.spawn(move || {
                    barrier.wait();
                    let _unused = data.session_data.get();
                    log::debug!("acquired");
                    assert!(comp.restart_complete.load(Ordering::Relaxed));
                });
            }

            scope.spawn(|| {
                data.restart_session();
            });
        });

        drop(data); // Session must be dropped before Vulkan data.
        drop(comp);
    }
}
