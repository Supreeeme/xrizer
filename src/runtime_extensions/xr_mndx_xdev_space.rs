#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::{
    ffi::{c_void, CStr},
    mem::transmute,
    ptr::{addr_of_mut, null_mut},
};

use log::info;

use openxr::AnyGraphics;

use crate::input::devices::generic_tracker::MAX_GENERIC_TRACKERS;

// Extension number 445 (444 prefix)
pub const XR_MNDX_XDEV_SPACE: i32 = 1;
pub const XR_MNDX_XDEV_SPACE_SPEC_VERSION: i32 = 1;
pub const XR_MNDX_XDEV_SPACE_EXTENSION_NAME: &'static str = "XR_MNDX_xdev_space";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct XrXDevListMNDX(u64);
pub type XrXDevIdMNDX = u64;

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct CustomStructureType(i32);
impl CustomStructureType {
    pub const XR_TYPE_SYSTEM_XDEV_SPACE_PROPERTIES_MNDX: CustomStructureType = Self(1000444001);
    pub const XR_TYPE_CREATE_XDEV_LIST_INFO_MNDX: CustomStructureType = Self(1000444002);
    pub const XR_TYPE_GET_XDEV_INFO_MNDX: CustomStructureType = Self(1000444003);
    pub const XR_TYPE_XDEV_PROPERTIES_MNDX: CustomStructureType = Self(1000444004);
    pub const XR_TYPE_CREATE_XDEV_SPACE_INFO_MNDX: CustomStructureType = Self(1000444005);
}

impl Into<openxr::sys::StructureType> for CustomStructureType {
    fn into(self) -> openxr::sys::StructureType {
        unsafe { std::mem::transmute(self) }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct XrSystemXDevSpacePropertiesMNDX {
    ty: openxr::sys::StructureType,
    next: *mut c_void,
    supports_xdev_space: openxr::sys::Bool32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct XrCreateXDevListInfoMNDX {
    ty: openxr::sys::StructureType,
    next: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct XrGetXDevInfoMNDX {
    ty: openxr::sys::StructureType,
    next: *mut c_void,
    id: XrXDevIdMNDX,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct XrXDevPropertiesMNDX {
    ty: openxr::sys::StructureType,
    next: *mut c_void,
    name: [i8; 256],
    serial: [i8; 256],
    can_create_space: openxr::sys::Bool32,
}

impl XrXDevPropertiesMNDX {
    pub fn name(&self) -> String {
        let name = unsafe { CStr::from_ptr(self.name.as_ptr()) };

        name.to_string_lossy().to_string()
    }

    pub fn serial(&self) -> String {
        let serial = unsafe { CStr::from_ptr(self.serial.as_ptr()) };

        serial.to_string_lossy().to_string()
    }

    pub fn can_create_space(&self) -> bool {
        self.can_create_space != openxr::sys::FALSE
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct XrCreateXDevSpaceInfoMNDX {
    ty: openxr::sys::StructureType,
    next: *mut c_void,
    xdev_list: XrXDevListMNDX,
    id: XrXDevIdMNDX,
    offset: openxr::sys::Posef,
}

pub type xrCreateXDevListMNDX = unsafe extern "system" fn(
    session: openxr::sys::Session,
    create_info: *const XrCreateXDevListInfoMNDX,
    xdev_list: *mut XrXDevListMNDX,
) -> openxr::sys::Result;

pub type xrGetXDevListGenerationNumberMNDX = unsafe extern "system" fn(
    xdev_list: XrXDevListMNDX,
    out_generation: *mut u64,
) -> openxr::sys::Result;

pub type xrEnumerateXDevsMNDX = unsafe extern "system" fn(
    xdev_list: XrXDevListMNDX,
    count_input: u32,
    count_output: *mut u32,
    xdevs: *mut XrXDevIdMNDX,
) -> openxr::sys::Result;

pub type xrGetXDevPropertiesMNDX = unsafe extern "system" fn(
    xdev_list: XrXDevListMNDX,
    info: *const XrGetXDevInfoMNDX,
    properties: *mut XrXDevPropertiesMNDX,
) -> openxr::sys::Result;

pub type xrDestroyXDevListMNDX =
    unsafe extern "system" fn(xdev_list: XrXDevListMNDX) -> openxr::sys::Result;

pub type xrCreateXDevSpaceMNDX = unsafe extern "system" fn(
    session: openxr::sys::Session,
    create_info: *const XrCreateXDevSpaceInfoMNDX,
    space: *mut openxr::sys::Space,
) -> openxr::sys::Result;

#[derive(Debug, Copy, Clone)]
pub struct XdevSpaceExtension {
    create_xdev_list_fn: Option<xrCreateXDevListMNDX>,
    get_xdev_list_generation_number_fn: Option<xrGetXDevListGenerationNumberMNDX>,
    enumerate_xdevs_fn: Option<xrEnumerateXDevsMNDX>,
    get_xdev_properties_fn: Option<xrGetXDevPropertiesMNDX>,
    destroy_xdev_list_fn: Option<xrDestroyXDevListMNDX>,
    create_xdev_space_fn: Option<xrCreateXDevSpaceMNDX>,
}

macro_rules! xr_bind {
    ($instance:expr, $name:expr, $function:expr) => {
        let res = openxr::sys::get_instance_proc_addr(
            $instance,
            CStr::from_bytes_until_nul($name).unwrap().as_ptr(),
            transmute(addr_of_mut!($function)),
        );
        if res != openxr::sys::Result::SUCCESS {
            return Err(res);
        }
    };
}

#[derive(Debug, Copy, Clone)]
pub struct Xdev {
    pub id: XrXDevIdMNDX,
    pub properties: XrXDevPropertiesMNDX,
    pub space: Option<openxr::sys::Space>,
}

impl XdevSpaceExtension {
    pub fn new(instance: &openxr::Instance) -> Result<Self, openxr::sys::Result> {
        unsafe {
            let mut s = Self {
                create_xdev_list_fn: None,
                get_xdev_list_generation_number_fn: None,
                enumerate_xdevs_fn: None,
                get_xdev_properties_fn: None,
                destroy_xdev_list_fn: None,
                create_xdev_space_fn: None,
            };

            xr_bind!(
                instance.as_raw(),
                b"xrCreateXDevListMNDX\0",
                s.create_xdev_list_fn
            );

            xr_bind!(
                instance.as_raw(),
                b"xrGetXDevListGenerationNumberMNDX\0",
                s.get_xdev_list_generation_number_fn
            );

            xr_bind!(
                instance.as_raw(),
                b"xrEnumerateXDevsMNDX\0",
                s.enumerate_xdevs_fn
            );

            xr_bind!(
                instance.as_raw(),
                b"xrGetXDevPropertiesMNDX\0",
                s.get_xdev_properties_fn
            );

            xr_bind!(
                instance.as_raw(),
                b"xrDestroyXDevListMNDX\0",
                s.destroy_xdev_list_fn
            );

            xr_bind!(
                instance.as_raw(),
                b"xrCreateXDevSpaceMNDX\0",
                s.create_xdev_space_fn
            );

            Ok(s)
        }
    }

    pub fn get_devices(
        &self,
        session: &openxr::Session<AnyGraphics>,
    ) -> Result<Vec<Xdev>, openxr::sys::Result> {
        let mut xdev_list = XrXDevListMNDX(0);
        let create_info = XrCreateXDevListInfoMNDX {
            ty: CustomStructureType::XR_TYPE_CREATE_XDEV_LIST_INFO_MNDX.into(),
            next: null_mut(),
        };

        let mut generic_tracker_id_count = 0;
        let mut generic_tracker_ids = vec![0; MAX_GENERIC_TRACKERS as usize];

        info!("Creating xdev list");
        self.create_xdev_list(session.as_raw(), &create_info, &mut xdev_list)?;
        info!("Created xdev list");
        self.enumerate_xdevs(
            xdev_list,
            MAX_GENERIC_TRACKERS,
            addr_of_mut!(generic_tracker_id_count),
            generic_tracker_ids.as_mut_ptr(),
        )?;
        generic_tracker_ids.truncate(generic_tracker_id_count as usize);

        info!("Found {} xdevs", generic_tracker_ids.len());

        let mut properties = XrXDevPropertiesMNDX {
            ty: CustomStructureType::XR_TYPE_XDEV_PROPERTIES_MNDX.into(),
            next: null_mut(),
            can_create_space: openxr::sys::FALSE,
            name: [0; 256],
            serial: [0; 256],
        };

        let mut get_info = XrGetXDevInfoMNDX {
            ty: CustomStructureType::XR_TYPE_GET_XDEV_INFO_MNDX.into(),
            next: null_mut(),
            id: 0,
        };

        let mut space_create_info = XrCreateXDevSpaceInfoMNDX {
            ty: CustomStructureType::XR_TYPE_CREATE_XDEV_SPACE_INFO_MNDX.into(),
            next: null_mut(),
            xdev_list,
            id: 0,
            offset: openxr::sys::Posef {
                orientation: openxr::sys::Quaternionf {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 1.0,
                },
                position: openxr::sys::Vector3f {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
            },
        };

        let xdevs = generic_tracker_ids
            .iter()
            .map(|&id| {
                get_info.id = id;
                self.get_xdev_properties(xdev_list, &get_info, &mut properties)?;

                let mut xdev = Xdev {
                    id,
                    properties,
                    space: None,
                };

                let mut space = openxr::sys::Space::default();

                if properties.can_create_space() {
                    space_create_info.id = id;
                    self.create_xdev_space(session.as_raw(), &space_create_info, &mut space)?;
                    xdev.space = Some(space);
                }

                Ok(xdev)
            })
            .collect::<Result<Vec<Xdev>, openxr::sys::Result>>()?;

        Ok(xdevs)
    }

    pub fn create_xdev_list(
        &self,
        session: openxr::sys::Session,
        create_info: *const XrCreateXDevListInfoMNDX,
        xdev_list: &mut XrXDevListMNDX,
    ) -> Result<(), openxr::sys::Result> {
        if self.create_xdev_list_fn.is_none() {
            return Err(openxr::sys::Result::ERROR_EXTENSION_NOT_PRESENT);
        }

        let res = unsafe { self.create_xdev_list_fn.unwrap()(session, create_info, xdev_list) };
        if res != openxr::sys::Result::SUCCESS {
            return Err(res);
        }

        Ok(())
    }

    pub fn get_xdev_list_generation_number(
        &self,
        xdev_list: XrXDevListMNDX,
        out_generation: *mut u64,
    ) -> Result<(), openxr::sys::Result> {
        if self.get_xdev_list_generation_number_fn.is_none() {
            return Err(openxr::sys::Result::ERROR_EXTENSION_NOT_PRESENT);
        }

        let res =
            unsafe { self.get_xdev_list_generation_number_fn.unwrap()(xdev_list, out_generation) };
        if res != openxr::sys::Result::SUCCESS {
            return Err(res);
        }

        Ok(())
    }

    pub fn enumerate_xdevs(
        &self,
        xdev_list: XrXDevListMNDX,
        count_input: u32,
        count_output: *mut u32,
        xdevs: *mut XrXDevIdMNDX,
    ) -> Result<(), openxr::sys::Result> {
        if self.enumerate_xdevs_fn.is_none() {
            return Err(openxr::sys::Result::ERROR_EXTENSION_NOT_PRESENT);
        }

        let res = unsafe {
            self.enumerate_xdevs_fn.unwrap()(xdev_list, count_input, count_output, xdevs)
        };
        if res != openxr::sys::Result::SUCCESS {
            return Err(res);
        }

        Ok(())
    }

    pub fn get_xdev_properties(
        &self,
        xdev_list: XrXDevListMNDX,
        info: *const XrGetXDevInfoMNDX,
        properties: *mut XrXDevPropertiesMNDX,
    ) -> Result<(), openxr::sys::Result> {
        if self.get_xdev_properties_fn.is_none() {
            return Err(openxr::sys::Result::ERROR_EXTENSION_NOT_PRESENT);
        }

        let res = unsafe { self.get_xdev_properties_fn.unwrap()(xdev_list, info, properties) };
        if res != openxr::sys::Result::SUCCESS {
            return Err(res);
        }

        Ok(())
    }

    pub fn destroy_xdev_list(&self, xdev_list: XrXDevListMNDX) -> Result<(), openxr::sys::Result> {
        if self.destroy_xdev_list_fn.is_none() {
            return Err(openxr::sys::Result::ERROR_EXTENSION_NOT_PRESENT);
        }

        let res = unsafe { self.destroy_xdev_list_fn.unwrap()(xdev_list) };
        if res != openxr::sys::Result::SUCCESS {
            return Err(res);
        }

        Ok(())
    }

    pub fn create_xdev_space(
        &self,
        session: openxr::sys::Session,
        create_info: *const XrCreateXDevSpaceInfoMNDX,
        space: *mut openxr::sys::Space,
    ) -> Result<(), openxr::sys::Result> {
        if self.create_xdev_space_fn.is_none() {
            return Err(openxr::sys::Result::ERROR_EXTENSION_NOT_PRESENT);
        }

        let res = unsafe { self.create_xdev_space_fn.unwrap()(session, create_info, space) };
        if res != openxr::sys::Result::SUCCESS {
            return Err(res);
        }

        Ok(())
    }
}
