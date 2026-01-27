use openvr as vr;
use std::ffi::c_char;

use log::info;
use std::process::Command;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRApplications"]
#[versions(007, 006, 005)]
pub struct Applications {
    vtables: Vtables,
}

impl vr::IVRApplications007_Interface for Applications {
    fn GetCurrentSceneProcessId(&self) -> u32 {
        todo!()
    }
    fn LaunchInternalProcess(
        &self,
        binary_path: *const c_char,
        arguments: *const c_char,
        working_directory: *const c_char,
    ) -> vr::EVRApplicationError {
        crate::warn_unimplemented!("LaunchInternalProcess");

        if binary_path.is_null() || arguments.is_null() || working_directory.is_null() {
            return vr::EVRApplicationError::InvalidParameter;
        }

        let binary_path = unsafe { std::ffi::CStr::from_ptr(binary_path) }
            .to_str()
            .unwrap_or("")
            .to_owned();
        let arguments = unsafe { std::ffi::CStr::from_ptr(arguments) }
            .to_str()
            .unwrap_or("")
            .to_owned();
        let working_directory = unsafe { std::ffi::CStr::from_ptr(working_directory) }
            .to_str()
            .unwrap_or("")
            .to_owned();

        info!(
            "LaunchInternalProcess called: {:?}, ARGS: {:?}, WD: {:?}",
            binary_path, arguments, working_directory
        );

        let process = Command::new(binary_path)
            .args(arguments.split_whitespace())
            .current_dir(working_directory)
            .spawn();

        match process {
            Ok(_) => vr::EVRApplicationError::None,
            Err(e) => {
                info!("Failed to launch internal process: {}", e);
                vr::EVRApplicationError::LaunchFailed
            }
        }
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
        crate::warn_unimplemented!("GetApplicationCount");
        0
    }
    fn IsApplicationInstalled(&self, _: *const c_char) -> bool {
        crate::warn_unimplemented!("IsApplicationInstalled");
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

impl vr::IVRApplications006On007 for Applications {
    fn GetTransitionState(&self) -> vr::EVRApplicationTransitionState {
        crate::warn_unimplemented!("GetTransitionState");
        vr::EVRApplicationTransitionState::None
    }
    fn GetApplicationsTransitionStateNameFromEnum(
        &self,
        state: vr::EVRApplicationTransitionState,
    ) -> *const ::std::os::raw::c_char {
        crate::warn_unimplemented!("GetApplicationsTransitionStateNameFromEnum");
        match state {
            vr::EVRApplicationTransitionState::None => c"None".as_ptr(),
            vr::EVRApplicationTransitionState::OldAppQuitSent => c"OldAppQuitSent".as_ptr(),
            vr::EVRApplicationTransitionState::WaitingForExternalLaunch => {
                c"WaitingForExternalLaunch".as_ptr()
            }
            vr::EVRApplicationTransitionState::NewAppLaunched => c"NewAppLaunched".as_ptr(),
        }
    }
    fn IsQuitUserPromptRequested(&self) -> bool {
        crate::warn_unimplemented!("IsQuitUserPromptRequested");
        false
    }
}
