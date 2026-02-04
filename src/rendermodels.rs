use std::ffi::CStr;

use log::debug;
use openvr as vr;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRRenderModels"]
#[versions(006, 005, 004)]
pub struct RenderModels {
    vtables: Vtables,
}

#[allow(non_snake_case)]
impl vr::IVRRenderModels006_Interface for RenderModels {
    fn GetRenderModelErrorNameFromEnum(
        &self,
        _: vr::EVRRenderModelError,
    ) -> *const std::ffi::c_char {
        c"<unknown>".as_ptr()
    }
    fn GetRenderModelOriginalPath(
        &self,
        _: *const std::ffi::c_char,
        _: *mut std::ffi::c_char,
        _: u32,
        _: *mut vr::EVRRenderModelError,
    ) -> u32 {
        todo!()
    }
    fn GetRenderModelThumbnailURL(
        &self,
        _: *const std::ffi::c_char,
        _: *mut std::ffi::c_char,
        _: u32,
        _: *mut vr::EVRRenderModelError,
    ) -> u32 {
        todo!()
    }
    fn RenderModelHasComponent(
        &self,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
    ) -> bool {
        todo!()
    }
    fn GetComponentState(
        &self,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
        _: *const vr::VRControllerState_t,
        _: *const vr::RenderModel_ControllerMode_State_t,
        _: *mut vr::RenderModel_ComponentState_t,
    ) -> bool {
        crate::warn_unimplemented!("GetComponentState");
        false
    }
    fn GetComponentStateForDevicePath(
        &self,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
        _: vr::VRInputValueHandle_t,
        _: *const vr::RenderModel_ControllerMode_State_t,
        _: *mut vr::RenderModel_ComponentState_t,
    ) -> bool {
        crate::warn_unimplemented!("GetComponentStateForDevicePath");
        false
    }
    fn GetComponentRenderModelName(
        &self,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
        _: *mut std::ffi::c_char,
        _: u32,
    ) -> u32 {
        crate::warn_unimplemented!("GetComponentRenderModelName");
        0
    }

    fn GetComponentButtonMask(
        &self,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
    ) -> u64 {
        crate::warn_unimplemented!("GetComponentButtonMask");
        0
    }
    fn GetComponentName(
        &self,
        render_model_name: *const std::ffi::c_char,
        component_index: u32,
        component_name: *mut std::ffi::c_char,
        component_name_len: u32,
    ) -> u32 {
        crate::warn_unimplemented!("GetComponentName");

        // minimal meaningless implementation to get Derail Valley to acknowledge controller input
        let name = unsafe { CStr::from_ptr(render_model_name) };
        debug!("getting component {component_index} for {name:?}");

        if component_index > 0 {
            return 0;
        }

        static C: &CStr = c"placeholder!";

        let bytes = unsafe { std::slice::from_raw_parts(C.as_ptr(), C.count_bytes() + 1) };
        if component_name_len >= bytes.len() as u32 {
            let out = unsafe {
                std::slice::from_raw_parts_mut(component_name, component_name_len as usize)
            };
            out[..bytes.len()].copy_from_slice(bytes);
        }

        bytes.len() as u32
    }
    fn GetComponentCount(&self, render_model_name: *const std::ffi::c_char) -> u32 {
        let name = unsafe { CStr::from_ptr(render_model_name) };
        debug!("getting components for {name:?}");

        if name.count_bytes() == 0 { 0 } else { 1 }
    }
    fn GetRenderModelCount(&self) -> u32 {
        crate::warn_unimplemented!("GetRenderModelCount");
        0
    }
    fn GetRenderModelName(&self, _: u32, _: *mut std::ffi::c_char, _: u32) -> u32 {
        todo!()
    }
    fn FreeTextureD3D11(&self, _: *mut std::ffi::c_void) {
        todo!()
    }
    fn LoadIntoTextureD3D11_Async(
        &self,
        _: vr::TextureID_t,
        _: *mut std::ffi::c_void,
    ) -> vr::EVRRenderModelError {
        todo!()
    }
    fn LoadTextureD3D11_Async(
        &self,
        _: vr::TextureID_t,
        _: *mut std::ffi::c_void,
        _: *mut *mut std::ffi::c_void,
    ) -> vr::EVRRenderModelError {
        todo!()
    }
    fn FreeTexture(&self, _: *mut vr::RenderModel_TextureMap_t) {
        todo!()
    }
    fn LoadTexture_Async(
        &self,
        _: vr::TextureID_t,
        _: *mut *mut vr::RenderModel_TextureMap_t,
    ) -> vr::EVRRenderModelError {
        todo!()
    }
    fn FreeRenderModel(&self, _: *mut vr::RenderModel_t) {
        todo!()
    }
    fn LoadRenderModel_Async(
        &self,
        _: *const std::ffi::c_char,
        _: *mut *mut vr::RenderModel_t,
    ) -> vr::EVRRenderModelError {
        vr::EVRRenderModelError::NotSupported
    }
}
