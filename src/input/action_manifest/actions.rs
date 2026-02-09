#![allow(private_interfaces)]

use super::bindings::ActionPath;
use crate::openxr_data::Hand;
use log::{error, trace, warn};
use openvr as vr;
use openxr as xr;
use serde::{
    Deserialize,
    de::{Error, Unexpected},
};
use std::collections::HashMap;
use std::path::PathBuf;

/**
 * Structure for action manifests.
 * https://github.com/ValveSoftware/openvr/wiki/Action-manifest
 */

#[derive(Deserialize)]
pub struct ActionManifest {
    pub default_bindings: Vec<DefaultBindings>,
    #[serde(default)] // optional apparently
    pub action_sets: Vec<ActionSetJson>,
    pub actions: Vec<ActionType>,
    pub localization: Option<Vec<Localization>>,
    // localization_files
}

#[derive(Deserialize)]
pub struct DefaultBindings {
    pub binding_url: PathBuf,
    pub controller_type: ControllerType,
}

#[derive(Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ControllerType {
    ViveController,
    #[serde(rename = "vive_focus3_controller")]
    ViveFocus3,
    Knuckles,
    OculusTouch,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Deserialize)]
pub struct ActionSetJson {
    #[serde(rename = "name")]
    path: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum ActionType {
    Boolean(ActionDataCommon),
    Vector1(ActionDataCommon),
    Vector2(ActionDataCommon),
    Vector3(ActionDataCommon),
    Vibration(ActionDataCommon),
    Pose(ActionDataCommon),
    Skeleton(SkeletonData),
}

#[derive(Deserialize)]
struct ActionDataCommon {
    name: ActionPath,
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
pub struct Localization {
    pub language_tag: String,
    #[serde(flatten)]
    pub localized_names: HashMap<String, String>,
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

pub fn load_action_sets(
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

fn create_action<T: xr::ActionTy>(
    instance: &xr::Instance,
    data: &ActionDataCommon,
    sets: &mut HashMap<String, xr::ActionSet>,
    english: Option<&Localization>,
    paths: &[xr::Path],
    long_name_idx: &mut usize,
) -> xr::Result<xr::Action<T>> {
    let localized = english
        .and_then(|e| e.localized_names.get(&data.name.path))
        .map(|s| s.as_str());

    let set_name = data.name.action_set_name();
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
    let mut xr_friendly_name = data.name.cleaned_name();
    if xr_friendly_name.len() + 1 > xr::sys::MAX_ACTION_NAME_SIZE {
        let idx_str = ["_ln", &long_name_idx.to_string()].concat();
        xr_friendly_name.replace_range(
            xr::sys::MAX_ACTION_NAME_SIZE - idx_str.len() - 1..,
            &idx_str,
        );
        *long_name_idx += 1;
    }
    let localized = localized.unwrap_or(&xr_friendly_name);
    trace!("Creating action {xr_friendly_name} (localized: {localized}) in set {set_name:?}");

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

pub type LoadedActionDataMap = HashMap<String, crate::input::ActionData>;
pub fn load_actions(
    instance: &xr::Instance,
    session: &xr::Session<xr::AnyGraphics>,
    english: Option<&Localization>,
    sets: &mut HashMap<String, xr::ActionSet>,
    actions: Vec<ActionType>,
    left_hand: xr::Path,
    right_hand: xr::Path,
) -> Result<LoadedActionDataMap, vr::EVRInputError> {
    let mut ret = HashMap::with_capacity(actions.len());
    let mut long_name_idx = 0;
    for action in actions {
        let paths = &[left_hand, right_hand];
        macro_rules! create_action {
            ($ty:ty, $data:expr) => {
                create_action::<$ty>(instance, &$data, sets, english, paths, &mut long_name_idx)
                    .unwrap()
            };
        }
        use crate::input::ActionData::*;
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
            ActionType::Vector3(data) => {
                warn!(
                    "Got vector3 action {}, but these are currently unsupported.",
                    data.name.path
                );
                continue;
            }
            ActionType::Pose(data) => (&data.name, Pose),
            ActionType::Skeleton(SkeletonData { skeleton, data }) => {
                trace!("Creating skeleton action {}", data.name.path);
                (&data.name, Skeleton(*skeleton))
            }
            ActionType::Vibration(data) => (&data.name, Haptic(create_action!(xr::Haptic, data))),
        };
        ret.insert(path.path.clone(), action);
    }
    Ok(ret)
}
