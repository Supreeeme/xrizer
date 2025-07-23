use crate::openxr_data::RealOpenXrData;
use openvr as vr;
use std::sync::Arc;

#[derive(macros::InterfaceImpl)]
#[interface = "IVRChaperone"]
#[versions(004, 003)]
pub struct Chaperone {
    vtables: Vtables,
    openxr: Arc<RealOpenXrData>,
}

impl Chaperone {
    pub fn new(openxr: Arc<RealOpenXrData>) -> Self {
        Self {
            vtables: Default::default(),
            openxr,
        }
    }
}

impl vr::IVRChaperone004_Interface for Chaperone {
    fn ResetZeroPose(&self, origin: vr::ETrackingUniverseOrigin) {
        self.openxr.reset_tracking_space(origin);
    }

    fn ForceBoundsVisible(&self, _: bool) {
        crate::warn_unimplemented!("ForceBoundsVisible");
    }
    fn AreBoundsVisible(&self) -> bool {
        false
    }
    fn GetBoundsColor(
        &self,
        _: *mut vr::HmdColor_t,
        _: std::os::raw::c_int,
        _: f32,
        _: *mut vr::HmdColor_t,
    ) {
        todo!()
    }
    fn SetSceneColor(&self, _: vr::HmdColor_t) {
        todo!()
    }
    fn ReloadInfo(&self) {
        todo!()
    }
    fn GetPlayAreaRect(&self, rect: *mut vr::HmdQuad_t) -> bool {
        crate::warn_unimplemented!("GetPlayAreaRect");
        unsafe {
            *rect = Default::default();
        }
        false
    }
    fn GetPlayAreaSize(&self, size_x: *mut f32, size_z: *mut f32) -> bool {
        crate::warn_unimplemented!("GetPlayAreaSize");
        unsafe {
            *size_x = 1.0;
            *size_z = 1.0;
        };
        true
    }
    fn GetCalibrationState(&self) -> vr::ChaperoneCalibrationState {
        vr::ChaperoneCalibrationState::OK
    }
}
