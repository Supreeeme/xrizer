#![allow(private_interfaces)]

use super::context::{BindingsProfileLoadContext, DpadActivatorData, DpadHapticData};
use crate::input::action_manifest::context;
use crate::input::profiles::paths::DynComponent;
use crate::input::profiles::{Component, DynInputPath, paths};
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
use serde::de::value::StringDeserializer;
use serde::{
    Deserialize,
    de::{Error, IgnoredAny, Unexpected},
};
use std::collections::HashMap;
use std::marker::PhantomData;
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

/// Note that when this is used, it's typically missing the final component
#[derive(Deserialize)]
#[serde(from = "String")]
enum MaybeInputPath {
    Valid(DynInputPath),
    Invalid { path: String, error: String },
}

impl From<String> for MaybeInputPath {
    fn from(value: String) -> Self {
        match value.parse() {
            Ok(path) => Self::Valid(path),
            Err(error) => Self::Invalid { path: value, error },
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionBinding {
    None(IgnoredAny),
    Button(ActionBindingData<ButtonInput, ButtonParameters>),
    ToggleButton(ActionBindingData<ButtonInput>),
    Dpad(ActionBindingData<DpadInput, DpadParameters>),
    Trigger(ActionBindingData<TriggerInput, ClickThresholdParams>),
    ScalarConstant(ActionBindingData<ScalarConstantInput, ScalarConstantParameters>),
    ForceSensor(ActionBindingData<ForceSensorInput, ForceSensorParameters>),
    Grab(ActionBindingData<GrabInput, GrabParameters>),
    Scroll(ActionBindingData<ScrollInput, ScrollParameters>),
    Trackpad(ActionBindingData<Vector2Input, Vector2Parameters>),
    Joystick(ActionBindingData<Vector2Input, Vector2Parameters>),
}

#[derive(Deserialize)]
struct ActionBindingData<Inputs, Parameters = ()> {
    path: MaybeInputPath,
    inputs: Inputs,
    parameters: Option<Parameters>,
}

struct ValidActionBindingData<'a, Inputs, Parameters> {
    path: DynInputPath,
    inputs: &'a Inputs,
    parameters: Option<&'a Parameters>,
}

impl<Inputs, Parameters> ActionBindingData<Inputs, Parameters> {
    fn validate_path(&self) -> Option<ValidActionBindingData<'_, Inputs, Parameters>> {
        match &self.path {
            MaybeInputPath::Valid(path) => Some(ValidActionBindingData {
                path: *path,
                inputs: &self.inputs,
                parameters: self.parameters.as_ref(),
            }),
            MaybeInputPath::Invalid { path, error } => {
                warn!("got invalid input path {path} - {error}");
                None
            }
        }
    }
}

pub trait PathValidator: Fn(DynInputPath) -> Option<DynInputPath> {}
impl<F> PathValidator for F where F: Fn(DynInputPath) -> Option<DynInputPath> {}

#[derive(Deserialize, Debug)]
struct ActionBindingOutput<C> {
    output: ActionPath,
    #[serde(skip)]
    _marker: PhantomData<C>,
}

/// Marker for actions that should be bound to a custom binding
#[derive(Debug)]
struct Custom;

struct InvalidActionPath<'a>(DynInputPath, &'a str);
impl InvalidActionPath<'_> {
    fn warn(&self) {
        warn!("invalid path {} for {}", self.0, self.1);
    }
}

impl<C: Component> ActionBindingOutput<C>
where
    C::Output: context::WithActionPattern,
{
    fn try_bind_with_component(
        &self,
        partial_input_path: DynInputPath,
        context: &mut BindingsProfileLoadContext<'_>,
        validator: impl PathValidator,
    ) -> Result<(), InvalidActionPath<'_>> {
        let complete_path = match C::DYN {
            Some(component) => partial_input_path.with_component(component),
            None => partial_input_path,
        };

        match validator(complete_path) {
            Some(path) => {
                context.try_suggest_binding::<C::Output>(self.output.path.clone(), path);
                Ok(())
            }
            None => Err(InvalidActionPath(complete_path, &self.output.path)),
        }
    }
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
    touch: Option<ActionBindingOutput<paths::Touch>>,
    /// Click can be overridden to use a different path via the `force_input` parameter.
    click: Option<ActionBindingOutput<paths::Click>>,
    double: Option<ActionBindingOutput<Custom>>,
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
    #[serde(default, deserialize_with = "ButtonForceInput::default_deserialize")]
    force_input: Option<ButtonForceInput>,
    #[serde(flatten)]
    click_threshold: ClickThresholdParams,
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
enum ButtonForceInput {
    Click,
    Value,
    Force,
    Position,
}

impl ButtonForceInput {
    fn default_deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<ButtonForceInput>, D::Error> {
        let s: Option<String> = Deserialize::deserialize(d)?;
        let Some(s) = s else {
            return Ok(None);
        };
        if s.is_empty() {
            Ok(Some(Self::Click))
        } else {
            Self::deserialize(StringDeserializer::new(s)).map(Some)
        }
    }
}

#[derive(Deserialize, Debug)]
struct DpadInput {
    east: Option<ActionBindingOutput<Custom>>,
    south: Option<ActionBindingOutput<Custom>>,
    north: Option<ActionBindingOutput<Custom>>,
    west: Option<ActionBindingOutput<Custom>>,
    center: Option<ActionBindingOutput<Custom>>,
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
    pull: Option<ActionBindingOutput<paths::Value>>,
    touch: Option<ActionBindingOutput<paths::Touch>>,
    click: Option<ActionBindingOutput<paths::Click>>,
}

#[derive(Deserialize)]
struct ScalarConstantInput {
    value: ActionBindingOutput<paths::Value>,
}

#[derive(Deserialize)]
struct ForceSensorInput {
    force: ActionBindingOutput<paths::Force>,
}

#[derive(Deserialize)]
struct ForceSensorParameters {
    #[allow(unused)]
    haptic_amplitude: Option<String>,
}

#[derive(Deserialize)]
struct GrabInput {
    grab: ActionBindingOutput<Custom>,
}

#[derive(Deserialize)]
pub struct GrabParameters {
    pub value_hold_threshold: Option<FromString<f32>>,
    pub value_release_threshold: Option<FromString<f32>>,
}

#[derive(Deserialize)]
struct ScrollInput {
    scroll: ActionBindingOutput<()>,
}

#[derive(Deserialize)]
struct ScrollParameters {
    #[allow(unused)]
    scroll_mode: Option<String>,
    #[allow(unused)]
    smooth_scroll_multiplier: Option<String>, // float
}

#[derive(Deserialize)]
struct Vector2Input {
    position: Option<ActionBindingOutput<()>>,
    click: Option<ActionBindingOutput<paths::Click>>,
    touch: Option<ActionBindingOutput<paths::Touch>>,
}

#[derive(Deserialize)]
struct Vector2Parameters {
    #[allow(unused)]
    deadzone_pct: Option<FromString<u8>>,
    #[allow(unused)]
    maxzone_pct: Option<FromString<u8>>,
    #[allow(unused)]
    sticky_click: Option<FromString<bool>>,
}

pub fn handle_dpad_binding(
    string_to_path: impl Fn(&str) -> Option<xr::Path>,
    parent_path: DynInputPath,
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
        F: for<'a> Fn(
            &'a Option<ActionBindingOutput<Custom>>,
            DpadDirection,
        ) -> Option<&'a ActionPath>,
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

    for (path, direction) in bound_actions {
        context.add_custom_binding::<DpadData>(
            path,
            parent_path.hand,
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
    context.push_binding(
        parent_action_key,
        string_to_path(&parent_path.to_string()).unwrap(),
    );
    if let Some((s, p)) = activator_binding {
        context.push_binding(s, p);
    }
    if let Some((s, p)) = haptic_binding {
        context.push_binding(s, p);
    }
}

pub fn handle_sources(
    validate_path: &dyn PathValidator,
    context: &mut BindingsProfileLoadContext,
    action_set_name: &str,
    action_set: &xr::ActionSet,
    sources: &[ActionBinding],
) {
    for mode in sources {
        match mode {
            ActionBinding::None(_) => {}
            ActionBinding::ToggleButton(data) => {
                let Some(ValidActionBindingData {
                    path,
                    inputs: ButtonInput { touch, click, .. },
                    parameters: _,
                }) = data.validate_path()
                else {
                    continue;
                };

                if let Some(touch) = touch {
                    let _ = touch
                        .try_bind_with_component(path, context, validate_path)
                        .inspect_err(InvalidActionPath::warn);
                }

                if let Some(click) = click {
                    let click_path = path.with_component(DynComponent::Click);
                    let Some(click_path) = validate_path(click_path) else {
                        continue;
                    };

                    if !context.find_action(&click.output.path) {
                        continue;
                    }

                    let action = context.add_custom_binding::<ToggleData>(
                        &click.output,
                        path.hand,
                        action_set_name,
                        action_set,
                        None,
                    );

                    trace!("suggesting {click_path} for {} (toggle)", click.output.path);
                    context.push_binding(
                        action,
                        context
                            .instance
                            .string_to_path(&click_path.to_string())
                            .unwrap(),
                    );
                }
            }
            ActionBinding::Button(data) => {
                let Some(ValidActionBindingData {
                    path,
                    inputs:
                        ButtonInput {
                            touch,
                            click,
                            double,
                        },
                    parameters,
                }) = data.validate_path()
                else {
                    continue;
                };

                if let Some(touch) = touch {
                    let _ = touch
                        .try_bind_with_component(path, context, validate_path)
                        .inspect_err(InvalidActionPath::warn);
                }

                let click_path = path.with_component(DynComponent::Click);
                if let Some(double) = double
                    && let Ok(complete_path) = validate_path(click_path)
                        .ok_or_else(|| InvalidActionPath(click_path, &double.output.path))
                        .inspect_err(InvalidActionPath::warn)
                {
                    let name = context.add_custom_binding::<DoubleTapData>(
                        &double.output,
                        complete_path.hand,
                        action_set_name,
                        action_set,
                        None,
                    );

                    context.push_binding(
                        name,
                        context
                            .instance
                            .string_to_path(&complete_path.to_string())
                            .unwrap(),
                    );
                }

                if let Some(click) = click {
                    let target = parameters.and_then(|x| x.force_input).unwrap_or(
                        // Default to value for clicky components, because the click point
                        // does not necessarily match SteamVR's click point.
                        ButtonForceInput::Value,
                    );

                    let complete_path = match target {
                        ButtonForceInput::Click => path.with_component(DynComponent::Click),
                        ButtonForceInput::Value => path.with_component(DynComponent::Value),
                        ButtonForceInput::Force => path.with_component(DynComponent::Force),
                        ButtonForceInput::Position => path, // No component = 2D binding
                    };

                    let complete_path = match validate_path(complete_path) {
                        None
                        // If the translated path we get is just the click component, there's no need
                        // to create and bind to our threshold action.
                        | Some(DynInputPath {
                            component: Some(DynComponent::Click),
                            ..
                        }) => {
                            if !matches!(target, ButtonForceInput::Click) {
                                debug!(
                                    "falling back to click component for {} on {} (target: {:?})",
                                    click.output.path, path, target
                                );
                            }
                            let _ = click
                                .try_bind_with_component(
                                    path,
                                    context,
                                    validate_path,
                                )
                                .inspect_err(InvalidActionPath::warn);
                            continue;
                        }
                        Some(path) => path,
                    };

                    let params = parameters.map(|b| &b.click_threshold);
                    let float_name_with_as = if complete_path.component.is_none() {
                        context.add_custom_binding::<ThresholdBindingVector2>(
                            &click.output,
                            complete_path.hand,
                            action_set_name,
                            action_set,
                            params,
                        )
                    } else {
                        context.add_custom_binding::<ThresholdBindingFloat>(
                            &click.output,
                            complete_path.hand,
                            action_set_name,
                            action_set,
                            params,
                        )
                    };

                    context.push_binding(
                        float_name_with_as,
                        context
                            .instance
                            .string_to_path(&complete_path.to_string())
                            .unwrap(),
                    );
                }
            }
            ActionBinding::Dpad(data) => {
                let Some(ValidActionBindingData {
                    path,
                    inputs,
                    parameters,
                }) = data.validate_path()
                else {
                    continue;
                };

                if validate_path(path).is_none() {
                    InvalidActionPath(path, &format!("{inputs:#?}")).warn();
                    continue;
                }
                handle_dpad_binding(
                    |s| {
                        // TODO: don't do this conversion dance
                        let Ok(path) = s.parse::<DynInputPath>() else {
                            warn!("invalid path {s} for dpad binding");
                            return None;
                        };

                        if validate_path(path).is_none() {
                            InvalidActionPath(path, s).warn();
                            return None;
                        }

                        Some(context.instance.string_to_path(&path.to_string()).unwrap())
                    },
                    path,
                    action_set_name,
                    action_set,
                    context,
                    inputs,
                    parameters,
                );
            }
            ActionBinding::Trigger(data) => {
                let Some(ValidActionBindingData {
                    path,
                    inputs: TriggerInput { pull, touch, click },
                    parameters: _,
                }) = data.validate_path()
                else {
                    continue;
                };

                if let Some(pull) = pull {
                    let _ = pull
                        .try_bind_with_component(path, context, validate_path)
                        .inspect_err(InvalidActionPath::warn);
                }

                if let Some(click) = click {
                    let _ = click
                        .try_bind_with_component(path, context, validate_path)
                        .inspect_err(InvalidActionPath::warn);
                }

                if let Some(touch) = touch
                    && touch
                        .try_bind_with_component(path, context, validate_path)
                        .is_err()
                {
                    debug!(
                        "Falling back to pull for touch on {path} (action {:?})",
                        &touch.output.path
                    );
                    // SteamVR fallbacks "touch" bindings on triggers to "any pull amount" if there's no native capsense
                    let with_pull = path.with_component(DynComponent::Value);
                    if let Some(with_pull) = validate_path(with_pull) {
                        let parameters = ClickThresholdParams::new_for_touch_conversion();
                        let float_name_with_as = context
                            .add_custom_binding::<ThresholdBindingFloat>(
                                &touch.output,
                                with_pull.hand,
                                action_set_name,
                                action_set,
                                Some(&parameters),
                            );
                        context.push_binding(
                            float_name_with_as,
                            context
                                .instance
                                .string_to_path(&with_pull.to_string())
                                .unwrap(),
                        );
                    } else {
                        warn!(
                            "failed to bind trigger pull or touch for action {} - invalid path ({with_pull})",
                            touch.output.path
                        );
                    }
                }
            }
            ActionBinding::ScalarConstant(data) => {
                let Some(ValidActionBindingData {
                    path,
                    inputs: ScalarConstantInput { value },
                    ..
                }) = data.validate_path()
                else {
                    continue;
                };

                let _ = value
                    .try_bind_with_component(path, context, validate_path)
                    .inspect_err(InvalidActionPath::warn);
            }
            ActionBinding::ForceSensor(data) => {
                let Some(ValidActionBindingData {
                    path,
                    inputs: ForceSensorInput { force },
                    ..
                }) = data.validate_path()
                else {
                    continue;
                };

                let _ = force
                    .try_bind_with_component(path, context, validate_path)
                    .inspect_err(InvalidActionPath::warn);
            }
            ActionBinding::Grab(data) => {
                let Some(ValidActionBindingData {
                    path,
                    inputs: GrabInput { grab },
                    parameters,
                }) = data.validate_path()
                else {
                    continue;
                };

                let force_path = path.with_component(DynComponent::Force);
                let value_path = path.with_component(DynComponent::Value);

                let Ok((force_path, value_path)) = validate_path(force_path)
                    .ok_or_else(|| InvalidActionPath(force_path, &grab.output.path))
                    .and_then(|force| {
                        Ok((
                            force,
                            validate_path(value_path)
                                .ok_or_else(|| InvalidActionPath(value_path, &grab.output.path))?,
                        ))
                    })
                    .inspect_err(InvalidActionPath::warn)
                else {
                    continue;
                };

                if !context.find_action(&grab.output.path) {
                    continue;
                }

                let GrabActions {
                    force_action,
                    value_action,
                } = context.add_custom_binding::<GrabBindingData>(
                    &grab.output,
                    path.hand,
                    action_set_name,
                    action_set,
                    parameters,
                );

                trace!(
                    "suggesting {force_path} and {value_path} for {force_action} (grab binding)"
                );
                context.push_binding(
                    force_action,
                    context
                        .instance
                        .string_to_path(&force_path.to_string())
                        .unwrap(),
                );
                context.push_binding(
                    value_action,
                    context
                        .instance
                        .string_to_path(&value_path.to_string())
                        .unwrap(),
                );
            }
            ActionBinding::Scroll(data) => {
                let Some(ValidActionBindingData {
                    inputs,
                    path,
                    parameters: _,
                }) = data.validate_path()
                else {
                    continue;
                };
                let ScrollInput { scroll } = inputs;
                // TODO: custom scrolling for trackpads
                let _ = scroll
                    .try_bind_with_component(path, context, validate_path)
                    .inspect_err(InvalidActionPath::warn);
            }
            ActionBinding::Trackpad(data) | ActionBinding::Joystick(data) => {
                let Some(ValidActionBindingData { path, inputs, .. }) = data.validate_path() else {
                    continue;
                };

                let Vector2Input {
                    position,
                    click,
                    touch,
                } = inputs;

                if let Some(click) = click {
                    let _ = click.try_bind_with_component(path, context, validate_path);
                }

                if let Some(touch) = touch {
                    let _ = touch.try_bind_with_component(path, context, validate_path);
                }

                if let Some(position) = position {
                    let _ = position.try_bind_with_component(path, context, validate_path);
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
    context: &mut BindingsProfileLoadContext,
    bindings: &[SimpleActionBinding],
) {
    for SimpleActionBinding { output, path } in bindings {
        if !matches!(
            path.as_str(),
            "/user/hand/left/output/haptic" | "/user/hand/right/output/haptic",
        ) {
            warn!("invalid haptic path {path} for {}", output.path);
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
            "expected haptic action for haptic binding {path}, got {}",
            output.path
        );
        let xr_path = instance.string_to_path(path).unwrap();
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
