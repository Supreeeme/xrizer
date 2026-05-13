mod actions;
mod bindings;
mod context;

pub(super) use actions::ControllerType;
pub(super) use bindings::{ClickThresholdParams, GrabParameters};

use crate::input::InteractionProfile;
use crate::input::action_manifest::context::BindingsLoadContext;
use crate::input::profiles::LegalPathsT;
use crate::input::{ActionKey, Input, profiles::RunWithProfile, skeletal::SkeletalInputActionData};
use crate::openxr_data::{self, Hand, SessionData};
use log::{debug, error, info, warn};
use openvr as vr;
use openxr as xr;
use slotmap::{SecondaryMap, SlotMap};
use std::collections::{HashMap, HashSet};
use std::env::current_dir;
use std::path::{Path, PathBuf};

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
                if session_data.input_data.actions.get().is_some() {
                    return Ok(());
                }
            }
            None => {
                if let Some(loaded) = session_data.input_data.actions.get() {
                    error!(
                        "{} actions are already loaded!",
                        if matches!(loaded, super::LoadedActions::Legacy(_)) {
                            "Legacy"
                        } else {
                            "Manifest"
                        }
                    );
                    return Err(vr::EVRInputError::MismatchedActionManifest);
                }
                self.loaded_actions_path
                    .set(manifest_path.to_path_buf())
                    .unwrap();
            }
        }

        let data = std::fs::read(manifest_path).map_err(|e| {
            error!("Failed to read manifest {}: {e}", manifest_path.display());
            vr::EVRInputError::InvalidParam
        })?;

        let manifest: actions::ActionManifest = serde_json::from_slice(&data).map_err(|e| {
            error!("Failed to parse action manifest: {e}");
            vr::EVRInputError::InvalidParam
        })?;

        // TODO: support non english localization?
        let english = manifest
            .localization
            .and_then(|l| l.into_iter().find(|l| l.language_tag == "en_US"));

        let mut sets = actions::load_action_sets(
            &self.openxr.instance,
            english.as_ref(),
            manifest.action_sets,
        )?;
        debug!("Loaded {} action sets.", sets.len());

        let left_hand_subaction_path = self.get_subaction_path(Hand::Left);
        let right_hand_subaction_path = self.get_subaction_path(Hand::Right);

        let actions = actions::load_actions(
            &self.openxr.instance,
            english.as_ref(),
            &mut sets,
            manifest.actions,
            left_hand_subaction_path,
            right_hand_subaction_path,
        )?;
        debug!("Loaded {} actions.", actions.len());

        let skeletal_input = session_data
            .input_data
            .estimated_skeleton_actions
            .get_or_init(|| {
                SkeletalInputActionData::new(
                    &self.openxr.instance,
                    left_hand_subaction_path,
                    right_hand_subaction_path,
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
        // Generate an action set & action for handling haptic pulses.
        // See `System::TriggerHapticPulse` & `Input::legacy_haptic`.
        let haptic_set = self
            .openxr
            .instance
            .create_action_set("xrizer-haptic-set", "XRizer haptic set", 0)
            .unwrap();
        let haptic_action = haptic_set
            .create_action::<xr::Haptic>(
                "xrizer-haptic-action",
                "XRizer haptic action",
                &[self.subaction_paths.left, self.subaction_paths.right],
            )
            .unwrap();

        let mut binding_context = context::BindingsLoadContext::new(
            &sets,
            actions,
            &session_data.input_data.pose_data.get().unwrap().grip,
            &info_action,
            &haptic_action,
            skeletal_input,
        );

        self.load_bindings(
            manifest_path.parent().unwrap(),
            manifest.default_bindings,
            &mut binding_context,
        );

        let context::BindingsLoadContext {
            actions,
            extra_actions,
            per_profile_bindings,
            per_profile_pose_bindings,
            ..
        } = binding_context;

        let xr_sets: Vec<_> = sets
            .values()
            .chain([
                &session_data.input_data.pose_data.get().unwrap().set,
                &info_set,
                &haptic_set,
                &skeletal_input.set,
            ])
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

        let mut actions_with_custom_bindings = HashSet::new();
        let per_profile_bindings = per_profile_bindings
            .into_iter()
            .map(|(k, v)| {
                (k, {
                    v.into_iter()
                        .map(|(name, actions)| {
                            let key = act_guard
                                .iter()
                                .find_map(|(key, super::Action { path })| {
                                    (*path == name).then_some(key)
                                })
                                .unwrap_or_else(|| {
                                    act_guard.insert(super::Action { path: name.clone() })
                                });

                            actions_with_custom_bindings.insert(key);

                            (key, actions)
                        })
                        .collect()
                })
            })
            .collect();

        let per_profile_pose_bindings = per_profile_pose_bindings
            .into_iter()
            .map(|(k, v)| (k, action_map_to_secondary(&mut act_guard, v)))
            .collect();

        let loaded = super::ManifestLoadedActions {
            sets,
            actions,
            actions_with_custom_bindings,
            extra_actions,
            per_profile_bindings,
            per_profile_pose_bindings,
            _info_action: info_action,
            info_set,
            haptic_action,
            haptic_set,
        };

        session_data
            .input_data
            .actions
            .set(super::LoadedActions::Manifest(loaded))
            .unwrap_or_else(|_| unreachable!());
        Ok(())
    }
}

impl<C: openxr_data::Compositor> Input<C> {
    fn load_bindings(
        &self,
        parent_path: &Path,
        bindings: Vec<actions::DefaultBindings>,
        context: &mut context::BindingsLoadContext,
    ) {
        let mut it = bindings.into_iter().peekable();
        while let Some(actions::DefaultBindings {
            binding_url,
            controller_type,
        }) = it.next()
        {
            let custom_path = if let Ok(custom_dir) = std::env::var("XRIZER_CUSTOM_BINDINGS_DIR") {
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

            let data = match std::fs::read(bindings_path) {
                Ok(data) => data,
                Err(e) => {
                    error!("Couldn't load bindings for {controller_type:?}: {e}");
                    continue;
                }
            };

            let bindings = match serde_json::from_slice(&data) {
                Ok(bindings) => bindings,
                Err(e) => {
                    error!("Failed to parse bindings for {controller_type:?}: {e}");
                    continue;
                }
            };

            match controller_type {
                actions::ControllerType::Unknown(ref other) => {
                    info!("Ignoring bindings for unknown profile {other}")
                }
                ref other => {
                    let mut runner = Runner(self, context, bindings);
                    other.run_for_profile(&mut runner);

                    // lifetime moment
                    struct Runner<'a, 'b, 'c, C: openxr_data::Compositor>(
                        &'a Input<C>,
                        &'b mut BindingsLoadContext<'c>,
                        bindings::Bindings,
                    );
                    impl<C: openxr_data::Compositor> RunWithProfile for Runner<'_, '_, '_, C> {
                        fn run<P: super::InteractionProfile>(&mut self) {
                            if let Some(mut context) = self.1.for_profile::<C, P>(self.0) {
                                self.0
                                    .load_bindings_for_profile::<P>(&self.2.bindings, &mut context);
                            } else {
                                warn!(
                                    "Couldn't create context for profile {}?",
                                    std::any::type_name::<P>()
                                );
                            }
                        }
                    }
                }
            }

            while let Some(b) = it.next_if(|b| b.controller_type == controller_type) {
                info!("skipping bindings in {:?}", b.binding_url);
            }
        }
    }

    fn load_bindings_for_profile<P: InteractionProfile>(
        &self,
        bindings: &HashMap<String, bindings::ActionSetBinding>,
        context: &mut context::BindingsProfileLoadContext<'_>,
    ) {
        info!("loading bindings for {}", P::profile_path());

        let conv = super::profiles::InputToXrPath::new(&self.openxr.instance);
        let legacy_bindings = P::legacy_bindings(&conv);
        let skeletal_bindings = P::skeletal_input_bindings(&conv);
        let profile_path = self
            .openxr
            .instance
            .string_to_path(P::profile_path())
            .unwrap();
        let path_validator = |path| {
            <P::LegalPaths as LegalPathsT>::is_legal(path)
                .then_some(path)
                .or_else(|| P::translate_path(path))
        };

        for (action_set_name, bindings) in bindings.iter() {
            let Some(set) = context.get_action_set(action_set_name) else {
                warn!("Action set {action_set_name} missing.");
                continue;
            };

            let set = set.clone();

            if let Some(bindings) = &bindings.haptics {
                bindings::handle_haptic_bindings(&self.openxr.instance, context, bindings);
            }

            if let Some(bindings) = &bindings.poses {
                bindings::handle_pose_bindings(context, bindings);
            }

            if let Some(bindings) = &bindings.skeleton {
                bindings::handle_skeleton_bindings(context, bindings);
            }

            bindings::handle_sources(
                &path_validator,
                context,
                action_set_name,
                &set,
                &bindings.sources,
            );
        }

        let info_action_binding = *legacy_bindings
            .trigger_click
            .first()
            .unwrap_or_else(|| panic!("Missing trigger_click binding for {}", P::profile_path()));
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
                    Skeleton { .. } | Pose => unreachable!(),
                }
            })
            .chain(
                legacy_bindings
                    .extra
                    .grip_pose
                    .into_iter()
                    .map(|path| xr::Binding::new(context.grip_action, path)),
            )
            .chain(std::iter::once(xr::Binding::new(
                context.info_action,
                info_action_binding,
            )))
            .chain(
                legacy_bindings
                    .haptic
                    .into_iter()
                    .map(|path| xr::Binding::new(context.haptic_action, path)),
            )
            .chain(skeletal_bindings.binding_iter(&context.skeletal_input.actions))
            .collect();

        self.openxr
            .instance
            .suggest_interaction_profile_bindings(profile_path, &bindings)
            .unwrap_or_else(|e| {
                panic!(
                    "Couldn't suggest profile bindings for {}: {e}",
                    std::any::type_name::<P>()
                )
            });
        debug!(
            "suggested {} bindings for {}",
            bindings.len(),
            P::profile_path()
        );
    }
}
