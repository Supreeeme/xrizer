mod convert;

pub use bindings::vr::*;
pub use bindings::{VkInstance_T, VkPhysicalDevice_T};
pub use convert::space_relation_to_openvr_pose;
use std::ffi::{c_void, CStr};
use std::sync::{Arc, Weak};

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

/// Types that are interfaces.
/// # Safety
///
/// Should only be implemented by generated code.
unsafe trait OpenVrInterface: 'static {
    type Vtable: Sync;
}

/// Trait for inheriting from an interface.
/// The thread safety/usage patterns of OpenVR interfaces is not clear, so we err on the safe side and require
/// inheritors to be Sync.
///
/// # Safety
///
/// should not be implemented by hand
#[allow(private_bounds)]
pub unsafe trait Inherits<T: OpenVrInterface>: Sync
where
    Self: Sized,
{
    fn new_wrapped(wrapped: &Arc<Self>) -> VtableWrapper<T, Self>;
    fn init_fntable(init: &Arc<Self>) -> *mut c_void;
}

/// A wrapper around a vtable, to safely pass across FFI.
#[repr(C)]
#[allow(private_bounds)]
pub struct VtableWrapper<T: OpenVrInterface, Wrapped> {
    pub base: T,
    wrapped: Weak<Wrapped>,
}

pub type InterfaceGetter<T> = Box<dyn FnOnce(&Arc<T>) -> *mut c_void>;
pub trait InterfaceImpl: Sync + Send + 'static {
    fn supported_versions() -> &'static [&'static CStr];
    /// Gets a specific interface version
    fn get_version(version: &CStr) -> Option<InterfaceGetter<Self>>;
}

#[allow(clippy::derivable_impls)]
impl Default for ETrackingResult {
    fn default() -> Self {
        Self::Uninitialized
    }
}

impl VRTextureBounds_t {
    #[inline]
    pub fn valid(&self) -> bool {
        matches!(
            self,
            VRTextureBounds_t {
                uMin: 0.0..=1.0,
                uMax: 0.0..=1.0,
                vMin: 0.0..=1.0,
                vMax: 0.0..=1.0
            }
        ) && self.vMin != self.vMax
            && self.uMin != self.uMax
    }

    #[inline]
    pub fn vertically_flipped(&self) -> bool {
        self.vMin > self.vMax
    }
}
