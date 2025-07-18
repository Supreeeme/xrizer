use crate::input::action_manifest::{
    ActionPath, ButtonParameters, ControllerType, GrabParameters, LoadedActionDataMap,
};
use crate::input::custom_bindings::{
    BindingData, DpadActions, DpadData, DpadDirection, GrabActions, GrabBindingData,
    ThresholdBindingData,
};
use crate::input::legacy::LegacyActions;
use crate::input::skeletal::SkeletalInputActionData;
use crate::input::ActionData::{Bool, Vector1, Vector2};
use crate::input::{ActionData, BoundPose, ExtraActionData, InteractionProfile};
use crate::openxr_data;
use crate::openxr_data::OpenXrData;
use log::{info, trace, warn};
use openxr as xr;
use std::collections::HashMap;

pub(super) struct BindingsLoadContext<'a> {
    pub action_sets: &'a HashMap<String, xr::ActionSet>,
    pub actions: LoadedActionDataMap,
    pub extra_actions: HashMap<String, ExtraActionData>,
    pub per_profile_bindings: HashMap<xr::Path, HashMap<String, Vec<BindingData>>>,
    pub per_profile_pose_bindings: HashMap<xr::Path, HashMap<String, BoundPose>>,
    pub legacy_actions: &'a LegacyActions,
    pub info_action: &'a xr::Action<bool>,
    pub skeletal_input: &'a SkeletalInputActionData,
}

impl<'a> BindingsLoadContext<'a> {
    pub fn new(
        action_sets: &'a HashMap<String, xr::ActionSet>,
        actions: LoadedActionDataMap,
        legacy_actions: &'a LegacyActions,
        info_action: &'a xr::Action<bool>,
        skeletal_input: &'a SkeletalInputActionData,
    ) -> Self {
        BindingsLoadContext {
            action_sets,
            actions,
            extra_actions: Default::default(),
            per_profile_bindings: Default::default(),
            per_profile_pose_bindings: Default::default(),
            legacy_actions,
            info_action,
            skeletal_input,
        }
    }
}

impl BindingsLoadContext<'_> {
    pub fn for_profile<'a, 'b: 'a, C: openxr_data::Compositor>(
        &'b mut self,
        openxr: &'a OpenXrData<C>,
        profile: &'a dyn InteractionProfile,
        controller_type: &'a ControllerType,
    ) -> Option<BindingsProfileLoadContext<'a>> {
        let instance = &openxr.instance;
        let Ok(interaction_profile) = instance.string_to_path(profile.profile_path()) else {
            warn!("Controller type {controller_type:?} has no OpenXR path supported?");
            return None;
        };

        let hands = [
            openxr.left_hand.subaction_path,
            openxr.right_hand.subaction_path,
        ];

        let bindings_parsed = self
            .per_profile_bindings
            .entry(interaction_profile)
            .or_default();
        let pose_bindings = self
            .per_profile_pose_bindings
            .entry(interaction_profile)
            .or_default();
        Some(BindingsProfileLoadContext {
            profile,
            controller_type,
            action_sets: self.action_sets,
            actions: &mut self.actions,
            extra_actions: &mut self.extra_actions,
            bindings_parsed,
            pose_bindings,
            legacy_actions: self.legacy_actions,
            info_action: self.info_action,
            skeletal_input: self.skeletal_input,
            instance,
            hands,
            bindings: Vec::new(),
        })
    }
}

pub(super) struct BindingsProfileLoadContext<'a> {
    pub profile: &'a dyn InteractionProfile,
    pub controller_type: &'a ControllerType,
    pub action_sets: &'a HashMap<String, xr::ActionSet>,
    pub actions: &'a mut LoadedActionDataMap,
    extra_actions: &'a mut HashMap<String, ExtraActionData>,
    bindings_parsed: &'a mut HashMap<String, Vec<BindingData>>,
    pub pose_bindings: &'a mut HashMap<String, BoundPose>,
    pub legacy_actions: &'a LegacyActions,
    pub info_action: &'a xr::Action<bool>,
    pub skeletal_input: &'a SkeletalInputActionData,
    pub instance: &'a xr::Instance,
    pub hands: [xr::Path; 2],
    pub bindings: Vec<(String, xr::Path)>,
}

pub(super) struct DpadActivatorData {
    pub key: String,
    pub action: xr::Action<f32>,
    pub binding: xr::Path,
}

pub(super) struct DpadHapticData {
    pub key: String,
    pub action: xr::Action<xr::Haptic>,
    pub binding: xr::Path,
}

fn get_hand_prefix(path: &str) -> Option<&str> {
    if path.starts_with("/user/hand/left") {
        Some("/user/hand/left")
    } else if path.starts_with("/user/hand/right") {
        Some("/user/hand/right")
    } else {
        None
    }
}

fn parse_hand_from_path(instance: &xr::Instance, path: &str) -> Option<xr::Path> {
    let hand_prefix = get_hand_prefix(path)?;

    let path = instance.string_to_path(hand_prefix).ok();
    path.and_then(|x| if x == xr::Path::NULL { None } else { Some(x) })
}

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

impl BindingsProfileLoadContext<'_> {
    pub fn get_action_set(&self, p0: &String) -> Option<&xr::ActionSet> {
        self.action_sets.get(p0)
    }

    #[track_caller]
    pub fn find_action(&self, name: &str) -> bool {
        let ret = self.actions.contains_key(name);
        if !ret {
            let caller = std::panic::Location::caller();
            warn!(
                "Couldn't find action {name}, skipping (line {})",
                caller.line()
            );
        }
        ret
    }

    fn try_get_binding(
        &mut self,
        action_path: String,
        input_path: String,
        action_pattern: &dyn ActionPattern,
    ) {
        if self.find_action(&action_path) {
            action_pattern.check_match(&self.actions[&action_path], &action_path);
            trace!("suggesting {input_path} for {action_path}");
            let binding_path = self.instance.string_to_path(&input_path).unwrap();
            self.bindings.push((action_path, binding_path));
        }
    }

    pub fn try_get_bool_binding(&mut self, action_path: String, input_path: String) {
        self.try_get_binding(
            action_path,
            input_path,
            action_match!(Bool(_) | Vector1 { .. }),
        );
    }

    pub fn try_get_float_binding(&mut self, action_path: String, input_path: String) {
        self.try_get_binding(action_path, input_path, action_match!(Vector1 { .. }));
    }

    pub fn try_get_v2_binding(&mut self, action_path: String, input_path: String) {
        self.try_get_binding(action_path, input_path, action_match!(Vector2 { .. }));
    }

    pub fn add_custom_toggle_binding(&mut self, output: &ActionPath, translated: &str) {
        if let Some(binding_hand) = parse_hand_from_path(self.instance, translated) {
            self.bindings_parsed
                .entry(output.path.clone())
                .or_default()
                .push(BindingData::Toggle(Default::default(), binding_hand));
        } else {
            warn!("Binding on {translated} has unknown hand path, it will be ignored")
        }
    }

    pub fn add_custom_button_binding(
        &mut self,
        output: &ActionPath,
        translated: &str,
        parameters: Option<&ButtonParameters>,
    ) {
        if let Some(binding_hand) = parse_hand_from_path(self.instance, translated) {
            let thresholds = parameters.map(|x| &x.click_threshold);
            self.bindings_parsed
                .entry(output.path.clone())
                .or_default()
                .push(BindingData::Threshold(
                    ThresholdBindingData::new(
                        thresholds
                            .and_then(|x| x.click_activate_threshold.as_ref())
                            .map(|x| x.0),
                        thresholds
                            .and_then(|x| x.click_deactivate_threshold.as_ref())
                            .map(|x| x.0),
                    ),
                    binding_hand,
                ));
        } else {
            info!(
                "Binding on {} has unknown hand path, it will be ignored",
                &translated
            )
        }
    }

    pub fn add_custom_grab_binding(
        &mut self,
        output: &ActionPath,
        translated_force: &str,
        parameters: &Option<GrabParameters>,
    ) {
        if let Some(binding_hand) = parse_hand_from_path(self.instance, translated_force) {
            self.bindings_parsed
                .entry(output.path.clone())
                .or_default()
                .push(BindingData::Grab(
                    GrabBindingData::new(
                        parameters
                            .as_ref()
                            .and_then(|x| x.value_hold_threshold.as_ref())
                            .map(|x| x.0),
                        parameters
                            .as_ref()
                            .and_then(|x| x.value_release_threshold.as_ref())
                            .map(|x| x.0),
                    ),
                    binding_hand,
                ));
        } else {
            info!(
                "Binding on {} has unknown hand path, it will be ignored",
                &translated_force
            )
        }
    }

    pub fn add_custom_dpad_binding(
        &mut self,
        parent_path: &str,
        action_name: &str,
        direction: DpadDirection,
        created_actions: &(
            xr::Action<xr::Vector2f>,
            Option<DpadActivatorData>,
            Option<DpadHapticData>,
        ),
    ) {
        if let Some(binding_hand) = parse_hand_from_path(self.instance, parent_path) {
            let (parent_action, click_or_touch, haptic) = created_actions;
            // Add an empty extra actions holder - custom bindings are gated by their presence
            self.extra_actions
                .entry(action_name.to_string())
                .or_default();
            let dpad_actions = DpadActions {
                xy: parent_action.clone(),
                click_or_touch: click_or_touch.as_ref().map(|d| d.action.clone()),
                haptic: haptic.as_ref().map(|x| x.action.clone()),
            };
            self.bindings_parsed
                .entry(action_name.to_string())
                .or_default()
                .push(BindingData::Dpad(
                    DpadData {
                        dpad_actions,
                        direction,
                        last_state: false.into(),
                    },
                    binding_hand,
                ));
        } else {
            warn!("Binding on {parent_path} has unknown hand path, it will be ignored")
        }
    }

    pub fn push_binding(&mut self, action: String, path: xr::Path) {
        self.bindings.push((action, path));
    }

    pub fn get_or_create_toggle_extra_action(
        &mut self,
        output: &ActionPath,
        action_set_name: &str,
        action_set: &xr::ActionSet,
    ) -> String {
        let name_only = output.cleaned_name();
        let toggle_name = format!("{name_only}_tgl");
        let as_name = format!("{action_set_name}/{toggle_name}");

        let mut extra_data = self.extra_actions.remove(&output.path).unwrap_or_default();

        if extra_data.toggle_action.is_none() {
            let localized = format!("{name_only} toggle");
            let action = action_set
                .create_action(&toggle_name, &localized, &self.hands)
                .unwrap();

            self.actions.insert(as_name.clone(), Bool(action.clone()));

            extra_data.toggle_action = Some(action);
        }
        self.extra_actions.insert(output.path.clone(), extra_data);

        as_name
    }

    pub fn get_or_create_analog_extra_action(
        &mut self,
        output: &ActionPath,
        action_set_name: &str,
        action_set: &xr::ActionSet,
    ) -> String {
        let mut extra_data = self.extra_actions.remove(&output.path).unwrap_or_default();
        let name_only = output.cleaned_name();
        let float_name = format!("{name_only}_asfloat");
        let float_name_with_as = format!("{action_set_name}/{float_name}");
        if extra_data.analog_action.is_none() {
            let localized = format!("{name_only} from float");
            let float_action = action_set
                .create_action(&float_name, &localized, &self.hands)
                .unwrap();

            self.actions.insert(
                float_name_with_as.clone(),
                Vector1 {
                    action: float_action.clone(),
                    last_value: 0.0.into(),
                },
            );

            extra_data.analog_action = Some(float_action);
        }
        self.extra_actions.insert(output.path.clone(), extra_data);

        float_name_with_as
    }

    pub fn get_or_create_v2_extra_action(
        &mut self,
        output: &ActionPath,
        action_set_name: &str,
        action_set: &xr::ActionSet,
    ) -> String {
        let mut extra_data = self.extra_actions.remove(&output.path).unwrap_or_default();
        let name_only = output.cleaned_name();
        let float_name = format!("{name_only}_asfloat2");
        let float_name_with_as = format!("{action_set_name}/{float_name}");
        if extra_data.vector2_action.is_none() {
            let localized = format!("{name_only} from float2");
            let float_action = action_set
                .create_action(&float_name, &localized, &self.hands)
                .unwrap();

            self.actions.insert(
                float_name_with_as.clone(),
                Vector2 {
                    action: float_action.clone(),
                    last_value: (0.0.into(), 0.0.into()),
                },
            );

            extra_data.vector2_action = Some(float_action);
        }
        self.extra_actions.insert(output.path.clone(), extra_data);

        float_name_with_as
    }

    pub fn get_or_create_grab_action_pair(
        &mut self,
        output: &ActionPath,
        action_set_name: &str,
        action_set: &xr::ActionSet,
    ) -> (String, String) {
        let name_only = output.cleaned_name();
        let force_name = format!("{name_only}_grabactionf");
        let value_name = format!("{name_only}_grabactionv");

        let force_full_name = format!("{action_set_name}/{force_name}");
        let value_full_name = format!("{action_set_name}/{value_name}");

        let mut data = self.extra_actions.remove(&output.path).unwrap_or_default();
        if data.grab_action.is_none() {
            let localized = format!("{name_only} grab action (force)");
            let force_action = action_set
                .create_action(&force_name, &localized, &self.hands)
                .unwrap();
            let localizedv = format!("{name_only} grab action (value)");
            let value_action = action_set
                .create_action(&value_name, &localizedv, &self.hands)
                .unwrap();

            self.actions.insert(
                force_full_name.clone(),
                Vector1 {
                    action: force_action.clone(),
                    last_value: Default::default(),
                },
            );
            self.actions.insert(
                value_full_name.clone(),
                Vector1 {
                    action: value_action.clone(),
                    last_value: Default::default(),
                },
            );

            data.grab_action = Some(GrabActions {
                force_action,
                value_action,
            });
        }
        self.extra_actions.insert(output.path.clone(), data);

        (force_full_name, value_full_name)
    }

    pub fn get_dpad_parent(
        &mut self,
        string_to_path: &impl Fn(&str) -> Option<xr::Path>,
        parent_path: &str,
        parent_action_key: &str,
        action_set_name: &str,
        action_set: &xr::ActionSet,
        parameters: Option<&crate::input::action_manifest::DpadParameters>,
    ) -> (
        xr::Action<xr::Vector2f>,
        Option<DpadActivatorData>,
        Option<DpadHapticData>,
    ) {
        // Share parent actions that use the same action set and same bound path
        let parent_action = self
            .actions
            .entry(parent_action_key.to_string())
            .or_insert_with(|| {
                let clean_parent_path = parent_path.replace("/", "_");
                let parent_action_name = format!("xrizer-dpad-parent-{clean_parent_path}");
                let localized = format!("XRizer dpad parent ({parent_path})");
                let action = action_set
                    .create_action::<xr::Vector2f>(&parent_action_name, &localized, &self.hands)
                    .unwrap();

                trace!("created new dpad parent ({parent_action_key})");

                ActionData::Vector2 {
                    action,
                    last_value: Default::default(),
                }
            });
        let ActionData::Vector2 {
            action: parent_action,
            ..
        } = parent_action
        else {
            unreachable!();
        };
        // Remove lifetime
        let parent_action = parent_action.clone();
        let use_force = matches!(self.controller_type, ControllerType::Knuckles)
            && parent_path.ends_with("trackpad");

        // Create our path to our parent click/touch, if such a path exists
        let (activator_binding_str, activator_binding_path) = parameters
            .as_ref()
            .and_then(|p| {
                let name = match p.sub_mode {
                    crate::input::action_manifest::DpadSubMode::Click => {
                        if use_force {
                            format!("{parent_path}/force")
                        } else {
                            format!("{parent_path}/click")
                        }
                    }
                    crate::input::action_manifest::DpadSubMode::Touch => {
                        format!("{parent_path}/touch")
                    }
                };
                string_to_path(&name).map(|p| (name, p))
            })
            .unzip();

        let activator_key = activator_binding_str
            .as_ref()
            .map(|n| format!("{n}-{action_set_name}"));
        // Action only needs to exist if our path was successfully created
        let len = self.actions.len();
        let activator_action = activator_key.as_ref().map(|key| {
            let action = self.actions.entry(key.clone()).or_insert_with(|| {
                let dpad_activator_name = format!("xrizer-dpad-active{len}");
                let localized = format!("XRizer dpad active ({len})");

                ActionData::Vector1 {
                    action: action_set
                        .create_action(&dpad_activator_name, &localized, &self.hands)
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

        let haptic_data = if use_force {
            // the need for haptic coincides with force-using dpads for now
            let hand_path = get_hand_prefix(parent_path)
                .and_then(|x| string_to_path(&format!("{x}/output/haptic")));
            let haptic_key = format!("{parent_path}-{action_set_name}-haptic");
            hand_path.map(|hand_path| {
                let action = self.actions.entry(haptic_key.clone()).or_insert_with(|| {
                    let haptic_name = format!("xrizer-dpad-haptic{len}");
                    let localized = format!("XRizer dpad haptic ({len})");

                    ActionData::Haptic(
                        action_set
                            .create_action(&haptic_name, &localized, &self.hands)
                            .unwrap(),
                    )
                });

                let ActionData::Haptic(action) = action else {
                    unreachable!();
                };
                DpadHapticData {
                    action: action.clone(),
                    key: haptic_key,
                    binding: hand_path,
                }
            })
        } else {
            None
        };

        (
            parent_action,
            click_or_touch.map(|action| DpadActivatorData {
                key: activator_key.unwrap(),
                action,
                binding: activator_binding_path.unwrap(),
            }),
            haptic_data,
        )
    }
}
