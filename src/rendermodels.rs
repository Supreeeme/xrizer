use openvr as vr;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRRenderModels"]
#[versions(006, 005)]
pub struct RenderModels {
    vtables: Vtables,
}

#[allow(non_snake_case)]
impl vr::IVRRenderModels006_Interface for RenderModels {
    fn GetRenderModelErrorNameFromEnum(
        &self,
        _: vr::EVRRenderModelError,
    ) -> *const std::os::raw::c_char {
        c"<unknown>".as_ptr()
    }
    fn GetRenderModelOriginalPath(
        &self,
        _: *const std::os::raw::c_char,
        _: *mut std::os::raw::c_char,
        _: u32,
        _: *mut vr::EVRRenderModelError,
    ) -> u32 {
        todo!()
    }
    fn GetRenderModelThumbnailURL(
        &self,
        _: *const std::os::raw::c_char,
        _: *mut std::os::raw::c_char,
        _: u32,
        _: *mut vr::EVRRenderModelError,
    ) -> u32 {
        todo!()
    }
    fn RenderModelHasComponent(
        &self,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
    ) -> bool {
        todo!()
    }
    fn GetComponentState(
        &self,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
        _: *const vr::VRControllerState_t,
        _: *const vr::RenderModel_ControllerMode_State_t,
        _: *mut vr::RenderModel_ComponentState_t,
    ) -> bool {
        crate::warn_unimplemented!("GetComponentState");
        false
    }
    fn GetComponentStateForDevicePath(
        &self,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
        _: vr::VRInputValueHandle_t,
        _: *const vr::RenderModel_ControllerMode_State_t,
        _: *mut vr::RenderModel_ComponentState_t,
    ) -> bool {
        crate::warn_unimplemented!("GetComponentStateForDevicePath");
        false
    }
    fn GetComponentRenderModelName(
        &self,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
        _: *mut std::os::raw::c_char,
        _: u32,
    ) -> u32 {
        todo!()
    }
    fn GetComponentButtonMask(
        &self,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
    ) -> u64 {
        crate::warn_unimplemented!("GetComponentButtonMask");
        0
    }
    fn GetComponentName(
        &self,
        _: *const std::os::raw::c_char,
        _: u32,
        _: *mut std::os::raw::c_char,
        _: u32,
    ) -> u32 {
        todo!()
    }
    fn GetComponentCount(&self, _: *const std::os::raw::c_char) -> u32 {
        crate::warn_unimplemented!("GetComponentCount");
        0
    }
    fn GetRenderModelCount(&self) -> u32 {
        crate::warn_unimplemented!("GetRenderModelCount");
        0
    }
    fn GetRenderModelName(&self, _: u32, _: *mut std::os::raw::c_char, _: u32) -> u32 {
        todo!()
    }
    fn FreeTextureD3D11(&self, _: *mut std::os::raw::c_void) {
        todo!()
    }
    fn LoadIntoTextureD3D11_Async(
        &self,
        _: vr::TextureID_t,
        _: *mut std::os::raw::c_void,
    ) -> vr::EVRRenderModelError {
        todo!()
    }
    fn LoadTextureD3D11_Async(
        &self,
        _: vr::TextureID_t,
        _: *mut std::os::raw::c_void,
        _: *mut *mut std::os::raw::c_void,
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
        _: *const std::os::raw::c_char,
        _: *mut *mut vr::RenderModel_t,
    ) -> vr::EVRRenderModelError {
        vr::EVRRenderModelError::NotSupported
    }
}
