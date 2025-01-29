#[path = "skeletal_generated.rs"]
mod gen;

use super::Input;
use crate::input::devices::tracked_device::TrackedDeviceType;
use crate::openxr_data::{self, OpenXrData, SessionData};
use glam::{Affine3A, Quat, Vec3};
use openvr as vr;
use openxr as xr;
use paste::paste;
use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, PI};
use HandSkeletonBone::*;

impl<C: openxr_data::Compositor> Input<C> {
    /// Returns false if hand tracking data couldn't be generated for some reason.
    pub(super) fn get_bones_from_hand_tracking(
        &self,
        xr_data: &OpenXrData<C>,
        session_data: &SessionData,
        space: vr::EVRSkeletalTransformSpace,
        hand_tracker: &xr::HandTracker,
        hand: TrackedDeviceType,
        transforms: &mut [vr::VRBoneTransform_t],
    ) {
        use HandSkeletonBone::*;

        let legacy = session_data.input_data.legacy_actions.get().unwrap();
        let display_time = self.openxr.display_time.get();
        let Some(raw) = match hand {
            TrackedDeviceType::LeftHand => &legacy.left_spaces,
            TrackedDeviceType::RightHand => &legacy.right_spaces,
            _ => unreachable!(),
        }
        .try_get_or_init_raw(xr_data, session_data, &legacy.actions, display_time) else {
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

            if hand == TrackedDeviceType::LeftHand {
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
            TrackedDeviceType::LeftHand => {
                Affine3A::from_quat(Quat::from_euler(glam::EulerRot::YZXEx, FRAC_PI_2, PI, 0.0))
            }
            TrackedDeviceType::RightHand => Affine3A::from_rotation_y(-FRAC_PI_2),
            _ => unreachable!(),
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
        hand: TrackedDeviceType,
        transforms: &mut [vr::VRBoneTransform_t],
    ) {
        let path = match hand {
            TrackedDeviceType::LeftHand => self.openxr.devices.get_controller(TrackedDeviceType::LeftHand).unwrap().subaction_path,
            TrackedDeviceType::RightHand => self.openxr.devices.get_controller(TrackedDeviceType::RightHand).unwrap().subaction_path,
            _ => unreachable!(),
        };
        let legacy = session_data.input_data.legacy_actions.get().unwrap();
        let actions = &legacy.actions;
        let trigger_state = actions.trigger.state(&session_data.session, path).unwrap();
        let squeeze_state = actions.squeeze.state(&session_data.session, path).unwrap();
        let (bind, squeeze, open) = match hand {
            TrackedDeviceType::LeftHand => (
                &gen::left_hand::BINDPOSE,
                &gen::left_hand::SQUEEZE,
                &gen::left_hand::OPENHAND,
            ),
            TrackedDeviceType::RightHand => (
                &gen::right_hand::BINDPOSE,
                &gen::right_hand::SQUEEZE,
                &gen::right_hand::OPENHAND,
            ),
            _ => unreachable!(),
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
                let (closed_pos, closed_rot) = bone_transform_to_glam(squeeze[idx]);

                let pos = start_pos.lerp(closed_pos, state);
                let rot = start_rot.slerp(closed_rot, state);

                (pos, rot)
            }
        });

        // If squeezing and not pressing the trigger, index finger should be pointed straight
        let index_start = if squeeze_state.current_state > 0.0 && trigger_state.current_state == 0.0
        {
            open
        } else {
            bind
        };
        let index_it = (IndexFinger0 as usize..=IndexFinger4 as usize)
            .map(bone_transform_map(index_start, trigger_state.current_state));

        let rest_map = bone_transform_map(bind, squeeze_state.current_state);
        let pre_it = (Root as usize..=Thumb3 as usize).map(rest_map);
        let rest_it = (MiddleFinger0 as usize..Count as usize).map(rest_map);

        // If we need to convert our iterator to model space, it will become a different type -
        // this is the only reason for this enum existing
        enum TransformedIt<T: PoseIterator, U: PoseIterator> {
            Parent(T),
            Model(U),
        }
        let mut full_it = TransformedIt::Parent(pre_it.chain(index_it).chain(rest_it));

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

        // TODO: This is an arbitrary transform, and only appears to work as expected in parent space.
        transforms[Root as usize].position = Vec3::new(0.0, 0.0, -0.15).into();

        *self.skeletal_tracking_level.write().unwrap() = vr::EVRSkeletalTrackingLevel::Estimated;
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
