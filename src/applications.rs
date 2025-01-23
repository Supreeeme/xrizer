use openvr as vr;
use std::ffi::c_char;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRApplications"]
#[versions(007)]
pub struct Applications {
    vtables: Vtables,
}

impl vr::IVRApplications007_Interface for Applications {
    fn GetCurrentSceneProcessId(&self) -> u32 {
        todo!()
    }
    fn LaunchInternalProcess(
        &self,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
    ) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetSceneApplicationStateNameFromEnum(
        &self,
        _: vr::EVRSceneApplicationState,
    ) -> *const c_char {
        todo!()
    }
    fn PerformApplicationPrelaunchCheck(&self, _: *const c_char) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetSceneApplicationState(&self) -> vr::EVRSceneApplicationState {
        todo!()
    }
    fn GetStartingApplication(&self, _: *mut c_char, _: u32) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetApplicationLaunchArguments(&self, _: u32, _: *mut c_char, _: u32) -> u32 {
        todo!()
    }
    fn GetApplicationsThatSupportMimeType(&self, _: *const c_char, _: *mut c_char, _: u32) -> u32 {
        todo!()
    }
    fn GetApplicationSupportedMimeTypes(&self, _: *const c_char, _: *mut c_char, _: u32) -> bool {
        todo!()
    }
    fn GetDefaultApplicationForMimeType(&self, _: *const c_char, _: *mut c_char, _: u32) -> bool {
        todo!()
    }
    fn SetDefaultApplicationForMimeType(
        &self,
        _: *const c_char,
        _: *const c_char,
    ) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetApplicationAutoLaunch(&self, _: *const c_char) -> bool {
        todo!()
    }
    fn SetApplicationAutoLaunch(&self, _: *const c_char, _: bool) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetApplicationPropertyUint64(
        &self,
        _: *const c_char,
        _: vr::EVRApplicationProperty,
        _: *mut vr::EVRApplicationError,
    ) -> u64 {
        todo!()
    }
    fn GetApplicationPropertyBool(
        &self,
        _: *const c_char,
        _: vr::EVRApplicationProperty,
        _: *mut vr::EVRApplicationError,
    ) -> bool {
        todo!()
    }
    fn GetApplicationPropertyString(
        &self,
        _: *const c_char,
        _: vr::EVRApplicationProperty,
        _: *mut c_char,
        _: u32,
        _: *mut vr::EVRApplicationError,
    ) -> u32 {
        todo!()
    }
    fn GetApplicationsErrorNameFromEnum(&self, _: vr::EVRApplicationError) -> *const c_char {
        todo!()
    }
    fn GetApplicationProcessId(&self, _: *const c_char) -> u32 {
        todo!()
    }
    fn IdentifyApplication(&self, _: u32, _: *const c_char) -> vr::EVRApplicationError {
        crate::warn_unimplemented!("IdentifyApplication");
        vr::EVRApplicationError::None
    }
    fn CancelApplicationLaunch(&self, _: *const c_char) -> bool {
        todo!()
    }
    fn LaunchDashboardOverlay(&self, _: *const c_char) -> vr::EVRApplicationError {
        todo!()
    }
    fn LaunchApplicationFromMimeType(
        &self,
        _: *const c_char,
        _: *const c_char,
    ) -> vr::EVRApplicationError {
        todo!()
    }
    fn LaunchTemplateApplication(
        &self,
        _: *const c_char,
        _: *const c_char,
        _: *const vr::AppOverrideKeys_t,
        _: u32,
    ) -> vr::EVRApplicationError {
        todo!()
    }
    fn LaunchApplication(&self, _: *const c_char) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetApplicationKeyByProcessId(
        &self,
        _: u32,
        _: *mut c_char,
        _: u32,
    ) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetApplicationKeyByIndex(&self, _: u32, _: *mut c_char, _: u32) -> vr::EVRApplicationError {
        todo!()
    }
    fn GetApplicationCount(&self) -> u32 {
        0
    }
    fn IsApplicationInstalled(&self, _: *const c_char) -> bool {
        false
    }
    fn RemoveApplicationManifest(&self, _: *const c_char) -> vr::EVRApplicationError {
        crate::warn_unimplemented!("RemoveApplicationManifest");
        vr::EVRApplicationError::None
    }
    fn AddApplicationManifest(&self, _: *const c_char, _: bool) -> vr::EVRApplicationError {
        crate::warn_unimplemented!("AddApplicationManifest");
        vr::EVRApplicationError::None
    }
}
