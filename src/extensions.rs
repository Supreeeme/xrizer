// This file contains complete extension bindings, but only some of the bindings
// are used.
#![allow(dead_code)]

use std::ffi::CStr;
use std::mem;
use std::os::raw::c_void;

use openxr as xr;
use openxr::sys as xrsys;

fn convert_result(result: xrsys::Result) -> xr::Result<()> {
    if result.into_raw() == 0 {
        Ok(())
    } else {
        Err(result)
    }
}

unsafe fn get_instance_proc_addr(
    instance: &xr::Instance,
    name: &CStr,
) -> xr::Result<xrsys::pfn::VoidFunction> {
    let mut f = None;
    convert_result((instance.entry().fp().get_instance_proc_addr)(
        instance.as_raw(), name.as_ptr(), &mut f))?;
    Ok(f.unwrap())
}

#[derive(Clone, Debug, Default)]
pub struct ExtraExtensionSet {
    pub mnd_xdev_space: bool,
}

impl ExtraExtensionSet {
    pub fn to_vec(&self, names: &mut Vec<String>) {
        if self.mnd_xdev_space {
            names.push(XDevSpaceMNDX::NAME.to_owned());
        }
    }
}

impl From<&xr::ExtensionSet> for ExtraExtensionSet {
    fn from(set: &xr::ExtensionSet) -> Self {
        let mut extra = ExtraExtensionSet::default();

        for ext in &set.other {
            match ext.as_str() {
                XDevSpaceMNDX::NAME => extra.mnd_xdev_space = true,
                _ => {}
            }
        }

        extra
    }
}

#[derive(Default)]
pub struct ExtraExtensions {
    pub mnd_xdev_space: Option<XDevSpaceMNDX>,
}

impl ExtraExtensions {
    pub fn load(instance: &xr::Instance, set: &ExtraExtensionSet) -> xr::Result<ExtraExtensions> {
        let mut extra = ExtraExtensions::default();

        // SAFETY: We're only transmuting to types from the extension specifications.
        unsafe {
            if set.mnd_xdev_space {
                extra.mnd_xdev_space = Some(XDevSpaceMNDX::load(instance)?);
            }
        }

        Ok(extra)
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct XDevIdMNDX(pub u64);

impl XDevIdMNDX {
    pub const NULL: XDevIdMNDX = XDevIdMNDX(0);
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct XDevListMNDX(pub u64);

impl XDevListMNDX {
    pub const NULL: XDevListMNDX = XDevListMNDX(0);
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SystemXDevSpacePropertiesMNDX {
    pub ty: xrsys::StructureType,
    pub next: *mut c_void,
    pub supports_xdev_space: xrsys::Bool32,
}

impl SystemXDevSpacePropertiesMNDX {
    pub fn structure_type() -> xrsys::StructureType {
        xrsys::StructureType::from_raw(1000444001)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CreateXDevListInfoMNDX {
    pub ty: xrsys::StructureType,
    pub next: *mut c_void,
}

impl CreateXDevListInfoMNDX {
    pub fn structure_type() -> xrsys::StructureType {
        xrsys::StructureType::from_raw(1000444002)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct GetXDevInfoMNDX {
    pub ty: xrsys::StructureType,
    pub next: *const c_void,
    pub id: XDevIdMNDX,
}

impl GetXDevInfoMNDX {
    pub fn structure_type() -> xrsys::StructureType {
        xrsys::StructureType::from_raw(1000444003)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct XDevPropertiesMNDX {
    pub ty: xrsys::StructureType,
    pub next: *mut c_void,
    pub name: [u8; 256],
    pub serial: [u8; 256],
    pub can_create_space: xrsys::Bool32,
}

impl XDevPropertiesMNDX {
    pub fn structure_type() -> xrsys::StructureType {
        xrsys::StructureType::from_raw(1000444004)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CreateXDevSpaceInfoMNDX {
    pub ty: xrsys::StructureType,
    pub next: *const c_void,
    pub xdev_list: XDevListMNDX,
    pub id: XDevIdMNDX,
    pub offset: xr::Posef,
}

impl CreateXDevSpaceInfoMNDX {
    pub fn structure_type() -> xrsys::StructureType {
        xrsys::StructureType::from_raw(1000444005)
    }
}

pub type CreateXDevListMNDX = unsafe extern "system" fn(
    session: xrsys::Session,
    info: *const CreateXDevListInfoMNDX,
    xdev_list: *mut XDevListMNDX,
) -> xrsys::Result;

pub type GetXDevListGenerationNumberMNDX = unsafe extern "system" fn(
    xdev_list: XDevListMNDX,
    out_generation: *mut u64,
) -> xrsys::Result;

pub type EnumerateXDevsMNDX = unsafe extern "system" fn(
    xdev_list: XDevListMNDX,
    xdev_capacity_input: u32,
    xdev_count_output: *mut u32,
    xdevs: *mut XDevIdMNDX,
) -> xrsys::Result;

pub type GetXDevPropertiesMNDX = unsafe extern "system" fn(
    xdev_list: XDevListMNDX,
    info: *const GetXDevInfoMNDX,
    properties: *mut XDevPropertiesMNDX,
) -> xrsys::Result;

pub type DestroyXDevListMNDX = unsafe extern "system" fn(
    xdev_list: XDevListMNDX,
) -> xrsys::Result;

pub type CreateXDevSpaceMNDX = unsafe extern "system" fn(
    session: xrsys::Session,
    create_info: *const CreateXDevSpaceInfoMNDX,
    space: *mut xrsys::Space,
) -> xrsys::Result;

pub struct XDevList {
    ext: XDevSpaceMNDX,
    handle: XDevListMNDX,
}

impl XDevList {
    pub fn try_new<G>(session: &xr::Session<G>, ext: &XDevSpaceMNDX) -> xr::Result<XDevList> {
        let mut handle = XDevListMNDX::NULL;
        let info = CreateXDevListInfoMNDX {
            ty: CreateXDevListInfoMNDX::structure_type(),
            next: std::ptr::null_mut(),
        };

        // SAFETY: Only passing locally created pointers.
        convert_result(unsafe { (ext.create_xdev_list)(
            session.as_raw(), &info, &mut handle) })?;
        Ok(XDevList {
            ext: ext.clone(),
            handle,
        })
    }

    pub fn get_generation_number(&self) -> xr::Result<u64> {
        let mut gen = 0;
        convert_result(unsafe {
            (self.ext.get_xdev_list_generation_number)(self.handle, &mut gen)
        })?;
        Ok(gen)
    }

    pub fn enumerate(&self, devices: &mut [XDevIdMNDX]) -> xr::Result<usize> {
        let mut n = 0;
        convert_result(unsafe {
            (self.ext.enumerate_xdevs)(self.handle, devices.len() as u32, &mut n, devices.as_mut_ptr())
        })?;
        Ok(n as usize)
    }

    pub fn get_xdev_properties(&self, id: XDevIdMNDX) -> xr::Result<XDevPropertiesMNDX> {
        let info = GetXDevInfoMNDX {
            ty: GetXDevInfoMNDX::structure_type(),
            next: std::ptr::null(),
            id,
        };

        let mut properties = XDevPropertiesMNDX {
            ty: XDevPropertiesMNDX::structure_type(),
            next: std::ptr::null_mut(),
            name: [0; 256],
            serial: [0; 256],
            can_create_space: Default::default(),
        };
        convert_result(unsafe {
            (self.ext.get_xdev_properties)(self.handle, &info, &mut properties)
        })?;

        Ok(properties)
    }

    pub fn create_xdev_space<G>(
        &self,
        session: &xr::Session<G>,
        id: XDevIdMNDX,
        offset: xr::Posef,
    ) -> xr::Result<xr::Space> {
        let info = CreateXDevSpaceInfoMNDX {
            ty: CreateXDevSpaceInfoMNDX::structure_type(),
            next: std::ptr::null(),
            xdev_list: self.handle,
            id,
            offset,
        };

        let mut space = xrsys::Space::NULL;
        unsafe {
            convert_result((self.ext.create_xdev_space)(session.as_raw(), &info, &mut space))?;
            Ok(xr::Space::reference_from_raw(session.clone(), space))
        }
    }
}

impl Drop for XDevList {
    fn drop(&mut self) {
        // SAFETY: XDevList always has unique valid handle.
        unsafe {
            (self.ext.destroy_xdev_list)(self.handle);
        }
    }
}

#[derive(Copy, Clone)]
pub struct XDevSpaceMNDX {
    create_xdev_list: CreateXDevListMNDX,
    get_xdev_list_generation_number: GetXDevListGenerationNumberMNDX,
    enumerate_xdevs: EnumerateXDevsMNDX,
    get_xdev_properties: GetXDevPropertiesMNDX,
    destroy_xdev_list: DestroyXDevListMNDX,
    create_xdev_space: CreateXDevSpaceMNDX,
}

impl XDevSpaceMNDX {
    pub const NAME: &'static str = "XR_MNDX_xdev_space";

    /// Load the extension's function pointer table.
    ///
    /// # Safety
    /// `instance` must be a valid instance handle.
    pub unsafe fn load(instance: &xr::Instance) -> openxr::Result<Self> {
        Ok(Self {
            create_xdev_list: mem::transmute(get_instance_proc_addr(
                instance,
                CStr::from_bytes_with_nul_unchecked(b"xrCreateXDevListMNDX\0"),
            )?),
            get_xdev_list_generation_number: mem::transmute(get_instance_proc_addr(
                instance,
                CStr::from_bytes_with_nul_unchecked(b"xrGetXDevListGenerationNumberMNDX\0"),
            )?),
            enumerate_xdevs: mem::transmute(get_instance_proc_addr(
                instance,
                CStr::from_bytes_with_nul_unchecked(b"xrEnumerateXDevsMNDX\0"),
            )?),
            get_xdev_properties: mem::transmute(get_instance_proc_addr(
                instance,
                CStr::from_bytes_with_nul_unchecked(b"xrGetXDevPropertiesMNDX\0"),
            )?),
            destroy_xdev_list: mem::transmute(get_instance_proc_addr(
                instance,
                CStr::from_bytes_with_nul_unchecked(b"xrDestroyXDevListMNDX\0"),
            )?),
            create_xdev_space: mem::transmute(get_instance_proc_addr(
                instance,
                CStr::from_bytes_with_nul_unchecked(b"xrCreateXDevSpaceMNDX\0"),
            )?),
        })
    }
}
