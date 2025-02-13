pub mod vulkan;
use crossbeam_utils::atomic::AtomicCell;
use glam::{Affine3A, Quat, Vec3};
use openxr_sys as xr;
use paste::paste;
use slotmap::{DefaultKey, Key, KeyData, SlotMap};
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, LazyLock, Mutex, MutexGuard, OnceLock, RwLock, Weak,
};

#[derive(Clone, Copy, PartialEq)]
pub enum ActionState {
    Bool(bool),
    Pose,
    Float(f32),
    Vector2(f32, f32),
    Haptic,
}

impl From<bool> for ActionState {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

pub fn set_action_state(action: xr::Action, state: ActionState, hand: UserPath) {
    let action = action.to_handle().unwrap();
    assert_eq!(
        std::mem::discriminant(&state),
        std::mem::discriminant(&action.state.left.load().state)
    );
    let mut d = action.pending_state.take();
    match hand {
        UserPath::RightHand => {
            d.right = Some(state);
        }
        UserPath::LeftHand => {
            d.left = Some(state);
        }
    }
    action.pending_state.store(d);
    action.active.store(true, Ordering::Relaxed);
}

pub fn deactivate_action(action: xr::Action) {
    let action = action.to_handle().unwrap();
    action.active.store(false, Ordering::Relaxed);
}

#[derive(Copy, Clone, PartialEq)]
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

    fn to_path(&self) -> &'static str {
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

pub fn should_render_next_frame(instance: xr::Instance, should_render: bool) {
    let instance = instance.to_handle().unwrap();
    instance
        .should_render
        .store(should_render, Ordering::Relaxed)
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

pub extern "system" fn get_instance_proc_addr(
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
                (EnumerateSwapchainFormats),
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
                (ApplyHapticFeedback),
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
                (GetInputSourceLocalizedName)
                ]

                other => {
                    println!("unknown func: {other:?}");
                    return xr::Result::ERROR_FUNCTION_UNSUPPORTED;
                }
            ]
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
    unsafe { *property_count_output = 1 };
    if property_capacity_input > 0 {
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
        match <_ as XrType>::to_handle($handle) {
            Some(handle) => handle,
            None => {
                println!("unknown handle for {} ({:?})", stringify!($handle), $handle);
                return xr::Result::ERROR_HANDLE_INVALID;
            }
        }
    }};
}

macro_rules! impl_handle {
    ($ty:ty, $xr_type:ty) => {
        impl XrType for $xr_type {
            type Handle = $ty;
            const TO_RAW: fn(Self) -> u64 = <$xr_type>::into_raw;
            fn to_handle(self) -> Option<Arc<Self::Handle>> {
                Self::Handle::instances()
                    .get(DefaultKey::from(KeyData::from_ffi(self.into_raw())))
                    .map(|i| Arc::clone(i))
            }
        }
        impl Handle for $ty {
            type XrType = $xr_type;
            fn instances() -> MutexGuard<'static, SlotMap<DefaultKey, Arc<Self>>> {
                static I: LazyLock<Mutex<SlotMap<DefaultKey, Arc<$ty>>>> =
                    LazyLock::new(|| Mutex::default());
                I.lock().unwrap()
            }
            fn to_xr(self: Arc<Self>) -> $xr_type {
                let key = Self::instances().insert(self);
                <$xr_type>::from_raw(key.data().as_ffi())
            }
        }
    };
}

struct EventDataBuffer(Vec<u8>);

struct Instance {
    event_receiver: Mutex<mpsc::Receiver<EventDataBuffer>>,
    event_sender: mpsc::Sender<EventDataBuffer>,
    paths: Mutex<SlotMap<DefaultKey, String>>,
    string_to_path: Mutex<HashMap<String, DefaultKey>>,
    should_render: AtomicBool,
    action_sets: Mutex<HashSet<xr::ActionSet>>,
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
                .map(|s| Some(s))
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

struct Session {
    instance: Weak<Instance>,
    event_sender: mpsc::Sender<EventDataBuffer>,
    vk_device: AtomicU64,
    attached_sets: OnceLock<Box<[xr::ActionSet]>>,
    left_hand: HandData,
    right_hand: HandData,
    spaces: Mutex<HashSet<DefaultKey>>,
    frame_active: AtomicBool,
}

impl Drop for Session {
    fn drop(&mut self) {
        let spaces = self.spaces.lock().unwrap();
        for space in spaces.iter() {
            Space::instances().remove(*space);
        }
    }
}

static LOCATION_FLAGS_TRACKED: LazyLock<xr::SpaceLocationFlags> = LazyLock::new(|| {
    xr::SpaceLocationFlags::POSITION_VALID
        | xr::SpaceLocationFlags::POSITION_TRACKED
        | xr::SpaceLocationFlags::ORIENTATION_VALID
        | xr::SpaceLocationFlags::ORIENTATION_TRACKED
});

struct Space {
    hand: Option<UserPath>,
    offset: xr::Posef,
    session: Weak<Session>,
    action: Weak<Action>,
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

        // Check if this hand has an interaction profile
        let hand = self.hand.unwrap_or(UserPath::LeftHand);
        let hand_data = match hand {
            UserPath::LeftHand => &session.left_hand,
            UserPath::RightHand => &session.right_hand,
        };
        let hand_path = hand.to_path();
        let profile = match hand_data.profile.load() {
            xr::Path::NULL => {
                // no profile - no data
                return Ok(default());
            }
            other => other,
        };

        // Check if this action has bindings for the current profile
        let action = self.action.upgrade().unwrap();
        let bindings = action.suggested.lock().unwrap();
        let Some(bindings) = bindings.get(&profile) else {
            return Ok(default());
        };

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
            .expect(&format!(
                "expected binding for space for action {:?}",
                action.name
            ));

        let pose = match binding.strip_prefix(hand.to_path()).unwrap() {
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
    pending_state: AtomicCell<LeftRight<Option<ActionState>>>,
    suggested: Mutex<HashMap<xr::Path, Vec<xr::Path>>>,
}

impl Action {
    fn get_hand_state(&self, instance: &Instance, path: xr::Path) -> ActionStateData {
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
    paths.insert_with_key(|key| {
        string_to_path.insert(left.clone(), key);
        left
    });
    paths.insert_with_key(|key| {
        string_to_path.insert(right.clone(), key);
        right
    });
    let inst = Arc::new(Instance {
        event_receiver: rx.into(),
        event_sender: tx,
        paths: Mutex::new(paths),
        string_to_path: Mutex::new(string_to_path),
        should_render: false.into(),
        action_sets: Default::default(),
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
        frame_active: false.into(),
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
        xr::ActionType::POSE_INPUT => ActionState::Pose,
        xr::ActionType::FLOAT_INPUT => ActionState::Float(0.0),
        xr::ActionType::VECTOR2F_INPUT => ActionState::Vector2(0.0, 0.0),
        xr::ActionType::VIBRATION_OUTPUT => ActionState::Haptic,
        other => unimplemented!("unhandled action type: {other:?}"),
    };
    let data = ActionStateData {
        state,
        changed: false,
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
    if !matches!(action.state.left.load().state, ActionState::Pose) {
        return xr::Result::ERROR_ACTION_TYPE_MISMATCH;
    }

    let Some(instance) = session.instance.upgrade() else {
        return xr::Result::ERROR_INSTANCE_LOST;
    };
    let Ok(hand) = instance.get_user_path(info.subaction_path) else {
        return xr::Result::ERROR_PATH_INVALID;
    };
    let s = Arc::new(Space {
        hand,
        offset: info.pose_in_action_space,
        session: Arc::downgrade(&session),
        action: Arc::downgrade(&action),
    });
    let key = Space::instances().insert(s);
    let mut spaces = session.spaces.lock().unwrap();
    spaces.insert(key);
    unsafe { space.write(xr::Space::from_raw(key.data().as_ffi())) };
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

fn send_event<T: Copy>(tx: &mpsc::Sender<EventDataBuffer>, event: T) {
    const {
        assert!(std::mem::size_of::<T>() <= std::mem::size_of::<xr::EventDataBuffer>());
    }

    let s = unsafe {
        std::slice::from_raw_parts(&event as *const T as *const u8, std::mem::size_of::<T>())
    }
    .to_vec();
    tx.send(EventDataBuffer(s)).unwrap();
}

extern "system" fn begin_session(_: xr::Session, _: *const xr::SessionBeginInfo) -> xr::Result {
    xr::Result::SUCCESS
}

extern "system" fn destroy_space(space: xr::Space) -> xr::Result {
    destroy_handle(space)
}

static VIEW: LazyLock<xr::Space> = LazyLock::new(|| xr::Space::from_raw(1));
static LOCAL: LazyLock<xr::Space> = LazyLock::new(|| xr::Space::from_raw(2));
static STAGE: LazyLock<xr::Space> = LazyLock::new(|| xr::Space::from_raw(3));

extern "system" fn create_reference_space(
    _: xr::Session,
    create_info: *const xr::ReferenceSpaceCreateInfo,
    space: *mut xr::Space,
) -> xr::Result {
    let info = unsafe { create_info.as_ref().unwrap() };
    assert_eq!(info.pose_in_reference_space, xr::Posef::IDENTITY);
    unsafe {
        *space = match info.reference_space_type {
            xr::ReferenceSpaceType::VIEW => *VIEW,
            xr::ReferenceSpaceType::LOCAL => *LOCAL,
            xr::ReferenceSpaceType::STAGE => *STAGE,
            other => panic!("unimplemented reference space type: {other:?}"),
        };
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
            unsafe {
                buffer
                    .cast::<u8>()
                    .copy_from(event.0.as_ptr(), event.0.len());
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
            .entry(profile_path.clone())
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
        if let Some(profile) = hand.pending_profile.load().take() {
            hand.profile.store(profile);
            send_event(
                &session.event_sender,
                xr::EventDataInteractionProfileChanged {
                    ty: xr::EventDataInteractionProfileChanged::TYPE,
                    next: std::ptr::null(),
                    session: session_xr,
                },
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
            let data = action.pending_state.take();
            for (new, state) in [
                (data.left, &action.state.left),
                (data.right, &action.state.right),
            ] {
                let mut d = state.load();
                match new {
                    Some(new_state) => {
                        if d.state != new_state {
                            d.changed = true;
                            d.state = new_state;
                        }
                    }
                    None => {
                        d.changed = false;
                    }
                }
                state.store(d);
            }
        }
    }

    xr::Result::SUCCESS
}

fn get_action_if_attached(
    session: &Session,
    info: *const xr::ActionStateGetInfo,
) -> Option<(Arc<ActionSet>, Arc<Action>)> {
    let sets = session.attached_sets.get()?;
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
    let Some((set, action)) = get_action_if_attached(&session, info) else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };

    let info = unsafe { info.as_ref().unwrap() };
    let instance = session.instance.upgrade().unwrap();
    let hand_state = action.get_hand_state(&instance, info.subaction_path);
    let ActionState::Bool(b) = hand_state.state else {
        return xr::Result::ERROR_ACTION_TYPE_MISMATCH;
    };
    let state = unsafe { state.as_mut().unwrap() };
    if set.active.load(Ordering::Relaxed) {
        let active = action.active.load(Ordering::Relaxed);
        if active {
            state.current_state = b.into();
            state.changed_since_last_sync = hand_state.changed.into();
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
    let Some((set, action)) = get_action_if_attached(&session, info) else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };
    let instance = session.instance.upgrade().unwrap();
    let hand_state = action.get_hand_state(&instance, unsafe { (*info).subaction_path });
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
    let Some((set, action)) = get_action_if_attached(&session, info) else {
        return xr::Result::ERROR_ACTIONSET_NOT_ATTACHED;
    };

    let instance = session.instance.upgrade().unwrap();
    let hand_state = action.get_hand_state(&instance, unsafe { (*info).subaction_path });
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
    let profile = match val.as_ref().map(String::as_str) {
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
    assert!(
        base_space != *STAGE && base_space != *VIEW,
        "stage/view locate unimplemented"
    );
    assert_ne!(space, *LOCAL);

    let space = get_handle!(space);
    let next = unsafe { *&raw mut (*location).next };
    let mut out_loc = xr::SpaceLocation {
        ty: xr::SpaceLocation::TYPE,
        next,
        location_flags: xr::SpaceLocationFlags::EMPTY,
        pose: xr::Posef::IDENTITY,
    };

    if !next.is_null() {
        let header = next as *mut xr::BaseOutStructure;
        unsafe {
            if *&raw mut (*header).ty == xr::SpaceVelocity::TYPE {
                let velo = next as *mut xr::SpaceVelocity;
                velo.write(xr::SpaceVelocity {
                    ty: xr::SpaceVelocity::TYPE,
                    next: *&raw mut (*velo).next,
                    velocity_flags: xr::SpaceVelocityFlags::EMPTY,
                    linear_velocity: Default::default(),
                    angular_velocity: Default::default(),
                });
                out_loc.next = velo as _;
            }
        }
    }
    if base_space == *LOCAL {
        match space.get_pose_relative_to_local() {
            Ok(loc) => {
                out_loc = loc;
            }
            Err(e) => return e,
        };
    } else {
        let base_space = get_handle!(base_space);
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
    let instance = session.instance.upgrade().unwrap();
    unsafe {
        state.write(xr::FrameState {
            ty: xr::FrameState::TYPE,
            next: std::ptr::null_mut(),
            predicted_display_time: xr::Time::from_nanos(1),
            predicted_display_period: xr::Duration::from_nanos(1),
            should_render: instance.should_render.load(Ordering::Relaxed).into(),
        })
    }
    xr::Result::SUCCESS
}

extern "system" fn begin_frame(
    session: xr::Session,
    _info: *const xr::FrameBeginInfo,
) -> xr::Result {
    let session = get_handle!(session);
    session.frame_active.store(true, Ordering::Relaxed);
    xr::Result::SUCCESS
}

extern "system" fn end_frame(session: xr::Session, _info: *const xr::FrameEndInfo) -> xr::Result {
    let session = get_handle!(session);
    if !session.frame_active.load(Ordering::Relaxed) {
        return xr::Result::ERROR_CALL_ORDER_INVALID;
    }
    session.frame_active.store(false, Ordering::Relaxed);
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
