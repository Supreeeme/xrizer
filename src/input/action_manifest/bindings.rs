#![allow(private_interfaces)]

use super::context::{BindingsProfileLoadContext, DpadActivatorData, DpadHapticData};
use crate::input::{ActionData, BoundPoseType, custom_bindings::DpadDirection};
use crate::{
    input::{
        GrabActions,
        custom_bindings::{
            DoubleTapData, DpadActions, DpadBindingParams, DpadData, GrabBindingData,
            ThresholdBindingFloat, ThresholdBindingVector2, ToggleData,
        },
    },
    openxr_data::Hand,
};
use log::{debug, trace, warn};
use openxr as xr;
use serde::{
    Deserialize,
    de::{Error, IgnoredAny, Unexpected},
};
use std::collections::HashMap;
use std::str::FromStr;

/**
 * Structure for binding files
 */

#[derive(Deserialize)]
pub struct Bindings {
    pub bindings: HashMap<String, ActionSetBinding>,
}

#[derive(Deserialize)]
pub struct ActionSetBinding {
    pub sources: Vec<ActionBinding>,
    pub poses: Option<Vec<PoseBinding>>,
    pub haptics: Option<Vec<SimpleActionBinding>>,
    pub skeleton: Option<Vec<SimpleActionBinding>>,
}

#[derive(Debug)]
pub struct ActionPath {
    /// This is the full path as pulled from the manifest, but set to lowercase
    /// Action handles appear to be case insensitive.
    pub path: String,
}

impl ActionPath {
    /// Returns just the action name - the end part of the path - cleaned
    /// so that it's compatible with the OpenXR path semantics
    /// See Section 6.2 (Well-Formed Path Strings) of the OpenXR spec
    pub fn cleaned_name(&self) -> String {
        self.path
            .rsplit_once('/')
            .expect("Action path missing slash?")
            .1
            .replace(
                |c| !matches!(c, 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '/'),
                "_",
            )
    }

    pub fn action_set_name(&self) -> &str {
        let set_end_idx = self.path.match_indices('/').nth(2).unwrap().0;
        &self.path[0..set_end_idx]
    }
}

impl<'de> Deserialize<'de> for ActionPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|s| Self {
            path: s.to_ascii_lowercase(),
        })
    }
}

#[derive(Deserialize)]
pub struct PoseBinding {
    output: ActionPath,
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
            ));
        }
    };

    let pose = match pose {
        "raw" => BoundPoseType::Raw,
        "tip" => BoundPoseType::Tip,
        "gdc2015" => BoundPoseType::Gdc2015,
        other => {
            warn!("Unknown pose type: {other:?}");
            BoundPoseType::Raw
        }
    };

    Ok((hand, pose))
}

#[derive(Deserialize)]
pub struct SimpleActionBinding {
    output: ActionPath,
    path: String,
}

#[derive(Deserialize, Debug)]
struct ActionBindingOutput {
    output: ActionPath,
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
#[allow(private_interfaces)]
pub enum ActionBinding {
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
#[derive(Copy, Clone, derive_more::Deref)]
pub struct FromString<T>(T);

impl<T: FromStr> FromStr for FromString<T> {
    type Err = T::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_str(s).map(Self)
    }
}

impl<T> From<T> for FromString<T> {
    fn from(t: T) -> Self {
        FromString(t)
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
pub struct ClickThresholdParams {
    pub click_activate_threshold: Option<FromString<f32>>,
    pub click_deactivate_threshold: Option<FromString<f32>>,
}

impl ClickThresholdParams {
    fn new_for_touch_conversion() -> Self {
        Self {
            click_activate_threshold: Some(0.01f32.into()),
            click_deactivate_threshold: Some(0.005f32.into()),
        }
    }
}

#[derive(Deserialize)]
struct ScalarConstantParameters {
    #[serde(rename = "on/x")]
    #[allow(unused)]
    on_x: Option<String>,
}

#[derive(Deserialize)]
struct ButtonParameters {
    force_input: Option<String>,
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
pub struct DpadParameters {
    pub sub_mode: DpadSubMode,
    pub deadzone_pct: FromString<u8>,
    pub overlap_pct: FromString<u8>,
    pub sticky: FromString<bool>,
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
pub enum DpadSubMode {
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
pub struct GrabParameters {
    pub value_hold_threshold: Option<FromString<f32>>,
    pub value_release_threshold: Option<FromString<f32>>,
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

pub fn handle_dpad_binding(
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
        F: for<'a> Fn(&'a Option<ActionBindingOutput>, DpadDirection) -> Option<&'a ActionPath>,
    {
        f
    }
    let maybe_find_action = constrain(|a, direction| {
        let output = &a.as_ref()?.output;
        let ret = context.actions.contains_key(&output.path);
        if !ret {
            warn!(
                "Couldn't find dpad action {} (for path {parent_path}, {direction:?})",
                output.path
            );
        }
        ret.then_some(output)
    });

    use DpadDirection::*;

    let bound_actions: Vec<(&ActionPath, DpadDirection)> = [
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

    let (xy, click_or_touch_data, haptic_data) = context.get_dpad_parent(
        &string_to_path,
        parent_path,
        &parent_action_key,
        action_set_name,
        action_set,
        parameters,
    );

    let hand = super::context::parse_hand_from_path(context.instance, parent_path).unwrap();
    for (path, direction) in bound_actions {
        context.add_custom_binding::<DpadData>(
            path,
            hand,
            action_set_name,
            action_set,
            Some(&DpadBindingParams {
                actions: DpadActions {
                    xy: xy.clone(),
                    click_or_touch: click_or_touch_data.as_ref().map(|d| d.action.clone()),
                    haptic: haptic_data.as_ref().map(|d| d.action.clone()),
                },
                direction,
            }),
        );
    }

    let activator_binding = click_or_touch_data
        .as_ref()
        .map(|DpadActivatorData { key, binding, .. }| (key.clone(), *binding));
    let haptic_binding = haptic_data
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

pub struct InvalidActionPath(pub String);

pub fn handle_sources(
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
                        .inspect_err(translate_warn(&output.path))
                    {
                        // Touch is always directly bindable
                        context.try_get_bool_binding(output.path.clone(), translated);
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
                        .inspect_err(translate_warn(&output.path))
                    else {
                        continue;
                    };

                    if !context.find_action(&output.path) {
                        continue;
                    }

                    let action = context.add_custom_binding::<ToggleData>(
                        output,
                        super::context::parse_hand_from_path(context.instance, &translated)
                            .unwrap(),
                        action_set_name,
                        action_set,
                        None,
                    );

                    trace!("suggesting {translated} for {} (toggle)", output.path);
                    context.push_binding(
                        action,
                        context.instance.string_to_path(&translated).unwrap(),
                    );
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

                    let binding_to_2d = target == "position";
                    let translated = if binding_to_2d {
                        path_translator(path).inspect_err(|e| {
                            warn!(
                                "Button binding on {} can't bind to joystick ({})",
                                output.path, e.0
                            )
                        })
                    } else {
                        path_translator(&format!("{path}/{target}"))
                            .inspect_err(|e| {
                                debug!("Falling back to click for {} ({})", output.path, e.0)
                            })
                            .or_else(|_| path_translator(&format!("{path}/click")))
                            .inspect_err(translate_warn(&output.path))
                    };
                    let Ok(translated) = translated else {
                        continue;
                    };

                    // These two sources are typically bool, so bind directly
                    if translated.ends_with("/click") || translated.ends_with("/touch") {
                        context.try_get_bool_binding(output.path.clone(), translated);
                    } else {
                        // for everything actually binding to /value or /force, use custom thresholds
                        let params = parameters.map(|b| &b.click_threshold);
                        let hand =
                            super::context::parse_hand_from_path(context.instance, &translated)
                                .unwrap();
                        let float_name_with_as = if binding_to_2d {
                            context.add_custom_binding::<ThresholdBindingVector2>(
                                output,
                                hand,
                                action_set_name,
                                action_set,
                                params,
                            )
                        } else {
                            context.add_custom_binding::<ThresholdBindingFloat>(
                                output,
                                hand,
                                action_set_name,
                                action_set,
                                params,
                            )
                        };

                        context.push_binding(
                            float_name_with_as,
                            context.instance.string_to_path(&translated).unwrap(),
                        );
                    }
                }

                if let Some(ActionBindingOutput { output }) = &inputs.double
                    && let Ok(translated) = path_translator(&format!("{path}/click"))
                        .inspect_err(translate_warn(&output.path))
                {
                    let name = context.add_custom_binding::<DoubleTapData>(
                        output,
                        super::context::parse_hand_from_path(context.instance, &translated)
                            .unwrap(),
                        action_set_name,
                        action_set,
                        None,
                    );

                    context
                        .push_binding(name, context.instance.string_to_path(&translated).unwrap());
                }
            }
            ActionBinding::Dpad {
                path,
                inputs,
                parameters,
            } => {
                let Ok(parent_translated) =
                    path_translator(path).inspect_err(translate_warn(&format!("{inputs:#?}")))
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
                    match path_translator(&format!("{path}/{suffix}")) {
                        Ok(translated) => {
                            context.try_get_bool_binding(output.path.clone(), translated);
                        }
                        Err(_) if suffix == "touch" => {
                            debug!(
                                "Falling back to pull for touch on {path} (action {:?})",
                                &output.path
                            );
                            // SteamVR fallbacks "touch" bindings on triggers to "any pull amount" if there's no native capsense
                            if let Ok(translated_pull) = path_translator(&format!("{path}/pull")) {
                                let parameters = ClickThresholdParams::new_for_touch_conversion();
                                let hand = super::context::parse_hand_from_path(
                                    context.instance,
                                    &translated_pull,
                                )
                                .unwrap();
                                let float_name_with_as = context
                                    .add_custom_binding::<ThresholdBindingFloat>(
                                        output,
                                        hand,
                                        action_set_name,
                                        action_set,
                                        Some(&parameters),
                                    );
                                context.push_binding(
                                    float_name_with_as,
                                    context.instance.string_to_path(&translated_pull).unwrap(),
                                );
                            } else {
                                warn!(
                                    "Couldn't bind touch to {} as there's neither touch nor pull input available",
                                    &output.path
                                );
                            }
                        }
                        Err(err) => {
                            translate_warn(&output.path)(&err);
                        }
                    }
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
                    .inspect_err(translate_warn(&output.path))
                else {
                    continue;
                };

                context.try_get_float_binding(output.path.clone(), translated)
            }
            ActionBinding::ForceSensor {
                path,
                inputs:
                    ForceSensorInput {
                        force: ActionBindingOutput { output },
                    },
                ..
            } => {
                let Ok(translated) = path_translator(&format!("{path}/force"))
                    .inspect_err(translate_warn(&output.path))
                else {
                    continue;
                };

                context.try_get_float_binding(output.path.clone(), translated);
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
                        .inspect_err(translate_warn(&output.path))
                        .and_then(|f| {
                            Ok((
                                f,
                                path_translator(&[path, "/value"].concat())
                                    .inspect_err(translate_warn(&output.path))?,
                            ))
                        })
                else {
                    continue;
                };

                if !context.find_action(&output.path) {
                    continue;
                }

                let GrabActions {
                    force_action,
                    value_action,
                } = context.add_custom_binding::<GrabBindingData>(
                    output,
                    super::context::parse_hand_from_path(context.instance, &translated_force)
                        .unwrap(),
                    action_set_name,
                    action_set,
                    parameters.as_ref(),
                );

                trace!(
                    "suggesting {translated_force} and {translated_value} for {force_action} (grab binding)"
                );
                context.push_binding(
                    force_action,
                    context.instance.string_to_path(&translated_force).unwrap(),
                );
                context.push_binding(
                    value_action,
                    context.instance.string_to_path(&translated_value).unwrap(),
                );
            }
            ActionBinding::Scroll { inputs, .. } => {
                warn!(
                    "Got scroll binding for input {}, but these are currently unimplemented, skipping",
                    inputs.scroll.output.path
                );
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
                            .inspect_err(translate_warn(&b.output.path))
                            .ok(),
                    )
                }) {
                    context.try_get_bool_binding(output.path.clone(), click_path);
                }

                if let Some((output, touch_path)) = touch.as_ref().and_then(|b| {
                    Some(&b.output).zip(
                        path_translator(&format!("{translated}/touch"))
                            .inspect_err(translate_warn(&b.output.path))
                            .ok(),
                    )
                }) {
                    context.try_get_bool_binding(output.path.clone(), touch_path);
                }

                if let Some(position) = position.as_ref() {
                    context.try_get_v2_binding(position.output.path.clone(), translated);
                }
            }
        }
    }
}

pub fn handle_skeleton_bindings(
    context: &BindingsProfileLoadContext,
    bindings: &[SimpleActionBinding],
) {
    for SimpleActionBinding { output, path } in bindings {
        trace!("binding skeleton action {} to {path:?}", output.path);
        if !context.find_action(&output.path) {
            continue;
        };

        match &context.actions[&output.path] {
            crate::input::ActionData::Skeleton(hand) => {
                let bound_hand = match path.as_str() {
                    "/user/hand/left/input/skeleton/left" => Hand::Left,
                    "/user/hand/right/input/skeleton/right" => Hand::Right,
                    other => {
                        warn!(
                            "Got invalid skeleton binding {other} for action {}",
                            output.path
                        );
                        continue;
                    }
                };

                if bound_hand != *hand {
                    warn!(
                        "Action {} was created with hand {hand:?}, but is bound to hand {bound_hand:?}",
                        output.path
                    );
                }
            }
            _ => panic!(
                "Expected skeleton action for skeleton binding {}",
                output.path
            ),
        }
    }
}

pub fn handle_haptic_bindings(
    instance: &xr::Instance,
    path_translator: impl Fn(&str) -> Result<String, InvalidActionPath>,
    context: &mut BindingsProfileLoadContext,
    bindings: &[SimpleActionBinding],
) {
    for SimpleActionBinding { output, path } in bindings {
        let Ok(translated) = path_translator(path).inspect_err(translate_warn(&output.path)) else {
            continue;
        };
        if !context.find_action(&output.path) {
            continue;
        };

        assert!(
            matches!(
                &context.actions[&output.path],
                crate::input::ActionData::Haptic(_)
            ),
            "expected haptic action for haptic binding {translated}, got {}",
            output.path
        );
        let xr_path = instance.string_to_path(&translated).unwrap();
        context.push_binding(output.path.clone(), xr_path);
    }
}

pub fn handle_pose_bindings(context: &mut BindingsProfileLoadContext, bindings: &[PoseBinding]) {
    for PoseBinding {
        output,
        path: (hand, pose_ty),
    } in bindings
    {
        if !context.find_action(&output.path) {
            continue;
        };

        assert!(
            matches!(
                context.actions.get_mut(&output.path).unwrap(),
                ActionData::Pose
            ),
            "Expected pose action for pose binding on {}",
            output.path
        );

        let bound = context
            .pose_bindings
            .entry(output.path.clone())
            .or_default();

        let b = match hand {
            Hand::Left => &mut bound.left,
            Hand::Right => &mut bound.right,
        };
        *b = Some(*pose_ty);
        trace!(
            "bound {:?} to pose {} for hand {hand:?}",
            *pose_ty, output.path
        );
    }
}
