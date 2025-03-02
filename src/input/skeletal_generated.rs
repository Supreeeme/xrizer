use super::HandSkeletonBone;
use openvr as vr;

// The bone data in this file is given in parent space. Using parent space
// allows for lerping between poses to always work correctly, and the bones can be
// easily transformed to model space when needed.
// This data was dumped from SteamVR using Quest controllers, see: https://codeberg.org/Orion_Moonclaw/OpenVR-Skeleton-Grabber
pub mod left_hand {
    use super::*;
    pub static BINDPOSE: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: -0.92028,
                z: 0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00018, 0.00029, 0.00025, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.27639,
                x: 0.54119,
                y: 0.18203,
                z: 0.77304,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00040, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.96917,
                x: 0.00006,
                y: -0.00137,
                z: 0.24638,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00033, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98817,
                x: 0.00010,
                y: 0.00140,
                z: 0.15334,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00030, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00002, 0.00021, 0.00015, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.55075,
                x: 0.53106,
                y: -0.35143,
                z: 0.53958,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00074, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.96898,
                x: 0.00162,
                y: -0.05289,
                z: 0.24140,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00043, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98277,
                x: -0.00009,
                y: 0.00504,
                z: 0.18476,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00028, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99707,
                x: 0.00003,
                y: -0.00117,
                z: 0.07646,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00023, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00002, 0.00007, 0.00016, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.53342,
                x: 0.56175,
                y: -0.41974,
                z: 0.47299,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00071, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.97339,
                x: -0.00000,
                y: -0.00019,
                z: 0.22916,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00043, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98753,
                x: 0.00009,
                y: -0.00369,
                z: 0.15740,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00033, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98996,
                x: -0.00011,
                y: 0.00413,
                z: 0.14128,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00026, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: -0.00000,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00001, -0.00007, 0.00016, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.51669,
                x: 0.55014,
                y: -0.49555,
                z: 0.42989,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00066, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.97456,
                x: -0.00090,
                y: -0.04096,
                z: 0.22037,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00040, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99100,
                x: -0.00007,
                y: 0.00253,
                z: 0.13383,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00028, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99079,
                x: 0.00020,
                y: -0.00426,
                z: 0.13535,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00022, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: -0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00002, -0.00019, 0.00015, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.48576,
                x: -0.51533,
                y: 0.61502,
                z: -0.34675,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00063, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99349,
                x: 0.00394,
                y: 0.02816,
                z: 0.11031,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00030, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99111,
                x: 0.00038,
                y: -0.01146,
                z: 0.13252,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00018, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99401,
                x: -0.00054,
                y: 0.01270,
                z: 0.10858,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00018, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: -0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00039, 0.00060, -0.00084, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.04504,
                x: 0.81959,
                y: -0.04861,
                z: -0.56911,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00018, 0.00037, -0.00149, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.59723,
                x: 0.70842,
                y: 0.20956,
                z: -0.31233,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00013, 0.00008, -0.00155, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.64706,
                x: 0.67740,
                y: 0.22114,
                z: -0.27117,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00018, -0.00023, -0.00142, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.72163,
                x: 0.59503,
                y: 0.23741,
                z: -0.26235,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00016, -0.00046, -0.00119, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.73903,
                x: 0.51142,
                y: 0.34900,
                z: -0.26548,
            },
        },
    ];
    pub static OPENHAND: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: -0.92028,
                z: 0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01208, 0.02807, 0.02505, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.46411,
                x: 0.56742,
                y: 0.27211,
                z: 0.62337,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04041, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99484,
                x: 0.08294,
                y: 0.01945,
                z: 0.05513,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03252, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.97479,
                x: -0.00321,
                y: 0.02187,
                z: -0.22201,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03046, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00063, 0.02687, 0.01500, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.64425,
                x: 0.42198,
                y: -0.47820,
                z: 0.42213,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.07420, -0.00500, 0.00023, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99533,
                x: 0.00701,
                y: -0.03912,
                z: 0.08795,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04393, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99789,
                x: 0.04581,
                y: 0.00214,
                z: -0.04594,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02870, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99965,
                x: 0.00185,
                y: -0.02278,
                z: -0.01341,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02282, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00218, 0.00712, 0.01632, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.54672,
                x: 0.54128,
                y: -0.44252,
                z: 0.46075,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.07095, 0.00078, 0.00100, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98029,
                x: -0.16726,
                y: -0.07896,
                z: 0.06937,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04311, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99795,
                x: 0.01849,
                y: 0.01319,
                z: 0.05989,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03327, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99739,
                x: -0.00333,
                y: -0.02822,
                z: -0.06632,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02589, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99919,
                x: 0.00000,
                y: 0.00000,
                z: 0.04013,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00051, -0.00654, 0.01635, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.51669,
                x: 0.55014,
                y: -0.49555,
                z: 0.42989,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.06588, 0.00179, 0.00069, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99042,
                x: -0.05870,
                y: -0.10182,
                z: 0.07249,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04070, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99954,
                x: -0.00224,
                y: 0.00000,
                z: 0.03008,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02875, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99910,
                x: -0.00072,
                y: -0.01269,
                z: 0.04042,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02243, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00248, -0.01898, 0.01521, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.52692,
                x: 0.52394,
                y: -0.58403,
                z: 0.32674,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.06288, 0.00284, 0.00033, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98661,
                x: -0.05962,
                y: -0.13516,
                z: 0.06913,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03022, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99432,
                x: 0.00190,
                y: -0.00013,
                z: 0.10645,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01819, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99593,
                x: -0.00201,
                y: -0.05208,
                z: -0.07353,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01802, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: -0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00606, 0.05629, 0.06006, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.73724,
                x: 0.20275,
                y: 0.59427,
                z: 0.24944,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04042, -0.04302, 0.01935, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.29033,
                x: 0.62353,
                y: -0.66381,
                z: -0.29373,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03935, -0.07567, 0.04705, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.18705,
                x: 0.67806,
                y: -0.65929,
                z: -0.26568,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03834, -0.09099, 0.08258, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.18304,
                x: 0.73679,
                y: -0.63476,
                z: -0.14394,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03181, -0.08721, 0.12101, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.00366,
                x: 0.75841,
                y: -0.63934,
                z: -0.12668,
            },
        },
    ];
    pub static FIST: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: -0.92028,
                z: 0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01643, 0.03087, 0.02512, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.40385,
                x: 0.59570,
                y: 0.08245,
                z: 0.68938,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04041, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98966,
                x: -0.09043,
                y: 0.02846,
                z: 0.10769,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03252, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98859,
                x: 0.14398,
                y: 0.04152,
                z: 0.01536,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03046, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00380, 0.02151, 0.01280, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.61731,
                x: 0.39518,
                y: -0.51087,
                z: 0.44919,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.07420, -0.00500, 0.00023, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.73729,
                x: -0.03201,
                y: -0.11501,
                z: 0.66494,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04329, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.61138,
                x: 0.00329,
                y: 0.00382,
                z: 0.79132,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02827, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.74539,
                x: -0.00068,
                y: -0.00094,
                z: 0.66663,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02282, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00579, 0.00681, 0.01653, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.51420,
                x: 0.52231,
                y: -0.47835,
                z: 0.48370,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.07095, 0.00078, 0.00100, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.72365,
                x: -0.09790,
                y: 0.04855,
                z: 0.68146,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04311, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.63746,
                x: -0.00237,
                y: -0.00283,
                z: 0.77047,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03327, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.65801,
                x: 0.00261,
                y: 0.00320,
                z: 0.75300,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02589, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99919,
                x: 0.00000,
                y: 0.00000,
                z: 0.04013,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00412, -0.00686, 0.01656, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.48961,
                x: 0.52337,
                y: -0.52064,
                z: 0.46400,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.06588, 0.00179, 0.00069, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.75997,
                x: -0.05561,
                y: 0.01157,
                z: 0.64747,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04033, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.66431,
                x: 0.00159,
                y: 0.00197,
                z: 0.74745,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02849, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.62696,
                x: -0.00278,
                y: -0.00323,
                z: 0.77904,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02243, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00113, -0.01929, 0.01543, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.47977,
                x: 0.47783,
                y: -0.63020,
                z: 0.37993,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.06288, 0.00284, 0.00033, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.82700,
                x: 0.03428,
                y: 0.00344,
                z: 0.56114,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02987, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.70218,
                x: -0.00672,
                y: -0.00929,
                z: 0.71190,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01798, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.67685,
                x: 0.00796,
                y: 0.00992,
                z: 0.73601,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01802, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: -0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00519, 0.05419, 0.06003, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.74737,
                x: 0.18239,
                y: 0.59962,
                z: 0.22052,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00017, 0.01647, 0.09651, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.00646,
                x: 0.02275,
                y: -0.93293,
                z: -0.35929,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00045, 0.00154, 0.11654, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.03936,
                x: 0.10514,
                y: -0.92883,
                z: -0.35308,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00395, -0.01487, 0.13061, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05507,
                x: 0.06870,
                y: -0.94402,
                z: -0.31793,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00326, -0.03469, 0.13993, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.01969,
                x: -0.10074,
                y: -0.95733,
                z: -0.27015,
            },
        },
    ];
    pub static GRIPLIMIT: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: -0.92028,
                z: 0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01631, 0.02753, 0.01780, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.22570,
                x: 0.48333,
                y: 0.12641,
                z: 0.83634,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04041, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.89434,
                x: -0.01330,
                y: -0.08290,
                z: 0.43945,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03252, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.84243,
                x: 0.00065,
                y: 0.00124,
                z: 0.53881,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03046, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00380, 0.02151, 0.01280, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.61731,
                x: 0.39517,
                y: -0.51087,
                z: 0.44919,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.07420, -0.00500, 0.00023, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.73729,
                x: -0.03201,
                y: -0.11501,
                z: 0.66494,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04329, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.61138,
                x: 0.00329,
                y: 0.00382,
                z: 0.79132,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02828, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.74539,
                x: -0.00068,
                y: -0.00095,
                z: 0.66663,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02282, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00579, 0.00681, 0.01653, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.51420,
                x: 0.52232,
                y: -0.47835,
                z: 0.48370,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.07095, 0.00078, 0.00100, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.72365,
                x: -0.09790,
                y: 0.04855,
                z: 0.68146,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04311, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.63746,
                x: -0.00237,
                y: -0.00283,
                z: 0.77047,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03327, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.65801,
                x: 0.00261,
                y: 0.00320,
                z: 0.75300,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02589, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99919,
                x: 0.00000,
                y: 0.00000,
                z: 0.04013,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00412, -0.00686, 0.01656, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.48961,
                x: 0.52337,
                y: -0.52064,
                z: 0.46400,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.06588, 0.00179, 0.00069, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.75997,
                x: -0.05561,
                y: 0.01157,
                z: 0.64747,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.04033, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.66431,
                x: 0.00159,
                y: 0.00197,
                z: 0.74745,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02849, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.62696,
                x: -0.00278,
                y: -0.00323,
                z: 0.77904,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02243, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00113, -0.01929, 0.01543, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.47977,
                x: 0.47783,
                y: -0.63020,
                z: 0.37993,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.06288, 0.00284, 0.00033, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.82700,
                x: 0.03428,
                y: 0.00344,
                z: 0.56114,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.02987, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.70218,
                x: -0.00672,
                y: -0.00929,
                z: 0.71190,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01798, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.67685,
                x: 0.00796,
                y: 0.00992,
                z: 0.73601,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01802, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: -0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01972, 0.00280, 0.09394, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.37729,
                x: -0.54083,
                y: 0.15045,
                z: -0.73656,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00017, 0.01647, 0.09652, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.00646,
                x: 0.02275,
                y: -0.93293,
                z: -0.35929,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00045, 0.00154, 0.11654, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.03936,
                x: 0.10514,
                y: -0.92883,
                z: -0.35308,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00395, -0.01487, 0.13061, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05507,
                x: 0.06870,
                y: -0.94402,
                z: -0.31793,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00326, -0.03469, 0.13993, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.01969,
                x: -0.10074,
                y: -0.95733,
                z: -0.27015,
            },
        },
    ];
}
pub mod right_hand {
    use super::*;
    pub static BINDPOSE: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: 0.92028,
                z: -0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00018, 0.00029, 0.00025, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.54119,
                x: -0.27639,
                y: 0.77304,
                z: -0.18203,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00040, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.96917,
                x: 0.00006,
                y: -0.00137,
                z: 0.24638,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00033, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98817,
                x: 0.00010,
                y: 0.00140,
                z: 0.15334,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00030, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00002, 0.00021, 0.00015, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.53106,
                x: -0.55075,
                y: 0.53958,
                z: 0.35143,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00074, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.96898,
                x: 0.00162,
                y: -0.05289,
                z: 0.24140,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00043, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98277,
                x: -0.00009,
                y: 0.00504,
                z: 0.18476,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00028, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99707,
                x: 0.00003,
                y: -0.00117,
                z: 0.07646,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00023, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00002, 0.00007, 0.00016, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.56175,
                x: -0.53342,
                y: 0.47299,
                z: 0.41974,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00071, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.97339,
                x: -0.00000,
                y: -0.00019,
                z: 0.22916,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00043, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98753,
                x: 0.00009,
                y: -0.00369,
                z: 0.15740,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00033, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98996,
                x: -0.00011,
                y: 0.00413,
                z: 0.14128,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00026, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: -0.00000,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00001, -0.00007, 0.00016, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.55014,
                x: -0.51669,
                y: 0.42989,
                z: 0.49555,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00066, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.97456,
                x: -0.00090,
                y: -0.04096,
                z: 0.22037,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00040, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99100,
                x: -0.00007,
                y: 0.00253,
                z: 0.13383,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00028, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99079,
                x: 0.00020,
                y: -0.00426,
                z: 0.13535,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00022, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: -0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00002, -0.00019, 0.00015, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.51533,
                x: -0.48576,
                y: 0.34675,
                z: 0.61502,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00063, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99349,
                x: 0.00394,
                y: 0.02816,
                z: 0.11031,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00030, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99111,
                x: 0.00038,
                y: -0.01146,
                z: 0.13252,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00018, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99401,
                x: -0.00054,
                y: 0.01270,
                z: 0.10858,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00018, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00039, 0.00060, -0.00084, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.81959,
                x: -0.04504,
                y: -0.56911,
                z: 0.04861,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00018, 0.00037, -0.00149, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.70842,
                x: -0.59723,
                y: -0.31233,
                z: -0.20956,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00013, 0.00008, -0.00155, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.67740,
                x: -0.64706,
                y: -0.27117,
                z: -0.22114,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00018, -0.00023, -0.00142, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.59503,
                x: -0.72163,
                y: -0.26235,
                z: -0.23741,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00016, -0.00046, -0.00119, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.51142,
                x: -0.73903,
                y: -0.26548,
                z: -0.34900,
            },
        },
    ];
    pub static OPENHAND: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: 0.92028,
                z: -0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01233, 0.02866, 0.02505, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.57106,
                x: -0.45128,
                y: 0.63006,
                z: -0.27068,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04041, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99457,
                x: 0.07828,
                y: 0.01828,
                z: 0.06618,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03252, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.97766,
                x: -0.00304,
                y: 0.02072,
                z: -0.20916,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03046, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00063, 0.02687, 0.01500, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.42183,
                x: -0.64379,
                y: 0.42246,
                z: 0.47866,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.07420, 0.00500, -0.00023, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99478,
                x: 0.00705,
                y: -0.04129,
                z: 0.09301,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04393, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99840,
                x: 0.04591,
                y: 0.00278,
                z: -0.03277,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02870, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99970,
                x: 0.00195,
                y: -0.02277,
                z: -0.00828,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02282, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00218, 0.00712, 0.01632, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.54187,
                x: -0.54743,
                y: 0.46000,
                z: 0.44170,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.07095, -0.00078, -0.00100, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.97984,
                x: -0.16806,
                y: -0.07591,
                z: 0.07690,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04311, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99727,
                x: 0.01828,
                y: 0.01338,
                z: 0.07027,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03327, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99840,
                x: -0.00314,
                y: -0.02642,
                z: -0.04985,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02589, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99919,
                x: 0.00000,
                y: 0.00000,
                z: 0.04013,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00051, -0.00654, 0.01635, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.54898,
                x: -0.51907,
                y: 0.42691,
                z: 0.49692,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.06588, -0.00179, -0.00069, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98979,
                x: -0.06588,
                y: -0.09642,
                z: 0.08172,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04070, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99910,
                x: -0.00217,
                y: -0.00002,
                z: 0.04232,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02875, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99858,
                x: -0.00067,
                y: -0.01271,
                z: 0.05165,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02243, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00248, -0.01898, 0.01521, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.51860,
                x: -0.52730,
                y: 0.32826,
                z: 0.58758,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.06288, -0.00284, -0.00033, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98729,
                x: -0.06336,
                y: -0.12596,
                z: 0.07327,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03022, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99341,
                x: 0.00157,
                y: -0.00015,
                z: 0.11458,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01819, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99705,
                x: -0.00069,
                y: -0.05201,
                z: -0.05649,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01802, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: -0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00520, 0.05420, 0.06003, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.74732,
                x: 0.18251,
                y: -0.59959,
                z: -0.22069,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03878, -0.04297, 0.01982, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.29744,
                x: 0.63937,
                y: 0.64891,
                z: 0.28573,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03803, -0.07484, 0.04694, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.19990,
                x: 0.69822,
                y: 0.63577,
                z: 0.26141,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03684, -0.08978, 0.08197, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.19096,
                x: 0.75647,
                y: 0.60759,
                z: 0.14873,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03025, -0.08606, 0.11989, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.01895,
                x: 0.77925,
                y: 0.61218,
                z: 0.13285,
            },
        },
    ];
    pub static FIST: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: 0.92028,
                z: -0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01643, 0.03087, 0.02512, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.59570,
                x: -0.40385,
                y: 0.68938,
                z: -0.08245,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04041, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98966,
                x: -0.09043,
                y: 0.02846,
                z: 0.10769,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03252, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.98859,
                x: 0.14398,
                y: 0.04152,
                z: 0.01536,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03046, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00380, 0.02151, 0.01280, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.39517,
                x: -0.61731,
                y: 0.44919,
                z: 0.51087,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.07420, 0.00500, -0.00023, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.73729,
                x: -0.03201,
                y: -0.11501,
                z: 0.66494,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04329, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.61138,
                x: 0.00329,
                y: 0.00382,
                z: 0.79132,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02827, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.74539,
                x: -0.00068,
                y: -0.00094,
                z: 0.66663,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02282, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00579, 0.00681, 0.01653, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.52231,
                x: -0.51420,
                y: 0.48370,
                z: 0.47835,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.07095, -0.00078, -0.00100, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.72365,
                x: -0.09790,
                y: 0.04855,
                z: 0.68146,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04311, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.63746,
                x: -0.00237,
                y: -0.00283,
                z: 0.77047,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03327, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.65801,
                x: 0.00261,
                y: 0.00320,
                z: 0.75300,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02589, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99919,
                x: 0.00000,
                y: 0.00000,
                z: 0.04013,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00412, -0.00686, 0.01656, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.52337,
                x: -0.48961,
                y: 0.46400,
                z: 0.52064,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.06588, -0.00179, -0.00069, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.75997,
                x: -0.05561,
                y: 0.01157,
                z: 0.64747,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04033, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.66431,
                x: 0.00159,
                y: 0.00197,
                z: 0.74745,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02849, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.62696,
                x: -0.00278,
                y: -0.00323,
                z: 0.77904,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02243, 0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00113, -0.01929, 0.01543, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.47783,
                x: -0.47977,
                y: 0.37994,
                z: 0.63020,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.06288, -0.00284, -0.00033, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.82700,
                x: 0.03428,
                y: 0.00344,
                z: 0.56114,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02987, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.70218,
                x: -0.00672,
                y: -0.00929,
                z: 0.71190,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01798, -0.00000, -0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.67685,
                x: 0.00796,
                y: 0.00992,
                z: 0.73601,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01802, -0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: -0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00439, 0.05551, 0.06025, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.74592,
                x: 0.15676,
                y: -0.59795,
                z: -0.24795,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00017, 0.01647, 0.09651, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.00646,
                x: 0.02275,
                y: 0.93293,
                z: 0.35929,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00045, 0.00154, 0.11654, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.03936,
                x: 0.10514,
                y: 0.92883,
                z: 0.35308,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00395, -0.01487, 0.13061, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05507,
                x: 0.06870,
                y: 0.94402,
                z: 0.31793,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00326, -0.03469, 0.13993, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.01969,
                x: -0.10074,
                y: 0.95733,
                z: 0.27015,
            },
        },
    ];
    pub static GRIPLIMIT: [vr::VRBoneTransform_t; HandSkeletonBone::Count as usize] = [
        // Root
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.00000, 0.00000, 0.00000, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // Wrist
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.03404, 0.03650, 0.16472, 1.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05515,
                x: -0.07861,
                y: 0.92028,
                z: -0.37930,
            },
        },
        // Thumb0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [0.01631, 0.02753, 0.01780, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.48333,
                x: -0.22570,
                y: 0.83634,
                z: -0.12641,
            },
        },
        // Thumb1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04041, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.89434,
                x: -0.01330,
                y: -0.08290,
                z: 0.43945,
            },
        },
        // Thumb2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03252, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.84243,
                x: 0.00065,
                y: 0.00124,
                z: 0.53881,
            },
        },
        // Thumb3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03046, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // IndexFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00380, 0.02151, 0.01280, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.39517,
                x: -0.61731,
                y: 0.44919,
                z: 0.51087,
            },
        },
        // IndexFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.07420, 0.00500, -0.00023, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.73729,
                x: -0.03201,
                y: -0.11501,
                z: 0.66494,
            },
        },
        // IndexFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04329, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.61138,
                x: 0.00329,
                y: 0.00382,
                z: 0.79132,
            },
        },
        // IndexFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02828, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.74539,
                x: -0.00068,
                y: -0.00095,
                z: 0.66663,
            },
        },
        // IndexFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02282, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: -0.00000,
                z: 0.00000,
            },
        },
        // MiddleFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00579, 0.00681, 0.01653, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.52232,
                x: -0.51420,
                y: 0.48370,
                z: 0.47835,
            },
        },
        // MiddleFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.07095, -0.00078, -0.00100, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.72365,
                x: -0.09790,
                y: 0.04855,
                z: 0.68146,
            },
        },
        // MiddleFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04311, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.63746,
                x: -0.00237,
                y: -0.00283,
                z: 0.77047,
            },
        },
        // MiddleFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.03327, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.65801,
                x: 0.00261,
                y: 0.00320,
                z: 0.75300,
            },
        },
        // MiddleFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02589, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.99919,
                x: 0.00000,
                y: 0.00000,
                z: 0.04013,
            },
        },
        // RingFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00412, -0.00686, 0.01656, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.52337,
                x: -0.48961,
                y: 0.46400,
                z: 0.52064,
            },
        },
        // RingFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.06588, -0.00179, -0.00069, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.75997,
                x: -0.05561,
                y: 0.01157,
                z: 0.64747,
            },
        },
        // RingFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.04033, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.66431,
                x: 0.00159,
                y: 0.00197,
                z: 0.74745,
            },
        },
        // RingFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02849, 0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.62696,
                x: -0.00278,
                y: -0.00323,
                z: 0.77904,
            },
        },
        // RingFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02243, 0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: 0.00000,
                y: 0.00000,
                z: 0.00000,
            },
        },
        // PinkyFinger0
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00113, -0.01929, 0.01543, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.47783,
                x: -0.47977,
                y: 0.37993,
                z: 0.63020,
            },
        },
        // PinkyFinger1
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.06288, -0.00284, -0.00033, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.82700,
                x: 0.03428,
                y: 0.00344,
                z: 0.56114,
            },
        },
        // PinkyFinger2
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.02987, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.70218,
                x: -0.00672,
                y: -0.00929,
                z: 0.71190,
            },
        },
        // PinkyFinger3
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01798, -0.00000, -0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.67685,
                x: 0.00796,
                y: 0.00992,
                z: 0.73601,
            },
        },
        // PinkyFinger4
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01802, -0.00000, 0.00000, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 1.00000,
                x: -0.00000,
                y: -0.00000,
                z: -0.00000,
            },
        },
        // AuxThumb
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.01972, 0.00280, 0.09394, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.37729,
                x: -0.54083,
                y: -0.15045,
                z: 0.73656,
            },
        },
        // AuxIndexFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00017, 0.01647, 0.09652, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.00646,
                x: 0.02275,
                y: 0.93293,
                z: 0.35929,
            },
        },
        // AuxMiddleFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00045, 0.00154, 0.11654, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.03936,
                x: 0.10514,
                y: 0.92883,
                z: 0.35308,
            },
        },
        // AuxRingFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00395, -0.01487, 0.13061, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: -0.05507,
                x: 0.06870,
                y: 0.94402,
                z: 0.31793,
            },
        },
        // AuxPinkyFinger
        vr::VRBoneTransform_t {
            position: vr::HmdVector4_t {
                v: [-0.00326, -0.03469, 0.13993, 0.00000],
            },
            orientation: vr::HmdQuaternionf_t {
                w: 0.01969,
                x: -0.10074,
                y: 0.95733,
                z: 0.27015,
            },
        },
    ];
}
