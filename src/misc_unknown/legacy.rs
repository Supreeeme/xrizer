// Compatibility stubs for legacy OpenVR interfaces required by older titles
// (e.g. Doom VFR) that predate current xrizer/OpenXR support.
use log::{debug, info};
use openvr as vr;
use openxr as xr;
use std::borrow::Cow;
use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use super::{Wrapper, UNKNOWN_TAG};

// Mirrors the gen_vtable! macro from the parent module.
macro_rules! gen_vtable {
    (struct $name:ident {
        $(
            fn $fn_name:ident($($arg:ident: $ty:ty),*$(,)?) $(-> $output:ty)? {$($tt:tt)*}
        )+
    }) => {
        #[repr(C)]
        pub(super) struct $name {
            $(
                $fn_name: extern "C" fn(*mut Wrapper<$name> $(,$ty)*) $(-> $output)?
            ),*
        }

        impl $name {
            $(
            extern "C" fn $fn_name(_: *mut Wrapper<$name> $(,$arg:$ty)*) $(-> $output)? {$($tt)*}
            )*
        }

        impl Default for &'static $name {
            fn default() -> Self {
                &$name {
                    $(
                        $fn_name: $name::$fn_name
                    ),*
                }
            }
        }
    }
}

// Driver name reported through IVRDriverManager.
const XRIZER_DRIVER_NAME: &[u8] = b"xrizer\0";

// Default virtual display resolution returned by IVRExtendedDisplay.
const DEFAULT_EXTENDED_DISPLAY_WIDTH: u32 = 2160;
const DEFAULT_EXTENDED_DISPLAY_HEIGHT: u32 = 1200;

// Default chaperone play-area side length (metres).
const DEFAULT_PLAY_AREA_SIZE: f32 = 2.0;

// EVRTrackedCameraError numeric values from openvr.h.
const TRACKED_CAMERA_ERR_OPERATION_FAILED: i32 = 100;
const TRACKED_CAMERA_ERR_NOT_SUPPORTED: i32 = 105;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn copy_bytes_to_buffer(bytes: &[u8], buffer: *mut c_char, buffer_len: u32) -> u32 {
    let required = bytes.len() as u32;
    if !buffer.is_null() && buffer_len >= required {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buffer, bytes.len());
        }
    }
    required
}

fn copy_cstring_to_buffer(text: &[u8], buffer: *mut c_char, buffer_len: u32) -> u32 {
    copy_bytes_to_buffer(text, buffer, buffer_len)
}

fn steamvr_resources_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = Path::new(&home)
        .join(".local/share/Steam/steamapps/common/SteamVR/resources");
    path.is_dir().then_some(path)
}

fn resource_path(resource_name: &CStr, resource_type_directory: Option<&CStr>) -> Option<PathBuf> {
    let base = steamvr_resources_dir()?;

    let resource_name = resource_name.to_string_lossy();
    let resource_name = resource_name.trim_start_matches('/');
    // Strip optional {driver}-style prefix used in some SteamVR resource paths.
    let resource_name = resource_name
        .strip_prefix('{')
        .and_then(|s| s.split_once('}').map(|(_, path)| path))
        .unwrap_or(resource_name);

    let mut path = base;
    if let Some(dir) = resource_type_directory {
        let dir = dir.to_string_lossy();
        let dir = dir.trim_matches('/');
        if !dir.is_empty() {
            path.push(dir);
        }
    }

    path.push(resource_name);
    Some(path)
}

fn path_to_cstring_bytes(path: &Path) -> Cow<'static, [u8]> {
    let text = path.to_string_lossy().into_owned() + "\0";
    Cow::Owned(text.into_bytes())
}

fn write_out<T: Copy>(ptr: *mut T, value: T) {
    unsafe {
        if let Some(ptr) = ptr.as_mut() {
            *ptr = value;
        }
    }
}

fn default_play_area_rect() -> vr::HmdQuad_t {
    let half_extent = DEFAULT_PLAY_AREA_SIZE * 0.5;
    vr::HmdQuad_t {
        vCorners: [
            vr::HmdVector3_t { v: [-half_extent, 0.0, -half_extent] },
            vr::HmdVector3_t { v: [half_extent, 0.0, -half_extent] },
            vr::HmdVector3_t { v: [half_extent, 0.0, half_extent] },
            vr::HmdVector3_t { v: [-half_extent, 0.0, half_extent] },
        ],
    }
}

fn identity_tracking_pose() -> vr::HmdMatrix34_t {
    xr::Posef::IDENTITY.into()
}

// ── IVRChaperoneSetup_005 ────────────────────────────────────────────────────

gen_vtable! {
    struct ChaperoneSetup {
        fn CommitWorkingCopy(_config_file: u32) -> bool { true }
        fn RevertWorkingCopy() {}
        fn GetWorkingPlayAreaSize(size_x: *mut f32, size_z: *mut f32) -> bool {
            write_out(size_x, DEFAULT_PLAY_AREA_SIZE);
            write_out(size_z, DEFAULT_PLAY_AREA_SIZE);
            true
        }
        fn GetWorkingPlayAreaRect(rect: *mut c_void) -> bool {
            write_out(rect.cast::<vr::HmdQuad_t>(), default_play_area_rect());
            true
        }
        fn GetWorkingCollisionBoundsInfo(quads_buffer: *mut c_void, quads_count: *mut u32) -> bool {
            write_out(quads_count, 1);
            write_out(quads_buffer.cast::<vr::HmdQuad_t>(), default_play_area_rect());
            true
        }
        fn GetLiveCollisionBoundsInfo(quads_buffer: *mut c_void, quads_count: *mut u32) -> bool {
            write_out(quads_count, 1);
            write_out(quads_buffer.cast::<vr::HmdQuad_t>(), default_play_area_rect());
            true
        }
        fn GetWorkingSeatedZeroPoseToRawTrackingPose(mat: *mut c_void) -> bool {
            write_out(mat.cast::<vr::HmdMatrix34_t>(), identity_tracking_pose());
            true
        }
        fn GetWorkingStandingZeroPoseToRawTrackingPose(mat: *mut c_void) -> bool {
            write_out(mat.cast::<vr::HmdMatrix34_t>(), identity_tracking_pose());
            true
        }
        fn SetWorkingPlayAreaSize(_size_x: f32, _size_z: f32) {}
        fn SetWorkingCollisionBoundsInfo(_quads_buffer: *mut c_void, _quads_count: u32) {}
        fn SetWorkingSeatedZeroPoseToRawTrackingPose(_mat: *const c_void) {}
        fn SetWorkingStandingZeroPoseToRawTrackingPose(_mat: *const c_void) {}
        fn ReloadFromDisk(_config_file: u32) {}
        fn GetLiveSeatedZeroPoseToRawTrackingPose(mat: *mut c_void) -> bool {
            write_out(mat.cast::<vr::HmdMatrix34_t>(), identity_tracking_pose());
            true
        }
        fn SetWorkingCollisionBoundsTagsInfo(_tags_buffer: *mut u8, _tag_count: u32) {}
        fn GetLiveCollisionBoundsTagsInfo(_tags_buffer: *mut u8, tag_count: *mut u32) -> bool {
            write_out(tag_count, 0);
            true
        }
        fn SetWorkingPhysicalBoundsInfo(_quads_buffer: *mut c_void, _quads_count: u32) -> bool { true }
        fn GetLivePhysicalBoundsInfo(quads_buffer: *mut c_void, quads_count: *mut u32) -> bool {
            write_out(quads_count, 1);
            write_out(quads_buffer.cast::<vr::HmdQuad_t>(), default_play_area_rect());
            true
        }
        fn ExportLiveToBuffer(_buffer: *mut c_char, _buffer_length: *mut u32) -> bool { false }
        fn ImportFromBufferToWorking(_buffer: *const c_char, _import_flags: u32) -> bool { false }
    }
}

// ── IVRExtendedDisplay_001 ───────────────────────────────────────────────────

gen_vtable! {
    struct ExtendedDisplay {
        fn GetWindowBounds(x: *mut i32, y: *mut i32, width: *mut u32, height: *mut u32) {
            write_out(x, 0);
            write_out(y, 0);
            write_out(width, DEFAULT_EXTENDED_DISPLAY_WIDTH);
            write_out(height, DEFAULT_EXTENDED_DISPLAY_HEIGHT);
        }
        fn GetEyeOutputViewport(eye: i32, x: *mut u32, y: *mut u32, width: *mut u32, height: *mut u32) {
            let eye_width = DEFAULT_EXTENDED_DISPLAY_WIDTH / 2;
            write_out(x, if eye == 0 { 0 } else { eye_width });
            write_out(y, 0);
            write_out(width, eye_width);
            write_out(height, DEFAULT_EXTENDED_DISPLAY_HEIGHT);
        }
        fn GetDXGIOutputInfo(adapter_index: *mut i32, adapter_output_index: *mut i32) {
            write_out(adapter_index, -1);
            write_out(adapter_output_index, -1);
        }
    }
}

// ── IVRTrackedCamera_003 ─────────────────────────────────────────────────────
// xrizer does not expose a physical camera, so all acquisition calls return
// EVRTrackedCameraError_NotSupportedForThisDevice (105).

gen_vtable! {
    struct TrackedCamera {
        fn GetCameraErrorNameFromEnum(camera_error: i32) -> *const c_char {
            match camera_error {
                0 => c"VRTrackedCameraError_None".as_ptr(),
                TRACKED_CAMERA_ERR_NOT_SUPPORTED => c"VRTrackedCameraError_NotSupportedForThisDevice".as_ptr(),
                _ => c"VRTrackedCameraError_OperationFailed".as_ptr(),
            }
        }
        fn HasCamera(_device_index: u32, has_camera: *mut bool) -> i32 {
            info!(target: UNKNOWN_TAG, "IVRTrackedCamera_003 queried; reporting no tracked camera support");
            write_out(has_camera, false);
            TRACKED_CAMERA_ERR_NOT_SUPPORTED
        }
        fn GetCameraFrameSize(_device_index: u32, _frame_type: i32, width: *mut u32, height: *mut u32, frame_buffer_size: *mut u32) -> i32 {
            write_out(width, 0);
            write_out(height, 0);
            write_out(frame_buffer_size, 0);
            TRACKED_CAMERA_ERR_NOT_SUPPORTED
        }
        fn GetCameraIntrinsics(_device_index: u32, _frame_type: i32, _focal_length: *mut c_void, _center: *mut c_void) -> i32 { TRACKED_CAMERA_ERR_NOT_SUPPORTED }
        fn GetCameraProjection(_device_index: u32, _frame_type: i32, _z_near: f32, _z_far: f32, _projection: *mut c_void) -> i32 { TRACKED_CAMERA_ERR_NOT_SUPPORTED }
        fn AcquireVideoStreamingService(_device_index: u32, handle: *mut u64) -> i32 {
            write_out(handle, 0);
            TRACKED_CAMERA_ERR_NOT_SUPPORTED
        }
        fn ReleaseVideoStreamingService(tracked_camera: u64) -> i32 {
            if tracked_camera == 0 { TRACKED_CAMERA_ERR_OPERATION_FAILED } else { 0 }
        }
        fn GetVideoStreamFrameBuffer(_tracked_camera: u64, _frame_type: i32, _frame_buffer: *mut c_void, _frame_buffer_size: u32, _frame_header: *mut c_void, _frame_header_size: u32) -> i32 { TRACKED_CAMERA_ERR_NOT_SUPPORTED }
        fn GetVideoStreamTextureSize(_device_index: u32, _frame_type: i32, _texture_bounds: *mut c_void, width: *mut u32, height: *mut u32) -> i32 {
            write_out(width, 0);
            write_out(height, 0);
            TRACKED_CAMERA_ERR_NOT_SUPPORTED
        }
        fn GetVideoStreamTextureD3D11(_tracked_camera: u64, _frame_type: i32, _device_or_resource: *mut c_void, shader_resource_view: *mut *mut c_void, _frame_header: *mut c_void, _frame_header_size: u32) -> i32 {
            write_out(shader_resource_view, std::ptr::null_mut());
            TRACKED_CAMERA_ERR_NOT_SUPPORTED
        }
        fn GetVideoStreamTextureGL(_tracked_camera: u64, _frame_type: i32, texture_id: *mut u32, _frame_header: *mut c_void, _frame_header_size: u32) -> i32 {
            write_out(texture_id, 0);
            TRACKED_CAMERA_ERR_NOT_SUPPORTED
        }
        fn ReleaseVideoStreamTextureGL(_tracked_camera: u64, texture_id: u32) -> i32 {
            if texture_id == 0 { TRACKED_CAMERA_ERR_OPERATION_FAILED } else { 0 }
        }
    }
}

// ── IVRResources_001 ─────────────────────────────────────────────────────────
// Resolves resources from the local SteamVR installation when available.

gen_vtable! {
    struct Resources {
        fn LoadSharedResource(resource_name: *const c_char, buffer: *mut c_char, buffer_len: u32) -> u32 {
            if resource_name.is_null() {
                return 0;
            }

            let resource_name = unsafe { CStr::from_ptr(resource_name) };
            debug!(target: UNKNOWN_TAG, "LoadSharedResource requested for {:?}", resource_name);
            let Some(path) = resource_path(resource_name, None) else {
                debug!(target: UNKNOWN_TAG, "LoadSharedResource could not resolve path for {:?}", resource_name);
                return 0;
            };

            let Ok(bytes) = std::fs::read(&path) else {
                debug!(target: UNKNOWN_TAG, "LoadSharedResource missing {:?}", path);
                return 0;
            };

            copy_bytes_to_buffer(&bytes, buffer, buffer_len)
        }
        fn GetResourceFullPath(resource_name: *const c_char, resource_type_directory: *const c_char, path_buffer: *mut c_char, buffer_len: u32) -> u32 {
            if resource_name.is_null() {
                return 0;
            }

            let resource_name = unsafe { CStr::from_ptr(resource_name) };
            let resource_type_directory = (!resource_type_directory.is_null())
                .then(|| unsafe { CStr::from_ptr(resource_type_directory) });

            let Some(path) = resource_path(resource_name, resource_type_directory) else {
                debug!(target: UNKNOWN_TAG, "GetResourceFullPath could not resolve path for {:?}", resource_name);
                return 0;
            };

            debug!(target: UNKNOWN_TAG, "GetResourceFullPath resolved {:?} to {:?}", resource_name, path);

            let bytes = path_to_cstring_bytes(&path);
            copy_cstring_to_buffer(&bytes, path_buffer, buffer_len)
        }
    }
}

// ── IVRDriverManager_001 ─────────────────────────────────────────────────────
// Reports a single driver named "xrizer".

gen_vtable! {
    struct DriverManager {
        fn GetDriverCount() -> u32 {
            info!(target: UNKNOWN_TAG, "reporting 1 OpenVR driver");
            1
        }
        fn GetDriverName(driver: u32, value: *mut c_char, buffer_size: u32) -> u32 {
            debug!(target: UNKNOWN_TAG, "GetDriverName requested for driver index {driver}");
            if driver != 0 {
                return 0;
            }
            copy_cstring_to_buffer(XRIZER_DRIVER_NAME, value, buffer_size)
        }
    }
}
