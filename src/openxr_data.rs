use crate::{
    clientcore::{Injected, Injector},
    graphics_backends::{supported_apis_enum, GraphicsBackend, VulkanData},
    input::{InteractionProfile, Profiles},
};
use derive_more::Deref;
use glam::f32::{Quat, Vec3};
use log::{info, warn};
use openvr as vr;
use openxr as xr;
use std::mem::ManuallyDrop;
use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    Mutex, RwLock,
};

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
}

pub type RealOpenXrData = OpenXrData<crate::compositor::Compositor>;
pub struct OpenXrData<C: Compositor> {
    _entry: xr::Entry,
    pub instance: xr::Instance,
    pub system_id: xr::SystemId,
    pub session_data: SessionReadGuard,
    pub display_time: AtomicXrTime,
    pub left_hand: HandInfo,
    pub right_hand: HandInfo,
    pub enabled_extensions: xr::ExtensionSet,

    /// should only be externally accessed for testing
    pub(crate) input: Injected<crate::input::Input<C>>,
    pub(crate) compositor: Injected<C>,
}

impl<C: Compositor> Drop for OpenXrData<C> {
    fn drop(&mut self) {
        self.end_session();
        unsafe {
            ManuallyDrop::drop(&mut *self.session_data.0.get_mut().unwrap());
        }
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
        exts.ext_hand_tracking = supported_exts.ext_hand_tracking;
        exts.khr_visibility_mask = supported_exts.khr_visibility_mask;
        exts.khr_composition_layer_cylinder = supported_exts.khr_composition_layer_cylinder;
        exts.khr_composition_layer_equirect2 = supported_exts.khr_composition_layer_equirect2;
        exts.khr_composition_layer_color_scale_bias =
            supported_exts.khr_composition_layer_color_scale_bias;

        let instance = entry
            .create_instance(
                &xr::ApplicationInfo {
                    application_name: "XRizer",
                    application_version: 0,
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

        let left_hand = HandInfo::new(&instance, "/user/hand/left");
        let right_hand = HandInfo::new(&instance, "/user/hand/right");

        Ok(Self {
            _entry: entry,
            instance,
            system_id,
            session_data,
            display_time: AtomicXrTime(1.into()),
            left_hand,
            right_hand,
            enabled_extensions: exts,
            input: injector.inject(),
            compositor: injector.inject(),
        })
    }

    pub fn poll_events(&self) {
        let mut buf = xr::EventDataBuffer::new();
        while let Some(event) = self.instance.poll_event(&mut buf).unwrap() {
            match event {
                xr::Event::SessionStateChanged(event) => {
                    self.session_data.0.write().unwrap().state = event.state();
                    info!("OpenXR session state changed: {:?}", event.state());
                }
                xr::Event::InteractionProfileChanged(_) => {
                    let session = self.session_data.get();
                    for info in [&self.left_hand, &self.right_hand] {
                        let profile_path = session
                            .session
                            .current_interaction_profile(info.subaction_path)
                            .unwrap();

                        info.profile_path.store(profile_path);
                        let profile = match profile_path {
                            xr::Path::NULL => {
                                info.connected.store(false, Ordering::Relaxed);
                                "<null>".to_owned()
                            }
                            path => {
                                info.connected.store(true, Ordering::Relaxed);
                                self.instance.path_to_string(path).unwrap()
                            }
                        };

                        *info.profile.lock().unwrap() = Profiles::get().profile_from_name(&profile);

                        session.input_data.interaction_profile_changed();

                        info!(
                            "{} interaction profile changed: {}",
                            info.path_name, profile
                        );
                    }
                }
                xr::Event::EventsLost(_) => {
                    warn!("OpenXR events lost!");
                }
                xr::Event::InstanceLossPending(_) => {
                    warn!("OpenXR instance loss pending!");
                }
                xr::Event::ReferenceSpaceChangePending(_) => {
                    warn!("OpenXR reference space change pending!");
                }
                xr::Event::PerfSettingsEXT(_) => {
                    warn!("OpenXR performance settings changed!");
                }
                xr::Event::VisibilityMaskChangedKHR(_) => {
                    warn!("OpenXR visibility mask changed!");
                }
                xr::Event::MainSessionVisibilityChangedEXTX(_) => {
                    warn!("OpenXR main session visibility changed!");
                }
                xr::Event::DisplayRefreshRateChangedFB(_) => {
                    warn!("OpenXR display refresh rate changed!");
                }
                xr::Event::SpatialAnchorCreateCompleteFB(_) => {
                    warn!("OpenXR spatial anchor create complete!");
                }
                xr::Event::SpaceSetStatusCompleteFB(_) => {
                    warn!("OpenXR space set status complete!");
                }
                xr::Event::SpaceQueryResultsAvailableFB(_) => {
                    warn!("OpenXR space query results available!");
                }
                xr::Event::SpaceQueryCompleteFB(_) => {
                    warn!("OpenXR space query complete!");
                }
                xr::Event::SpaceSaveCompleteFB(_) => {
                    warn!("OpenXR space save complete!");
                }
                xr::Event::SpaceEraseCompleteFB(_) => {
                    warn!("OpenXR space erase complete!");
                }
                xr::Event::SpaceShareCompleteFB(_) => {
                    warn!("OpenXR space share complete!");
                }
                xr::Event::SpaceListSaveCompleteFB(_) => {
                    warn!("OpenXR space list save complete!");
                }
                xr::Event::SceneCaptureCompleteFB(_) => {
                    warn!("OpenXR scene capture complete!");
                }
                xr::Event::PassthroughStateChangedFB(_) => {
                    warn!("OpenXR passthrough state changed!");
                }
                xr::Event::ViveTrackerConnectedHTCX(_) => {
                    warn!("OpenXR Vive tracker connected!");
                }
                xr::Event::MarkerTrackingUpdateVARJO(_) => {
                    warn!("OpenXR marker tracking update!");
                }
                xr::Event::VirtualKeyboardCommitTextMETA(_) => {
                    warn!("OpenXR virtual keyboard commit text!");
                }
                xr::Event::VirtualKeyboardBackspaceMETA(_) => {
                    warn!("OpenXR virtual keyboard backspace!");
                }
                xr::Event::VirtualKeyboardEnterMETA(_) => {
                    warn!("OpenXR virtual keyboard enter!");
                }
                xr::Event::VirtualKeyboardShownMETA(_) => {
                    warn!("OpenXR virtual keyboard shown!");
                }
                xr::Event::VirtualKeyboardHiddenMETA(_) => {
                    warn!("OpenXR virtual keyboard hidden!");
                }
                xr::Event::HeadsetFitChangedML(_) => {
                    warn!("OpenXR headset fit changed!");
                }
                xr::Event::EyeCalibrationChangedML(_) => {
                    warn!("OpenXR eye calibration changed!");
                }
                xr::Event::LocalizationChangedML(_) => {
                    warn!("OpenXR localization changed!");
                }
                xr::Event::UserPresenceChangedEXT(_) => {
                    warn!("OpenXR user presence changed!");
                }
                _ => {
                    info!("unknown event");
                }
            }
        }
    }

    pub fn restart_session(&self) {
        self.end_session();
        let mut session_guard = self.session_data.0.write().unwrap();

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
            .unwrap();

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

    fn end_session(&self) {
        self.session_data.get().session.request_exit().unwrap();
        let mut state = self.session_data.get().state;
        while state != xr::SessionState::STOPPING {
            self.poll_events();
            state = self.session_data.get().state;
        }
        self.session_data.get().session.end().unwrap();
        while state != xr::SessionState::EXITING {
            self.poll_events();
            state = self.session_data.get().state;
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
            {
                if state.state() == xr::SessionState::READY {
                    break;
                }
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
            .unwrap_or_else(|e| {
                panic!(
                    "Session was not using API {}: {e}",
                    std::any::type_name::<G>()
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

pub struct AtomicPath(AtomicU64);
impl AtomicPath {
    pub(crate) fn load(&self) -> xr::Path {
        xr::Path::from_raw(self.0.load(Ordering::Relaxed))
    }

    fn store(&self, path: xr::Path) {
        self.0.store(path.into_raw(), Ordering::Relaxed);
    }
}

pub struct HandInfo {
    path_name: &'static str,
    connected: AtomicBool,
    pub subaction_path: xr::Path,
    pub profile_path: AtomicPath,
    pub profile: Mutex<Option<&'static dyn InteractionProfile>>,
}

impl HandInfo {
    #[inline]
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn new(instance: &xr::Instance, path_name: &'static str) -> Self {
        Self {
            path_name,
            connected: false.into(),
            subaction_path: instance.string_to_path(path_name).unwrap(),
            profile_path: AtomicPath(0.into()),
            profile: Mutex::default(),
        }
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Hand {
    Left = 1,
    Right,
}

impl TryFrom<vr::TrackedDeviceIndex_t> for Hand {
    type Error = ();
    #[inline]
    fn try_from(value: vr::TrackedDeviceIndex_t) -> Result<Self, Self::Error> {
        match value {
            x if x == Hand::Left as u32 => Ok(Hand::Left),
            x if x == Hand::Right as u32 => Ok(Hand::Right),
            _ => Err(()),
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
