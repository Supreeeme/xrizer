use crate::input::action_manifest::{ActionPath, ControllerType, LoadedActionDataMap};
use crate::input::custom_bindings::{
    AsActionData, AsIter, BindingData, CustomBindingHelper, Names,
};
use crate::input::skeletal::SkeletalInputActionData;
use crate::input::ActionData::{Bool, Vector1, Vector2};
use crate::input::{ActionData, BoundPose, ExtraActionData, InteractionProfile};
use crate::openxr_data;
use crate::openxr_data::OpenXrData;
use log::{trace, warn};
use openxr as xr;
use std::collections::HashMap;

pub(super) struct BindingsLoadContext<'a> {
    pub action_sets: &'a HashMap<String, xr::ActionSet>,
    pub actions: LoadedActionDataMap,
    pub extra_actions: HashMap<String, ExtraActionData>,
    pub per_profile_bindings: HashMap<xr::Path, HashMap<String, Vec<BindingData>>>,
    pub per_profile_pose_bindings: HashMap<xr::Path, HashMap<String, BoundPose>>,
    pub grip_action: &'a xr::Action<xr::Posef>,
    pub info_action: &'a xr::Action<bool>,
    pub skeletal_input: &'a SkeletalInputActionData,
}

impl<'a> BindingsLoadContext<'a> {
    pub fn new(
        action_sets: &'a HashMap<String, xr::ActionSet>,
        actions: LoadedActionDataMap,
        grip_action: &'a xr::Action<xr::Posef>,
        info_action: &'a xr::Action<bool>,
        skeletal_input: &'a SkeletalInputActionData,
    ) -> Self {
        BindingsLoadContext {
            action_sets,
            actions,
            extra_actions: Default::default(),
            per_profile_bindings: Default::default(),
            per_profile_pose_bindings: Default::default(),
            grip_action,
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
            grip_action: self.grip_action,
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
    pub grip_action: &'a xr::Action<xr::Posef>,
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

pub(super) fn parse_hand_from_path(instance: &xr::Instance, path: &str) -> Option<xr::Path> {
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

    pub fn add_custom_binding<T: CustomBindingHelper>(
        &mut self,
        output: &ActionPath,
        hand: xr::Path,
        action_set_name: &str,
        action_set: &xr::ActionSet,
        params: Option<&T::BindingParams>,
    ) -> T::ExtraActions<Names> {
        let extra_data = self.extra_actions.entry(output.path.clone()).or_default();
        let names = T::extra_action_names(&output.cleaned_name());
        let full_names: Vec<String> = names
            .as_iter()
            .map(|name| format!("{action_set_name}/{name}"))
            .collect();

        if let Some(actions) = T::get_actions(extra_data) {
            if actions.is_none() {
                let extra_actions = T::create_actions(&names, action_set, &self.hands);
                for (name, action) in full_names.iter().zip(extra_actions.as_action_data()) {
                    trace!("creating custom binding: {name}");
                    self.actions.insert(name.clone(), action);
                }

                *actions = Some(extra_actions);
            }
        }

        self.bindings_parsed
            .entry(output.path.clone())
            .or_default()
            .push(T::create_binding_data(hand, params));

        T::ExtraActions::from_iter(full_names)
    }

    pub fn push_binding(&mut self, action: String, path: xr::Path) {
        self.bindings.push((action, path));
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
