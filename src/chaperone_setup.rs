use openvr::{self as vr, HmdQuad_t, HmdVector2_t};
use std::ffi::CStr;
use std::os::raw::c_char;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRChaperoneSetup"]
#[versions(006)]
pub struct ChaperoneSetup {
    vtables: Vtables,
}

impl vr::IVRChaperoneSetup006_Interface for ChaperoneSetup {
    fn CommitWorkingCopy(&self, _: vr::EChaperoneConfigFile) -> bool {
        crate::warn_unimplemented!("CommitWorkingCopy");
        false
    }

    fn RevertWorkingCopy(&self) {
        crate::warn_unimplemented!("RevertWorkingCopy");
    }

    fn GetWorkingPlayAreaSize(&self, size_x: *mut f32, size_z: *mut f32) -> bool {
        crate::warn_unimplemented!("GetWorkingPlayAreaSize");
        if !size_x.is_null() && !size_z.is_null() {
            unsafe {
                *size_x = 1.0;
                *size_z = 1.0;
            }
        }
        false
    }

    fn GetWorkingPlayAreaRect(&self, _: *mut vr::HmdQuad_t) -> bool {
        crate::warn_unimplemented!("GetWorkingPlayAreaRect");
        false
    }

    fn GetWorkingCollisionBoundsInfo(
        &self,
        _: *mut vr::HmdQuad_t,
        quads_count: *mut u32,
    ) -> bool {
        crate::warn_unimplemented!("GetWorkingCollisionBoundsInfo");
        if !quads_count.is_null() {
            unsafe {
                *quads_count = 0;
            }
        }
        false
    }

    fn GetLiveCollisionBoundsInfo(
        &self,
        _: *mut vr::HmdQuad_t,
        quads_count: *mut u32,
    ) -> bool {
        crate::warn_unimplemented!("GetLiveCollisionBoundsInfo");
        if !quads_count.is_null() {
            unsafe {
                *quads_count = 0;
            }
        }
        false
    }

    fn GetWorkingSeatedZeroPoseToRawTrackingPose(
        &self,
        _: *mut vr::HmdMatrix34_t,
    ) -> bool {
        crate::warn_unimplemented!("GetWorkingSeatedZeroPoseToRawTrackingPose");
        false
    }

    fn GetWorkingStandingZeroPoseToRawTrackingPose(
        &self,
        _: *mut vr::HmdMatrix34_t,
    ) -> bool {
        crate::warn_unimplemented!("GetWorkingStandingZeroPoseToRawTrackingPose");
        false
    }

    fn SetWorkingPlayAreaSize(&self, _: f32, _: f32) {
        crate::warn_unimplemented!("SetWorkingPlayAreaSize");
    }

    fn SetWorkingCollisionBoundsInfo(
        &self,
        _: *mut HmdQuad_t,
        _: u32,
    ) {
        crate::warn_unimplemented!("SetWorkingCollisionBoundsInfo");
    }

    fn SetWorkingPerimeter(&self, _: *mut HmdVector2_t, _: u32) {
        crate::warn_unimplemented!("SetWorkingPerimeter");
    }

    fn SetWorkingSeatedZeroPoseToRawTrackingPose(
        &self,
        _: *const vr::HmdMatrix34_t,
    ) {
        crate::warn_unimplemented!("SetWorkingSeatedZeroPoseToRawTrackingPose");
    }

    fn SetWorkingStandingZeroPoseToRawTrackingPose(
        &self,
        _: *const vr::HmdMatrix34_t,
    ) {
        crate::warn_unimplemented!("SetWorkingStandingZeroPoseToRawTrackingPose");
    }

    fn ReloadFromDisk(&self, _: vr::EChaperoneConfigFile) {
        crate::warn_unimplemented!("ReloadFromDisk");
    }

    fn GetLiveSeatedZeroPoseToRawTrackingPose(
        &self,
        _: *mut vr::HmdMatrix34_t,
    ) -> bool {
        crate::warn_unimplemented!("GetLiveSeatedZeroPoseToRawTrackingPose");
        false
    }

    fn ExportLiveToBuffer(&self, _: *mut c_char, buffer_length: *mut u32) -> bool {
        crate::warn_unimplemented!("ExportLiveToBuffer");
        if !buffer_length.is_null() {
            unsafe {
                *buffer_length = 0;
            }
        }
        false
    }

    fn ImportFromBufferToWorking(
        &self,
        buffer: *const c_char,
        _: u32,
    ) -> bool {
        let _buffer_str = if buffer.is_null() {
            "null".to_string()
        } else {
            unsafe { CStr::from_ptr(buffer) }.to_string_lossy().to_string()
        };
        crate::warn_unimplemented!("ImportFromBufferToWorking");
        false
    }

    fn ShowWorkingSetPreview(&self) {
        crate::warn_unimplemented!("ShowWorkingSetPreview");
    }

    fn HideWorkingSetPreview(&self) {
        crate::warn_unimplemented!("HideWorkingSetPreview");
    }

    fn RoomSetupStarting(&self) {
        crate::warn_unimplemented!("RoomSetupStarting");
    }
}