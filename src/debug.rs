use log::debug;
use openvr as vr;
use std::ffi::CStr;
use std::os::raw::c_char;

use openvr::EVRDebugError;
use openvr::VrProfilerEventHandle_t;
use openvr::TrackedDeviceIndex_t;

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRDebug"]
#[versions(001)]
pub struct Debug {
    vtables: Vtables,
}

impl vr::IVRDebug001_Interface for Debug {
    fn EmitVrProfilerEvent(&self, message: *const c_char) -> EVRDebugError {
        let message = unsafe { CStr::from_ptr(message) }.to_string_lossy();
        debug!("Emitting VR profiler event: {message}");
        EVRDebugError::Success
    }

    fn BeginVrProfilerEvent(&self, handle_out: *mut VrProfilerEventHandle_t) -> EVRDebugError {
        debug!("Beginning VR profiler event");
        unsafe {
            *handle_out = 1;
        }
        EVRDebugError::Success
    }

    fn FinishVrProfilerEvent(&self, handle: VrProfilerEventHandle_t, message: *const c_char) -> EVRDebugError {
        let message = unsafe { CStr::from_ptr(message) }.to_string_lossy();
        debug!("Finishing VR profiler event {handle}: {message}");
        EVRDebugError::Success
    }

    fn DriverDebugRequest(
        &self, 
        device_index: TrackedDeviceIndex_t, 
        request: *const c_char, 
        response_buffer: *mut c_char, 
        response_buffer_size: u32
    ) -> u32 {
        let request = unsafe { CStr::from_ptr(request) }.to_string_lossy();
        debug!("Driver debug request for device {device_index}: {request}");
        
        if response_buffer_size == 0 {
            return 0;
        }
        
        unsafe {
            *response_buffer = 0;
        }
        
        // Return 1 for the null terminator
        1
    }
}