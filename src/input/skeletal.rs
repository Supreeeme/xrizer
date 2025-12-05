#[path = "skeletal_generated.rs"]
mod gen;

use super::Input;
use crate::openxr_data::{self, Hand, SessionData};
use glam::{Affine3A, Quat, Vec3};
use log::debug;
use openvr as vr;
use openxr::{self as xr};
use paste::paste;
use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Instant;
use HandSkeletonBone::*;

impl<C: openxr_data::Compositor> Input<C> {
    /// Returns false if hand tracking data couldn't be generated for some reason.
    pub(super) fn get_bones_from_hand_tracking(
        &self,
        session_data: &SessionData,
        space: vr::EVRSkeletalTransformSpace,
        hand_tracker: &xr::HandTracker,
        hand: Hand,
        transforms: &mut [vr::VRBoneTransform_t],
    ) {
        use HandSkeletonBone::*;

        let pose_data = session_data.input_data.pose_data.get().unwrap();
        let display_time = self.openxr.display_time.get();
        let devices = session_data.input_data.devices.read().unwrap();

        let Some(controller) = devices.get_controller(hand) else {
            self.get_estimated_bones(session_data, space, hand, transforms);
            return;
        };

        let Some(raw) = match hand {
            Hand::Left => &pose_data.left_space,
            Hand::Right => &pose_data.right_space,
        }
        .try_get_or_init_raw(&controller.interaction_profile, session_data, pose_data) else {
            self.get_estimated_bones(session_data, space, hand, transforms);
            return;
        };

        let Some(joints) = raw.locate_hand_joints(hand_tracker, display_time).unwrap() else {
            self.get_estimated_bones(session_data, space, hand, transforms);
            return;
        };

        let mut joints: Box<[_]> = joints
            .into_iter()
            .map(|joint_location| {
                let position = joint_location.pose.position;
                let orientation = joint_location.pose.orientation;
                Affine3A::from_rotation_translation(
                    Quat::from_xyzw(orientation.x, orientation.y, orientation.z, orientation.w),
                    Vec3::from_array([position.x, position.y, position.z]),
                )
            })
            .collect();

        let xr_joint_to_vr_bone = |joint: &Affine3A, bone: &mut vr::VRBoneTransform_t| {
            let (_, mut rot, mut pos) = joint.to_scale_rotation_translation();

            // The following transform converts our joints to the OpenVR coordinate system.
            // I have no idea what this transform is or how it works, but both Monado and ALVR
            // appear to have it, and it seems to work, so here it is.
            // https://github.com/alvr-org/ALVR/blob/cf52f875c2720b2c17ef490cfbec4c07ee5f41aa/alvr/server_openvr/src/tracking.rs#L82
            // https://gitlab.freedesktop.org/monado/monado/-/blob/d7089f182b0514e13554e99512d63e69c30523c5/src/xrt/state_trackers/steamvr_drv/ovrd_driver.cpp#L239
            std::mem::swap(&mut pos.x, &mut pos.z);
            pos.z = -pos.z;

            let r = &mut *rot;
            std::mem::swap(&mut r.x, &mut r.z);
            rot.z = -rot.z;

            if hand == Hand::Left {
                pos.x = -pos.x;
                pos.y = -pos.y;
                rot.x = -rot.x;
                rot.y = -rot.y;
            }

            *bone = vr::VRBoneTransform_t {
                position: pos.into(),
                orientation: rot.into(),
            };
        };

        for (aux, joint) in AUX_BONES.iter().copied() {
            xr_joint_to_vr_bone(&joints[joint], &mut transforms[aux as usize]);
        }

        // The wrists appear to have to some sort of strange orientation compared
        // to the other joints - this rotation fixes it up
        joints[xr::HandJoint::WRIST] *= Affine3A::from_quat(Quat::from_euler(
            glam::EulerRot::YZXEx,
            -FRAC_PI_2,
            FRAC_PI_2,
            0.0,
        ));

        // OpenXR reports all our bones in "model" space (basically), so we need to
        // convert everything into parent space.
        // For each finger, the metacarpal is a child of the wrist, and then each consecutive
        // joint in that finger is a parent->child relationship.
        // https://github.com/ValveSoftware/openvr/wiki/Hand-Skeleton#bone-structure
        let parent_id = RefCell::new(xr::HandJoint::WRIST);
        let mut parented_joints = joints.clone();
        let mut localize = |joint: xr::HandJoint| {
            let mut parent_id = parent_id.borrow_mut();
            parented_joints[joint] = joints[*parent_id].inverse() * parented_joints[joint];
            *parent_id = joint;
        };

        for joint_list in JOINTS_TO_BONES.iter().copied().skip(1) {
            for (joint, _) in joint_list.iter().copied() {
                localize(joint);
            }
            *parent_id.borrow_mut() = xr::HandJoint::WRIST;
        }

        joints = parented_joints;

        // The root bone is supposed to not transform
        // Changing the root bone appears to change the offset of the hand, but causes issues in
        // games such as The Lab, it also won't work in model space because the transform won't get
        // applied to the wrist in the conversion method.
        transforms[Root as usize] = Affine3A::IDENTITY.into();

        // Currently as is, the hands will point down
        // This rotation corrects them so they are pointing the correct direction
        // Note that it is hand specific.
        joints[xr::HandJoint::WRIST] *= match hand {
            Hand::Left => {
                Affine3A::from_quat(Quat::from_euler(glam::EulerRot::YZXEx, FRAC_PI_2, PI, 0.0))
            }
            Hand::Right => Affine3A::from_rotation_y(-FRAC_PI_2),
        };
        transforms[Wrist as usize] = joints[xr::HandJoint::WRIST].into();

        for (joint, bone) in JOINTS_TO_BONES[1..]
            .iter()
            .flat_map(|list| list.iter())
            .copied()
        {
            xr_joint_to_vr_bone(&joints[joint], &mut transforms[bone as usize])
        }

        // Convert back to model space if needed
        // it is unnecessary to convert back and forth, but it works and it's easy
        if space == vr::EVRSkeletalTransformSpace::Model {
            let bone_data: Vec<(Vec3, Quat)> = parent_to_model_space_bone_data(
                transforms.iter().map(|t| bone_transform_to_glam(*t)),
            )
            .collect();

            for (transform, (pos, rot)) in transforms.iter_mut().zip(bone_data) {
                transform.position = pos.into();
                transform.orientation = rot.into();
            }
        }

        *self.skeletal_tracking_level.write().unwrap() = vr::EVRSkeletalTrackingLevel::Full;
    }

    pub(super) fn get_estimated_bones(
        &self,
        session_data: &SessionData,
        space: vr::EVRSkeletalTransformSpace,
        hand: Hand,
        transforms: &mut [vr::VRBoneTransform_t],
    ) {
        let finger_state = self.get_finger_state(session_data, hand);
        let (open, fist) = match hand {
            Hand::Left => (&gen::left_hand::OPENHAND, &gen::left_hand::FIST),
            Hand::Right => (&gen::right_hand::OPENHAND, &gen::right_hand::FIST),
        };

        const fn constrain<'a, F, G>(f: F) -> F
        where
            F: Fn(&'a [vr::VRBoneTransform_t], f32) -> G,
            G: Fn(usize) -> (Vec3, Quat) + 'a,
        {
            f
        }
        let bone_transform_map = constrain(|start_data: &[vr::VRBoneTransform_t], state| {
            move |idx| {
                let (start_pos, start_rot) = bone_transform_to_glam(start_data[idx]);
                let (closed_pos, closed_rot) = bone_transform_to_glam(fist[idx]);

                let pos = start_pos.lerp(closed_pos, state);
                let rot = start_rot.slerp(closed_rot, state);

                (pos, rot)
            }
        });

        let bone_it = (0..HandSkeletonBone::Count as usize).map(|idx| {
            let bone = unsafe { std::mem::transmute::<usize, HandSkeletonBone>(idx) };
            let curl_state = finger_state.get_bone_state(bone);

            let map_fn = bone_transform_map(open, curl_state);
            map_fn(idx)
        });

        finalize_transforms(bone_it, space, transforms);
        *self.skeletal_tracking_level.write().unwrap() = vr::EVRSkeletalTrackingLevel::Estimated;
    }

    fn get_finger_state(&self, session_data: &SessionData, hand: Hand) -> FingerState {
        // Determines the speed at which fingers follow the input states
        // This value seems to feel right for both analog inputs and binary ones (like vive wands)
        const FINGER_SMOOTHING_SPEED: f32 = 24.0;

        let actions = &session_data
            .input_data
            .estimated_skeleton_actions
            .get()
            .unwrap()
            .actions;

        let subaction = self.get_subaction_path(hand);

        let thumb_touch = actions
            .thumb_touch
            .state(&session_data.session, subaction)
            .unwrap()
            .current_state;
        let index_touch = actions
            .index_touch
            .state(&session_data.session, subaction)
            .unwrap()
            .current_state;
        let index_curl = actions
            .index_curl
            .state(&session_data.session, subaction)
            .unwrap()
            .current_state;
        let rest_curl = actions
            .rest_curl
            .state(&session_data.session, subaction)
            .unwrap()
            .current_state;

        let index = index_curl.max(
            // Curl the index finger slightly on touch input
            if index_touch || index_curl > 0.0 {
                0.3
            } else {
                0.0
            },
        );

        let mut state = self.estimated_finger_state[hand as usize - 1]
            .lock()
            .unwrap();

        let current_time = std::time::Instant::now();

        let target = FingerState {
            index,
            // Make other fingers curl with the index slightly to mimic how real human hands work
            middle: rest_curl.max(index / 2.0),
            ring: rest_curl.max(index / 4.0),
            pinky: rest_curl.max(index / 6.0),
            thumb: if thumb_touch { 1.0 } else { 0.0 },
            time: current_time,
        };

        let elapsed_time = current_time.duration_since(state.time).as_secs_f32();
        let t = (elapsed_time * FINGER_SMOOTHING_SPEED).min(1.0);

        *state = state.lerp(&target, t);

        *state
    }

    pub(super) fn get_reference_transforms(
        &self,
        hand: Hand,
        space: vr::EVRSkeletalTransformSpace,
        pose: vr::EVRSkeletalReferencePose,
        transforms: &mut [vr::VRBoneTransform_t],
    ) {
        let skeleton = match hand {
            Hand::Left => match pose {
                vr::EVRSkeletalReferencePose::BindPose => &gen::left_hand::BINDPOSE,
                vr::EVRSkeletalReferencePose::OpenHand => &gen::left_hand::OPENHAND,
                vr::EVRSkeletalReferencePose::Fist => &gen::left_hand::FIST,
                vr::EVRSkeletalReferencePose::GripLimit => &gen::left_hand::GRIPLIMIT,
            },
            Hand::Right => match pose {
                vr::EVRSkeletalReferencePose::BindPose => &gen::right_hand::BINDPOSE,
                vr::EVRSkeletalReferencePose::OpenHand => &gen::right_hand::OPENHAND,
                vr::EVRSkeletalReferencePose::Fist => &gen::right_hand::FIST,
                vr::EVRSkeletalReferencePose::GripLimit => &gen::right_hand::GRIPLIMIT,
            },
        };

        let bone_it =
            (0..HandSkeletonBone::Count as usize).map(|idx| bone_transform_to_glam(skeleton[idx]));

        finalize_transforms(bone_it, space, transforms);
    }
}

/// trait alias
trait PoseIterator: Iterator<Item = (Vec3, Quat)> {}
impl<T: Iterator<Item = (Vec3, Quat)>> PoseIterator for T {}

fn parent_to_model_space_bone_data(it: impl PoseIterator) -> impl PoseIterator {
    struct State {
        /// Index for current finger in JOINTS_TO_BONES
        finger_slice_idx: usize,
        wrist_transform: Affine3A,
        /// None for metacarpal joints, in which case we use the wrist transform
        parent_transform: Option<Affine3A>,
    }

    it.enumerate().scan(
        State {
            finger_slice_idx: 1,
            wrist_transform: Affine3A::ZERO,
            parent_transform: None,
        },
        |state, (idx, (pos, rot))| {
            if idx == Wrist as usize {
                state.wrist_transform = Affine3A::from_rotation_translation(rot, pos);
            }
            if idx <= Wrist as usize || idx >= AuxThumb as usize {
                return Some((pos, rot));
            }

            let mut mat = Affine3A::from_rotation_translation(rot, pos);
            if let Some(parent) = state.parent_transform.take() {
                mat = parent * mat;
            } else {
                mat = state.wrist_transform * mat;
            }

            if idx == JOINTS_TO_BONES[state.finger_slice_idx].last().unwrap().1 as usize {
                state.finger_slice_idx += 1;
                state.parent_transform = None;
            } else {
                state.parent_transform = Some(mat);
            }

            let (_, rot, pos) = mat.to_scale_rotation_translation();
            Some((pos, rot))
        },
    )
}

fn bone_transform_to_glam(transform: vr::VRBoneTransform_t) -> (Vec3, Quat) {
    let rot = transform.orientation;
    (
        Vec3::from_slice(&transform.position.v[..3]),
        Quat::from_xyzw(rot.x, rot.y, rot.z, rot.w),
    )
}

fn finalize_transforms(
    bone_iterator: impl PoseIterator,
    space: vr::EVRSkeletalTransformSpace,
    transforms: &mut [vr::VRBoneTransform_t],
) {
    // If we need to convert our iterator to model space, it will become a different type -
    // this is the only reason for this enum existing
    enum TransformedIt<T: PoseIterator, U: PoseIterator> {
        Parent(T),
        Model(U),
    }
    let mut full_it = TransformedIt::Parent(bone_iterator);

    if space == vr::EVRSkeletalTransformSpace::Model {
        let TransformedIt::Parent(it) = full_it else {
            unreachable!();
        };
        full_it = TransformedIt::Model(parent_to_model_space_bone_data(it));
    }

    fn convert(it: impl PoseIterator, transforms: &mut [vr::VRBoneTransform_t]) {
        for ((pos, rot), transform) in it.zip(transforms) {
            *transform = vr::VRBoneTransform_t {
                position: pos.into(),
                orientation: rot.into(),
            };
        }
    }

    match full_it {
        TransformedIt::Parent(it) => convert(it, transforms),
        TransformedIt::Model(it) => convert(it, transforms),
    }
}

macro_rules! joints_for_finger {
    ($xr_finger:ident, $vr_finger:ident) => {
        paste! {[
            (xr::HandJoint::[<$xr_finger _METACARPAL>], [<$vr_finger Finger0>]),
            (xr::HandJoint::[<$xr_finger _PROXIMAL>], [<$vr_finger Finger1>]),
            (xr::HandJoint::[<$xr_finger _INTERMEDIATE>], [<$vr_finger Finger2>]),
            (xr::HandJoint::[<$xr_finger _DISTAL>], [<$vr_finger Finger3>]),
            (xr::HandJoint::[<$xr_finger _TIP>], [<$vr_finger Finger4>])
        ].as_slice()}
    };
}

static JOINTS_TO_BONES: &[&[(xr::HandJoint, HandSkeletonBone)]] = &[
    [(xr::HandJoint::WRIST, Wrist)].as_slice(),
    &[
        (xr::HandJoint::THUMB_METACARPAL, Thumb0),
        (xr::HandJoint::THUMB_PROXIMAL, Thumb1),
        (xr::HandJoint::THUMB_DISTAL, Thumb2),
        (xr::HandJoint::THUMB_TIP, Thumb3),
    ],
    joints_for_finger!(INDEX, Index),
    joints_for_finger!(MIDDLE, Middle),
    joints_for_finger!(RING, Ring),
    joints_for_finger!(LITTLE, Pinky),
];

static AUX_BONES: &[(HandSkeletonBone, xr::HandJoint)] = &[
    (AuxThumb, xr::HandJoint::THUMB_DISTAL),
    (AuxIndexFinger, xr::HandJoint::INDEX_DISTAL),
    (AuxMiddleFinger, xr::HandJoint::MIDDLE_DISTAL),
    (AuxRingFinger, xr::HandJoint::RING_DISTAL),
    (AuxPinkyFinger, xr::HandJoint::LITTLE_DISTAL),
];

#[derive(Copy, Clone)]
pub(super) struct FingerState {
    index: f32,
    middle: f32,
    ring: f32,
    pinky: f32,
    thumb: f32,
    time: Instant,
}

impl FingerState {
    pub fn new() -> FingerState {
        FingerState {
            index: 0.0,
            middle: 0.0,
            ring: 0.0,
            pinky: 0.0,
            thumb: 0.0,
            time: Instant::now(),
        }
    }

    fn lerp(&self, target: &Self, amount: f32) -> Self {
        Self {
            index: self.index + (target.index - self.index) * amount,
            middle: self.middle + (target.middle - self.middle) * amount,
            ring: self.ring + (target.ring - self.ring) * amount,
            pinky: self.pinky + (target.pinky - self.pinky) * amount,
            thumb: self.thumb + (target.thumb - self.thumb) * amount,
            time: target.time,
        }
    }

    fn get_bone_state(&self, bone: HandSkeletonBone) -> f32 {
        match bone {
            HandSkeletonBone::IndexFinger0
            | HandSkeletonBone::IndexFinger1
            | HandSkeletonBone::IndexFinger2
            | HandSkeletonBone::IndexFinger3
            | HandSkeletonBone::IndexFinger4
            | HandSkeletonBone::AuxIndexFinger => self.index,
            HandSkeletonBone::MiddleFinger0
            | HandSkeletonBone::MiddleFinger1
            | HandSkeletonBone::MiddleFinger2
            | HandSkeletonBone::MiddleFinger3
            | HandSkeletonBone::MiddleFinger4
            | HandSkeletonBone::AuxMiddleFinger => self.middle,
            HandSkeletonBone::RingFinger0
            | HandSkeletonBone::RingFinger1
            | HandSkeletonBone::RingFinger2
            | HandSkeletonBone::RingFinger3
            | HandSkeletonBone::RingFinger4
            | HandSkeletonBone::AuxRingFinger => self.ring,
            HandSkeletonBone::PinkyFinger0
            | HandSkeletonBone::PinkyFinger1
            | HandSkeletonBone::PinkyFinger2
            | HandSkeletonBone::PinkyFinger3
            | HandSkeletonBone::PinkyFinger4
            | HandSkeletonBone::AuxPinkyFinger => self.pinky,
            HandSkeletonBone::Thumb0
            | HandSkeletonBone::Thumb1
            | HandSkeletonBone::Thumb2
            | HandSkeletonBone::Thumb3
            | HandSkeletonBone::AuxThumb => self.thumb,
            _ => 0.0,
        }
    }
}

#[repr(usize)]
#[derive(Copy, Clone)]
pub(super) enum HandSkeletonBone {
    Root = 0,
    Wrist,
    Thumb0,
    Thumb1,
    Thumb2,
    Thumb3,
    IndexFinger0,
    IndexFinger1,
    IndexFinger2,
    IndexFinger3,
    IndexFinger4,
    MiddleFinger0,
    MiddleFinger1,
    MiddleFinger2,
    MiddleFinger3,
    MiddleFinger4,
    RingFinger0,
    RingFinger1,
    RingFinger2,
    RingFinger3,
    RingFinger4,
    PinkyFinger0,
    PinkyFinger1,
    PinkyFinger2,
    PinkyFinger3,
    PinkyFinger4,
    AuxThumb,
    AuxIndexFinger,
    AuxMiddleFinger,
    AuxRingFinger,
    AuxPinkyFinger,
    Count,
}

macro_rules! skeletal_input_actions {
    ($($field:ident: $ty:ty),+$(,)?) => {
        pub struct SkeletalInputActions {
            $(pub $field: xr::Action<$ty>),+
        }
        pub struct SkeletalInputBindings {
            $(pub $field: Vec<xr::Path>),+
        }
        impl SkeletalInputBindings {
            pub fn binding_iter(self, actions: &SkeletalInputActions) -> impl Iterator<Item = xr::Binding<'_>> {
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

skeletal_input_actions! {
    thumb_touch: bool,
    index_touch: bool,
    index_curl: f32,
    rest_curl: f32,
}

pub struct SkeletalInputActionData {
    pub set: xr::ActionSet,
    pub actions: SkeletalInputActions,
}

impl SkeletalInputActionData {
    pub fn new(instance: &xr::Instance, left_hand: xr::Path, right_hand: xr::Path) -> Self {
        debug!("creating skeletal input actions");
        let leftright = [left_hand, right_hand];
        let set = instance
            .create_action_set("xrizer-skeletal-input", "XRizer Skeletal Input", 0)
            .unwrap();
        let thumb_touch = set
            .create_action("thumb-touch", "Thumb Touch", &leftright)
            .unwrap();
        let index_touch = set
            .create_action("index-touch", "Index Touch", &leftright)
            .unwrap();
        let index_curl = set
            .create_action("index-curl", "Index Curl", &leftright)
            .unwrap();
        let rest_curl = set
            .create_action("rest-curl", "Rest Curl", &leftright)
            .unwrap();

        Self {
            set,
            actions: SkeletalInputActions {
                thumb_touch,
                index_touch,
                index_curl,
                rest_curl,
            },
        }
    }
}
