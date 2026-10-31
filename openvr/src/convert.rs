use super::*;
use glam::{Affine3A, Mat3, Mat4, Quat, Vec3};
use openxr as xr;

pub fn space_relation_to_openvr_pose(
    location: xr::SpaceLocation,
    velocity: xr::SpaceVelocity,
) -> TrackedDevicePose_t {
    if !location.location_flags.contains(
        xr::SpaceLocationFlags::POSITION_VALID | xr::SpaceLocationFlags::ORIENTATION_VALID,
    ) {
        return TrackedDevicePose_t {
            bPoseIsValid: false,
            bDeviceIsConnected: false,
            mDeviceToAbsoluteTracking: Default::default(),
            vVelocity: Default::default(),
            vAngularVelocity: Default::default(),
            eTrackingResult: ETrackingResult::Running_OutOfRange,
        };
    }

    let location = HmdMatrix34_t::from(location.pose);
    let linear_velo = velocity
        .velocity_flags
        .contains(xr::SpaceVelocityFlags::LINEAR_VALID)
        .then(|| velocity.linear_velocity.into());
    let angular_velo = velocity
        .velocity_flags
        .contains(xr::SpaceVelocityFlags::ANGULAR_VALID)
        .then(|| velocity.angular_velocity.into());

    TrackedDevicePose_t {
        mDeviceToAbsoluteTracking: location,
        vVelocity: linear_velo.unwrap_or_default(),
        vAngularVelocity: angular_velo.unwrap_or_default(),
        eTrackingResult: ETrackingResult::Running_OK,
        bPoseIsValid: true,
        bDeviceIsConnected: true,
    }
}

impl From<Mat4> for HmdMatrix44_t {
    fn from(value: Mat4) -> Self {
        // OpenVR wants data in row major order, so we transpose it
        Self {
            m: value.transpose().to_cols_array_2d(),
        }
    }
}

impl From<xr::Vector3f> for HmdVector3_t {
    fn from(value: xr::Vector3f) -> Self {
        Self {
            v: [value.x, value.y, value.z],
        }
    }
}

impl From<Vec3> for HmdVector3_t {
    fn from(value: Vec3) -> Self {
        Self {
            v: value.to_array(),
        }
    }
}

impl From<Vec3> for HmdVector4_t {
    fn from(value: Vec3) -> Self {
        let mut v = [0.0; 4];
        v[..3].copy_from_slice(&value.to_array());
        v[3] = 1.0;
        Self { v }
    }
}

impl From<Quat> for HmdQuaternionf_t {
    fn from(value: Quat) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
            w: value.w,
        }
    }
}

// https://github.com/ValveSoftware/openvr/wiki/Matrix-Usage-Example
impl From<xr::Posef> for HmdMatrix34_t {
    fn from(pose: xr::Posef) -> Self {
        // openvr matrices are row major, glam matrices are column major

        let rot = Mat3::from_quat(Quat::from_xyzw(
            pose.orientation.x,
            pose.orientation.y,
            pose.orientation.z,
            // Valve sample code doesn't hande correctly matrix->quaternion conversion when w is
            // close to 0:
            // https://github.com/ValveSoftware/openvr/blob/0924064316de3effbcd1acf1e309182a2deb1c05/samples/unity_teleport_sample/Assets/SteamVR/Scripts/SteamVR_Utils.cs#L142
            if pose.orientation.w.abs() > 1e-4 {
                pose.orientation.w
            } else {
                1e-4
            },
        ))
        .transpose();

        let gen_array = |translation, rot_axis: Vec3| {
            std::array::from_fn(|i| if i == 3 { translation } else { rot_axis[i] })
        };

        Self {
            m: [
                gen_array(pose.position.x, rot.x_axis),
                gen_array(pose.position.y, rot.y_axis),
                gen_array(pose.position.z, rot.z_axis),
            ],
        }
    }
}

impl From<HmdMatrix34_t> for xr::Posef {
    fn from(mat: HmdMatrix34_t) -> Self {
        let mat = mat.m;
        let pos = xr::Vector3f {
            x: mat[0][3],
            y: mat[1][3],
            z: mat[2][3],
        };
        let rot = Quat::from_mat3(
            &Mat3::from_cols(
                Vec3::from_slice(&mat[0][..3]),
                Vec3::from_slice(&mat[1][..3]),
                Vec3::from_slice(&mat[2][..3]),
            )
            .transpose(),
        );
        xr::Posef {
            position: pos,
            orientation: xr::Quaternionf {
                x: rot.x,
                y: rot.y,
                z: rot.z,
                w: rot.w,
            },
        }
    }
}

impl From<Affine3A> for VRBoneTransform_t {
    fn from(value: Affine3A) -> Self {
        let (_, rot, pos) = value.to_scale_rotation_translation();
        Self {
            position: pos.into(),
            orientation: rot.into(),
        }
    }
}
