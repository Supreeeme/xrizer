pub mod vulkan;

mod monado_xdev;
pub use monado_xdev::add_trackers;

use crossbeam_utils::atomic::AtomicCell;
use glam::{Affine3A, Quat, Vec3};
use openxr_sys as xr;
use paste::paste;
use slotmap::{DefaultKey, Key, KeyData, SlotMap};
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, c_char};
use std::sync::{
    Arc, LazyLock, Mutex, MutexGuard, OnceLock, RwLock, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

#[derive(Clone, Copy, PartialEq)]
pub enum ActionState {
    Bool(bool),
    /// True if locatable (action set was synced this frame).
    Pose(bool),
    Float(f32),
    Vector2(f32, f32),
    /// True if active
    Haptic(bool),
}

impl From<bool> for ActionState {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

pub fn set_action_state_with_time(
    action: xr::Action,
    state: ActionState,
    hand: UserPath,
    time: xr::Time,
) {
    let action = action.to_handle().unwrap();
    assert_eq!(
        std::mem::discriminant(&state),
        std::mem::discriminant(&action.state.left.load().state)
    );
    let mut d = action.pending_state.take();
    match hand {
        UserPath::RightHand => {
            d.right = Some((state, time));
        }
        UserPath::LeftHand => {
            d.left = Some((state, time));
        }
    }
    action.pending_state.store(d);
    action.active.store(true, Ordering::Relaxed);
}

pub fn set_action_state(action: xr::Action, state: ActionState, hand: UserPath) {
    set_action_state_with_time(action, state, hand, xr::Time::from_nanos(0));
}

pub fn deactivate_action(action: xr::Action) {
    let action = action.to_handle().unwrap();
    action.active.store(false, Ordering::Relaxed);
}

#[track_caller]
pub fn is_haptic_activated(action: xr::Action, hand: UserPath) -> bool {
    println!("{}", action.into_raw());
    let action = action.to_handle().unwrap();
    let instance = action.instance.upgrade().expect("Failed to get instance");

    let hand_key = instance.string_to_path.lock().unwrap()[hand.as_path()];
    let path = xr::Path::from_raw(hand_key.data().as_ffi());
    let ActionState::Haptic(state) = action.get_hand_state(path).state else {
        panic!("Wrong action type!");
    };

    state
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UserPath {
    /// /user/hand/left
    LeftHand,
    /// /user/hand/right
    RightHand,
}

impl UserPath {
    fn from_path(s: &str) -> Option<Self> {
        match s {
            "/user/hand/left" => Some(Self::LeftHand),
            "/user/hand/right" => Some(Self::RightHand),
            _ => None,
        }
    }

    fn as_path(&self) -> &'static str {
        match self {
            Self::LeftHand => "/user/hand/left",
            Self::RightHand => "/user/hand/right",
        }
    }
}

fn get_hand_data(hand: UserPath, session: &Session) -> &HandData {
    match hand {
        UserPath::RightHand => &session.right_hand,
        UserPath::LeftHand => &session.left_hand,
    }
}

pub fn set_interaction_profile(session: xr::Session, hand: UserPath, profile: xr::Path) {
    let s = session.to_handle().unwrap();
    get_hand_data(hand, &s).pending_profile.store(Some(profile));
}

pub fn set_grip(session: xr::Session, path: UserPath, pose: xr::Posef) {
    let session = session.to_handle().unwrap();
    get_hand_data(path, &session).grip_pose.store(pose);
}

pub fn set_aim(session: xr::Session, path: UserPath, pose: xr::Posef) {
    let session = session.to_handle().unwrap();
    get_hand_data(path, &session).aim_pose.store(pose);
}

#[track_caller]
pub fn get_suggested_bindings(action: xr::Action, profile: xr::Path) -> Vec<String> {
    let action = xr::Action::to_handle(action).unwrap();
    let instance = action.instance.upgrade().unwrap();
    let suggested = action.suggested.lock().unwrap();

    suggested
        .get(&profile)
        .unwrap_or_else(|| {
            panic!(
                "No suggested bindings for profile {} for action {:?}",
                instance.get_path_value(profile).unwrap().unwrap(),
                action.name,
            )
        })
        .iter()
        .map(|path| instance.get_path_value(*path).unwrap().unwrap())
        .collect()
}

pub fn session_frame_state(session: xr::Session) -> FrameState {
    let session = session.to_handle().unwrap();
    session.frame_state.load()
}

macro_rules! fn_unimplemented_impl {
    ($($param:ident),+) => {
        fn_unimplemented_impl!($($param),+  -> []);
    };
    ($param:ident $(,$rest:ident)* -> [$($params:ident),*]) => {
        paste! {
            #[allow(dead_code)]
            trait [<FnUnimplemented $param>]<$($params,)* $param> {
                extern "system" fn unimplemented($(_: $params,)* _: $param) -> xr::Result {
                    unimplemented!()
                }
            }

            impl<$($params,)* $param> [<FnUnimplemented $param>]<$($params,)* $param> for unsafe extern "system" fn($($params,)* $param) -> xr::Result {}
        }

        fn_unimplemented_impl!($($rest),* -> [$($params,)* $param]);
    };
    (-> [$($params:ident),+]) => {}
}

fn_unimplemented_impl!(A, B, C, D, E, F);

#[allow(clippy::missing_transmute_annotations, clippy::missing_safety_doc)]
pub unsafe extern "system" fn get_instance_proc_addr(
    instance: xr::Instance,
    name: *const c_char,
    function: *mut Option<xr::pfn::VoidFunction>,
) -> xr::Result {
    let name = unsafe { CStr::from_ptr(name) };

    /// Generates match arms for supported functions.
    /// Functions in parenthesis are returned as unimplemented functions - they should be
    /// implemented if a test needs it.
    macro_rules! get_fn {
        ([$($func:tt),+] $pat:pat => $expr:expr) => {
            get_fn!(@arm [$($func),+] -> [] {$pat => $expr})
        };
        (@arm [ {mndx::$name:ident} $(,$rest:tt)* ] -> [$($arms:tt),*] {$pat:pat => $expr:expr}) => {
            get_fn!(
                @arm
                [$($rest),*] ->
                [
                    $($arms,)*
                    [
                        x if x == const {
                            CStr::from_bytes_with_nul_unchecked(concat!("xr", stringify!($name), "\0").as_bytes())
                        } => Some(std::mem::transmute( paste! { monado_xdev::[<$name:snake>] as openxr_mndx_xdev_space::bindings::$name }))
                    ]
                ]
                {$pat => $expr}
            )
        };
        (@arm [$name:ident $(,$rest:tt)*] -> [$($arms:tt),*] {$pat:pat => $expr:expr}) => {
            get_fn!(
                @arm
                [$($rest),*] ->
                [
                    $($arms,)*
                    [
                        x if x == const {
                            CStr::from_bytes_with_nul_unchecked(concat!("xr", stringify!($name), "\0").as_bytes())
                        } => Some(std::mem::transmute( paste! { [<$name:snake>] as xr::pfn::$name }))
                    ]
                ]
                {$pat => $expr}
            )
        };
        (@arm [($name:ident) $(,$rest:tt)*] -> [$($arms:tt),*] {$pat:pat => $expr:expr}) => {
            get_fn!(
                @arm
                [$($rest),*] ->
                [
                    $($arms,)*
                    [
                        x if x == const {
                            CStr::from_bytes_with_nul_unchecked(concat!("xr", stringify!($name), "\0").as_bytes())
                        } => Some(std::mem::transmute(xr::pfn::$name::unimplemented as xr::pfn::$name))
                    ]
                ]
                {$pat => $expr}
            )
        };
        (@arm []-> [$([$($arms:tt)*]),+] {$pat:pat => $expr:expr}) => {
            match name {
                $($($arms)*,)+
                $pat => $expr
            }
        }
    }

    if instance == xr::Instance::NULL {
        unsafe {
            *function = get_fn!([CreateInstance, EnumerateInstanceExtensionProperties, (EnumerateApiLayerProperties)]
                other => {
                    println!("unknown func without instance: {other:?}");
                    return xr::Result::ERROR_HANDLE_INVALID;
                }
            );
        }
    } else {
        use vulkan::xr::*;

        unsafe {
            {
                *function = get_fn![[
                    GetInstanceProcAddr,
                    CreateInstance,
                    DestroyInstance,
                    (EnumerateInstanceExtensionProperties),
                    (EnumerateApiLayerProperties),
                    GetVulkanInstanceExtensionsKHR,
                    GetVulkanDeviceExtensionsKHR,
                    GetVulkanGraphicsDeviceKHR,
                    GetVulkanGraphicsRequirementsKHR,
                    GetSystem,
                    CreateSession,
                    DestroySession,
                    BeginSession,
                    EndSession,
                    CreateReferenceSpace,
                    PollEvent,
                    DestroySpace,
                    LocateViews,
                    RequestExitSession,
                    (ResultToString),
                    (StructureTypeToString),
                    (GetInstanceProperties),
                    (GetSystemProperties),
                    CreateSwapchain,
                    DestroySwapchain,
                    EnumerateSwapchainImages,
                    AcquireSwapchainImage,
                    WaitSwapchainImage,
                    ReleaseSwapchainImage,
                    EnumerateSwapchainFormats,
                    (EnumerateReferenceSpaces),
                    CreateActionSpace,
                    LocateSpace,
                    (EnumerateViewConfigurations),
                    (EnumerateEnvironmentBlendModes),
                    (GetViewConfigurationProperties),
                    (EnumerateViewConfigurationViews),
                    BeginFrame,
                    EndFrame,
                    WaitFrame,
                    ApplyHapticFeedback,
                    (StopHapticFeedback),
                    (PollEvent),
                    StringToPath,
                    PathToString,
                    (GetReferenceSpaceBoundsRect),
                    GetActionStateBoolean,
                    GetActionStateFloat,
                    GetActionStateVector2f,
                    (GetActionStatePose),
                    CreateActionSet,
                    DestroyActionSet,
                    CreateAction,
                    DestroyAction,
                    SuggestInteractionProfileBindings,
                    AttachSessionActionSets,
                    GetCurrentInteractionProfile,
                    SyncActions,
                    (EnumerateBoundSourcesForAction),
                    (GetInputSourceLocalizedName),
                    {mndx::CreateXDevListMNDX},
                    {mndx::GetXDevListGenerationNumberMNDX},
                    {mndx::EnumerateXDevsMNDX},
                    {mndx::GetXDevPropertiesMNDX},
                    {mndx::DestroyXDevListMNDX},
                    {mndx::CreateXDevSpaceMNDX}
                    ]

                    other => {
                        println!("unknown func: {other:?}");
                        return xr::Result::ERROR_FUNCTION_UNSUPPORTED;
                    }
                ]
            }
        }
    }

    xr::Result::SUCCESS
}

extern "system" fn enumerate_instance_extension_properties(
    layer_name: *const c_char,
    property_capacity_input: u32,
    property_count_output: *mut u32,
    properties: *mut xr::ExtensionProperties,
) -> xr::Result {
    assert!(layer_name.is_null());
    unsafe { *property_count_output = 2 };
    if property_capacity_input >= 2 {
        let props =
            unsafe { std::slice::from_raw_parts_mut(properties, property_capacity_input as usize) };
        props[0] = xr::ExtensionProperties {
            ty: xr::ExtensionProperties::TYPE,
            next: std::ptr::null_mut(),
            extension_name: [0 as c_char; xr::MAX_EXTENSION_NAME_SIZE],
            extension_version: 1,
        };
        let name = xr::KHR_VULKAN_ENABLE_EXTENSION_NAME;
        let name =
            unsafe { std::slice::from_raw_parts(name.as_ptr() as *const c_char, name.len()) };
        props[0].extension_name[..name.len()].copy_from_slice(name);

        props[1] = xr::ExtensionProperties {
            ty: xr::ExtensionProperties::TYPE,
            next: std::ptr::null_mut(),
            extension_name: [0 as c_char; xr::MAX_EXTENSION_NAME_SIZE],
            extension_version: 1,
        };

        let name = openxr_mndx_xdev_space::XR_MNDX_XDEV_SPACE_EXTENSION_NAME;
        let name =
            unsafe { std::slice::from_raw_parts(name.as_ptr() as *const c_char, name.len()) };
        props[1].extension_name[..name.len()].copy_from_slice(name);
    }
    xr::Result::SUCCESS
}

trait Handle: 'static {
    type XrType: XrType;
    fn instances() -> MutexGuard<'static, SlotMap<DefaultKey, Arc<Self>>>;
    fn to_xr(self: Arc<Self>) -> Self::XrType;
}

trait XrType {
    type Handle: Handle;
    const TO_RAW: fn(Self) -> u64;
    fn to_handle(self) -> Option<Arc<Self::Handle>>;
}

macro_rules! get_handle {
    ($handle:expr) => {{
        match <_ as crate::XrType>::to_handle($handle) {
            Some(handle) => handle,
            None => {
                eprintln!("unknown handle for {} ({:?})", stringify!($handle), $handle);
                return xr::Result::ERROR_HANDLE_INVALID;
            }
        }
    }};
}
pub(crate) use get_handle;

macro_rules! impl_handle {
    ($ty:ty, $xr_type:ty) => {
        impl crate::XrType for $xr_type {
            type Handle = $ty;
            const TO_RAW: fn(Self) -> u64 = <$xr_type>::into_raw;
            fn to_handle(self) -> Option<Arc<Self::Handle>> {
                Self::Handle::instances()
                    .get(slotmap::DefaultKey::from(slotmap::KeyData::from_ffi(
                        self.into_raw(),
                    )))
                    .map(|i| Arc::clone(i))
            }
        }
        impl Handle for $ty {
            type XrType = $xr_type;
            fn instances()
            -> std::sync::MutexGuard<'static, slotmap::SlotMap<slotmap::DefaultKey, Arc<Self>>>
            {
                static I: std::sync::LazyLock<
                    std::sync::Mutex<slotmap::SlotMap<slotmap::DefaultKey, Arc<$ty>>>,
                > = std::sync::LazyLock::new(|| std::sync::Mutex::default());
                I.lock().unwrap()
            }
            fn to_xr(self: Arc<Self>) -> $xr_type {
                let key = Self::instances().insert(self);
                <$xr_type>::from_raw(<_ as slotmap::Key>::data(&key).as_ffi())
            }
        }
    };
}
pub(crate) use impl_handle;

struct EventDataBuffer {
    buffer: Vec<u8>,
    on_polled: Option<Box<dyn FnOnce() + Send + Sync>>,
}

struct Instance {
    event_receiver: Mutex<mpsc::Receiver<EventDataBuffer>>,
    event_sender: mpsc::Sender<EventDataBuffer>,
    paths: Mutex<SlotMap<DefaultKey, String>>,
    string_to_path: Mutex<HashMap<String, DefaultKey>>,
    action_sets: Mutex<HashSet<xr::ActionSet>>,
    left_hand_key: DefaultKey,
    right_hand_key: DefaultKey,
}

impl Instance {
    fn get_path_value(&self, path: xr::Path) -> Result<Option<String>, ()> {
        if path == xr::Path::NULL {
            Ok(None)
        } else {
            let key = DefaultKey::from(KeyData::from_ffi(path.into_raw()));
            self.paths
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .map(Some)
                .ok_or(())
        }
    }

    fn get_user_path(&self, path: xr::Path) -> Result<Option<UserPath>, ()> {
        Ok(self
            .get_path_value(path)?
            .and_then(|v| UserPath::from_path(&v)))
    }
}

struct HandData {
    pending_profile: AtomicCell<Option<xr::Path>>,
    profile: AtomicCell<xr::Path>,
    grip_pose: AtomicCell<xr::Posef>,
    aim_pose: AtomicCell<xr::Posef>,
}

impl Default for HandData {
    fn default() -> Self {
        Self {
            pending_profile: Default::default(),
            profile: Default::default(),
            grip_pose: xr::Posef::IDENTITY.into(),
            aim_pose: xr::Posef::IDENTITY.into(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrameState {
    Waited,
    Begun,
    Ended,
}

struct Session {
    instance: Weak<Instance>,
    event_sender: mpsc::Sender<EventDataBuffer>,
    vk_device: AtomicU64,
    attached_sets: OnceLock<Box<[xr::ActionSet]>>,
    left_hand: HandData,
    right_hand: HandData,
    spaces: Mutex<HashSet<DefaultKey>>,
    state: AtomicCell<xr::SessionState>,
    state_synced: AtomicBool,
    should_render: AtomicBool,
    frame_state: AtomicCell<FrameState>,
    with_trackers: AtomicBool,
}

impl Session {
    fn synchronized(self: &Arc<Self>) {
        self.state.store(xr::SessionState::SYNCHRONIZED);
        let session = Self::instances()
            .iter()
            .find_map(|(key, s)| {
                Arc::ptr_eq(s, self).then(|| xr::Session::from_raw(key.data().as_ffi()))
            })
            .expect("Couldn't find session?");
        let s = self.clone();
        send_event(
            &self.event_sender,
            xr::EventDataSessionStateChanged {
                ty: xr::EventDataSessionStateChanged::TYPE,
                next: std::ptr::null_mut(),
                session,
                state: xr::SessionState::SYNCHRONIZED,
                time: xr::Time::from_nanos(0),
            },
            Some(Box::new(move || {
                s.state_synced.store(true, Ordering::Relaxed);
                s.should_render.store(true, Ordering::Relaxed);
            })),
        );
    }

    fn add_space(&self, space: Arc<Space>) -> xr::Space {
        let xr = space.to_xr();
        let key = DefaultKey::from(KeyData::from_ffi(xr.into_raw()));
        let mut spaces = self.spaces.lock().unwrap();
        spaces.insert(key);

        xr
    }

    fn get_action_if_attached(
        &self,
        info: *const xr::ActionStateGetInfo,
    ) -> Option<(Arc<ActionSet>, Arc<Action>)> {
        let sets = self.attached_sets.get()?;
        let action = xr::Action::to_handle(unsafe { (*info).action })?;
        sets.into_iter().find_map(|set| {
            let set = xr::ActionSet::to_handle(*set)?;
            for a in set.actions.get().unwrap() {
                if Arc::as_ptr(a) == Arc::as_ptr(&action) {
                    return Some((set, action.clone()));
                }
            }
            None
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let spaces = self.spaces.lock().unwrap();
        for space in spaces.iter() {
            Space::instances().remove(*space);
        }
    }
}

fn transition_frame_state(
    state: &AtomicCell<FrameState>,
    new: FrameState,
) -> Result<(), xr::Result> {
    let old = state.load();
    let transition = |allowed_state| {
        if allowed_state == new {
            Ok(())
        } else {
            println!("Invalid transition from {old:?} to {new:?}");
            Err(xr::Result::ERROR_CALL_ORDER_INVALID)
        }
    };

    let ret = match old {
        FrameState::Waited => transition(FrameState::Begun),
        FrameState::Begun => Ok(()),
        FrameState::Ended => transition(FrameState::Waited),
    };

    if ret.is_ok() {
        state.store(new);
    }

    ret
}

static LOCATION_FLAGS_TRACKED: LazyLock<xr::SpaceLocationFlags> = LazyLock::new(|| {
    xr::SpaceLocationFlags::POSITION_VALID
        | xr::SpaceLocationFlags::POSITION_TRACKED
        | xr::SpaceLocationFlags::ORIENTATION_VALID
        | xr::SpaceLocationFlags::ORIENTATION_TRACKED
});

enum SpaceType {
    Action {
        hand: Option<UserPath>,
        action: Weak<Action>,
    },
    Reference(xr::ReferenceSpaceType),
}

struct Space {
    ty: SpaceType,
    offset: xr::Posef,
    session: Weak<Session>,
}

impl Space {
    fn get_pose_relative_to_local(&self) -> Result<xr::SpaceLocation, xr::Result> {
        let default = || xr::SpaceLocation {
            ty: xr::SpaceLocation::TYPE,
            next: std::ptr::null_mut(),
            location_flags: xr::SpaceLocationFlags::default(),
            pose: xr::Posef::default(),
        };
        let session = self
            .session
            .upgrade()
            .ok_or(xr::Result::ERROR_SESSION_LOST)?;

        let SpaceType::Action { hand, action } = &self.ty else {
            let pose = xr::Posef::IDENTITY;
            let mat = pose_to_mat(pose);
            let offset = pose_to_mat(self.offset);

            let ret = mat_to_pose(mat * offset);

            return Ok(xr::SpaceLocation {
                ty: xr::SpaceLocation::TYPE,
                next: std::ptr::null_mut(),
                location_flags: *LOCATION_FLAGS_TRACKED,
                pose: ret,
            });
        };

        // Check if this hand has an interaction profile
        let hand = hand.unwrap_or(UserPath::LeftHand);
        let hand_data = match hand {
            UserPath::LeftHand => &session.left_hand,
            UserPath::RightHand => &session.right_hand,
        };
        let hand_path = hand.as_path();
        let profile = match hand_data.profile.load() {
            xr::Path::NULL => {
                // no profile - no data
                return Ok(default());
            }
            other => other,
        };

        // Check if this action has bindings for the current profile
        let action = action.upgrade().unwrap();
        let bindings = action.suggested.lock().unwrap();
        let Some(bindings) = bindings.get(&profile) else {
            return Ok(default());
        };

        // Check if this action has been synced
        let state = match hand {
            UserPath::LeftHand => &action.state.left,
            UserPath::RightHand => &action.state.right,
        };

        let ActionState::Pose(state) = state.load().state else {
            unreachable!();
        };
        if !state {
            return Ok(default());
        }

        // Find what it's bound to
        let instance = session
            .instance
            .upgrade()
            .ok_or(xr::Result::ERROR_SESSION_LOST)?;

        let binding = bindings
            .iter()
            .copied()
            .find_map(|p| {
                let val = instance.get_path_value(p).unwrap().unwrap();
                val.starts_with(hand_path).then_some(val)
            })
            .unwrap_or_else(|| panic!("expected binding for space for action {:?}", action.name));

        let pose = match binding.strip_prefix(hand.as_path()).unwrap() {
            "/input/grip/pose" => hand_data.grip_pose.load(),
            "/input/aim/pose" => hand_data.aim_pose.load(),
            other => panic!(
                "unrecognized pose binding {other} for action {:?}",
                action.name
            ),
        };

        let mat = pose_to_mat(pose);
        let offset = pose_to_mat(self.offset);

        let ret = mat_to_pose(mat * offset);

        Ok(xr::SpaceLocation {
            ty: xr::SpaceLocation::TYPE,
            next: std::ptr::null_mut(),
            location_flags: *LOCATION_FLAGS_TRACKED,
            pose: ret,
        })
    }
}

struct ActionSet {
    instance: Weak<Instance>,
    name: CString,
    localized: CString,
    pending_actions: RwLock<Vec<Arc<Action>>>,
    actions: OnceLock<Vec<Arc<Action>>>,
    active: AtomicBool,
}
impl ActionSet {
    fn make_immutable(&self) {
        let actions = std::mem::take(&mut *self.pending_actions.write().unwrap());
        self.actions
            .set(actions)
            .unwrap_or_else(|_| panic!("Action set already immutable"));
    }
}

struct Action {
    instance: Weak<Instance>,
    name: CString,
    active: AtomicBool,
    localized_name: CString,
    state: LeftRight<AtomicCell<ActionStateData>>,
    pending_state: AtomicCell<LeftRight<Option<(ActionState, xr::Time)>>>,
    suggested: Mutex<HashMap<xr::Path, Vec<xr::Path>>>,
}

impl Action {
    fn get_hand_state(&self, path: xr::Path) -> ActionStateData {
        let instance = self.instance.upgrade().expect("Failed to get instance");
        match instance.get_user_path(path).unwrap() {
            None | Some(UserPath::LeftHand) => self.state.left.load(),
            Some(UserPath::RightHand) => self.state.right.load(),
        }
    }
}

#[derive(Default)]
struct LeftRight<T> {
    left: T,
    right: T,
}

#[derive(Copy, Clone, PartialEq)]
struct ActionStateData {
    state: ActionState,
    changed: bool,
    last_change_time: xr::Time,
}

struct Swapchain {
    image_acquired: AtomicBool,
}

impl_handle!(Instance, xr::Instance);
impl_handle!(Session, xr::Session);
impl_handle!(ActionSet, xr::ActionSet);
impl_handle!(Action, xr::Action);
impl_handle!(Space, xr::Space);
impl_handle!(Swapchain, xr::Swapchain);

fn destroy_handle<T: XrType>(xr: T) -> xr::Result {
    T::Handle::instances().remove(DefaultKey::from(KeyData::from_ffi(T::TO_RAW(xr))));
    xr::Result::SUCCESS
}

extern "system" fn create_instance(
    _info: *const xr::InstanceCreateInfo,
    instance: *mut xr::Instance,
) -> xr::Result {
    let (tx, rx) = mpsc::channel();

    let (left, right) = (
        "/user/hand/left".to_string(),
        "/user/hand/right".to_string(),
    );
    let mut paths = SlotMap::new();
    let mut string_to_path = HashMap::new();
    let left_hand_key = paths.insert_with_key(|key| {
        string_to_path.insert(left.clone(), key);
        left
    });
    let right_hand_key = paths.insert_with_key(|key| {
        string_to_path.insert(right.clone(), key);
        right
    });
    let inst = Arc::new(Instance {
        event_receiver: rx.into(),
        event_sender: tx,
        paths: Mutex::new(paths),
        string_to_path: Mutex::new(string_to_path),
        action_sets: Default::default(),
        left_hand_key,
        right_hand_key,
    });
    unsafe {
        *instance = inst.to_xr();
    }
    xr::Result::SUCCESS
}

extern "system" fn destroy_instance(instance: xr::Instance) -> xr::Result {
    destroy_handle(instance)
}

extern "system" fn create_session(
    instance: xr::Instance,
    create_info: *const xr::SessionCreateInfo,
    session: *mut xr::Session,
) -> xr::Result {
    let instance = get_handle!(instance);
    let info = unsafe { create_info.as_ref().unwrap() };
    let vk = unsafe {
        (info.next as *const xr::GraphicsBindingVulkanKHR)
            .as_ref()
            .unwrap()
    };
    assert_eq!(vk.ty, xr::GraphicsBindingVulkanKHR::TYPE);
    let sess = Arc::new(Session {
        instance: Arc::downgrade(&instance),
        event_sender: instance.event_sender.clone(),
        vk_device: (vk.device as u64).into(),
        attached_sets: OnceLock::new(),
        left_hand: Default::default(),
        right_hand: Default::default(),
        spaces: Default::default(),
        state: xr::SessionState::READY.into(),
        state_synced: true.into(),
        should_render: false.into(),
        frame_state: FrameState::Ended.into(),
        with_trackers: false.into(),
    });

    let tx = sess.event_sender.clone();
    unsafe {
        *session = sess.to_xr();
    }

    send_event(
        &tx,
        xr::EventDataSessionStateChanged {
            ty: xr::EventDataSessionStateChanged::TYPE,
            next: std::ptr::null(),
            session: unsafe { *session },
            state: xr::SessionState::READY,
            time: xr::Time::from_nanos(0),
        },
        None,
    );

    xr::Result::SUCCESS
}

extern "system" fn destroy_session(session: xr::Session) -> xr::Result {
    let s = get_handle!(session);
    // Our Vulkan device needs to still exist when we destroy the session - a real runtime will use
    // it!
    let device = s.vk_device.load(Ordering::Relaxed);
    if !vulkan::Device::validate(device) {
        panic!("Vulkan device invalid ({device})")
    }
    destroy_handle(session);

    xr::Result::SUCCESS
}

extern "system" fn create_action_set(
    instance: xr::Instance,
    info: *const xr::ActionSetCreateInfo,
    set: *mut xr::ActionSet,
) -> xr::Result {
    let instance = get_handle!(instance);
    let Some(info) = (unsafe { info.as_ref() }) else {
        return xr::Result::ERROR_VALIDATION_FAILURE;
    };

    let name = unsafe { CStr::from_ptr(info.action_set_name.as_ptr()) }.to_owned();
    let localized = unsafe { CStr::from_ptr(info.localized_action_set_name.as_ptr()) }.to_owned();

    for set in instance.action_sets.lock().unwrap().iter().copied() {
        let set = get_handle!(set);
        if set.name == name {
            return xr::Result::ERROR_NAME_DUPLICATED;
        }

        if set.localized == localized {
            return xr::Result::ERROR_LOCALIZED_NAME_DUPLICATED;
        }
    }

    let s = Arc::new(ActionSet {
        instance: Arc::downgrade(&instance),
        name,
        localized,
        actions: OnceLock::new(),
        pending_actions: RwLock::default(),
        active: false.into(),
    });

    unsafe {
        *set = s.to_xr();
        instance.action_sets.lock().unwrap().insert(*set);
    }
    xr::Result::SUCCESS
}

extern "system" fn destroy_action_set(set: xr::ActionSet) -> xr::Result {
    let set_ = get_handle!(set);
    let Some(instance) = set_.instance.upgrade() else {
        return xr::Result::ERROR_INSTANCE_LOST;
    };
    instance.action_sets.lock().unwrap().remove(&set);
    destroy_handle(set)
}

extern "system" fn create_action(
    set: xr::ActionSet,
    info: *const xr::ActionCreateInfo,
    action: *mut xr::Action,
) -> xr::Result {
    let set = get_handle!(set);
    if set.actions.get().is_some() {
        return xr::Result::ERROR_ACTIONSETS_ALREADY_ATTACHED;
    }

    let info = unsafe { info.as_ref().unwrap() };
    let name = CStr::from_bytes_until_nul(unsafe {
        std::slice::from_raw_parts(info.action_name.as_ptr() as _, info.action_name.len())
    })
    .unwrap();
    for b in name.to_bytes().iter().copied() {
        if !b.is_ascii_alphanumeric() && b != b'-' && b != b'.' && b != b'_' {
            println!(
                "bad character ({:?}) in action name {name:?}",
                std::str::from_utf8(&[b])
            );
            return xr::Result::ERROR_PATH_FORMAT_INVALID;
        }
    }
    let localized_name = CStr::from_bytes_until_nul(unsafe {
        std::slice::from_raw_parts(
            info.localized_action_name.as_ptr() as _,
            info.localized_action_name.len(),
        )
    })
    .unwrap();

    for action in set.pending_actions.read().unwrap().iter() {
        if action.name.as_c_str() == name {
            return xr::Result::ERROR_NAME_DUPLICATED;
        }
        if action.localized_name.as_c_str() == localized_name {
            return xr::Result::ERROR_LOCALIZED_NAME_DUPLICATED;
        }
    }

    let state = match info.action_type {
        xr::ActionType::BOOLEAN_INPUT => ActionState::Bool(false),
        xr::ActionType::POSE_INPUT => ActionState::Pose(false),
        xr::ActionType::FLOAT_INPUT => ActionState::Float(0.0),
        xr::ActionType::VECTOR2F_INPUT => ActionState::Vector2(0.0, 0.0),
        xr::ActionType::VIBRATION_OUTPUT => ActionState::Haptic(false),
        other => unimplemented!("unhandled action type: {other:?}"),
    };
    let data = ActionStateData {
        state,
        changed: false,
        last_change_time: xr::Time::from_nanos(0),
    };
    let a = Arc::new(Action {
        instance: set.instance.clone(),
        active: false.into(),
        name: name.to_owned(),
        localized_name: CStr::from_bytes_until_nul(unsafe {
            std::slice::from_raw_parts(
                info.localized_action_name.as_ptr() as _,
                info.localized_action_name.len(),
            )
        })
        .unwrap()
        .to_owned(),
        state: LeftRight {
            left: data.into(),
            right: data.into(),
        },
        pending_state: Default::default(),
        suggested: Mutex::default(),
    });

    set.pending_actions.write().unwrap().push(a.clone());
    unsafe {
        *action = a.to_xr();
    }
    xr::Result::SUCCESS
}

extern "system" fn destroy_action(action: xr::Action) -> xr::Result {
    destroy_handle(action)
}

extern "system" fn create_action_space(
    session: xr::Session,
    info: *const xr::ActionSpaceCreateInfo,
    space: *mut xr::Space,
) -> xr::Result {
    let session = get_handle!(session);
    let info = unsafe { info.as_ref() }.unwrap();
    let action = get_handle!(info.action);
    if !matches!(action.state.left.load().state, ActionState::Pose(_)) {
        return xr::Result::ERROR_ACTION_TYPE_MISMATCH;
    }

    let Some(instance) = session.instance.upgrade() else {
        return xr::Result::ERROR_INSTANCE_LOST;
    };
    let Ok(hand) = instance.get_user_path(info.subaction_path) else {
        return xr::Result::ERROR_PATH_INVALID;
    };
    let s = Arc::new(Space {
        ty: SpaceType::Action {
            hand,
            action: Arc::downgrade(&action),
        },
        offset: info.pose_in_action_space,
        session: Arc::downgrade(&session),
    });

    unsafe {
        *space = session.add_space(s);
    }
    xr::Result::SUCCESS
}

extern "system" fn get_system(
    _: xr::Instance,
    _: *const xr::SystemGetInfo,
    system_id: *mut xr::SystemId,
) -> xr::Result {
    unsafe { *system_id = xr::SystemId::from_raw(1) };
    xr::Result::SUCCESS
}

fn send_event<T: Copy>(
    tx: &mpsc::Sender<EventDataBuffer>,
    event: T,
    on_polled: Option<Box<dyn FnOnce() + Sync + Send>>,
) {
    const {
        assert!(std::mem::size_of::<T>() <= std::mem::size_of::<xr::EventDataBuffer>());
    }

    let buffer = unsafe {
        std::slice::from_raw_parts(&event as *const T as *const u8, std::mem::size_of::<T>())
    }
    .to_vec();
    tx.send(EventDataBuffer { buffer, on_polled }).unwrap();
}

extern "system" fn begin_session(_: xr::Session, _: *const xr::SessionBeginInfo) -> xr::Result {
    xr::Result::SUCCESS
}

extern "system" fn destroy_space(space: xr::Space) -> xr::Result {
    destroy_handle(space)
}

extern "system" fn create_reference_space(
    session: xr::Session,
    create_info: *const xr::ReferenceSpaceCreateInfo,
    space: *mut xr::Space,
) -> xr::Result {
    let info = unsafe { create_info.as_ref().unwrap() };
    assert_eq!(info.pose_in_reference_space, xr::Posef::IDENTITY);
    let session = get_handle!(session);

    unsafe {
        *space = session.add_space(Arc::new(Space {
            ty: SpaceType::Reference(info.reference_space_type),
            offset: info.pose_in_reference_space,
            session: Arc::downgrade(&session),
        }));
    }

    xr::Result::SUCCESS
}

extern "system" fn poll_event(
    instance: xr::Instance,
    buffer: *mut xr::EventDataBuffer,
) -> xr::Result {
    let instance = get_handle!(instance);
    let recv = instance.event_receiver.lock().unwrap();
    match recv.try_recv() {
        Ok(event) => {
            if let Some(on_polled) = event.on_polled {
                on_polled();
            }
            unsafe {
                buffer
                    .cast::<u8>()
                    .copy_from(event.buffer.as_ptr(), event.buffer.len());
            }
            xr::Result::SUCCESS
        }
        Err(mpsc::TryRecvError::Empty) => xr::Result::EVENT_UNAVAILABLE,
        Err(mpsc::TryRecvError::Disconnected) => unreachable!(),
    }
}

extern "system" fn string_to_path(
    instance: xr::Instance,
    string: *const c_char,
    path: *mut xr::Path,
) -> xr::Result {
    let instance = get_handle!(instance);
    let s = unsafe { CStr::from_ptr(string) }.to_str().unwrap();
    let mut string_to_path = instance.string_to_path.lock().unwrap();
    let key = match string_to_path.get(s) {
        Some(p) => *p,
        None => {
            let mut paths = instance.paths.lock().unwrap();
            let key = paths.insert(s.to_string());
            string_to_path.insert(s.to_string(), key);
            key
        }
    };

    unsafe { path.write(xr::Path::from_raw(key.data().as_ffi())) };

    xr::Result::SUCCESS
}

extern "system" fn path_to_string(
    instance: xr::Instance,
    path: xr::Path,
    capacity: u32,
    output: *mut u32,
    buffer: *mut c_char,
) -> xr::Result {
    let instance = get_handle!(instance);
    let key = DefaultKey::from(KeyData::from_ffi(path.into_raw()));
    let paths = instance.paths.lock().unwrap();
    let Some(val) = paths.get(key) else {
        return xr::Result::ERROR_PATH_INVALID;
    };
    let buf = [val.as_bytes(), &[0]].concat();
    unsafe { output.write(buf.len() as u32) };
    if capacity > 0 && capacity >= buf.len() as u32 {
        let out = unsafe { std::slice::from_raw_parts_mut(buffer as *mut _, capacity as usize) };
        out[0..buf.len()].copy_from_slice(&buf);
    }

    xr::Result::SUCCESS
}

extern "system" fn request_exit_session(session: xr::Session) -> xr::Result {
    let sess = get_handle!(session);
    send_event(
        &sess.event_sender,
        xr::EventDataSessionStateChanged {
            ty: xr::EventDataSessionStateChanged::TYPE,
            next: std::ptr::null(),
            session,
            state: xr::SessionState::STOPPING,
            time: xr::Time::from_nanos(0),
        },
        None,
    );
    xr::Result::SUCCESS
}

extern "system" fn end_session(session: xr::Session) -> xr::Result {
    let sess = get_handle!(session);
    send_event(
        &sess.event_sender,
        xr::EventDataSessionStateChanged {
            ty: xr::EventDataSessionStateChanged::TYPE,
            next: std::ptr::null(),
            session,
            state: xr::SessionState::EXITING,
            time: xr::Time::from_nanos(0),
        },
        None,
    );
    xr::Result::SUCCESS
}

extern "system" fn suggest_interaction_profile_bindings(
    instance: xr::Instance,
    binding: *const xr::InteractionProfileSuggestedBinding,
) -> xr::Result {
    let _ = get_handle!(instance);
    let binding = unsafe { binding.as_ref().unwrap() };

    let profile_path = binding.interaction_profile;
    let bindings = unsafe {
        std::slice::from_raw_parts(
            binding.suggested_bindings,
            binding.count_suggested_bindings as usize,
        )
    };

    for xr::ActionSuggestedBinding { action, binding } in bindings.iter().copied() {
        let action = get_handle!(action);
        action
            .suggested
            .lock()
            .unwrap()
            .entry(profile_path)
            .or_default()
            .push(binding);
    }

    xr::Result::SUCCESS
}

extern "system" fn attach_session_action_sets(
    session: xr::Session,
    info: *const xr::SessionActionSetsAttachInfo,
) -> xr::Result {
    let sesh = get_handle!(session);
    let sets =
        unsafe { std::slice::from_raw_parts((*info).action_sets, (*info).count_action_sets as _) };
    if sesh.attached_sets.set(sets.into()).is_ok() {
        // make action sets immutable
        for set in sesh.attached_sets.get().unwrap() {
            let set = get_handle!(*set);
            set.make_immutable();
        }
        xr::Result::SUCCESS
    } else {
        xr::Result::ERROR_ACTIONSETS_ALREADY_ATTACHED
    }
}

extern "system" fn sync_actions(
    session_xr: xr::Session,
    info: *const xr::ActionsSyncInfo,
) -> xr::Result {
    let session = get_handle!(session_xr);
    for hand in [&session.left_hand, &session.right_hand] {
        if let Some(profile) = hand.pending_profile.load() {
            hand.profile.store(profile);
            send_event(
                &session.event_sender,
                xr::EventDataInteractionProfileChanged {
                    ty: xr::EventDataInteractionProfileChanged::TYPE,
                    next: std::ptr::null(),
                    session: session_xr,
                },
                None,
            );
        }
    }
    let Some(attached) = session.attached_sets.get() else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };
    for set in attached {
        let set = get_handle!(*set);
        set.active.store(false, Ordering::Relaxed);
    }
    let sets = unsafe {
        std::slice::from_raw_parts(
            (*info).active_action_sets,
            (*info).count_active_action_sets as _,
        )
    };
    for set in sets {
        if !attached.contains(&set.action_set) {
            return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
        }
        let set = get_handle!(set.action_set);
        let Some(actions) = set.actions.get() else {
            return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
        };
        set.active.store(true, Ordering::Relaxed);

        for action in actions {
            // activate pose actions
            if matches!(action.state.left.load().state, ActionState::Pose(_)) {
                for state in [&action.state.left, &action.state.right] {
                    let mut d = state.load();
                    d.state = ActionState::Pose(true);
                    state.store(d);
                }
            } else {
                // other actions
                let data = action.pending_state.take();
                for (new, state) in [
                    (data.left, &action.state.left),
                    (data.right, &action.state.right),
                ] {
                    let mut d = state.load();
                    d.changed = false;
                    if let Some((new_state, change_time)) = new
                        && d.state != new_state {
                            d.changed = true;
                            d.state = new_state;
                            d.last_change_time = change_time;
                        }
                    state.store(d);
                }
            }
        }
    }

    let instance = session.instance.upgrade().unwrap();
    for inactive_set in instance
        .action_sets
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|set| !sets.iter().any(|active_set| *set == active_set.action_set))
    {
        let inactive_set = get_handle!(inactive_set);
        let Some(actions) = inactive_set.actions.get() else {
            continue;
        };
        for action in actions {
            if matches!(action.state.left.load().state, ActionState::Pose(_)) {
                for state in [&action.state.left, &action.state.right] {
                    let mut d = state.load();
                    d.state = ActionState::Pose(false);
                    state.store(d);
                }
            }
        }
    }

    xr::Result::SUCCESS
}

extern "system" fn get_action_state_boolean(
    session: xr::Session,
    info: *const xr::ActionStateGetInfo,
    state: *mut xr::ActionStateBoolean,
) -> xr::Result {
    unsafe {
        state.write(xr::ActionStateBoolean {
            ty: xr::ActionStateBoolean::TYPE,
            next: std::ptr::null_mut(),
            current_state: false.into(),
            changed_since_last_sync: false.into(),
            last_change_time: xr::Time::from_nanos(0),
            is_active: false.into(),
        });
    }
    let session = get_handle!(session);
    let Some((set, action)) = session.get_action_if_attached(info) else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };

    let info = unsafe { info.as_ref().unwrap() };
    let hand_state = action.get_hand_state(info.subaction_path);
    let ActionState::Bool(b) = hand_state.state else {
        return xr::Result::ERROR_ACTION_TYPE_MISMATCH;
    };
    let state = unsafe { state.as_mut().unwrap() };
    if set.active.load(Ordering::Relaxed) {
        let active = action.active.load(Ordering::Relaxed);
        if active {
            state.current_state = b.into();
            state.changed_since_last_sync = hand_state.changed.into();
            state.last_change_time = hand_state.last_change_time;
        }
        state.is_active = active.into();
    }
    xr::Result::SUCCESS
}

extern "system" fn get_action_state_float(
    session: xr::Session,
    info: *const xr::ActionStateGetInfo,
    state: *mut xr::ActionStateFloat,
) -> xr::Result {
    unsafe {
        state.write(xr::ActionStateFloat {
            ty: xr::ActionStateFloat::TYPE,
            next: std::ptr::null_mut(),
            current_state: 0.0,
            changed_since_last_sync: false.into(),
            last_change_time: xr::Time::from_nanos(0),
            is_active: false.into(),
        });
    }
    let session = get_handle!(session);
    let Some((set, action)) = session.get_action_if_attached(info) else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };
    let hand_state = action.get_hand_state(unsafe { (*info).subaction_path });
    let ActionState::Float(f) = hand_state.state else {
        return xr::Result::ERROR_ACTION_TYPE_MISMATCH;
    };
    let state = unsafe { state.as_mut().unwrap() };
    if set.active.load(Ordering::Relaxed) {
        let active = action.active.load(Ordering::Relaxed);
        if active {
            state.current_state = f;
        }
        state.is_active = active.into();
    }
    xr::Result::SUCCESS
}

extern "system" fn get_action_state_vector2f(
    session: xr::Session,
    info: *const xr::ActionStateGetInfo,
    state: *mut xr::ActionStateVector2f,
) -> xr::Result {
    unsafe {
        state.write(xr::ActionStateVector2f {
            ty: xr::ActionStateFloat::TYPE,
            next: std::ptr::null_mut(),
            current_state: xr::Vector2f::default(),
            changed_since_last_sync: false.into(),
            last_change_time: xr::Time::from_nanos(0),
            is_active: false.into(),
        });
    }
    let session = get_handle!(session);
    let Some((set, action)) = session.get_action_if_attached(info) else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };

    let hand_state = action.get_hand_state(unsafe { (*info).subaction_path });
    let ActionState::Vector2(x, y) = hand_state.state else {
        return xr::Result::ERROR_ACTION_TYPE_MISMATCH;
    };
    let state = unsafe { state.as_mut().unwrap() };
    if set.active.load(Ordering::Relaxed) {
        let active = action.active.load(Ordering::Relaxed);
        if active {
            state.current_state = xr::Vector2f { x, y };
        }
        state.is_active = active.into();
    }

    xr::Result::SUCCESS
}

extern "system" fn get_current_interaction_profile(
    session: xr::Session,
    user_path: xr::Path,
    state: *mut xr::InteractionProfileState,
) -> xr::Result {
    let session = get_handle!(session);
    let Some(instance) = session.instance.upgrade() else {
        return xr::Result::ERROR_INSTANCE_LOST;
    };
    let Ok(val) = instance.get_path_value(user_path) else {
        return xr::Result::ERROR_PATH_INVALID;
    };
    let profile = match val.as_deref() {
        Some("/user/hand/left") => session.left_hand.profile.load(),
        Some("/user/hand/right") => session.right_hand.profile.load(),
        _ => xr::Path::NULL,
    };

    unsafe {
        state.write(xr::InteractionProfileState {
            ty: xr::InteractionProfileState::TYPE,
            next: std::ptr::null_mut(),
            interaction_profile: profile,
        })
    }

    xr::Result::SUCCESS
}

extern "system" fn locate_space(
    space: xr::Space,
    base_space: xr::Space,
    _time: xr::Time,
    location: *mut xr::SpaceLocation,
) -> xr::Result {
    let base_space = get_handle!(base_space);
    assert!(
        !matches!(
            base_space.ty,
            SpaceType::Reference(xr::ReferenceSpaceType::STAGE | xr::ReferenceSpaceType::VIEW),
        ),
        "stage/view locate unimplemented"
    );

    let space = get_handle!(space);
    assert!(!matches!(
        space.ty,
        SpaceType::Reference(xr::ReferenceSpaceType::LOCAL)
    ));
    let next = unsafe { (*location).next };
    let mut out_loc = xr::SpaceLocation {
        ty: xr::SpaceLocation::TYPE,
        next,
        location_flags: xr::SpaceLocationFlags::EMPTY,
        pose: xr::Posef::IDENTITY,
    };

    if !next.is_null() {
        let header = next as *mut xr::BaseOutStructure;
        unsafe {
            if (*header).ty == xr::SpaceVelocity::TYPE {
                let velo = next as *mut xr::SpaceVelocity;
                velo.write(xr::SpaceVelocity {
                    ty: xr::SpaceVelocity::TYPE,
                    next: (*velo).next,
                    velocity_flags: xr::SpaceVelocityFlags::EMPTY,
                    linear_velocity: Default::default(),
                    angular_velocity: Default::default(),
                });
                out_loc.next = velo as _;
            }
        }
    }
    if matches!(
        base_space.ty,
        SpaceType::Reference(xr::ReferenceSpaceType::LOCAL)
    ) {
        match space.get_pose_relative_to_local() {
            Ok(loc) => {
                out_loc = loc;
            }
            Err(e) => return e,
        };
    } else {
        let base_loc = match base_space.get_pose_relative_to_local() {
            Ok(loc) => loc,
            Err(e) => return e,
        };

        let target_loc = match space.get_pose_relative_to_local() {
            Ok(loc) => loc,
            Err(e) => return e,
        };

        if base_loc.location_flags.contains(*LOCATION_FLAGS_TRACKED)
            && target_loc.location_flags.contains(*LOCATION_FLAGS_TRACKED)
        {
            out_loc.location_flags = *LOCATION_FLAGS_TRACKED;
            let base_mat = pose_to_mat(base_loc.pose);
            let target_mat = pose_to_mat(target_loc.pose);

            let out_mat = base_mat.inverse() * target_mat;
            out_loc.pose = mat_to_pose(out_mat);
        }
    }

    unsafe { location.write(out_loc) }

    xr::Result::SUCCESS
}
extern "system" fn create_swapchain(
    _session: xr::Session,
    info: *const xr::SwapchainCreateInfo,
    swapchain: *mut xr::Swapchain,
) -> xr::Result {
    let info = unsafe { info.as_ref() }.unwrap();
    if info.width == 0 || info.height == 0 {
        return xr::Result::ERROR_VALIDATION_FAILURE;
    }
    if info.format != 0 {
        return xr::Result::ERROR_SWAPCHAIN_FORMAT_UNSUPPORTED;
    }
    let swap = Arc::new(Swapchain {
        image_acquired: false.into(),
    });
    unsafe {
        swapchain.write(swap.to_xr());
    }
    xr::Result::SUCCESS
}

extern "system" fn destroy_swapchain(swapchain: xr::Swapchain) -> xr::Result {
    destroy_handle(swapchain)
}

extern "system" fn enumerate_swapchain_formats(
    _session: xr::Session,
    capacity: u32,
    output: *mut u32,
    formats: *mut i64,
) -> xr::Result {
    unsafe {
        output.write(1);
    }
    if capacity >= 1 {
        let formats = unsafe { std::slice::from_raw_parts_mut(formats, capacity as usize) };
        formats[0] = 0;
    }

    xr::Result::SUCCESS
}

extern "system" fn enumerate_swapchain_images(
    _swapchain: xr::Swapchain,
    _: u32,
    output: *mut u32,
    _: *mut xr::SwapchainImageBaseHeader,
) -> xr::Result {
    if let Some(output) = unsafe { output.as_mut() } {
        *output = 0;
    }
    xr::Result::SUCCESS
}

extern "system" fn acquire_swapchain_image(
    swapchain: xr::Swapchain,
    _info: *const xr::SwapchainImageAcquireInfo,
    _index: *mut u32,
) -> xr::Result {
    let swapchain = get_handle!(swapchain);
    swapchain.image_acquired.store(true, Ordering::Relaxed);
    xr::Result::SUCCESS
}

extern "system" fn wait_swapchain_image(
    swapchain: xr::Swapchain,
    _info: *const xr::SwapchainImageWaitInfo,
) -> xr::Result {
    let swapchain = get_handle!(swapchain);
    if !swapchain.image_acquired.load(Ordering::Relaxed) {
        return xr::Result::ERROR_CALL_ORDER_INVALID;
    }
    xr::Result::SUCCESS
}

extern "system" fn release_swapchain_image(
    swapchain: xr::Swapchain,
    _info: *const xr::SwapchainImageReleaseInfo,
) -> xr::Result {
    let swapchain = get_handle!(swapchain);
    if !swapchain.image_acquired.load(Ordering::Relaxed) {
        return xr::Result::ERROR_CALL_ORDER_INVALID;
    }
    swapchain.image_acquired.store(false, Ordering::Relaxed);
    xr::Result::SUCCESS
}

extern "system" fn wait_frame(
    session: xr::Session,
    _info: *const xr::FrameWaitInfo,
    state: *mut xr::FrameState,
) -> xr::Result {
    let session = get_handle!(session);
    if let Err(e) = transition_frame_state(&session.frame_state, FrameState::Waited) {
        return e;
    }
    unsafe {
        state.write(xr::FrameState {
            ty: xr::FrameState::TYPE,
            next: std::ptr::null_mut(),
            predicted_display_time: xr::Time::from_nanos(1),
            predicted_display_period: xr::Duration::from_nanos(1),
            should_render: session.should_render.load(Ordering::Relaxed).into(),
        })
    }
    xr::Result::SUCCESS
}

extern "system" fn begin_frame(
    session: xr::Session,
    _info: *const xr::FrameBeginInfo,
) -> xr::Result {
    let session = get_handle!(session);
    if let Err(e) = transition_frame_state(&session.frame_state, FrameState::Begun) {
        return e;
    }
    xr::Result::SUCCESS
}

extern "system" fn end_frame(session: xr::Session, _info: *const xr::FrameEndInfo) -> xr::Result {
    let session = get_handle!(session);
    if let Err(e) = transition_frame_state(&session.frame_state, FrameState::Ended) {
        return e;
    }
    if session.state.load() == xr::SessionState::READY {
        session.synchronized();
    }
    xr::Result::SUCCESS
}

extern "system" fn locate_views(
    session: xr::Session,
    _info: *const xr::ViewLocateInfo,
    state: *mut xr::ViewState,
    capacity: u32,
    output: *mut u32,
    views: *mut xr::View,
) -> xr::Result {
    let _session = get_handle!(session);
    if !state.is_null() {
        unsafe {
            state.write(xr::ViewState {
                ty: xr::ViewState::TYPE,
                next: std::ptr::null_mut(),
                view_state_flags: xr::ViewStateFlags::EMPTY,
            });
        }
    }

    if !output.is_null() {
        unsafe {
            output.write(2);
        }
    }
    if capacity > 0 {
        if capacity < 2 {
            return xr::Result::ERROR_SIZE_INSUFFICIENT;
        }
        let views = unsafe { std::slice::from_raw_parts_mut(views, capacity as usize) };
        let view = xr::View {
            ty: xr::View::TYPE,
            next: std::ptr::null_mut(),
            pose: xr::Posef::default(),
            fov: xr::Fovf::default(),
        };
        views[0] = view;
        views[1] = view;
    }

    xr::Result::SUCCESS
}

fn pose_to_mat(
    xr::Posef {
        position: p,
        orientation: r,
    }: xr::Posef,
) -> Affine3A {
    Affine3A::from_rotation_translation(
        Quat::from_xyzw(r.x, r.y, r.z, r.w),
        Vec3::new(p.x, p.y, p.z),
    )
}

fn mat_to_pose(mat: Affine3A) -> xr::Posef {
    let (_, rot, pos) = mat.to_scale_rotation_translation();
    xr::Posef {
        orientation: xr::Quaternionf {
            x: rot.x,
            y: rot.y,
            z: rot.z,
            w: rot.w,
        },
        position: xr::Vector3f {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        },
    }
}

extern "system" fn apply_haptic_feedback(
    session: xr::Session,
    action_info: *const xr::HapticActionInfo,
    haptic_feedback: *const xr::HapticBaseHeader,
) -> xr::Result {
    let session = get_handle!(session);

    const {
        assert!(
            std::mem::size_of::<xr::HapticActionInfo>()
                == std::mem::size_of::<xr::ActionStateGetInfo>()
        );
        assert!(
            std::mem::offset_of!(xr::HapticActionInfo, action)
                == std::mem::offset_of!(xr::ActionStateGetInfo, action)
        );
    }

    println!(
        "{}",
        unsafe { action_info.as_ref().unwrap().action }.into_raw()
    );
    let Some((_, action)) =
        session.get_action_if_attached(action_info as *const xr::ActionStateGetInfo)
    else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };

    let info = unsafe { action_info.as_ref().unwrap() };
    let mut hand_state = action.get_hand_state(info.subaction_path);
    let ActionState::Haptic(_) = hand_state.state else {
        return xr::Result::ERROR_ACTION_TYPE_MISMATCH;
    };

    #[allow(clippy::deref_addrof)]
    if unsafe { *(&raw const (*haptic_feedback).ty) } != xr::HapticVibration::TYPE {
        return xr::Result::ERROR_VALIDATION_FAILURE;
    }

    hand_state.state = ActionState::Haptic(true);

    let instance = session.instance.upgrade().unwrap();

    match DefaultKey::from(KeyData::from_ffi(info.subaction_path.into_raw())) {
        x if x == instance.left_hand_key => {
            action.state.left.store(hand_state);
        }
        x if x == instance.right_hand_key => {
            action.state.right.store(hand_state);
        }
        _ => unreachable!(),
    }

    xr::Result::SUCCESS
}
