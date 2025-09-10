use log::warn;
use openvr as vr;

#[derive(macros::InterfaceImpl)]
#[interface = "IVRExtendedDisplay"]
#[versions(001)]
pub struct ExtendedDisplay {
    vtables: Vtables,
}

impl ExtendedDisplay {
    pub fn default() -> Self {
        Self {
            vtables: Vtables::default(),
        }
    }
}

impl vr::IVRExtendedDisplay001_Interface for ExtendedDisplay {
    fn GetWindowBounds(&self, x: *mut i32, y: *mut i32, width: *mut u32, height: *mut u32) {
        crate::warn_unimplemented!("IVRExtendedDisplay::GetWindowBounds");
        if !(x.is_null() || y.is_null() || width.is_null() || height.is_null()) {
            unsafe {
                x.write(0);
                y.write(0);
                width.write(1280);
                height.write(720);
            }
        } else {
            warn!("One or more pointers passed to GetWindowBounds are null: x: {}, y: {}, width: {}, height: {}",
                x.is_null(), y.is_null(), width.is_null(), height.is_null());
        }
    }
    fn GetEyeOutputViewport(
        &self,
        _e_eye: vr::EVREye,
        _pn_x: *mut u32,
        _pn_y: *mut u32,
        _pn_width: *mut u32,
        _pn_height: *mut u32,
    ) {
        crate::warn_unimplemented!("IVRExtendedDisplay::GetEyeOutputViewport");
        todo!()
    }
    fn GetDXGIOutputInfo(&self, _pn_adapter_index: *mut i32, _pn_adapter_output_index: *mut i32) {
        crate::warn_unimplemented!("IVRExtendedDisplay::GetDXGIOutputInfo");
        todo!()
    }
}
