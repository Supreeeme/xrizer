use super::{
    custom_bindings::{
        DpadData, DpadDirection, BindingData, DpadActions, GrabActions, GrabBindingData
    },
    legacy::{LegacyActionData, LegacyActions},
    profiles::{InteractionProfile, PathTranslation, Profiles},
    ActionData, ActionKey, BoundPose, BoundPoseType, ExtraActionData, Input
};
use crate::{
    input::skeletal::SkeletalInputActionData,
    openxr_data::{self, Hand, SessionData},
};
use log::{debug, error, info, trace, warn};
use openvr as vr;
use openxr as xr;
use serde::{
    de::{Error, IgnoredAny, Unexpected},
    Deserialize,
};
use slotmap::{SecondaryMap, SlotMap};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::RwLock;
use std::{
    cell::{LazyCell, RefCell},
    env::current_dir,
};

fn action_map_to_secondary<T>(act_guard: &mut SlotMap<ActionKey, super::Action>, map: HashMap<String, T>) -> SecondaryMap<ActionKey, T> {
    map.into_iter()
        .map(|(name, action)| {
            let key = act_guard
                .iter()
                .find_map(|(key, super::Action { path })| (*path == name).then_some(key))
                .unwrap_or_else(|| act_guard.insert(super::Action { path: name }));

            (key, action)
        })
        .collect()
}

impl<C: openxr_data::Compositor> Input<C> {
    pub(super) fn load_action_manifest(
        &self,
        session_data: &SessionData,
        manifest_path: &Path,
    ) -> Result<(), vr::EVRInputError> {
        match self.loaded_actions_path.get() {
            Some(p) => {
                assert_eq!(p, manifest_path);
            }
            None => self
                .loaded_actions_path
                .set(manifest_path.to_path_buf())
                .unwrap(),
        }

        let data = std::fs::read(manifest_path).map_err(|e| {
            error!("Failed to read manifest {}: {e}", manifest_path.display());
            vr::EVRInputError::InvalidParam
        })?;

        let manifest: ActionManifest = serde_json::from_slice(&data).map_err(|e| {
            error!("Failed to parse action manifest: {e}");
            vr::EVRInputError::InvalidParam
        })?;

        // TODO: support non english localization?
        let english = manifest
            .localization
            .and_then(|l| l.into_iter().find(|l| l.language_tag == "en_US"));

        let mut sets = load_action_sets(
            &self.openxr.instance,
            english.as_ref(),
            manifest.action_sets,
        )?;
        debug!("Loaded {} action sets.", sets.len());

        let mut actions = load_actions(
            &self.openxr.instance,
            &session_data.session,
            english.as_ref(),
            &mut sets,
            manifest.actions,
            self.openxr.left_hand.subaction_path,
            self.openxr.right_hand.subaction_path,
        )?;
        debug!("Loaded {} actions.", actions.len());

        // Games can mix legacy and normal input, and the legacy bindings are used for
        // WaitGetPoses, so attach the legacy set here as well.
        let legacy = session_data.input_data.legacy_actions.get_or_init(|| {
            LegacyActionData::new(
                &self.openxr.instance,
                self.openxr.left_hand.subaction_path,
                self.openxr.right_hand.subaction_path,
            )
        });

        let skeletal_input = session_data
            .input_data
            .estimated_skeleton_actions
            .get_or_init(|| {
                SkeletalInputActionData::new(
                    &self.openxr.instance,
                    self.openxr.left_hand.subaction_path,
                    self.openxr.right_hand.subaction_path,
                )
            });

        // See Input::frame_start_update for the explanation of this.
        let info_set = self
            .openxr
            .instance
            .create_action_set("xrizer-info-set", "XRizer info set", 0)
            .unwrap();
        let info_action = info_set
            .create_action::<bool>("xrizer-info-action", "XRizer info action", &[])
            .unwrap();

        let mut per_profile_bindings = HashMap::new();
        let mut per_profile_pose_bindings = HashMap::new();
        let mut extra_actions = HashMap::new();

        self.load_bindings(
            manifest_path.parent().unwrap(),
            &sets,
            &mut actions,
            &mut extra_actions,
            &mut per_profile_bindings,
            &mut per_profile_pose_bindings,
            manifest.default_bindings,
            &legacy.actions,
            &info_action,
            skeletal_input,
        );

        let xr_sets: Vec<_> = sets
            .values()
            .chain([&legacy.set, &info_set, &skeletal_input.set])
            .collect();
        session_data.session.attach_action_sets(&xr_sets).unwrap();

        // Try forcing an interaction profile now
        session_data
            .session
            .sync_actions(&[xr::ActiveActionSet::new(&info_set)])
            .unwrap();

        // Transform actions and sets into maps
        // If the application has already requested the handle for an action/set, we need to
        // reuse the corresponding slot. Otherwise just create a new one.
        let mut set_guard = self.set_map.write().unwrap();
        let sets: SecondaryMap<_, _> = sets
            .into_iter()
            .map(|(set_name, set)| {
                // This function is only called when loading the action manifest, and most games
                // don't have a ton of actions, so a linear search through the map is probably fine.
                let key = set_guard
                    .iter()
                    .find_map(|(key, set_path)| (*set_path == set_name).then_some(key))
                    .unwrap_or_else(|| set_guard.insert(set_name));
                (key, set)
            })
            .collect();

        let mut act_guard = self.action_map.write().unwrap();
        let actions = action_map_to_secondary(&mut act_guard, actions);
        let extra_actions = action_map_to_secondary(&mut act_guard, extra_actions);

        let per_profile_bindings = per_profile_bindings
            .into_iter()
            .map(|(k, v)| (k, action_map_to_secondary(&mut act_guard, v)))
            .collect();

        let per_profile_pose_bindings = per_profile_pose_bindings
            .into_iter()
            .map(|(k, v)| (k, action_map_to_secondary(&mut act_guard, v)))
            .collect();

        let loaded = super::LoadedActions {
            sets,
            actions,
            extra_actions,
            per_profile_bindings,
            per_profile_pose_bindings,
            _info_action: info_action,
            info_set,
        };

        match session_data.input_data.loaded_actions.get() {
            Some(lock) => {
                *lock.write().unwrap() = loaded;
            }
            None => {
                session_data
                    .input_data
                    .loaded_actions
                    .set(RwLock::new(loaded))
                    .unwrap_or_else(|_| unreachable!());
            }
        }
        Ok(())
    }
}

/**
 * Structure for action manifests.
 * https://github.com/ValveSoftware/openvr/wiki/Action-manifest
 */

#[derive(Deserialize)]
struct ActionManifest {
    default_bindings: Vec<DefaultBindings>,
    #[serde(default)] // optional apparently
    action_sets: Vec<ActionSetJson>,
    actions: Vec<ActionType>,
    localization: Option<Vec<Localization>>,
    // localization_files
}

#[derive(Deserialize)]
struct DefaultBindings {
    binding_url: PathBuf,
    controller_type: ControllerType,
}

#[derive(Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub(super) enum ControllerType {
    ViveController,
    Knuckles,
    OculusTouch,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Deserialize)]
struct ActionSetJson {
    #[serde(rename = "name")]
    path: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum ActionType {
    Boolean(ActionDataCommon),
    Vector1(ActionDataCommon),
    Vector2(ActionDataCommon),
    Vibration(ActionDataCommon),
    Pose(ActionDataCommon),
    Skeleton(SkeletonData),
}

#[derive(Deserialize)]
struct ActionDataCommon {
    name: String,
}

#[derive(Deserialize)]
struct SkeletonData {
    #[serde(deserialize_with = "parse_skeleton")]
    skeleton: Hand,
    #[serde(flatten)]
    data: ActionDataCommon,
}

fn parse_skeleton<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Hand, D::Error> {
    let path: &str = Deserialize::deserialize(d)?;
    let Some(hand) = path.strip_prefix("/skeleton/hand") else {
        return Err(D::Error::invalid_value(
            Unexpected::Str(path),
            &"path starting with /skeleton/hand",
        ));
    };

    match hand {
        "/left" => Ok(Hand::Left),
        "/right" => Ok(Hand::Right),
        _ => Err(D::Error::invalid_value(
            Unexpected::Str(hand),
            &r#""/left" or "/right""#,
        )),
    }
}

#[derive(Deserialize)]
struct Localization {
    language_tag: String,
    #[serde(flatten)]
    localized_names: HashMap<String, String>,
}

fn create_action_set(
    instance: &xr::Instance,
    path: &str,
    localized: Option<&str>,
) -> Result<xr::ActionSet, vr::EVRInputError> {
    // OpenXR does not like the "/actions/<set name>" format, so we need to strip the prefix
    let Some(xr_friendly_name) = path.strip_prefix("/actions/") else {
        error!("Action set {path} missing actions prefix.");
        return Err(vr::EVRInputError::InvalidParam);
    };

    trace!("Creating action set {xr_friendly_name} ({path:?}) (localized: {localized:?})");
    instance
        .create_action_set(xr_friendly_name, localized.unwrap_or(path), 0)
        .map_err(|e| {
            error!("Failed to create action set {xr_friendly_name}: {e}");
            vr::EVRInputError::InvalidParam
        })
}

fn load_action_sets(
    instance: &xr::Instance,
    english: Option<&Localization>,
    sets: Vec<ActionSetJson>,
) -> Result<HashMap<String, xr::ActionSet>, vr::EVRInputError> {
    let mut action_sets = HashMap::new();
    for ActionSetJson { path } in sets {
        let localized = english.and_then(|e| e.localized_names.get(&path));

        let path = path.to_lowercase();
        let set = create_action_set(instance, &path, localized.map(String::as_str))?;
        action_sets.insert(path, set);
    }
    Ok(action_sets)
}

type LoadedActionDataMap = HashMap<String, super::ActionData>;

#[track_caller]
fn find_action(actions: &LoadedActionDataMap, name: &str) -> bool {
    let ret = actions.contains_key(name);
    if !ret {
        let caller = std::panic::Location::caller();
        warn!(
            "Couldn't find action {name}, skipping (line {})",
            caller.line()
        );
    }
    ret
}

fn load_actions(
    instance: &xr::Instance,
    session: &xr::Session<xr::AnyGraphics>,
    english: Option<&Localization>,
    sets: &mut HashMap<String, xr::ActionSet>,
    actions: Vec<ActionType>,
    left_hand: xr::Path,
    right_hand: xr::Path,
) -> Result<HashMap<String, super::ActionData>, vr::EVRInputError> {
    let mut ret = HashMap::with_capacity(actions.len());
    let mut long_name_idx = 0;
    for action in actions {
        fn create_action<T: xr::ActionTy>(
            instance: &xr::Instance,
            data: &ActionDataCommon,
            sets: &mut HashMap<String, xr::ActionSet>,
            english: Option<&Localization>,
            paths: &[xr::Path],
            long_name_idx: &mut usize,
        ) -> xr::Result<xr::Action<T>> {
            let localized = english
                .and_then(|e| e.localized_names.get(&data.name))
                .map(|s| s.as_str());

            let path = data.name.to_lowercase();
            let set_end_idx = path.match_indices('/').nth(2).unwrap().0;
            let set_name = &path[0..set_end_idx];
            let entry;
            let set = if let Some(set) = sets.get(set_name) {
                set
            } else {
                warn!("Action set {set_name} is missing from manifest, creating it...");
                let set = create_action_set(instance, set_name, None).map_err(|e| {
                    error!("Creating implicit action set failed: {e:?}");
                    xr::sys::Result::ERROR_INITIALIZATION_FAILED
                })?;
                entry = sets.entry(set_name.to_string()).insert_entry(set);
                entry.get()
            };
            let mut xr_friendly_name = path.rsplit_once('/').unwrap().1.replace([' ', ','], "_");
            if xr_friendly_name.len() > xr::sys::MAX_ACTION_NAME_SIZE {
                let idx_str = ["_ln", &long_name_idx.to_string()].concat();
                xr_friendly_name.replace_range(
                    xr::sys::MAX_ACTION_NAME_SIZE - idx_str.len() - 1..,
                    &idx_str,
                );
                *long_name_idx += 1;
            }
            let localized = localized.unwrap_or(&xr_friendly_name);
            trace!(
                "Creating action {xr_friendly_name} (localized: {localized}) in set {set_name:?}"
            );

            set.create_action(&xr_friendly_name, localized, paths)
                .or_else(|err| {
                    // If we get a duplicated localized name, just deduplicate it and try again
                    if err == xr::sys::Result::ERROR_LOCALIZED_NAME_DUPLICATED {
                        // Action names are inherently unique, so just throw it at the end of the
                        // localized name to make it a unique
                        let localized = format!("{localized} ({xr_friendly_name})");
                        set.create_action(&xr_friendly_name, &localized, paths)
                    } else {
                        Err(err)
                    }
                })
        }

        let paths = &[left_hand, right_hand];
        macro_rules! create_action {
            ($ty:ty, $data:expr) => {
                create_action::<$ty>(instance, &$data, sets, english, paths, &mut long_name_idx)
                    .unwrap()
            };
        }
        use super::ActionData::*;
        let (path, action) = match &action {
            ActionType::Boolean(data) => (
                &data.name,
                Bool(create_action!(bool, data)),
            ),
            ActionType::Vector1(data) => (
                &data.name,
                Vector1 {
                    action: create_action!(f32, data),
                    last_value: Default::default(),
                },
            ),
            ActionType::Vector2(data) => (
                &data.name,
                Vector2 {
                    action: create_action!(xr::Vector2f, data),
                    last_value: Default::default(),
                },
            ),
            ActionType::Pose(data) => (
                &data.name,
                Pose,
            ),
            ActionType::Skeleton(SkeletonData { skeleton, data }) => {
                trace!("Creating skeleton action {}", data.name.to_lowercase());
                let hand_tracker = match session.create_hand_tracker(match skeleton {
                    Hand::Left => xr::Hand::LEFT,
                    Hand::Right => xr::Hand::RIGHT,
                }) {
                    Ok(t) => Some(t),
                    Err(
                        xr::sys::Result::ERROR_EXTENSION_NOT_PRESENT
                        | xr::sys::Result::ERROR_FEATURE_UNSUPPORTED,
                    ) => None,
                    Err(other) => panic!("Creating hand tracker failed: {other:?}"),
                };

                (
                    &data.name,
                    Skeleton {
                        hand: *skeleton,
                        hand_tracker,
                    },
                )
            }
            ActionType::Vibration(data) => (&data.name, Haptic(create_action!(xr::Haptic, data))),
        };
        ret.insert(path.to_lowercase(), action);
    }
    Ok(ret)
}

/**
 * Structure for binding files
 */

#[derive(Deserialize)]
struct Bindings {
    bindings: HashMap<String, ActionSetBinding>,
}

#[derive(Deserialize)]
struct ActionSetBinding {
    sources: Vec<ActionBinding>,
    poses: Option<Vec<PoseBinding>>,
    haptics: Option<Vec<SimpleActionBinding>>,
    skeleton: Option<Vec<SkeletonActionBinding>>,
}

#[repr(transparent)]
#[derive(Hash, Eq, PartialEq)]
struct LowercaseActionPath(String);
impl std::fmt::Debug for LowercaseActionPath {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Display for LowercaseActionPath {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::ops::Deref for LowercaseActionPath {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<'de> Deserialize<'de> for LowercaseActionPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|s| Self(s.to_ascii_lowercase()))
    }
}

#[derive(Deserialize)]
struct PoseBinding {
    output: LowercaseActionPath,
    #[serde(deserialize_with = "parse_pose_binding")]
    path: (Hand, BoundPoseType),
}

fn parse_pose_binding<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(Hand, BoundPoseType), D::Error> {
    let pose_path: &str = Deserialize::deserialize(d)?;

    let (hand, pose) = pose_path.rsplit_once('/').ok_or(D::Error::invalid_value(
        Unexpected::Str(pose_path),
        &"a value matching /user/hand/{left,right}/pose/<pose>",
    ))?;

    let hand = match hand {
        "/user/hand/left/pose" => Hand::Left,
        "/user/hand/right/pose" => Hand::Right,
        _ => {
            return Err(D::Error::unknown_variant(
                hand,
                &["/user/hand/left/pose", "/user/hand/right/pose"],
            ))
        }
    };

    let pose = match pose {
        "raw" => BoundPoseType::Raw,
        "gdc2015" => BoundPoseType::Gdc2015,
        other => return Err(D::Error::unknown_variant(other, &["raw", "gdc2015"])),
    };

    Ok((hand, pose))
}

#[derive(Deserialize)]
struct SimpleActionBinding {
    output: LowercaseActionPath,
    path: String,
}

#[derive(Deserialize)]
struct SkeletonActionBinding {
    output: LowercaseActionPath,
    #[serde(deserialize_with = "path_to_skeleton")]
    path: Hand,
}

fn path_to_skeleton<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Hand, D::Error> {
    let path: &str = Deserialize::deserialize(d)?;
    match path {
        "/user/hand/left/input/skeleton/left" => Ok(Hand::Left),
        "/user/hand/right/input/skeleton/right" => Ok(Hand::Right),
        other => Err(D::Error::invalid_value(
            Unexpected::Str(other),
            &"/user/hand/left/input/skeleton/left or /user/hand/right/input/skeleton/right",
        )),
    }
}

#[derive(Deserialize, Debug)]
struct ActionBindingOutput {
    output: LowercaseActionPath,
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum ActionBinding {
    None(IgnoredAny),
    Button {
        path: String,
        inputs: ButtonInput,
        #[allow(unused)]
        parameters: Option<ButtonParameters>,
    },
    ToggleButton {
        path: String,
        inputs: ButtonInput,
    },
    Dpad {
        path: String,
        inputs: DpadInput,
        parameters: Option<DpadParameters>,
    },
    Trigger {
        path: String,
        inputs: TriggerInput,
        #[allow(unused)]
        parameters: Option<ClickThresholdParams>,
    },
    ScalarConstant {
        path: String,
        inputs: ScalarConstantInput,
        #[allow(unused)]
        parameters: Option<ScalarConstantParameters>,
    },
    ForceSensor {
        path: String,
        inputs: ForceSensorInput,
        #[allow(unused)]
        parameters: Option<ForceSensorParameters>,
    },
    Grab {
        path: String,
        inputs: GrabInput,
        #[allow(unused)]
        parameters: Option<GrabParameters>,
    },
    Scroll {
        #[allow(unused)]
        path: String,
        inputs: ScrollInput,
        #[allow(unused)]
        parameters: Option<ScrollParameters>,
    },
    Trackpad(Vector2Mode),
    Joystick(Vector2Mode),
}

#[repr(transparent)]
struct FromString<T>(T);

impl<T: FromStr> FromStr for FromString<T> {
    type Err = T::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_str(s).map(Self)
    }
}

impl<'de, T: Deserialize<'de> + FromStr> Deserialize<'de> for FromString<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ret = <&str>::deserialize(deserializer)?;
        ret.parse().map_err(|_| {
            D::Error::custom(format_args!(
                "invalid value: expected {}, got {ret}",
                std::any::type_name::<T>()
            ))
        })
    }
}

#[derive(Deserialize)]
struct ButtonInput {
    click: Option<ActionBindingOutput>,
    double: Option<ActionBindingOutput>,
}

#[derive(Deserialize)]
struct ClickThresholdParams {
    #[allow(unused)]
    click_activate_threshold: Option<FromString<f32>>,
    #[allow(unused)]
    click_deactivate_threshold: Option<FromString<f32>>,
}

#[derive(Deserialize)]
struct ScalarConstantParameters {
    #[serde(rename = "on/x")]
    #[allow(unused)]
    on_x: Option<String>
}

#[derive(Deserialize)]
struct ButtonParameters {
    #[allow(unused)]
    force_input: Option<String>,
    #[allow(unused)]
    #[serde(flatten)]
    click_threshold: ClickThresholdParams,
}

#[derive(Deserialize, Debug)]
struct DpadInput {
    east: Option<ActionBindingOutput>,
    south: Option<ActionBindingOutput>,
    north: Option<ActionBindingOutput>,
    west: Option<ActionBindingOutput>,
    center: Option<ActionBindingOutput>,
}

#[derive(Deserialize)]
#[serde(default)]
struct DpadParameters {
    sub_mode: DpadSubMode,
    deadzone_pct: FromString<u8>,
    overlap_pct: FromString<u8>,
    sticky: FromString<bool>,
}

impl Default for DpadParameters {
    fn default() -> Self {
        Self {
            sub_mode: DpadSubMode::Touch,
            deadzone_pct: FromString(50),
            overlap_pct: FromString(50),
            sticky: FromString(false),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum DpadSubMode {
    Click,
    Touch,
}

#[derive(Deserialize)]
struct TriggerInput {
    pull: Option<ActionBindingOutput>,
    touch: Option<ActionBindingOutput>,
    click: Option<ActionBindingOutput>,
}

#[derive(Deserialize)]
struct ScalarConstantInput {
    value: ActionBindingOutput,
}

#[derive(Deserialize)]
struct ForceSensorInput {
    force: ActionBindingOutput,
}

#[derive(Deserialize)]
struct ForceSensorParameters {
    #[allow(unused)]
    haptic_amplitude: Option<String>,
}

#[derive(Deserialize)]
struct GrabInput {
    grab: ActionBindingOutput,
}

#[derive(Deserialize)]
struct GrabParameters {
    #[allow(unused)]
    value_hold_threshold: Option<FromString<f32>>,
    #[allow(unused)]
    value_release_threshold: Option<FromString<f32>>,
}

#[derive(Deserialize)]
struct ScrollInput {
    scroll: ActionBindingOutput,
}

#[derive(Deserialize)]
struct ScrollParameters {
    #[allow(unused)]
    scroll_mode: Option<String>,
    #[allow(unused)]
    smooth_scroll_multiplier: Option<String>, // float
}

#[derive(Deserialize)]
struct Vector2Mode {
    path: String,
    inputs: Vector2Input,
}

#[derive(Deserialize)]
struct Vector2Input {
    position: Option<ActionBindingOutput>,
    click: Option<ActionBindingOutput>,
    touch: Option<ActionBindingOutput>,
}

impl<C: openxr_data::Compositor> Input<C> {
    #[allow(clippy::too_many_arguments)]
    fn load_bindings(
        &self,
        parent_path: &Path,
        action_sets: &HashMap<String, xr::ActionSet>,
        actions: &mut LoadedActionDataMap,
        extra_actions: &mut HashMap<String, ExtraActionData>,
        per_profile_bindings: &mut HashMap<xr::Path, HashMap<String, Vec<BindingData>>>,
        per_profile_pose_bindings: &mut HashMap<xr::Path, HashMap<String, BoundPose>>,
        bindings: Vec<DefaultBindings>,
        legacy_actions: &LegacyActions,
        info_action: &xr::Action<bool>,
        skeletal_input: &SkeletalInputActionData,
    ) {
        let mut it: Box<dyn Iterator<Item = DefaultBindings>> = Box::new(bindings.into_iter());
        while let Some(DefaultBindings {
            binding_url,
            controller_type,
        }) = it.next()
        {
            let load_bindings = || {
                let custom_path =
                    if let Ok(custom_dir) = std::env::var("XRIZER_CUSTOM_BINDINGS_DIR") {
                        PathBuf::from(custom_dir)
                    } else {
                        current_dir().unwrap().join("xrizer")
                    }
                    .join(format!("{controller_type:?}.json").to_lowercase());
                let bindings_path = match custom_path.exists() {
                    true => custom_path,
                    false => parent_path.join(binding_url),
                };
                debug!(
                    "Reading bindings for {controller_type:?} (at {})",
                    bindings_path.display()
                );

                let data = std::fs::read(bindings_path)
                    .inspect_err(|e| error!("Couldn't load bindings for {controller_type:?}: {e}"))
                    .ok()?;

                let Bindings { bindings } = serde_json::from_slice(&data)
                    .inspect_err(|e| {
                        error!("Failed to parse bindings for {controller_type:?}: {e}")
                    })
                    .ok()?;

                Some(bindings)
            };
            match controller_type {
                ControllerType::Unknown(ref other) => {
                    info!("Ignoring bindings for unknown profile {other}")
                }
                ref other => {
                    let profiles = Profiles::get()
                        .list
                        .iter()
                        .filter_map(|(ty, p)| (*ty == *other).then_some(*p));
                    let bindings = LazyCell::new(load_bindings);
                    for profile in profiles {
                        let Ok(interaction_profile) = self.openxr.instance.string_to_path(profile.profile_path()) else {
                            warn!("Controller type {other:?} has no OpenXR path supported?");
                            continue;
                        };
                        if let Some(bindings) = bindings.as_ref() {
                            self.load_bindings_for_profile(
                                profile,
                                action_sets,
                                actions,
                                extra_actions,
                                per_profile_bindings.entry(interaction_profile).or_insert_with(HashMap::new),
                                per_profile_pose_bindings.entry(interaction_profile).or_insert_with(Default::default),
                                legacy_actions,
                                info_action,
                                skeletal_input,
                                bindings,
                            );
                        }
                    }
                }
            }

            it = Box::new(it.skip_while(move |b| {
                if b.controller_type == controller_type {
                    info!("skipping bindings in {:?}", b.binding_url);
                    true
                } else {
                    false
                }
            }));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn load_bindings_for_profile(
        &self,
        profile: &dyn InteractionProfile,
        action_sets: &HashMap<String, xr::ActionSet>,
        actions: &mut LoadedActionDataMap,
        extra_actions: &mut HashMap<String, ExtraActionData>,
        bindings_parsed: &mut HashMap<String, Vec<BindingData>>,
        bound_pose: &mut HashMap<String, BoundPose>,
        legacy_actions: &LegacyActions,
        info_action: &xr::Action<bool>,
        skeletal_input: &SkeletalInputActionData,
        bindings: &HashMap<String, ActionSetBinding>,
    ) {
        info!("loading bindings for {}", profile.profile_path());

        // Workaround weird closure lifetime quirks.
        const fn constrain<F>(f: F) -> F
        where
            F: for<'a> Fn(&'a str) -> openxr::Path,
        {
            f
        }
        let stp = constrain(|s| self.openxr.instance.string_to_path(s).unwrap());
        let legacy_bindings = profile.legacy_bindings(&stp);
        let skeletal_bindings = profile.skeletal_input_bindings(&stp);
        let profile_path = stp(profile.profile_path());
        let legal_paths = profile.legal_paths();
        let translate_map = profile.translate_map();
        let path_translator = |path: &str| {
            let mut translated = path.to_string();
            for PathTranslation { from, to, stop } in translate_map {
                if translated.contains(from) {
                    translated = translated.replace(from, to);
                    if *stop {
                        break;
                    }
                }
            }
            trace!("translated {path} to {translated}");
            if !legal_paths.contains(&translated) {
                Err(InvalidActionPath(format!(
                    "Action for invalid path {translated}, ignoring"
                )))
            } else {
                Ok(translated)
            }
        };

        let mut xr_bindings = Vec::new();
        for (action_set_name, bindings) in bindings.iter() {
            let Some(set) = action_sets.get(action_set_name) else {
                warn!("Action set {action_set_name} missing.");
                continue;
            };

            if let Some(bindings) = &bindings.haptics {
                xr_bindings.extend(handle_haptic_bindings(
                    &self.openxr.instance,
                    path_translator,
                    actions,
                    bindings,
                ));
            }

            if let Some(bindings) = &bindings.poses {
                handle_pose_bindings(actions, bindings, bound_pose);
            }

            if let Some(bindings) = &bindings.skeleton {
                handle_skeleton_bindings(actions, bindings);
            }

            xr_bindings.extend(handle_sources(
                &self.openxr.instance,
                path_translator,
                actions,
                extra_actions,
                action_set_name,
                set,
                &bindings.sources,
                bindings_parsed,
                [
                    self.openxr.left_hand.subaction_path,
                    self.openxr.right_hand.subaction_path,
                ],
            ));
        }

        let info_action_binding = *legacy_bindings.trigger_click.first().unwrap_or_else(|| {
            panic!(
                "Missing trigger_click binding for {}",
                profile.profile_path()
            )
        });
        let bindings: Vec<xr::Binding<'_>> = xr_bindings
            .into_iter()
            .map(|(name, path)| {
                use super::ActionData::*;
                match actions
                    .get(&name)
                    .unwrap_or_else(|| panic!("Couldn't find data for action {name}"))
                {
                    Bool(action) => xr::Binding::new(action, path),
                    Vector1 { action, .. } => xr::Binding::new(action, path),
                    Vector2 { action, .. } => xr::Binding::new(action, path),
                    Haptic(action) => xr::Binding::new(action, path),
                    Skeleton { .. } | Pose { .. } => unreachable!(),
                }
            })
            .chain(legacy_bindings.binding_iter(legacy_actions))
            .chain(std::iter::once(xr::Binding::new(
                info_action,
                info_action_binding,
            )))
            .chain(skeletal_bindings.binding_iter(&skeletal_input.actions))
            .collect();

        self.openxr
            .instance
            .suggest_interaction_profile_bindings(profile_path, &bindings)
            .unwrap();
        debug!(
            "suggested {} bindings for {}",
            bindings.len(),
            profile.profile_path()
        );
    }
}

/// Returns a tuple of a parent action index and a path for its bindng
fn handle_dpad_binding(
    instance: &xr::Instance,
    string_to_path: impl Fn(&str) -> Option<xr::Path>,
    parent_path: &str,
    action_set_name: &str,
    action_set: &xr::ActionSet,
    actions: &mut LoadedActionDataMap,
    extra_actions: &mut HashMap<String, ExtraActionData>,
    DpadInput {
        east,
        south,
        north,
        west,
        center,
    }: &DpadInput,
    parameters: Option<&DpadParameters>,
    parsed_bindings: &mut HashMap<String, Vec<BindingData>>,
) -> Vec<(String, xr::Path)> {
    // Would love to use the dpad extension here, but it doesn't seem to
    // support touch trackpad dpads.
    // TODO: actually take the deadzone and overlap into account

    // Workaround weird closure lifetime quirks.
    const fn constrain<F>(f: F) -> F
    where
        F: for<'a> Fn(&'a Option<ActionBindingOutput>, DpadDirection) -> Option<&'a str>,
    {
        f
    }
    let maybe_find_action = constrain(|a, direction| {
        let output = &a.as_ref()?.output.0;
        let ret = actions.contains_key(output);
        if !ret {
            warn!(
                "Couldn't find dpad action {} (for path {parent_path}, {direction:?})",
                output
            );
        }
        ret.then_some(output)
    });

    use DpadDirection::*;

    let bound_actions: Vec<(&str, DpadDirection)> = [
        (maybe_find_action(north, North), North),
        (maybe_find_action(east, East), East),
        (maybe_find_action(south, South), South),
        (maybe_find_action(west, West), West),
        (maybe_find_action(center, Center), Center),
    ]
    .into_iter()
    .flat_map(|(name, direction)| name.zip(Some(direction)))
    .collect();

    if bound_actions.is_empty() {
        warn!("Dpad mode, but no actions ({parent_path} in {action_set_name})");
        return vec![];
    }

    let parent_action_key = format!("{parent_path}-{action_set_name}");
    let actions = RefCell::new(actions);
    let created_actions = LazyCell::new(|| {
        get_dpad_parent(
            &string_to_path,
            parent_path,
            &parent_action_key,
            action_set_name,
            action_set,
            &actions,
            parameters,
        )
    });
    for (action_name, direction) in bound_actions {
        // Temporarily remove action to avoid double mutable reference
        let mut data = extra_actions.remove(action_name).unwrap_or_default();

        if data.dpad_actions.is_none() {
            let (parent_action, click_or_touch) = LazyCell::force(&created_actions);
            data.dpad_actions = Some(DpadActions {
                xy: parent_action.clone(),
                click_or_touch: click_or_touch.as_ref().map(|d| d.action.clone()),
            })
        }

        if let Some(binding_hand) = parse_hand_from_path(instance, parent_path) {
            parsed_bindings.entry(action_name.to_string()).or_insert_with(Vec::new)
            .push(BindingData::Dpad(DpadData {
                direction,
                last_state: false.into(),
            }, binding_hand));
        } else {
            info!("Binding on {} has unknown hand path, it will be ignored", parent_path)
        }


        // Reinsert
        extra_actions.insert(action_name.to_string(), data);
    }

    let activator_binding = created_actions
        .1
        .as_ref()
        .map(|DpadActivatorData { key, binding, .. }| (key.clone(), *binding));
    let mut ret = vec![(parent_action_key, string_to_path(parent_path).unwrap())];
    if let Some(b) = activator_binding {
        ret.push(b);
    }
    ret
}

struct DpadActivatorData {
    key: String,
    action: xr::Action<f32>,
    binding: xr::Path,
}

fn get_dpad_parent(
    string_to_path: &impl Fn(&str) -> Option<xr::Path>,
    parent_path: &str,
    parent_action_key: &str,
    action_set_name: &str,
    action_set: &xr::ActionSet,
    actions: &RefCell<&mut LoadedActionDataMap>,
    parameters: Option<&DpadParameters>,
) -> (xr::Action<xr::Vector2f>, Option<DpadActivatorData>) {
    let mut actions = actions.borrow_mut();
    // Share parent actions that use the same action set and same bound path
    let parent_action = actions
        .entry(parent_action_key.to_string())
        .or_insert_with(|| {
            let clean_parent_path = parent_path.replace("/", "_");
            let parent_action_name = format!("xrizer-dpad-parent-{clean_parent_path}");
            let localized = format!("XRizer dpad parent ({parent_path})");
            let action = action_set
                .create_action::<xr::Vector2f>(&parent_action_name, &localized, &[])
                .unwrap();

            trace!("created new dpad parent ({parent_action_key})");

            super::ActionData::Vector2 {
                action,
                last_value: Default::default(),
            }
        });
    let super::ActionData::Vector2 {
        action: parent_action,
        ..
    } = parent_action
    else {
        unreachable!();
    };
    // Remove lifetime
    let parent_action = parent_action.clone();

    // Create our path to our parent click/touch, if such a path exists
    let (activator_binding_str, activator_binding_path) = parameters
        .as_ref()
        .and_then(|p| {
            let name = match p.sub_mode {
                DpadSubMode::Click => format!("{parent_path}/click"),
                DpadSubMode::Touch => format!("{parent_path}/touch"),
            };
            string_to_path(&name).map(|p| (name, p))
        })
        .unzip();

    let activator_key = activator_binding_str
        .as_ref()
        .map(|n| format!("{n}-{action_set_name}"));
    // Action only needs to exist if our path was successfully created
    let len = actions.len();
    let activator_action = activator_key.as_ref().map(|key| {
        let action = actions.entry(key.clone()).or_insert_with(|| {
            let dpad_activator_name = format!("xrizer-dpad-active{len}");
            let localized = format!("XRizer dpad active ({len})");

            ActionData::Vector1 {
                action: action_set
                    .create_action(&dpad_activator_name, &localized, &[])
                    .unwrap(),
                last_value: Default::default(),
            }
        });

        let ActionData::Vector1 { action, .. } = action else {
            unreachable!();
        };
        action
    });
    // Remove lifetime
    let click_or_touch = activator_action.cloned();

    (
        parent_action,
        click_or_touch.map(|action| DpadActivatorData {
            key: activator_key.unwrap(),
            action,
            binding: activator_binding_path.unwrap(),
        }),
    )
}

fn translate_warn(action: &str) -> impl FnOnce(&InvalidActionPath) + '_ {
    move |e| warn!("{} ({action})", e.0)
}

struct InvalidActionPath(String);

fn parse_hand_from_path(instance: &xr::Instance, path: &str) -> Option<xr::Path> {
    let hand_prefix = if path.starts_with("/user/hand/left") {
        "/user/hand/left"
    } else if path.starts_with("/user/hand/right") {
        "/user/hand/right"
    } else {
        return None;
    };

    let path = instance.string_to_path(hand_prefix).ok();
    path.and_then(|x| if x == xr::Path::NULL { None } else { Some(x) })
}

fn handle_sources(
    instance: &xr::Instance,
    path_translator: impl Fn(&str) -> Result<String, InvalidActionPath>,
    actions: &mut LoadedActionDataMap,
    extra_actions: &mut HashMap<String, ExtraActionData>,
    action_set_name: &str,
    action_set: &xr::ActionSet,
    sources: &[ActionBinding],
    bindings_parsed: &mut HashMap<String, Vec<BindingData>>,
    hands: [xr::Path; 2],
) -> Vec<(String, xr::Path)> {
    let bindings: RefCell<Vec<(String, xr::Path)>> = RefCell::new(Vec::new());

    trait ActionPattern {
        fn check_match(&self, data: &super::ActionData, name: &str);
    }
    macro_rules! action_match {
        ($pat:pat, $extra:literal) => {{
            struct S;
            impl ActionPattern for S {
                fn check_match(&self, data: &super::ActionData, name: &str) {
                    assert!(
                        matches!(data, $pat),
                        "Data for action {name} didn't match pattern {} ({})",
                        stringify!($pat),
                        $extra
                    );
                }
            }
            &S
        }};
        ($pat:pat) => {
            action_match!($pat, "")
        };
    }

    let actions = RefCell::new(actions);
    let try_get_binding =
        |action_path: String, input_path: String, action_pattern: &dyn ActionPattern| {
            let actions = actions.borrow();
            if find_action(&actions, &action_path) {
                action_pattern.check_match(&actions[&action_path], &action_path);
                trace!("suggesting {input_path} for {action_path}");
                let binding_path = instance.string_to_path(&input_path).unwrap();
                bindings.borrow_mut().push((action_path, binding_path));
            }
        };

    use super::ActionData::*;
    for mode in sources {
        let try_get_bool_binding = |action_path, input_path| {
            try_get_binding(
                action_path,
                input_path,
                action_match!(Bool(_) | Vector1 { .. }),
            );
        };

        match mode {
            ActionBinding::None(_) => {}
            ActionBinding::Button { path, inputs, .. }
            | ActionBinding::ToggleButton { path, inputs } => {
                if let Some(ActionBindingOutput { output }) = &inputs.click {
                    let Ok(translated) = path_translator(&format!("{path}/click"))
                        .inspect_err(translate_warn(output))
                    else {
                        continue;
                    };

                    if matches!(mode, ActionBinding::Button { .. }) {
                        try_get_bool_binding(output.to_string(), translated);
                    } else {
                        let mut actions = actions.borrow_mut();
                        if !find_action(&actions, output) {
                            continue;
                        }

                        let name_only = output.rsplit_once('/').unwrap().1;
                        let toggle_name = format!("{name_only}_tgl");

                        let mut extra_data = extra_actions.remove(&output.to_lowercase()).unwrap_or_default();

                        if extra_data.toggle_action.is_none() {
                            let localized = format!("{name_only} toggle");
                            let action = action_set
                                .create_action(&toggle_name, &localized, &hands)
                                .unwrap();

                            actions.insert(toggle_name.clone(), Bool(action.clone()));

                            extra_data.toggle_action = Some(action);
                        }
                        extra_actions.insert(output.to_lowercase(), extra_data);

                        trace!("suggesting {translated} for {output} (toggle)");
                        bindings.borrow_mut().push((
                            toggle_name.clone(),
                            instance.string_to_path(&translated).unwrap(),
                        ));

                        if let Some(binding_hand) = parse_hand_from_path(instance, &translated) {
                            bindings_parsed.entry(output.to_lowercase()).or_insert_with(Vec::new)
                                .push(BindingData::Toggle(Default::default(), binding_hand));
                        } else {
                            info!("Binding on {} has unknown hand path, it will be ignored", &translated)
                        }

                    }
                }

                if let Some(ActionBindingOutput { output }) = &inputs.double {
                    warn!("Double click binding for {output} currently unsupported.");
                }
            }
            ActionBinding::Dpad {
                path,
                inputs,
                parameters,
            } => {
                let Ok(parent_translated) =
                    path_translator(path).inspect_err(translate_warn(&format!("{inputs:?}")))
                else {
                    continue;
                };
                let data = handle_dpad_binding(
                    instance,
                    |s| {
                        path_translator(s)
                            .inspect_err(translate_warn("<dpad binding>"))
                            .ok()
                            .map(|s| instance.string_to_path(&s).unwrap())
                    },
                    &parent_translated,
                    action_set_name,
                    action_set,
                    &mut actions.borrow_mut(),
                    extra_actions,
                    inputs,
                    parameters.as_ref(),
                    bindings_parsed,
                );

                bindings.borrow_mut().extend(data);
            }
            ActionBinding::Trigger {
                path,
                inputs: TriggerInput { pull, touch, click },
                ..
            } => {
                let suffixes_and_outputs = [("pull", pull), ("touch", touch), ("click", click)]
                    .into_iter()
                    .filter_map(|(sfx, input)| Some(sfx).zip(input.as_ref().map(|i| &i.output)));
                for (suffix, output) in suffixes_and_outputs {
                    let Ok(translated) = path_translator(&format!("{path}/{suffix}"))
                        .inspect_err(translate_warn(output))
                    else {
                        continue;
                    };

                    try_get_binding(
                        output.to_string(),
                        translated,
                        action_match!(
                            Bool(_) | Vector1 { .. },
                            "Trigger action should be a bool or float"
                        ),
                    );
                }
            }
            ActionBinding::ScalarConstant {
                path,
                inputs:
                    ScalarConstantInput {
                        value: ActionBindingOutput { output },
                    },
                ..
            } => {
                let vpath = format!("{path}/value");
                let Ok(translated) = path_translator(&vpath)
                    .or_else(|_| {
                        trace!("Invalid scalar constant path {vpath}, trying click");
                        path_translator(&format!("{path}/click"))
                    })
                    .inspect_err(translate_warn(output))
                else {
                    continue;
                };

                try_get_binding(
                    output.to_string(),
                    translated,
                    action_match!(Vector1 { .. }),
                )
            }
            ActionBinding::ForceSensor {
                path,
                inputs:
                    ForceSensorInput {
                        force: ActionBindingOutput { output },
                    },
                ..
            } => {
                let Ok(translated) =
                    path_translator(&format!("{path}/force")).inspect_err(translate_warn(output))
                else {
                    continue;
                };

                try_get_binding(
                    output.to_string(),
                    translated,
                    action_match!(Vector1 { .. }),
                );
            }
            ActionBinding::Grab {
                path,
                inputs:
                    GrabInput {
                        grab: ActionBindingOutput { output },
                    },
                parameters
            } => {
                let Ok((translated_force, translated_value)) =
                    path_translator(&[path, "/force"].concat())
                        .inspect_err(translate_warn(output))
                        .and_then(|f| {
                            Ok((
                                f,
                                path_translator(&[path, "/value"].concat())
                                    .inspect_err(translate_warn(output))?,
                            ))
                        })
                else {
                    continue;
                };

                let mut actions = actions.borrow_mut();
                if !find_action(&actions, output) {
                    continue;
                }

                let name_only = output.rsplit_once('/').unwrap().1;
                let force_name = format!("{name_only}_grabactionf");
                let value_name = format!("{name_only}_grabactionv");

                let mut data = extra_actions.remove(&output.0).unwrap_or_default();
                if data.grab_action.is_none() {
                    let localized = format!("{name_only} grab action (force)");
                    let force_action = action_set
                        .create_action(&force_name, &localized, &hands)
                        .unwrap();
                    let localizedv = format!("{name_only} grab action (value)");
                    let value_action = action_set
                        .create_action(&value_name, &localizedv, &hands)
                        .unwrap();

                    actions.insert(force_name.clone(), Vector1 { action: force_action.clone(), last_value: Default::default() });
                    actions.insert(value_name.clone(), Vector1 { action: value_action.clone(), last_value: Default::default() });

                    data.grab_action = Some(GrabActions {
                        force_action,
                        value_action,
                    });
                }
                extra_actions.insert(output.to_string(), data);

                if let Some(binding_hand) = parse_hand_from_path(instance, &translated_force) {
                    bindings_parsed.entry(output.to_lowercase()).or_insert_with(Vec::new)
                        .push(BindingData::Grab(GrabBindingData::new(
                            parameters.as_ref()
                                .and_then(|x| x.value_hold_threshold.as_ref())
                                .map(|x| x.0),
                            parameters.as_ref()
                                .and_then(|x| x.value_release_threshold.as_ref())
                                .map(|x| x.0),
                        ), binding_hand));
                } else {
                    info!("Binding on {} has unknown hand path, it will be ignored", &translated_force)
                }

                trace!("suggesting {translated_force} and {translated_value} for {force_name} (grab binding)");
                bindings.borrow_mut().push((
                    force_name.clone(),
                    instance.string_to_path(&translated_force).unwrap(),
                ));
                bindings.borrow_mut().push((
                    value_name.clone(),
                    instance.string_to_path(&translated_value).unwrap(),
                ));
            }
            ActionBinding::Scroll { inputs, .. } => {
                warn!("Got scroll binding for input {}, but these are currently unimplemented, skipping", inputs.scroll.output);
            }
            ActionBinding::Trackpad(data) | ActionBinding::Joystick(data) => {
                let Vector2Mode { path, inputs } = data;
                let Ok(translated) =
                    path_translator(path).inspect_err(translate_warn("<vector2 input>"))
                else {
                    continue;
                };

                let Vector2Input {
                    position,
                    click,
                    touch,
                } = inputs;

                if let Some((output, click_path)) = click.as_ref().and_then(|b| {
                    Some(&b.output).zip(
                        path_translator(&format!("{translated}/click"))
                            .inspect_err(translate_warn(&b.output))
                            .ok(),
                    )
                }) {
                    try_get_bool_binding(output.to_string(), click_path);
                }

                if let Some((output, touch_path)) = touch.as_ref().and_then(|b| {
                    Some(&b.output).zip(
                        path_translator(&format!("{translated}/touch"))
                            .inspect_err(translate_warn(&b.output))
                            .ok(),
                    )
                }) {
                    try_get_bool_binding(output.to_string(), touch_path);
                }

                if let Some(position) = position.as_ref() {
                    try_get_binding(
                        position.output.to_string(),
                        translated,
                        action_match!(Vector2 { .. }),
                    );
                }
            }
        }
    }
    bindings.into_inner()
}

fn handle_skeleton_bindings(actions: &LoadedActionDataMap, bindings: &[SkeletonActionBinding]) {
    for SkeletonActionBinding { output, path } in bindings {
        trace!("binding skeleton action {output} to {path:?}");
        if !find_action(actions, output) {
            continue;
        };

        match &actions[&output.0] {
            super::ActionData::Skeleton { hand, .. } => assert_eq!(hand, path),
            _ => panic!("Expected skeleton action for skeleton binding {output}"),
        }
    }
}

fn handle_haptic_bindings(
    instance: &xr::Instance,
    path_translator: impl Fn(&str) -> Result<String, InvalidActionPath>,
    actions: &LoadedActionDataMap,
    bindings: &[SimpleActionBinding],
) -> Vec<(String, xr::Path)> {
    let mut ret = Vec::new();

    for SimpleActionBinding { output, path } in bindings {
        let Ok(translated) = path_translator(path).inspect_err(translate_warn(output)) else {
            continue;
        };
        if !find_action(actions, output) {
            continue;
        };

        assert!(
            matches!(&actions[&output.0], super::ActionData::Haptic(_)),
            "expected haptic action for haptic binding {}, got {}",
            translated,
            output
        );
        let xr_path = instance.string_to_path(&translated).unwrap();
        ret.push((output.0.clone(), xr_path));
    }

    ret
}

fn handle_pose_bindings(
    actions: &mut LoadedActionDataMap,
    bindings: &[PoseBinding],
    pose_bindings: &mut HashMap<String, BoundPose>
) {
    for PoseBinding {
        output,
        path: (hand, pose_ty),
    } in bindings
    {
        if !find_action(actions, output) {
            continue;
        };

        assert!(matches!(actions.get_mut(&output.0).unwrap(), ActionData::Pose),
                "Expected pose action for pose binding on {output}"
        );

        let bound = pose_bindings.entry(output.0.clone()).or_insert(Default::default());

        let b = match hand {
            Hand::Left => &mut bound.left,
            Hand::Right => &mut bound.right,
        };
        *b = Some(*pose_ty);
        trace!("bound {:?} to pose {output} for hand {hand:?}", *pose_ty);
    }
}
