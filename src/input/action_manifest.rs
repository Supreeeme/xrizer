use super::{
    custom_bindings::DpadDirection,
    legacy::LegacyActionData,
    profiles::{PathTranslation, Profiles},
    skeletal::SkeletalInputActionData,
    ActionData, ActionKey, BoundPoseType, Input,
};
use crate::openxr_data::{self, Hand, SessionData};
use helpers::{BindingsLoadContext, BindingsProfileLoadContext, DpadActivatorData, DpadHapticData};
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
use std::{cell::LazyCell, env::current_dir};

mod helpers;

fn action_map_to_secondary<T>(
    act_guard: &mut SlotMap<ActionKey, super::Action>,
    map: HashMap<String, T>,
) -> SecondaryMap<ActionKey, T> {
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

        let actions = load_actions(
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

        let mut binding_context = BindingsLoadContext::new(
            &sets,
            actions,
            &legacy.actions,
            &info_action,
            skeletal_input,
        );

        self.load_bindings(
            manifest_path.parent().unwrap(),
            manifest.default_bindings,
            &mut binding_context,
        );

        let BindingsLoadContext {
            actions,
            extra_actions,
            per_profile_bindings,
            per_profile_pose_bindings,
            ..
        } = binding_context;

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
            ActionType::Boolean(data) => (&data.name, Bool(create_action!(bool, data))),
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
            ActionType::Pose(data) => (&data.name, Pose),
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
struct FromString<T>(pub T);

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
    touch: Option<ActionBindingOutput>,
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
    on_x: Option<String>,
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
        bindings: Vec<DefaultBindings>,
        context: &mut BindingsLoadContext,
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
                        if let Some(bindings) = bindings.as_ref() {
                            if let Some(mut context) =
                                context.for_profile(&self.openxr, profile, other)
                            {
                                self.load_bindings_for_profile(bindings, &mut context);
                            }
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
        bindings: &HashMap<String, ActionSetBinding>,
        context: &mut BindingsProfileLoadContext,
    ) {
        let profile = context.profile;
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

        for (action_set_name, bindings) in bindings.iter() {
            let Some(set) = context.get_action_set(action_set_name) else {
                warn!("Action set {action_set_name} missing.");
                continue;
            };

            let set = set.clone();

            if let Some(bindings) = &bindings.haptics {
                handle_haptic_bindings(&self.openxr.instance, path_translator, context, bindings);
            }

            if let Some(bindings) = &bindings.poses {
                handle_pose_bindings(context, bindings);
            }

            if let Some(bindings) = &bindings.skeleton {
                handle_skeleton_bindings(context, bindings);
            }

            handle_sources(
                path_translator,
                context,
                action_set_name,
                &set,
                &bindings.sources,
            );
        }

        let info_action_binding = *legacy_bindings.trigger_click.first().unwrap_or_else(|| {
            panic!(
                "Missing trigger_click binding for {}",
                profile.profile_path()
            )
        });
        let bindings: Vec<xr::Binding<'_>> = context
            .bindings
            .iter()
            .map(|(name, path)| {
                use super::ActionData::*;
                let path = *path;
                match context
                    .actions
                    .get(name)
                    .unwrap_or_else(|| panic!("Couldn't find data for action {name}"))
                {
                    Bool(action) => xr::Binding::new(action, path),
                    Vector1 { action, .. } => xr::Binding::new(action, path),
                    Vector2 { action, .. } => xr::Binding::new(action, path),
                    Haptic(action) => xr::Binding::new(action, path),
                    Skeleton { .. } | Pose { .. } => unreachable!(),
                }
            })
            .chain(legacy_bindings.binding_iter(context.legacy_actions))
            .chain(std::iter::once(xr::Binding::new(
                context.info_action,
                info_action_binding,
            )))
            .chain(skeletal_bindings.binding_iter(&context.skeletal_input.actions))
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
    string_to_path: impl Fn(&str) -> Option<xr::Path>,
    parent_path: &str,
    action_set_name: &str,
    action_set: &xr::ActionSet,
    context: &mut BindingsProfileLoadContext,
    DpadInput {
        east,
        south,
        north,
        west,
        center,
    }: &DpadInput,
    parameters: Option<&DpadParameters>,
) {
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
        let ret = context.actions.contains_key(output);
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
        return;
    }

    let parent_action_key = format!("{parent_path}-{action_set_name}");

    let created_actions = context.get_dpad_parent(
        &string_to_path,
        parent_path,
        &parent_action_key,
        action_set_name,
        action_set,
        parameters,
    );

    for (action_name, direction) in bound_actions {
        context.add_custom_dpad_binding(parent_path, action_name, direction, &created_actions);
    }

    let activator_binding = created_actions
        .1
        .as_ref()
        .map(|DpadActivatorData { key, binding, .. }| (key.clone(), *binding));
    let haptic_binding = created_actions
        .2
        .as_ref()
        .map(|DpadHapticData { key, binding, .. }| (key.clone(), *binding));
    context.push_binding(parent_action_key, string_to_path(parent_path).unwrap());
    if let Some((s, p)) = activator_binding {
        context.push_binding(s, p);
    }
    if let Some((s, p)) = haptic_binding {
        context.push_binding(s, p);
    }
}

fn translate_warn(action: &str) -> impl FnOnce(&InvalidActionPath) + '_ {
    move |e| warn!("{} ({action})", e.0)
}

struct InvalidActionPath(String);

fn handle_sources(
    path_translator: impl Fn(&str) -> Result<String, InvalidActionPath>,
    context: &mut BindingsProfileLoadContext,
    action_set_name: &str,
    action_set: &xr::ActionSet,
    sources: &[ActionBinding],
) {
    for mode in sources {
        macro_rules! bind_button_touch {
            ($path:expr, $inputs:expr) => {
                if let Some(ActionBindingOutput { output }) = &$inputs.touch {
                    if let Ok(translated) = path_translator(&format!("{}/touch", $path))
                        .inspect_err(translate_warn(output))
                    {
                        // Touch is always directly bindable
                        context.try_get_bool_binding(output.to_string(), translated);
                    };
                }
            };
        }

        match mode {
            ActionBinding::None(_) => {}
            ActionBinding::ToggleButton { path, inputs } => {
                bind_button_touch!(path, inputs);

                if let Some(ActionBindingOutput { output }) = &inputs.click {
                    let Ok(translated) = path_translator(&format!("{path}/click"))
                        .inspect_err(translate_warn(output))
                    else {
                        continue;
                    };

                    if !context.find_action(output) {
                        continue;
                    }

                    let as_name = context.get_or_create_toggle_extra_action(
                        output,
                        action_set_name,
                        action_set,
                    );

                    trace!("suggesting {translated} for {output} (toggle)");
                    context.push_binding(
                        as_name,
                        context.instance.string_to_path(&translated).unwrap(),
                    );

                    context.add_custom_toggle_binding(output, &translated);
                }
            }
            ActionBinding::Button {
                path,
                inputs,
                parameters,
            } => {
                bind_button_touch!(path, inputs);

                if let Some(ActionBindingOutput { output }) = &inputs.click {
                    let parameters = parameters.as_ref();
                    let target = parameters
                        .and_then(|x| x.force_input.as_ref())
                        .map(|x| x.as_str())
                        .unwrap_or("value");
                    // TODO: ^ for button bindings on clicky triggers, it's unclear how to choose between /value and /click without hints
                    // Clicking feels bad for a lot of interaction tho, so prefer /value for now

                    let binding_to_2d = target == "position";
                    let translated = if binding_to_2d {
                        if let Ok(translated) = path_translator(path).inspect_err(|e| {
                            warn!(
                                "Button binding on {output} can't bind to joystick ({})",
                                e.0
                            )
                        }) {
                            translated
                        } else {
                            continue;
                        }
                    } else if let Ok(translated) = path_translator(&format!("{path}/{target}"))
                        .inspect_err(|e| debug!("Falling back to click for {output} ({})", e.0))
                    {
                        translated
                    } else if let Ok(translated) = path_translator(&format!("{path}/click"))
                        .inspect_err(translate_warn(output))
                    {
                        translated
                    } else {
                        continue;
                    };

                    // These two sources are typically bool, so bind directly
                    if translated.ends_with("/click") || translated.ends_with("/touch") {
                        context.try_get_bool_binding(output.to_string(), translated);
                    } else {
                        // for everything actually binding to /value or /force, use custom thresholds
                        let float_name_with_as = if binding_to_2d {
                            context.get_or_create_v2_extra_action(
                                output,
                                action_set_name,
                                action_set,
                            )
                        } else {
                            context.get_or_create_analog_extra_action(
                                output,
                                action_set_name,
                                action_set,
                            )
                        };

                        context.push_binding(
                            float_name_with_as,
                            context.instance.string_to_path(&translated).unwrap(),
                        );

                        context.add_custom_button_binding(output, &translated, parameters)
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
                handle_dpad_binding(
                    |s| {
                        path_translator(s)
                            .inspect_err(translate_warn("<dpad binding>"))
                            .ok()
                            .map(|s| context.instance.string_to_path(&s).unwrap())
                    },
                    &parent_translated,
                    action_set_name,
                    action_set,
                    context,
                    inputs,
                    parameters.as_ref(),
                );
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

                    context.try_get_bool_binding(output.to_string(), translated);
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

                context.try_get_float_binding(output.to_string(), translated)
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

                context.try_get_float_binding(output.to_string(), translated);
            }
            ActionBinding::Grab {
                path,
                inputs:
                    GrabInput {
                        grab: ActionBindingOutput { output },
                    },
                parameters,
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

                if !context.find_action(output) {
                    continue;
                }

                let (force_full_name, value_full_name) =
                    context.get_or_create_grab_action_pair(output, action_set_name, action_set);

                context.add_custom_grab_binding(output, &translated_force, parameters);

                trace!("suggesting {translated_force} and {translated_value} for {force_full_name} (grab binding)");
                context.push_binding(
                    force_full_name.clone(),
                    context.instance.string_to_path(&translated_force).unwrap(),
                );
                context.push_binding(
                    value_full_name.clone(),
                    context.instance.string_to_path(&translated_value).unwrap(),
                );
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
                    context.try_get_bool_binding(output.to_string(), click_path);
                }

                if let Some((output, touch_path)) = touch.as_ref().and_then(|b| {
                    Some(&b.output).zip(
                        path_translator(&format!("{translated}/touch"))
                            .inspect_err(translate_warn(&b.output))
                            .ok(),
                    )
                }) {
                    context.try_get_bool_binding(output.to_string(), touch_path);
                }

                if let Some(position) = position.as_ref() {
                    context.try_get_v2_binding(position.output.to_string(), translated);
                }
            }
        }
    }
}

fn handle_skeleton_bindings(
    context: &BindingsProfileLoadContext,
    bindings: &[SkeletonActionBinding],
) {
    for SkeletonActionBinding { output, path } in bindings {
        trace!("binding skeleton action {output} to {path:?}");
        if !context.find_action(output) {
            continue;
        };

        match &context.actions[&output.0] {
            super::ActionData::Skeleton { hand, .. } => assert_eq!(hand, path),
            _ => panic!("Expected skeleton action for skeleton binding {output}"),
        }
    }
}

fn handle_haptic_bindings(
    instance: &xr::Instance,
    path_translator: impl Fn(&str) -> Result<String, InvalidActionPath>,
    context: &mut BindingsProfileLoadContext,
    bindings: &[SimpleActionBinding],
) {
    for SimpleActionBinding { output, path } in bindings {
        let Ok(translated) = path_translator(path).inspect_err(translate_warn(output)) else {
            continue;
        };
        if !context.find_action(output) {
            continue;
        };

        assert!(
            matches!(&context.actions[&output.0], super::ActionData::Haptic(_)),
            "expected haptic action for haptic binding {}, got {}",
            translated,
            output
        );
        let xr_path = instance.string_to_path(&translated).unwrap();
        context.push_binding(output.0.clone(), xr_path);
    }
}

fn handle_pose_bindings(context: &mut BindingsProfileLoadContext, bindings: &[PoseBinding]) {
    for PoseBinding {
        output,
        path: (hand, pose_ty),
    } in bindings
    {
        if !context.find_action(output) {
            continue;
        };

        assert!(
            matches!(
                context.actions.get_mut(&output.0).unwrap(),
                ActionData::Pose
            ),
            "Expected pose action for pose binding on {output}"
        );

        let bound = context.pose_bindings.entry(output.0.clone()).or_default();

        let b = match hand {
            Hand::Left => &mut bound.left,
            Hand::Right => &mut bound.right,
        };
        *b = Some(*pose_ty);
        trace!("bound {:?} to pose {output} for hand {hand:?}", *pose_ty);
    }
}
