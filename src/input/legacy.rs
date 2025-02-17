use crate::openxr_data::{Hand, OpenXrData, SessionData};
use log::{debug, trace};
use openxr as xr;
use std::sync::OnceLock;

macro_rules! legacy_actions_and_bindings {
    ($($field:ident: $ty:ty),+$(,)?) => {
        pub(super) struct LegacyActions {
            $(pub $field: $ty),+
        }
        pub(super) struct LegacyBindings {
            $(pub $field: Vec<xr::Path>),+
        }
        impl LegacyBindings {
            pub fn binding_iter(self, actions: &LegacyActions) -> impl Iterator<Item = xr::Binding<'_>> {
                std::iter::empty()
                $(
                    .chain(
                        self.$field.into_iter().map(|binding| xr::Binding::new(&actions.$field, binding))
                    )
                )+
            }
        }
    }
}

legacy_actions_and_bindings! {
    grip_pose: xr::Action<xr::Posef>,
    aim_pose: xr::Action<xr::Posef>,
    app_menu: xr::Action<bool>,
    trigger_click: xr::Action<bool>,
    trigger: xr::Action<f32>,
    squeeze: xr::Action<f32>,
}

pub(super) struct LegacyActionData {
    pub set: xr::ActionSet,
    pub left_spaces: HandSpaces,
    pub right_spaces: HandSpaces,
    pub actions: LegacyActions,
}

impl LegacyActionData {
    pub fn new<'a>(instance: &'a xr::Instance, left_hand: xr::Path, right_hand: xr::Path) -> Self {
        debug!("creating legacy actions");
        let leftright = [left_hand, right_hand];
        let set = instance
            .create_action_set("xrizer-legacy-set", "XRizer Legacy Set", 0)
            .unwrap();
        let grip_pose = set
            .create_action("grip-pose", "Grip Pose", &leftright)
            .unwrap();
        let aim_pose = set
            .create_action("aim-pose", "Aim Pose", &leftright)
            .unwrap();
        let trigger_click = set
            .create_action("trigger-click", "Trigger Click", &leftright)
            .unwrap();
        let trigger = set.create_action("trigger", "Trigger", &leftright).unwrap();
        let squeeze = set.create_action("squeeze", "Squeeze", &leftright).unwrap();
        let app_menu = set
            .create_action("app-menu", "Application Menu", &leftright)
            .unwrap();

        let create_spaces = |hand| {
            let hand_path = match hand {
                Hand::Left => left_hand,
                Hand::Right => right_hand,
            };
            HandSpaces {
                hand,
                hand_path,
                raw: OnceLock::new(),
            }
        };

        let left_spaces = create_spaces(Hand::Left);
        let right_spaces = create_spaces(Hand::Right);

        Self {
            set,
            left_spaces,
            right_spaces,
            actions: LegacyActions {
                grip_pose,
                aim_pose,
                app_menu,
                trigger_click,
                trigger,
                squeeze,
            },
        }
    }
}

pub(super) struct HandSpaces {
    hand: Hand,
    hand_path: xr::Path,

    /// Based on the controller jsons in SteamVR, the "raw" pose
    /// (which seems to be equivalent to the pose returned by WaitGetPoses)
    /// is actually the grip pose, but in the same position as the aim pose.
    /// Using this pose instead of the grip fixes strange controller rotation in
    /// I Expect You To Die 3.
    /// This is stored as a space so we can locate hand joints relative to it for skeletal data.
    raw: OnceLock<xr::Space>,
}

impl HandSpaces {
    pub fn try_get_or_init_raw(
        &self,
        xr_data: &OpenXrData<impl crate::openxr_data::Compositor>,
        session_data: &SessionData,
        actions: &LegacyActions,
    ) -> Option<&xr::Space> {
        if let Some(raw) = self.raw.get() {
            return Some(raw);
        }

        let hand_profile = match self.hand {
            Hand::Right => &xr_data.right_hand.profile,
            Hand::Left => &xr_data.left_hand.profile,
        };

        let hand_profile = hand_profile.lock().unwrap();
        let Some(profile) = hand_profile.as_ref() else {
            trace!("no hand profile, no raw space will be created");
            return None;
        };

        self.raw
            .set(
                actions
                    .grip_pose
                    .create_space(
                        &session_data.session,
                        self.hand_path,
                        profile.offset_grip_pose(self.hand),
                    )
                    .unwrap(),
            )
            .unwrap_or_else(|_| unreachable!());

        self.raw.get()
    }
}
