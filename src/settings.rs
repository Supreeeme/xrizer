use log::info;
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
        let error: &'static str = match error {
            EVRSettingsError::None => "",
            EVRSettingsError::IPCFailed => "IPC Failed",
            EVRSettingsError::WriteFailed => "Write Failed",
            EVRSettingsError::ReadFailed => "Read Failed",
            EVRSettingsError::JsonParseFailed => "JSON Parse Failed",
            EVRSettingsError::UnsetSettingHasNoDefault => "Unset setting has no default",
            EVRSettingsError::AccessDenied => "Access denied",
            _ => "Unknown error",
        };
        error.as_ptr() as *const c_char
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
        info!("Setting bool on {section}/{key} to {value}");
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
        info!("Setting int on {section}/{key} to {value}");
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
        info!("Setting float on {section}/{key} to {value}");
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
        info!("Setting string on {section}/{key} to {value}");
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
        info!("Getting bool on {section}/{key}");
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
        info!("Getting int on {section}/{key}");
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
        info!("Getting float on {section}/{key}");
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
        info!("Getting string on {section}/{key}");
    }

    fn RemoveSection(&self, section: *const c_char, error: *mut EVRSettingsError) {
        let section = unsafe { CStr::from_ptr(section) }.to_string_lossy();
        unsafe {
            *error = EVRSettingsError::None;
        }
        info!("Removing section {section}");
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
        info!("Removing {section}/{key}");
    }
}
