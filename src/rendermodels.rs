use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    io::Read,
    ptr,
    str::FromStr,
    sync::Mutex,
};

use glam::Vec3;
use log::{debug, warn};
use lz4_flex::frame::FrameDecoder;
use obj::{IndexTuple, ObjData};
use openvr as vr;
use openxr as xr;

// all render models will use this color
const RENDER_MODEL_COLOR: [u8; 4] = [32, 32, 32, 255];

// use a single static component for all rendermodels
static DEFAULT_COMPONENT: &CStr = c"xrizer_component";

struct RenderModelObjLz4 {
    /// lz4-compressed obj data
    bytes: &'static [u8],
    flipped: bool,
}

static RENDER_MODELS: &[(&str, RenderModelObjLz4)] = {
    let oculus_quest2_controller_left =
        include_bytes!("../resources/rendermodels/oculus_quest2_controller_left.obj.lz4");
    let oculus_quest_plus_controller_left =
        include_bytes!("../resources/rendermodels/oculus_quest_plus_controller_left.obj.lz4");
    let valve_controller_knu_1_0_left =
        include_bytes!("../resources/rendermodels/valve_controller_knu_1_0_left.obj.lz4");
    let vive_focus3_controller_left =
        include_bytes!("../resources/rendermodels/vive_focus3_controller_left.obj.lz4");

    // must be in strict alphabetical order due to binary-search lookup
    &[
        (
            "generic_controller",
            RenderModelObjLz4 {
                bytes: include_bytes!("../resources/rendermodels/generic_controller.obj.lz4"),
                flipped: false,
            },
        ),
        (
            "oculus_quest2_controller_left",
            RenderModelObjLz4 {
                bytes: oculus_quest2_controller_left,
                flipped: false,
            },
        ),
        (
            "oculus_quest2_controller_right",
            RenderModelObjLz4 {
                bytes: oculus_quest2_controller_left,
                flipped: true,
            },
        ),
        (
            "oculus_quest_plus_controller_left",
            RenderModelObjLz4 {
                bytes: oculus_quest_plus_controller_left,
                flipped: false,
            },
        ),
        (
            "oculus_quest_plus_controller_right",
            RenderModelObjLz4 {
                bytes: oculus_quest_plus_controller_left,
                flipped: true,
            },
        ),
        (
            "valve_controller_knu_1_0_left",
            RenderModelObjLz4 {
                bytes: valve_controller_knu_1_0_left,
                flipped: false,
            },
        ),
        (
            "valve_controller_knu_1_0_right",
            RenderModelObjLz4 {
                bytes: valve_controller_knu_1_0_left,
                flipped: true,
            },
        ),
        (
            "vive_focus3_controller_left",
            RenderModelObjLz4 {
                bytes: vive_focus3_controller_left,
                flipped: false,
            },
        ),
        (
            "vive_focus3_controller_right",
            RenderModelObjLz4 {
                bytes: vive_focus3_controller_left,
                flipped: true,
            },
        ),
        (
            "vr_controller_vive_1_5",
            RenderModelObjLz4 {
                bytes: include_bytes!("../resources/rendermodels/vr_controller_vive_1_5.obj.lz4"),
                flipped: false,
            },
        ),
        (
            "vr_tracker_vive_3_0",
            RenderModelObjLz4 {
                bytes: include_bytes!("../resources/rendermodels/vr_tracker_vive_3_0.obj.lz4"),
                flipped: false,
            },
        ),
    ]
};

fn get_render_model_data(render_model_name: &CStr) -> Option<&'static RenderModelObjLz4> {
    let name = render_model_name.to_str().ok().or_else(|| {
        warn!(
            "render_model_name is not a valid UTF-8 string: {:?}",
            render_model_name
        );
        None
    })?;

    let model_name = name
        .strip_prefix('{')
        .and_then(|s| s.split_once('}').map(|(_driver, model)| model))
        .unwrap_or(name);

    if let Ok(found_idx) = RENDER_MODELS.binary_search_by_key(&model_name, |(a, _)| *a) {
        Some(&RENDER_MODELS[found_idx].1)
    } else {
        None
    }
}

#[derive(Default, macros::InterfaceImpl)]
#[interface = "IVRRenderModels"]
#[versions(006, 005, 004)]
pub struct RenderModels {
    vtables: Vtables,
    data: Mutex<RenderModelData>,
}

#[derive(Default)]
struct RenderModelData {
    loaded_model_data: HashMap<usize, OwnedRenderModel>,
    loaded_textures: HashMap<usize, OwnedTexture>,
}

#[allow(non_snake_case)]
impl vr::IVRRenderModels006_Interface for RenderModels {
    fn GetRenderModelErrorNameFromEnum(
        &self,
        _: vr::EVRRenderModelError,
    ) -> *const std::ffi::c_char {
        c"<unknown>".as_ptr()
    }
    fn GetRenderModelOriginalPath(
        &self,
        _: *const std::ffi::c_char,
        _: *mut std::ffi::c_char,
        _: u32,
        _: *mut vr::EVRRenderModelError,
    ) -> u32 {
        todo!()
    }
    fn GetRenderModelThumbnailURL(
        &self,
        _: *const std::ffi::c_char,
        _: *mut std::ffi::c_char,
        _: u32,
        _: *mut vr::EVRRenderModelError,
    ) -> u32 {
        todo!()
    }
    fn RenderModelHasComponent(
        &self,
        render_model_name: *const std::ffi::c_char,
        component_name: *const std::ffi::c_char,
    ) -> bool {
        if render_model_name.is_null() || component_name.is_null() {
            debug!(
                "RenderModelHasComponent called with null ptr(s): rm={render_model_name:?} comp={component_name:?}"
            );
            return false;
        }

        let render_model_name = unsafe { CStr::from_ptr(render_model_name) };
        let component_name = unsafe { CStr::from_ptr(component_name) };

        component_name == DEFAULT_COMPONENT && get_render_model_data(render_model_name).is_some()
    }
    fn GetComponentState(
        &self,
        render_model_name: *const std::ffi::c_char,
        component_name: *const std::ffi::c_char,
        _: *const vr::VRControllerState_t,
        _: *const vr::RenderModel_ControllerMode_State_t,
        state: *mut vr::RenderModel_ComponentState_t,
    ) -> bool {
        if render_model_name.is_null() || component_name.is_null() || state.is_null() {
            debug!(
                "GetComponentState called with null ptr(s): rm={render_model_name:?} comp={component_name:?} state={state:?}"
            );
            return false;
        }

        let render_model_name = unsafe { CStr::from_ptr(render_model_name) };
        let component_name = unsafe { CStr::from_ptr(component_name) };

        if component_name != DEFAULT_COMPONENT || get_render_model_data(render_model_name).is_none()
        {
            // unsupported model or component
            return false;
        }

        // all of our models are static and have offsets baked in
        let identity = xr::Posef::IDENTITY;
        let identity = vr::HmdMatrix34_t::from(identity);

        unsafe {
            (*state).mTrackingToComponentRenderModel = identity;
            (*state).mTrackingToComponentLocal = identity;
            (*state).uProperties =
                (vr::EVRComponentProperty::IsVisible | vr::EVRComponentProperty::IsStatic).0;
        }

        true
    }
    fn GetComponentStateForDevicePath(
        &self,
        render_model_name: *const std::ffi::c_char,
        component_name: *const std::ffi::c_char,
        _: vr::VRInputValueHandle_t,
        _: *const vr::RenderModel_ControllerMode_State_t,
        state: *mut vr::RenderModel_ComponentState_t,
    ) -> bool {
        // static models only, don't care about state
        self.GetComponentState(
            render_model_name,
            component_name,
            ptr::null(),
            ptr::null(),
            state,
        )
    }
    fn GetComponentRenderModelName(
        &self,
        render_model_name: *const std::ffi::c_char,
        component_name: *const std::ffi::c_char,
        component_render_model_name: *mut std::ffi::c_char,
        component_render_model_name_len: u32,
    ) -> u32 {
        if render_model_name.is_null() || component_name.is_null() {
            debug!(
                "GetComponentRenderModelName called with null ptr(s): rm={render_model_name:?} comp={component_name:?}"
            );
            return 0;
        }

        let render_model_name = unsafe { CStr::from_ptr(render_model_name) };
        let component_name = unsafe { CStr::from_ptr(component_name) };

        if component_name != DEFAULT_COMPONENT || get_render_model_data(render_model_name).is_none()
        {
            // unsupported model or component
            return 0;
        }

        cstr_write_out(
            render_model_name,
            component_render_model_name,
            component_render_model_name_len,
        )
    }

    fn GetComponentButtonMask(
        &self,
        _: *const std::ffi::c_char,
        _: *const std::ffi::c_char,
    ) -> u64 {
        crate::warn_unimplemented!("GetComponentButtonMask");
        0
    }
    fn GetComponentName(
        &self,
        render_model_name: *const std::ffi::c_char,
        component_index: u32,
        component_name: *mut std::ffi::c_char,
        component_name_len: u32,
    ) -> u32 {
        if render_model_name.is_null() {
            return 0;
        }

        // even for unknown render models, always act as if there's a valid component
        // needed for titles like Derail Valley to function
        if component_index > 0 {
            return 0;
        }
        cstr_write_out(DEFAULT_COMPONENT, component_name, component_name_len)
    }
    fn GetComponentCount(&self, render_model_name: *const std::ffi::c_char) -> u32 {
        if render_model_name.is_null() {
            return 0;
        }
        let render_model_name = unsafe { CStr::from_ptr(render_model_name) };
        if render_model_name.count_bytes() == 0 {
            0
        } else {
            1 // DEFAULT_COMPONENT
        }
    }
    fn GetRenderModelCount(&self) -> u32 {
        RENDER_MODELS.len() as _
    }
    fn GetRenderModelName(
        &self,
        index: u32,
        render_model_name: *mut std::ffi::c_char,
        render_model_name_len: u32,
    ) -> u32 {
        let Some((render_model, _)) = RENDER_MODELS.get(index as usize) else {
            return 0;
        };

        let Ok(cstr) = CString::from_str(render_model) else {
            return 0;
        };

        cstr_write_out(&cstr, render_model_name, render_model_name_len)
    }
    fn FreeTextureD3D11(&self, _: *mut std::ffi::c_void) {
        todo!()
    }
    fn LoadIntoTextureD3D11_Async(
        &self,
        _: vr::TextureID_t,
        _: *mut std::ffi::c_void,
    ) -> vr::EVRRenderModelError {
        crate::warn_unimplemented!("LoadIntoTextureD3D11_Async");
        vr::EVRRenderModelError::NotSupported
    }
    fn LoadTextureD3D11_Async(
        &self,
        _: vr::TextureID_t,
        _: *mut std::ffi::c_void,
        _: *mut *mut std::ffi::c_void,
    ) -> vr::EVRRenderModelError {
        crate::warn_unimplemented!("LoadTextureD3D11_Async");
        vr::EVRRenderModelError::NotSupported
    }
    fn FreeTexture(&self, texture_map: *mut vr::RenderModel_TextureMap_t) {
        if texture_map.is_null() {
            return;
        }

        let removed = self
            .data
            .lock()
            .unwrap()
            .loaded_textures
            .remove(&(texture_map as usize));

        // only free if it was ours
        if removed.is_some() {
            unsafe {
                drop(Box::from_raw(texture_map));
            }
        } else {
            warn!("FreeTexture called with unknown pointer {texture_map:?}");
        }
    }
    fn LoadTexture_Async(
        &self,
        tex_id: vr::TextureID_t,
        out_texture_map: *mut *mut vr::RenderModel_TextureMap_t,
    ) -> vr::EVRRenderModelError {
        if out_texture_map.is_null() {
            warn!("LoadTexture_Async called with null out_texture_map");
            return vr::EVRRenderModelError::InvalidArg;
        }

        unsafe {
            *out_texture_map = std::ptr::null_mut();
        }

        if tex_id < 0 {
            warn!("LoadTexture_Async called with invalid tex_id {tex_id:?}");
            return vr::EVRRenderModelError::InvalidTexture;
        }

        let tex = OwnedTexture::new_dummy();
        let texture_map_t = tex.to_texture_map_t();
        let ptr = Box::into_raw(texture_map_t);

        self.data
            .lock()
            .unwrap()
            .loaded_textures
            .insert(ptr as usize, tex);

        unsafe {
            *out_texture_map = ptr;
        }

        vr::EVRRenderModelError::None
    }
    fn FreeRenderModel(&self, render_model: *mut vr::RenderModel_t) {
        if render_model.is_null() {
            return;
        }

        let removed = {
            let mut data = self.data.lock().unwrap();
            data.loaded_model_data.remove(&(render_model as usize))
        };

        // only free if it was ours
        if removed.is_some() {
            unsafe {
                drop(Box::from_raw(render_model));
            }
        } else {
            warn!("FreeRenderModel called with unknown pointer {render_model:?}");
        }
    }
    fn LoadRenderModel_Async(
        &self,
        render_model_name: *const std::ffi::c_char,
        out_render_model: *mut *mut vr::RenderModel_t,
    ) -> vr::EVRRenderModelError {
        if out_render_model.is_null() {
            warn!("LoadRenderModel_Async called with null out_render_model");
            return vr::EVRRenderModelError::InvalidArg;
        }
        unsafe {
            *out_render_model = std::ptr::null_mut();
        }

        if render_model_name.is_null() {
            warn!("LoadRenderModel_Async called with null render_model_name");
            return vr::EVRRenderModelError::InvalidArg;
        }

        let render_model_name = unsafe { CStr::from_ptr(render_model_name) };
        let Some(bytes) = get_render_model_data(render_model_name) else {
            warn!("render model name {render_model_name:?} did not resolve to a valid resource");
            return vr::EVRRenderModelError::InvalidArg;
        };

        let model_data = match OwnedRenderModel::load(render_model_name, bytes) {
            Ok(m) => m,
            Err(e) => {
                warn!("failed to load rendermodel {render_model_name:?}: {e:?}");
                return e;
            }
        };

        let render_model_box = model_data.to_render_model_t();
        let ptr = Box::into_raw(render_model_box);

        {
            let mut data = self.data.lock().unwrap();
            data.loaded_model_data.insert(ptr as usize, model_data);
        }

        unsafe {
            *out_render_model = ptr;
        }
        vr::EVRRenderModelError::None
    }
}

#[allow(dead_code)]
struct OwnedTexture {
    width: u16,
    height: u16,
    data: Box<[u8]>,
}

impl OwnedTexture {
    /// creates a 1x1 dummy texture
    fn new_dummy() -> Self {
        let data = Box::new(RENDER_MODEL_COLOR);
        Self {
            data,
            width: 1,
            height: 1,
        }
    }
    fn to_texture_map_t(&self) -> Box<vr::RenderModel_TextureMap_t> {
        Box::new(vr::RenderModel_TextureMap_t {
            unWidth: self.width,
            unHeight: self.height,
            unMipLevels: 1,
            rubTextureMapData: self.data.as_ptr(),
            format: openvr::EVRRenderModelTextureFormat::RGBA8_SRGB,
        })
    }
}

struct OwnedRenderModel {
    verts: Box<[vr::RenderModel_Vertex_t]>,
    indices: Box<[u16]>,
}

impl OwnedRenderModel {
    fn load(
        name: &CStr,
        model_data_obj_lz4: &RenderModelObjLz4,
    ) -> Result<Self, vr::EVRRenderModelError> {
        let mut decompressed_bytes = vec![];
        FrameDecoder::new(model_data_obj_lz4.bytes)
            .read_to_end(&mut decompressed_bytes)
            .map_err(|e| {
                warn!("could not decompress model {name:?}: {e:?}");
                vr::EVRRenderModelError::InvalidModel
            })?;

        let obj = ObjData::load_buf(decompressed_bytes.as_slice()).map_err(|e| {
            warn!("OBJ load failed for {name:?}: {e:?}");
            vr::EVRRenderModelError::InvalidModel
        })?;

        Self::new_from_obj(&obj, model_data_obj_lz4.flipped)
    }

    fn new_from_obj(obj: &ObjData, flipped: bool) -> Result<Self, vr::EVRRenderModelError> {
        let mut verts: Vec<vr::RenderModel_Vertex_t> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let mut vertex_map: HashMap<IndexTuple, u16> = HashMap::new();

        let mut map_vertex_index = |idx: IndexTuple| -> Result<u16, vr::EVRRenderModelError> {
            if let Some(&i) = vertex_map.get(&idx) {
                return Ok(i);
            }

            let mut position = *obj
                .position
                .get(idx.0)
                .ok_or(vr::EVRRenderModelError::InvalidModel)?;

            if flipped {
                position[0] *= -1.0;
            }

            // not needed as we don't use textures
            let texcoord = [0.; 2];

            // will generate normals later
            let normal = [0.0, 0.0, 0.0];

            let new_index =
                u16::try_from(verts.len()).map_err(|_| vr::EVRRenderModelError::TooManyVertices)?;

            verts.push(vr::RenderModel_Vertex_t {
                vPosition: vr::HmdVector3_t { v: position },
                vNormal: vr::HmdVector3_t { v: normal },
                rfTextureCoord: texcoord,
            });

            vertex_map.insert(idx, new_index);
            Ok(new_index)
        };

        for object in &obj.objects {
            for group in &object.groups {
                for poly in &group.polys {
                    let poly_indices = &poly.0;

                    if poly_indices.len() < 3 {
                        continue;
                    }

                    if poly_indices.len() > 4 {
                        warn!(
                            "OBJ has {}-gon; using fan triangulation (may be wrong for concave faces)",
                            poly_indices.len()
                        );
                    }

                    let i0 = map_vertex_index(poly_indices[0])?;

                    for i in 1..(poly_indices.len() - 1) {
                        let i1 = map_vertex_index(poly_indices[i])?;
                        let i2 = map_vertex_index(poly_indices[i + 1])?;

                        if flipped {
                            // reverse winding to keep normals/front-faces correct
                            indices.push(i0);
                            indices.push(i2);
                            indices.push(i1);
                        } else {
                            indices.push(i0);
                            indices.push(i1);
                            indices.push(i2);
                        }
                    }
                }
            }
        }

        // calculate normals
        let mut accum = vec![Vec3::ZERO; verts.len()];

        for tri in indices.chunks_exact(3) {
            let ia = tri[0] as usize;
            let ib = tri[1] as usize;
            let ic = tri[2] as usize;

            let a = Vec3::from_array(verts[ia].vPosition.v);
            let b = Vec3::from_array(verts[ib].vPosition.v);
            let c = Vec3::from_array(verts[ic].vPosition.v);

            // area-weighted face normal
            let n = (b - a).cross(c - a);

            // skip degenerate triangles
            if n.length_squared() == 0.0 {
                continue;
            }

            accum[ia] += n;
            accum[ib] += n;
            accum[ic] += n;
        }

        for (v, n) in verts.iter_mut().zip(accum.into_iter()) {
            let n = if n.length_squared() > 0.0 {
                n.normalize()
            } else {
                Vec3::Z // fallback
            };
            v.vNormal = vr::HmdVector3_t { v: n.to_array() };
        }

        Ok(Self {
            verts: verts.into_boxed_slice(),
            indices: indices.into_boxed_slice(),
        })
    }

    fn to_render_model_t(&self) -> Box<vr::RenderModel_t> {
        Box::new(vr::RenderModel_t {
            rVertexData: self.verts.as_ptr(),
            unVertexCount: self.verts.len() as _,
            rIndexData: self.indices.as_ptr(),
            unTriangleCount: (self.indices.len() / 3) as _,
            diffuseTextureId: 0, // dummy texture
        })
    }
}

fn cstr_write_out(cstr: &CStr, out_ptr: *mut std::ffi::c_char, out_len: u32) -> u32 {
    let bytes = unsafe { std::slice::from_raw_parts(cstr.as_ptr(), cstr.count_bytes() + 1) };
    if out_ptr.is_null() {
        return bytes.len() as u32; // querying required buffer length
    }
    if out_len < bytes.len() as u32 {
        return 0; // doesn't fit
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out_ptr, out_len as usize) };
    out[..bytes.len()].copy_from_slice(bytes);

    bytes.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use vr::IVRRenderModels006_Interface;

    #[test]
    fn render_models_are_strictly_alphabetical() {
        let keys: Vec<&str> = RENDER_MODELS.iter().map(|(k, _)| *k).collect();

        let mut sorted = keys.clone();
        sorted.sort_unstable();

        sorted.dedup();
        assert_eq!(
            sorted.len(),
            keys.len(),
            "Duplicate keys in RENDER_MODELS: {:#?}",
            keys
        );

        let mut sorted_no_dedup = keys.clone();
        sorted_no_dedup.sort_unstable();
        assert_eq!(
            keys, sorted_no_dedup,
            "RENDER_MODELS is not in alphabetical order.\nExpected:\n{:#?}\nGot:\n{:#?}",
            sorted_no_dedup, keys
        );
    }

    #[test]
    fn get_render_model_data_able_to_resolve_all_elements() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_bytes(bytes: &[u8]) -> u64 {
            let mut h = DefaultHasher::new();
            bytes.hash(&mut h);
            h.finish()
        }

        for (name, expected) in RENDER_MODELS {
            let asset_name = CString::new(*name).unwrap();
            let asset =
                get_render_model_data(asset_name.as_c_str()).expect("plain name should resolve");
            assert_eq!(
                asset.bytes.len(),
                expected.bytes.len(),
                "resolved bytes should match embedded resource length for {name}"
            );
            assert_eq!(
                hash_bytes(asset.bytes),
                hash_bytes(expected.bytes),
                "resolved bytes should point at the embedded resource for {name}"
            );

            let asset_name = CString::new(format!("{{some_driver}}{name}")).unwrap();
            let asset =
                get_render_model_data(asset_name.as_c_str()).expect("braced name should resolve");
            assert_eq!(
                asset.bytes.len(),
                expected.bytes.len(),
                "braced name should resolve to the same embedded resource length for {name}"
            );
            assert_eq!(
                hash_bytes(asset.bytes),
                hash_bytes(expected.bytes),
                "braced name should resolve to the same embedded resource for {name}"
            );
        }
    }

    #[test]
    fn cstr_write_out_contract() {
        // no pointer → no deref, return required size
        let s = c"hello";
        let required = cstr_write_out(s, ptr::null_mut(), 0);
        assert_eq!(required, s.to_bytes_with_nul().len() as u32);

        // pointer, but buffer too small → no deref, return 0
        let mut small = vec![0 as std::ffi::c_char; (required as usize).saturating_sub(1)];
        let wrote = cstr_write_out(s, small.as_mut_ptr(), small.len() as u32);
        assert_eq!(wrote, 0);

        // pointer with enhough buf size → deref, return size
        let mut buf = vec![0 as std::ffi::c_char; required as usize];
        let wrote = cstr_write_out(s, buf.as_mut_ptr(), buf.len() as u32);
        assert_eq!(wrote, required);

        let out = unsafe { CStr::from_ptr(buf.as_ptr()) };
        assert_eq!(out.to_bytes_with_nul(), s.to_bytes_with_nul());
    }

    #[test]
    fn render_model_count_and_iter_name() {
        let rm = RenderModels::default();

        assert_eq!(rm.GetRenderModelCount() as usize, RENDER_MODELS.len());

        for (idx, (expected, _bytes)) in RENDER_MODELS.iter().enumerate() {
            let required = rm.GetRenderModelName(idx as u32, ptr::null_mut(), 0);
            assert!(required > 0);

            let mut buf = vec![0 as std::ffi::c_char; required as usize];
            let wrote = rm.GetRenderModelName(idx as u32, buf.as_mut_ptr(), buf.len() as u32);
            assert_eq!(wrote, required);

            let out = unsafe { CStr::from_ptr(buf.as_ptr()) };
            assert_eq!(out.to_str().unwrap(), *expected);
            assert_eq!(out.to_bytes_with_nul().len(), required as usize);
        }

        // out of bounds → 0
        let mut buf = [0 as std::ffi::c_char; 8];
        assert_eq!(
            rm.GetRenderModelName(
                RENDER_MODELS.len() as u32,
                buf.as_mut_ptr(),
                buf.len() as u32
            ),
            0
        );
    }

    #[test]
    fn get_component_count_contract() {
        let rm = RenderModels::default();
        let some_model = CString::new(RENDER_MODELS[0].0).unwrap();

        // return 0 if render_model_name is null
        assert_eq!(rm.GetComponentCount(ptr::null()), 0);

        let empty_cstr = c"";

        // return 0 for empty cstr
        assert_eq!(rm.GetComponentCount(empty_cstr.as_ptr()), 0);

        // return 1 component for non-null render_model_name (Derail Valley fix)
        assert_eq!(rm.GetComponentCount(some_model.as_ptr()), 1);
    }

    #[test]
    fn get_component_name_contract() {
        let rm = RenderModels::default();
        let some_model = CString::new(RENDER_MODELS[0].0).unwrap();
        let mut buf = vec![32];

        // return 0 if render_model_name is null
        assert_eq!(rm.GetComponentName(ptr::null(), 0, ptr::null_mut(), 0), 0);

        // return 0 if render_model_name is null, even if out ptr is non-null
        assert_eq!(
            rm.GetComponentName(ptr::null(), 0, buf.as_mut_ptr(), buf.len() as u32),
            0
        );

        // component_index 0 returns DEFAULT_COMPONENT
        let required = rm.GetComponentName(some_model.as_ptr(), 0, ptr::null_mut(), 0);
        assert!(required > 0);
        let mut buf = vec![0 as std::ffi::c_char; required as usize];
        let wrote = rm.GetComponentName(some_model.as_ptr(), 0, buf.as_mut_ptr(), buf.len() as u32);
        assert_eq!(wrote, required);
        let out = unsafe { CStr::from_ptr(buf.as_ptr()) };
        assert_eq!(out, DEFAULT_COMPONENT);

        // component_index > 0 returns 0
        let mut buf2 = [0 as std::ffi::c_char; 32];
        assert_eq!(
            rm.GetComponentName(ptr::null(), 1, buf2.as_mut_ptr(), buf2.len() as u32),
            0
        );
    }

    #[test]
    fn render_model_has_component_contract() {
        let rm = RenderModels::default();
        let some_model = CString::new(RENDER_MODELS[0].0).unwrap();

        // true for known model + DEFAULT_COMPONENT
        assert!(rm.RenderModelHasComponent(some_model.as_ptr(), DEFAULT_COMPONENT.as_ptr()));

        // false for unknown component
        assert!(!rm.RenderModelHasComponent(some_model.as_ptr(), c"not_xrizer_component".as_ptr()));

        // false for unknown model
        assert!(!rm.RenderModelHasComponent(c"unknown_model".as_ptr(), DEFAULT_COMPONENT.as_ptr()));
    }

    #[test]
    fn component_render_model_name_contract() {
        let rm = RenderModels::default();
        let some_model = CString::new(RENDER_MODELS[0].0).unwrap();
        let mut buf = Vec::with_capacity(64);

        // return 0 if render_model_name is null
        assert_eq!(
            rm.GetComponentRenderModelName(
                ptr::null(),
                DEFAULT_COMPONENT.as_ptr(),
                ptr::null_mut(),
                0
            ),
            0
        );

        // return 0 if component_name is null
        assert_eq!(
            rm.GetComponentRenderModelName(some_model.as_ptr(), ptr::null(), ptr::null_mut(), 0),
            0
        );

        // return 0 if render_model_name is null, even if out ptr is non-null
        assert_eq!(
            rm.GetComponentRenderModelName(
                ptr::null(),
                DEFAULT_COMPONENT.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() as u32
            ),
            0
        );

        // return 0 if component_name is null, even if out ptr is non-null
        assert_eq!(
            rm.GetComponentRenderModelName(
                some_model.as_ptr(),
                ptr::null(),
                buf.as_mut_ptr(),
                buf.len() as u32
            ),
            0
        );

        // valid component echoes back the render_model_name
        let required = rm.GetComponentRenderModelName(
            some_model.as_ptr(),
            DEFAULT_COMPONENT.as_ptr(),
            ptr::null_mut(),
            0,
        );
        assert!(required > 0);
        let mut buf = vec![0 as std::ffi::c_char; required as usize];
        let wrote = rm.GetComponentRenderModelName(
            some_model.as_ptr(),
            DEFAULT_COMPONENT.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as u32,
        );
        assert_eq!(wrote, required);
        let out = unsafe { CStr::from_ptr(buf.as_ptr()) };
        assert_eq!(out, some_model.as_c_str());

        // invalid component returns 0
        assert_eq!(
            rm.GetComponentRenderModelName(
                some_model.as_ptr(),
                c"not_xrizer_component".as_ptr(),
                ptr::null_mut(),
                0
            ),
            0
        );

        // unknown model returns 0
        assert_eq!(
            rm.GetComponentRenderModelName(
                c"unknown_model".as_ptr(),
                DEFAULT_COMPONENT.as_ptr(),
                ptr::null_mut(),
                0
            ),
            0
        );
    }

    #[test]
    fn get_component_state_contract() {
        let rm = RenderModels::default();
        let some_model = CString::new(RENDER_MODELS[0].0).unwrap();

        // null safety
        let mut state = unsafe { std::mem::zeroed::<vr::RenderModel_ComponentState_t>() };
        assert!(!rm.GetComponentState(
            ptr::null(),
            DEFAULT_COMPONENT.as_ptr(),
            ptr::null(),
            ptr::null(),
            &mut state
        ));
        assert!(!rm.GetComponentState(
            some_model.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            &mut state
        ));
        assert!(!rm.GetComponentState(
            some_model.as_ptr(),
            DEFAULT_COMPONENT.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut()
        ));

        // valid call
        let mut state = unsafe { std::mem::zeroed::<vr::RenderModel_ComponentState_t>() };
        let ok = rm.GetComponentState(
            some_model.as_ptr(),
            DEFAULT_COMPONENT.as_ptr(),
            ptr::null(),
            ptr::null(),
            &mut state,
        );
        assert!(ok);

        let ident = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        assert_eq!(state.mTrackingToComponentRenderModel.m, ident);
        assert_eq!(state.mTrackingToComponentLocal.m, ident);

        let visible = vr::EVRComponentProperty::IsVisible.0;
        let is_static = vr::EVRComponentProperty::IsStatic.0;
        assert_ne!(state.uProperties & visible, 0);
        assert_ne!(state.uProperties & is_static, 0);
    }

    #[test]
    fn load_and_free_texture_contract() {
        let rm = RenderModels::default();

        // null out ptr → InvalidArg
        assert_eq!(
            rm.LoadTexture_Async(0, ptr::null_mut()),
            vr::EVRRenderModelError::InvalidArg
        );

        // negative tex id → InvalidTexture
        let mut out: *mut vr::RenderModel_TextureMap_t = ptr::null_mut();
        assert_eq!(
            rm.LoadTexture_Async(-1, &mut out as *mut _),
            vr::EVRRenderModelError::InvalidTexture
        );
        assert!(out.is_null());

        // valid
        let mut out: *mut vr::RenderModel_TextureMap_t = ptr::null_mut();
        assert_eq!(
            rm.LoadTexture_Async(0, &mut out as *mut _),
            vr::EVRRenderModelError::None
        );
        assert!(!out.is_null());

        {
            let data = rm.data.lock().unwrap();
            assert!(data.loaded_textures.contains_key(&(out as usize)));
        }

        rm.FreeTexture(out);

        {
            let data = rm.data.lock().unwrap();
            assert!(!data.loaded_textures.contains_key(&(out as usize)));
        }

        // freeing already freed element → noop
        rm.FreeTexture(out);

        // does not dereference garbage pointer
        let garbage_pointer = 111111usize as *mut vr::RenderModel_TextureMap_t;
        rm.FreeTexture(garbage_pointer);
    }

    #[test]
    fn load_and_free_render_model_contract() {
        let rm = RenderModels::default();
        let some_model = CString::new(RENDER_MODELS[0].0).unwrap();

        // null out ptr → InvalidArg
        assert_eq!(
            rm.LoadRenderModel_Async(some_model.as_ptr(), ptr::null_mut()),
            vr::EVRRenderModelError::InvalidArg
        );

        // null name → InvalidArg
        let mut out: *mut vr::RenderModel_t = ptr::null_mut();
        assert_eq!(
            rm.LoadRenderModel_Async(ptr::null(), &mut out as *mut _),
            vr::EVRRenderModelError::InvalidArg
        );

        // unknown name → InvalidArg
        let mut out: *mut vr::RenderModel_t = ptr::null_mut();
        assert_eq!(
            rm.LoadRenderModel_Async(c"unknown_model".as_ptr(), &mut out as *mut _),
            vr::EVRRenderModelError::InvalidArg
        );
        assert!(out.is_null());

        // valid
        let name = RENDER_MODELS[0].0;
        let braced = CString::new(format!("{{driver}}{name}")).unwrap();
        let mut out: *mut vr::RenderModel_t = ptr::null_mut();
        assert_eq!(
            rm.LoadRenderModel_Async(braced.as_ptr(), &mut out as *mut _),
            vr::EVRRenderModelError::None
        );
        assert!(!out.is_null());

        {
            let data = rm.data.lock().unwrap();
            assert!(data.loaded_model_data.contains_key(&(out as usize)));
        }

        rm.FreeRenderModel(out);

        {
            let data = rm.data.lock().unwrap();
            assert!(!data.loaded_model_data.contains_key(&(out as usize)));
        }

        // freeing already freed element → noop
        rm.FreeRenderModel(out);

        // does not dereference garbage pointer
        let garbage_pointer = 111111usize as *mut vr::RenderModel_t;
        rm.FreeRenderModel(garbage_pointer);
    }

    #[test]
    fn embedded_models_parse_and_have_reasonable_geometry() {
        for (name, asset) in RENDER_MODELS {
            let c_name = CString::new(*name).unwrap();
            let model = OwnedRenderModel::load(c_name.as_c_str(), asset)
                .unwrap_or_else(|e| panic!("failed to load embedded model {name}: {e:?}"));

            assert!(!model.verts.is_empty(), "{name}: has no vertex buffer");
            assert!(!model.indices.is_empty(), "{name}: has no index buffer");
            assert_eq!(
                model.indices.len() % 3,
                0,
                "{name}: indices not a multiple of 3"
            );

            let vlen = model.verts.len();
            assert!(
                model.indices.iter().all(|&i| (i as usize) < vlen),
                "{name}: found out-of-range index"
            );

            for (vi, v) in model.verts.iter().enumerate() {
                let p = v.vPosition.v;
                let n = v.vNormal.v;

                assert!(
                    p.iter().all(|f| f.is_finite()),
                    "{name}: vertex {vi} has non-finite position"
                );
                assert!(
                    n.iter().all(|f| f.is_finite()),
                    "{name}: vertex {vi} has non-finite normal"
                );

                let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
                assert!(
                    (len2 - 1.0).abs() <= 1e-2,
                    "{name}: vertex {vi} normal not ~unit length (len^2={len2})"
                );
            }

            // RenderModel_t contract: triangle count matches indices/3
            let rm_t = model.to_render_model_t();
            assert_eq!(
                rm_t.unTriangleCount as usize,
                model.indices.len() / 3,
                "{name}: triangle count mismatch"
            );
            assert_eq!(
                rm_t.unVertexCount as usize,
                model.verts.len(),
                "{name}: vertex count mismatch"
            );
        }
    }
}
