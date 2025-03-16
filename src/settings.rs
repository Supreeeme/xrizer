use log::debug;
use openvr as vr;
use openvr::EVRSettingsError;
use std::ffi::CStr;
use std::os::raw::c_char;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRSettings"]
#[versions(003)]
pub struct Settings {
    vtables: Vtables,
}

impl vr::IVRSettings003_Interface for Settings {
    fn GetSettingsErrorNameFromEnum(&self, error: EVRSettingsError) -> *const c_char {
        #[allow(unreachable_patterns)]
        let error: &'static CStr = match error {
            EVRSettingsError::None => c"",
            EVRSettingsError::IPCFailed => c"IPC Failed",
            EVRSettingsError::WriteFailed => c"Write Failed",
            EVRSettingsError::ReadFailed => c"Read Failed",
            EVRSettingsError::JsonParseFailed => c"JSON Parse Failed",
            EVRSettingsError::UnsetSettingHasNoDefault => c"Unset setting has no default",
            EVRSettingsError::AccessDenied => c"Access denied",
            _ => c"Unknown error",
        };
        error.as_ptr()
    }

    fn SetBool(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        value: bool,
        error: *mut EVRSettingsError,
    ) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        debug!("Setting bool on {section}/{key} to {value}");
        unsafe {
            *error = EVRSettingsError::None;
        }
    }

    fn SetInt32(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        value: i32,
        error: *mut EVRSettingsError,
    ) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        debug!("Setting int on {section}/{key} to {value}");
        unsafe {
            *error = EVRSettingsError::None;
        }
    }

    fn SetFloat(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        value: f32,
        error: *mut EVRSettingsError,
    ) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        debug!("Setting float on {section}/{key} to {value}");
        unsafe {
            *error = EVRSettingsError::None;
        }
    }

    fn SetString(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        value: *const c_char,
        error: *mut EVRSettingsError,
    ) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        let value = unsafe { CStr::from_ptr(value) }.to_string_lossy();
        debug!("Setting string on {section}/{key} to {value}");
        unsafe {
            *error = EVRSettingsError::None;
        }
    }

    fn GetBool(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        error: *mut EVRSettingsError,
    ) -> bool {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        unsafe {
            *error = EVRSettingsError::None;
        }
        debug!("Getting bool on {section}/{key}");
        false
    }

    fn GetInt32(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        error: *mut EVRSettingsError,
    ) -> i32 {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        unsafe {
            *error = EVRSettingsError::None;
        }
        debug!("Getting int on {section}/{key}");
        0
    }

    fn GetFloat(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        error: *mut EVRSettingsError,
    ) -> f32 {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        unsafe {
            *error = EVRSettingsError::None;
        }
        debug!("Getting float on {section}/{key}");
        0.0
    }

    fn GetString(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        value: *mut c_char,
        value_len: u32,
        error: *mut EVRSettingsError,
    ) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        unsafe {
            *error = EVRSettingsError::None;
        }
        if value_len > 0 {
            unsafe {
                *value = 0;
            }
        }
        debug!("Getting string on {section}/{key}");
    }

    fn RemoveSection(&self, section: *const c_char, error: *mut EVRSettingsError) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        unsafe {
            *error = EVRSettingsError::None;
        }
        debug!("Removing section {section}");
    }

    fn RemoveKeyInSection(
        &self,
        section: *const c_char,
        settings_key: *const c_char,
        error: *mut EVRSettingsError,
    ) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        let key = unsafe { CStr::from_ptr(settings_key) }.to_string_lossy();
        unsafe {
            *error = EVRSettingsError::None;
        }
        debug!("Removing {section}/{key}");
    }
}
