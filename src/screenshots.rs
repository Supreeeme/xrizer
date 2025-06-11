use openvr as vr;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRScreenshots"]
#[versions(001)]
pub struct Screenshots {
    vtables: Vtables,
}

impl vr::IVRScreenshots001_Interface for Screenshots {
    fn SubmitScreenshot(
        &self,
        _: vr::ScreenshotHandle_t,
        _: vr::EVRScreenshotType,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
    ) -> vr::EVRScreenshotError {
        vr::EVRScreenshotError::IncompatibleVersion
    }
    fn TakeStereoScreenshot(
        &self,
        _: *mut vr::ScreenshotHandle_t,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
    ) -> vr::EVRScreenshotError {
        vr::EVRScreenshotError::IncompatibleVersion
    }
    fn UpdateScreenshotProgress(
        &self,
        _: vr::ScreenshotHandle_t,
        _: f32,
    ) -> vr::EVRScreenshotError {
        vr::EVRScreenshotError::IncompatibleVersion
    }
    fn GetScreenshotPropertyFilename(
        &self,
        _: vr::ScreenshotHandle_t,
        _: vr::EVRScreenshotPropertyFilenames,
        _: *mut std::os::raw::c_char,
        _: u32,
        err: *mut vr::EVRScreenshotError,
    ) -> u32 {
        if !err.is_null() {
            unsafe { *err = vr::EVRScreenshotError::IncompatibleVersion };
        }
        0
    }
    fn GetScreenshotPropertyType(
        &self,
        _: vr::ScreenshotHandle_t,
        err: *mut vr::EVRScreenshotError,
    ) -> vr::EVRScreenshotType {
        if !err.is_null() {
            unsafe { *err = vr::EVRScreenshotError::IncompatibleVersion };
        }
        vr::EVRScreenshotType::None
    }
    fn HookScreenshot(
        &self,
        _: *const vr::EVRScreenshotType,
        _: std::os::raw::c_int,
    ) -> vr::EVRScreenshotError {
        vr::EVRScreenshotError::None
    }
    fn RequestScreenshot(
        &self,
        _: *mut vr::ScreenshotHandle_t,
        _: vr::EVRScreenshotType,
        _: *const std::os::raw::c_char,
        _: *const std::os::raw::c_char,
    ) -> vr::EVRScreenshotError {
        vr::EVRScreenshotError::IncompatibleVersion
    }
}
