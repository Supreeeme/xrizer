use super::{
    InteractionProfile, MainAxisType, PathTranslation, ProfileProperties, Property,
    SkeletalInputBindings, StringToPath,
};
use crate::button_mask_from_ids;
use crate::input::legacy::{LegacyBindings, button_mask_from_id};
use crate::openxr_data::Hand;
use glam::Mat4;
use openvr::EVRButtonId;

pub struct VRLinkHand;

impl InteractionProfile for VRLinkHand {
    fn profile_path(&self) -> &'static str {
        "/interaction_profiles/ext/hand_interaction_ext"
    }
    fn has_required_extensions(&self, _: &openxr::ExtensionSet) -> bool {
        unimplemented!()
    }
    fn properties(&self) -> &'static ProfileProperties {
        static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
            model: Property::PerHand {
                left: c"VRLink Hand Tracker (Left Hand)",
                right: c"VRLink Hand Tracker (Right Hand)",
            },
            openvr_controller_type: c"svl_hand_interaction_augmented",
            render_model_name: Property::BothHands(c"{vrlink}/rendermodels/shuttlecock"),
            main_axis: MainAxisType::Thumbstick,
            registered_device_type: Property::PerHand {
                left: c"vrlink/VRLINKQ_HandTracker_Left",
                right: c"vrlink/VRLINKQ_HandTracker_Right",
            },
            serial_number: Property::PerHand {
                left: c"VRLINKQ_Hand_Left",
                right: c"VRLINKQ_Hand_Right",
            },
            tracking_system_name: c"vrlink",
            manufacturer_name: c"VRLink",
            legacy_buttons_mask: button_mask_from_ids!(
                EVRButtonId::System,
                EVRButtonId::ApplicationMenu,
                EVRButtonId::Grip,
                EVRButtonId::A,
                EVRButtonId::Axis0,
                EVRButtonId::Axis1,
                EVRButtonId::Axis2
            ),
        };
        &DEVICE_PROPERTIES
    }
    fn translate_map(&self) -> &'static [PathTranslation] {
        &[]
    }

    fn legal_paths(&self) -> Box<[String]> {
        [].into()
    }

    fn legacy_bindings(&self, _: &dyn StringToPath) -> LegacyBindings {
        unimplemented!();
    }

    fn skeletal_input_bindings(&self, _: &dyn StringToPath) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: Vec::new(),
            index_touch: Vec::new(),
            index_curl: Vec::new(),
            rest_curl: Vec::new(),
        }
    }

    fn offset_grip_pose(&self, _: Hand) -> Mat4 {
        Mat4::IDENTITY
    }
}
