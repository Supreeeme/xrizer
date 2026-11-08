//! Early interaction profile discovery.
//!
//! Some games (Fallout 4 VR, Skyrim VR, Serious Sam 3 VR) read the HMD's identity properties
//! once, right after init, to decide on a control scheme - and never again. Our HMD identity is
//! derived from the controller interaction profile, but runtimes only resolve that on
//! xrSyncActions in a focused session, which normally isn't until the game submits its first
//! frame. So before the real session is created, spin up a throwaway session, attach a dummy
//! action set bound to every supported profile, submit empty frames until the session is
//! focused, sync once and remember what got bound.

use super::profiles::{self, InteractionProfile, RunWithProfile};
use crate::openxr_data::{Hand, SessionData};
use log::{info, warn};
use openvr as vr;
use openxr as xr;
use std::time::{Duration, Instant};

/// The interaction profiles bound to each hand at init.
#[derive(Default)]
pub struct ProbedProfiles {
    pub left: Option<String>,
    pub right: Option<String>,
    /// The suggested bindings live on the instance and reference this action. Keep it alive for
    /// as long as the instance rather than finding out what runtimes do with bindings to a
    /// destroyed action.
    _probe_set: Option<(xr::ActionSet, xr::Action<xr::Posef>)>,
}

impl ProbedProfiles {
    pub fn get(&self, hand: Hand) -> Option<&str> {
        match hand {
            Hand::Left => self.left.as_deref(),
            Hand::Right => self.right.as_deref(),
        }
    }
}

const FOCUS_TIMEOUT: Duration = Duration::from_secs(1);
const EXIT_TIMEOUT: Duration = Duration::from_secs(1);

pub fn probe_interaction_profiles(
    instance: &xr::Instance,
    system_id: xr::SystemId,
    enabled_extensions: &xr::ExtensionSet,
) -> ProbedProfiles {
    match probe(instance, system_id, enabled_extensions) {
        Ok(probed) => probed,
        Err(e) => {
            warn!("Failed to probe interaction profiles: {e}");
            Default::default()
        }
    }
}

fn probe(
    instance: &xr::Instance,
    system_id: xr::SystemId,
    enabled_extensions: &xr::ExtensionSet,
) -> xr::Result<ProbedProfiles> {
    let left = instance.string_to_path("/user/hand/left")?;
    let right = instance.string_to_path("/user/hand/right")?;

    // Every interaction profile supports the grip pose, so a single pose action is enough to get
    // any of them bound.
    let set = instance.create_action_set("xrizer_probe", "XRizer profile probe", 0)?;
    let action = set.create_action::<xr::Posef>("grip", "Grip", &[left, right])?;

    struct Suggester<'a> {
        instance: &'a xr::Instance,
        enabled_extensions: &'a xr::ExtensionSet,
        action: &'a xr::Action<xr::Posef>,
        result: xr::Result<()>,
    }
    impl RunWithProfile for Suggester<'_> {
        fn run<P: InteractionProfile>(&mut self) {
            if !P::has_required_extensions(self.enabled_extensions) {
                return;
            }
            self.result = (|| {
                let profile = self.instance.string_to_path(P::profile_path())?;
                let bindings = [
                    self.instance
                        .string_to_path("/user/hand/left/input/grip/pose")?,
                    self.instance
                        .string_to_path("/user/hand/right/input/grip/pose")?,
                ]
                .map(|path| xr::Binding::new(self.action, path));
                self.instance
                    .suggest_interaction_profile_bindings(profile, &bindings)
            })();
        }

        fn keep_running(&self) -> bool {
            self.result.is_ok()
        }
    }
    let mut suggester = Suggester {
        instance,
        enabled_extensions,
        action: &action,
        result: Ok(()),
    };
    profiles::run_for_all_profiles(&mut suggester);
    suggester.result?;

    let (session_data, mut waiter, stream) = SessionData::new(
        instance,
        system_id,
        vr::ETrackingUniverseOrigin::Standing,
        None,
    )
    .map_err(|e| {
        warn!("Failed to create probe session: {e:?}");
        xr::sys::Result::ERROR_RUNTIME_FAILURE
    })?;
    // Sessions without create info are always created with Vulkan.
    let mut stream: xr::FrameStream<xr::Vulkan> =
        stream.try_into().unwrap_or_else(|_| unreachable!());
    let session = &session_data.session;

    session.attach_action_sets(&[&set])?;

    // Profiles are only resolved on xrSyncActions in a focused session, and the runtime won't
    // focus us until we submit frames.
    let mut state = xr::SessionState::READY;
    let mut events = EventPoller::new(instance);
    let start = Instant::now();
    while state != xr::SessionState::FOCUSED && start.elapsed() < FOCUS_TIMEOUT {
        let frame = waiter.wait()?;
        stream.begin()?;
        stream.end(
            frame.predicted_display_time,
            xr::EnvironmentBlendMode::OPAQUE,
            &[],
        )?;
        state = events.poll(state);
    }

    let mut probed = ProbedProfiles::default();
    if state == xr::SessionState::FOCUSED {
        session.sync_actions(&[xr::ActiveActionSet::new(&set)])?;
        state = events.poll(state);
        let profile_for = |subaction| {
            session
                .current_interaction_profile(subaction)
                .ok()
                .filter(|p| *p != xr::Path::NULL)
                .and_then(|p| instance.path_to_string(p).ok())
        };
        probed.left = profile_for(left);
        probed.right = profile_for(right);
        info!(
            "Probed interaction profiles: left: {}, right: {}",
            probed.left.as_deref().unwrap_or("<null>"),
            probed.right.as_deref().unwrap_or("<null>")
        );
    } else {
        warn!("Probe session never became focused, interaction profiles unknown until later");
    }

    // Tear the session down cleanly before the real one gets created.
    session.request_exit()?;
    state = events.wait_for(state, xr::SessionState::STOPPING, EXIT_TIMEOUT);
    session.end()?;
    events.wait_for(state, xr::SessionState::EXITING, EXIT_TIMEOUT);
    // The frame stream and waiter hold a reference to the session, which has to be gone before
    // the temporary graphics setup inside SessionData is destroyed.
    drop(stream);
    drop(waiter);
    drop(session_data);

    probed._probe_set = Some((set, action));
    Ok(probed)
}

struct EventPoller<'a> {
    instance: &'a xr::Instance,
    buf: xr::EventDataBuffer,
}

impl<'a> EventPoller<'a> {
    fn new(instance: &'a xr::Instance) -> Self {
        Self {
            instance,
            buf: xr::EventDataBuffer::new(),
        }
    }

    /// Drains pending events, returning the latest session state.
    fn poll(&mut self, mut state: xr::SessionState) -> xr::SessionState {
        while let Ok(Some(event)) = self.instance.poll_event(&mut self.buf) {
            if let xr::Event::SessionStateChanged(event) = event {
                state = event.state();
            }
        }
        state
    }

    fn wait_for(
        &mut self,
        mut state: xr::SessionState,
        target: xr::SessionState,
        timeout: Duration,
    ) -> xr::SessionState {
        let start = Instant::now();
        while state != target && start.elapsed() < timeout {
            state = self.poll(state);
        }
        state
    }
}
