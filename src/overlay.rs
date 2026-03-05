use crate::{
    clientcore::{Injected, Injector},
    compositor::{Compositor, is_usable_swapchain},
    graphics_backends::{GraphicsBackend, SupportedBackend, supported_apis_enum},
    openxr_data::{GraphicalSession, OpenXrData, Session, SessionData},
};
use glam::{Quat, Vec3, vec3};
use log::{debug, trace};
use openvr as vr;
use openxr as xr;
use slotmap::{Key, KeyData, SecondaryMap, SlotMap, new_key_type};
use std::f32::consts::{FRAC_1_SQRT_2, PI};
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, Mutex, RwLock};
use std::{collections::HashMap, ops::Deref};

// OpenVR overlays are allowed to use ≥ 0
pub const SKYBOX_Z_ORDER: i64 = -1;

#[derive(macros::InterfaceImpl)]
#[interface = "IVROverlay"]
#[versions(
    028, 027, 026, 025, 024, 021, 020, 019, 018, 017, 016, 014, 013, 012, 011, 010, 007
)]
pub struct OverlayMan {
    vtables: Vtables,
    openxr: Arc<OpenXrData<Compositor>>,
    /// should only be externally accessed for testing
    pub(crate) compositor: Injected<Compositor>,
    overlays: RwLock<SlotMap<OverlayKey, Overlay>>,
    key_to_overlay: RwLock<HashMap<CString, OverlayKey>>,
    skybox: RwLock<Vec<OverlayKey>>,
}

#[derive(derive_more::Deref)]
struct RealSessionData<'a>(std::sync::RwLockReadGuard<'a, std::mem::ManuallyDrop<SessionData>>);

impl OverlayMan {
    pub fn new(openxr: Arc<OpenXrData<Compositor>>, injector: &Injector) -> Self {
        Self {
            vtables: Vtables::default(),
            openxr,
            compositor: injector.inject(),
            overlays: Default::default(),
            key_to_overlay: Default::default(),
            skybox: Default::default(),
        }
    }

    fn get_real_session_data(
        &self,
        texture: &vr::Texture_t,
        bounds: vr::VRTextureBounds_t,
    ) -> Result<RealSessionData<'_>, vr::EVROverlayError> {
        if !SupportedBackend::is_texture_type_supported(texture.eType) {
            log::warn!("Unsupported texture type: {:?}", texture.eType);
            return Err(vr::EVROverlayError::InvalidTexture);
        }

        if !self.openxr.session_data.get().is_real_session()
            && self
                .compositor
                .get()
                .expect("Need to restart session, but compositor hasn't been set up...")
                .initialize_real_session(texture, bounds)
                .is_err()
        {
            Err(vr::EVROverlayError::InvalidTexture)
        } else {
            Ok(RealSessionData(self.openxr.session_data.get()))
        }
    }

    pub fn set_skybox(&self, textures: &[vr::Texture_t]) -> Result<(), vr::EVRCompositorError> {
        // We don't yet follow HMD position, so the skybox needs to be
        // big enough so that the user never leaves it
        const SKYBOX_SIZE: f32 = 500.0;

        self.clear_skybox();

        let mut overlays = self.overlays.write().unwrap();
        let mut skybox = self.skybox.write().unwrap();

        match textures.len() {
            1..=2 => {
                // only single equirect supported for now, ignore any 2nd one
                let texture = textures.first().unwrap();
                let name = CString::new("__xrizer_skybox").unwrap();
                let key = overlays.insert(Overlay::new(name.clone(), name));
                let overlay = overlays.get_mut(key).unwrap();

                self.get_real_session_data(texture, overlay.bounds)
                    .and_then(|data| overlay.set_texture(key, data, *texture))
                    .map_err(|_| vr::EVRCompositorError::InvalidTexture)?;
                overlay.visible = true;
                overlay.width = SKYBOX_SIZE; // for equirect this becomes radius
                overlay.kind = OverlayKind::Sphere;
                overlay.z_order = SKYBOX_Z_ORDER;
                skybox.push(key);
            }
            6 => {
                for (idx, texture) in textures.iter().enumerate() {
                    // 6 quads forming a cursed box
                    let name = CString::new(format!("__xrizer_skybox_{idx}")).unwrap();
                    let key = overlays.insert(Overlay::new(name.clone(), name));
                    let overlay = overlays.get_mut(key).unwrap();
                    self.get_real_session_data(texture, overlay.bounds)
                        .and_then(|data| overlay.set_texture(key, data, *texture))
                        .map_err(|_| vr::EVRCompositorError::InvalidTexture)?;
                    overlay.visible = true;
                    overlay.width = SKYBOX_SIZE * 2.0;
                    overlay.kind = OverlayKind::Quad;
                    overlay.z_order = SKYBOX_Z_ORDER;

                    #[rustfmt::skip]
                    const QUAD_POSES: [xr::Posef; 6] = [
                        xr::Posef { // front
                            position: xr::Vector3f { x: 0.0, y: 0.0, z: -SKYBOX_SIZE },
                            orientation: xr::Quaternionf { x: 0.0, y: 0.0, z: 1.0, w: 0.0 },
                        },
                        xr::Posef { // back
                            position: xr::Vector3f { x: 0.0, y: 0.0, z: SKYBOX_SIZE },
                            orientation: xr::Quaternionf { x: 1.0, y: 0.0, z: 0.0, w: 0.0 },
                        },
                        xr::Posef { // left
                            position: xr::Vector3f { x: -SKYBOX_SIZE, y: 0.0, z: 0.0 },
                            orientation: xr::Quaternionf { x: FRAC_1_SQRT_2, y: 0.0, z: FRAC_1_SQRT_2, w: 0.0 },
                        },
                        xr::Posef { // right
                            position: xr::Vector3f { x: SKYBOX_SIZE, y: 0.0, z: 0.0 },
                            orientation: xr::Quaternionf { x: -FRAC_1_SQRT_2, y: 0.0, z: FRAC_1_SQRT_2, w: 0.0 },
                        },
                        xr::Posef { // up
                            position: xr::Vector3f { x: 0.0, y: SKYBOX_SIZE, z: 0.0 },
                            orientation: xr::Quaternionf {x: 0.0, y: -FRAC_1_SQRT_2, z: FRAC_1_SQRT_2, w: 0.0 },
                        },
                        xr::Posef { // down
                            position: xr::Vector3f { x: 0.0, y: -SKYBOX_SIZE, z: 0.0 },
                            orientation: xr::Quaternionf {x: 0.0, y: FRAC_1_SQRT_2, z: FRAC_1_SQRT_2, w: 0.0 },
                        },
                    ];

                    overlay.transform = Some((
                        vr::ETrackingUniverseOrigin::Standing,
                        QUAD_POSES[idx].into(),
                    ));

                    skybox.push(key);
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    pub fn clear_skybox(&self) {
        let mut overlays = self.overlays.write().unwrap();
        self.skybox.write().unwrap().drain(..).for_each(|key| {
            overlays.remove(key);
        });
    }

    pub fn get_layers<'a, G: xr::Graphics>(
        &self,
        session: &'a SessionData,
        render_skybox: bool,
    ) -> Vec<OverlayLayer<'a, G>>
    where
        for<'b> &'b AnySwapchainMap: TryInto<&'b SwapchainMap<G>, Error: std::fmt::Display>,
    {
        let mut overlays = self.overlays.write().unwrap();
        let swapchains = session.overlay_data.swapchains.lock().unwrap();
        let Some(swapchains) = swapchains.as_ref() else {
            return Vec::new();
        };
        let swapchains: &SwapchainMap<G> = swapchains.try_into().unwrap_or_else(|e| {
            panic!(
                "Requested layers for API {}, but overlays are using a different API - {e}",
                std::any::type_name::<G>()
            )
        });

        let mut layers = Vec::with_capacity(overlays.len());
        for (key, overlay) in overlays.iter_mut() {
            if !overlay.visible {
                continue;
            }
            if overlay.z_order == SKYBOX_Z_ORDER && !render_skybox {
                continue;
            }
            let Some(rect) = overlay.rect else {
                continue;
            };

            let SwapchainData { swapchain, .. } = swapchains.get(key).unwrap();
            let space = session.get_space_for_origin(
                overlay
                    .transform
                    .as_ref()
                    .map(|(o, _)| *o)
                    .unwrap_or(session.current_origin),
            );

            trace!("overlay rect: {rect:#?}");

            let pose = overlay
                .transform
                .as_ref()
                .map(|(_, t)| (*t).into())
                .unwrap_or(xr::Posef {
                    position: xr::Vector3f {
                        x: 0.0,
                        y: 0.0,
                        z: -0.5,
                    },
                    orientation: xr::Quaternionf::IDENTITY,
                });

            macro_rules! layer_init {
                ($ty:ident) => {{
                    $ty::new()
                        .space(space)
                        .layer_flags(
                            xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA
                                | xr::CompositionLayerFlags::UNPREMULTIPLIED_ALPHA,
                        )
                        .eye_visibility(xr::EyeVisibility::BOTH)
                        .sub_image(
                            xr::SwapchainSubImage::new()
                                .image_array_index(vr::EVREye::Left as u32)
                                .swapchain(swapchain)
                                .image_rect(rect),
                        )
                }};
            }

            macro_rules! lifetime_extend {
                ($ty:ident, $layer:expr) => {{
                    fn lifetime_extend<'a, 'b: 'a, G: xr::Graphics>(
                        layer: $ty<'a, G>,
                    ) -> $ty<'b, G> {
                        // SAFETY: We need to remove the lifetimes to be able to return this layer.
                        // Internally, CompositionLayerQuad is using the raw OpenXR handles and PhantomData, not actual
                        // references, so returning it as long as we can guarantee the lifetimes of the space and
                        // swapchain is fine. Both of these are derived from the SessionData,
                        // so we should have no lifetime problems.
                        unsafe { $ty::from_raw(layer.into_raw()) }
                    }

                    lifetime_extend($layer)
                }}
            }

            match overlay.kind {
                OverlayKind::Quad => {
                    use xr::CompositionLayerQuad;
                    let layer = layer_init!(CompositionLayerQuad)
                        .pose(pose)
                        .size(xr::Extent2Df {
                            width: overlay.width,
                            height: rect.extent.height as f32 * overlay.width
                                / rect.extent.width as f32,
                        });

                    let layer = lifetime_extend!(CompositionLayerQuad, layer);
                    let mut layer = OverlayLayer::from(OverlayLayerInner::Quad(layer));
                    overlay.alpha.iter().for_each(|a| layer.set_alpha(*a));
                    layers.push((overlay.z_order, layer));
                }
                // SetOverlayCurvature checks for khr_composition_layer_cylinder
                OverlayKind::Curved { curvature } => {
                    let radius = overlay.width / (2.0 * PI * curvature);
                    let pos = vec3(pose.position.x, pose.position.y, pose.position.z);
                    let rot = Quat::from_xyzw(
                        pose.orientation.x,
                        pose.orientation.y,
                        pose.orientation.z,
                        pose.orientation.w,
                    );

                    let center = pos + rot.mul_vec3(Vec3::Z * radius);
                    let angle = 2.0 * (overlay.width / (2.0 * radius));

                    use xr::CompositionLayerCylinderKHR;
                    let layer = layer_init!(CompositionLayerCylinderKHR)
                        .radius(radius)
                        .central_angle(angle)
                        .aspect_ratio(rect.extent.height as f32 / rect.extent.width as f32)
                        .pose(xr::Posef {
                            orientation: pose.orientation,
                            position: xr::Vector3f {
                                x: center.x,
                                y: center.y,
                                z: center.z,
                            },
                        });

                    let layer = lifetime_extend!(CompositionLayerCylinderKHR, layer);
                    let mut layer = OverlayLayer::from(OverlayLayerInner::Cylinder(layer));
                    overlay.alpha.iter().for_each(|a| layer.set_alpha(*a));
                    layers.push((overlay.z_order, layer));
                }
                // SetSkyboxOverride checks for khr_composition_layer_equirect2
                OverlayKind::Sphere => {
                    const HORIZONTAL_RAD: f32 = 2.0 * PI;
                    const VERTICAL_RAD_HIGH: f32 = 0.5 * PI;
                    const VERTICAL_RAD_LOW: f32 = -0.5 * PI;

                    use xr::CompositionLayerEquirect2KHR;
                    let layer = layer_init!(CompositionLayerEquirect2KHR)
                        .radius(overlay.width)
                        .central_horizontal_angle(HORIZONTAL_RAD)
                        .upper_vertical_angle(VERTICAL_RAD_HIGH)
                        .lower_vertical_angle(VERTICAL_RAD_LOW)
                        .pose(pose);

                    let layer = lifetime_extend!(CompositionLayerEquirect2KHR, layer);
                    let mut layer = OverlayLayer::from(OverlayLayerInner::Equirect2(layer));
                    overlay.alpha.iter().for_each(|a| layer.set_alpha(*a));
                    layers.push((overlay.z_order, layer));
                }
            }
        }

        // Sort by z_order asc
        layers.sort_by(|a, b| a.0.cmp(&b.0));

        let sorted_layers: Vec<OverlayLayer<_>> = layers.into_iter().map(|(_, l)| l).collect();

        trace!("returning {} layers", sorted_layers.len());
        sorted_layers
    }

    pub fn get_next_overlay_event(
        &self,
        _event: *mut vr::VREvent_t,
    ) -> Option<vr::VROverlayHandle_t> {
        // TODO: go through overlay handles and grab the next event.
        None
    }
}

pub struct OverlayLayer<'a, G: xr::Graphics> {
    /// Only ever None during next_chain_insert
    layer: Option<OverlayLayerInner<'a, G>>,
    color_bias_khr: Option<Box<xr::sys::CompositionLayerColorScaleBiasKHR>>,
}

impl<G: xr::Graphics> OverlayLayer<'_, G> {
    fn set_alpha(&mut self, alpha: f32) {
        // only one instance is stored, so this would cause segfault due to UAF
        debug_assert!(
            self.color_bias_khr.is_none(),
            "attempted to set_alpha on the same CompositorLayer twice!"
        );

        self.color_bias_khr = {
            let mut payload = Box::new(xr::sys::CompositionLayerColorScaleBiasKHR {
                ty: xr::StructureType::COMPOSITION_LAYER_COLOR_SCALE_BIAS_KHR,
                next: std::ptr::null(),
                color_bias: Default::default(),
                color_scale: xr::Color4f {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: alpha,
                },
            });

            let payload_ptr = payload.as_mut() as *mut _ as *mut xr::sys::BaseInStructure;
            unsafe { self.next_chain_insert(payload_ptr) };

            Some(payload)
        };
    }

    /// Insert the given item as the first element in the next chain.
    /// `item` must be a non-null pointer to a valid XrBaseInStructure object
    ///
    /// SAFETY: For lifetime guarantees, store item in Box inside CompositorLayer.
    unsafe fn next_chain_insert(&mut self, item: *mut xr::sys::BaseInStructure) {
        unsafe {
            let new_elem = item.as_mut().unwrap();
            self.layer = Some(match self.layer.take().unwrap() {
                OverlayLayerInner::Quad(quad) => {
                    let mut raw = quad.into_raw();
                    new_elem.next = raw.next as _;
                    raw.next = item as *const _;
                    OverlayLayerInner::Quad(xr::CompositionLayerQuad::from_raw(raw))
                }
                OverlayLayerInner::Cylinder(cylinder) => {
                    let mut raw = cylinder.into_raw();
                    new_elem.next = raw.next as _;
                    raw.next = item as *const _;
                    OverlayLayerInner::Cylinder(xr::CompositionLayerCylinderKHR::from_raw(raw))
                }
                OverlayLayerInner::Equirect2(equirect2) => {
                    let mut raw = equirect2.into_raw();
                    new_elem.next = raw.next as _;
                    raw.next = item as *const _;
                    OverlayLayerInner::Equirect2(xr::CompositionLayerEquirect2KHR::from_raw(raw))
                }
            });
        }
    }
}

impl<'a, G: xr::Graphics> From<OverlayLayerInner<'a, G>> for OverlayLayer<'a, G> {
    fn from(value: OverlayLayerInner<'a, G>) -> Self {
        Self {
            layer: Some(value),
            color_bias_khr: None,
        }
    }
}

impl<'a, G: xr::Graphics> Deref for OverlayLayer<'a, G> {
    type Target = xr::CompositionLayerBase<'a, G>;
    fn deref(&self) -> &Self::Target {
        self.layer.as_ref().unwrap().deref()
    }
}

pub enum OverlayLayerInner<'a, G: xr::Graphics> {
    Quad(xr::CompositionLayerQuad<'a, G>),
    // Curved overlays
    Cylinder(xr::CompositionLayerCylinderKHR<'a, G>),
    // Skybox
    Equirect2(xr::CompositionLayerEquirect2KHR<'a, G>),
}

impl<'a, G: xr::Graphics> Deref for OverlayLayerInner<'a, G> {
    type Target = xr::CompositionLayerBase<'a, G>;
    fn deref(&self) -> &Self::Target {
        match self {
            OverlayLayerInner::Quad(quad) => quad.deref(),
            OverlayLayerInner::Cylinder(cylinder) => cylinder.deref(),
            OverlayLayerInner::Equirect2(equirect2) => equirect2.deref(),
        }
    }
}

new_key_type!(
    pub(crate) struct OverlayKey;
);

pub(crate) struct SwapchainData<G: xr::Graphics> {
    swapchain: xr::Swapchain<G>,
    info: xr::SwapchainCreateInfo<G>,
    initial_format: G::Format,
}

pub(crate) type SwapchainMap<G> = SecondaryMap<OverlayKey, SwapchainData<G>>;
supported_apis_enum!(pub(crate) enum AnySwapchainMap: SwapchainMap);

#[derive(Default)]
pub struct OverlaySessionData {
    swapchains: Mutex<Option<AnySwapchainMap>>,
}

enum OverlayKind {
    Quad,
    Curved { curvature: f32 },
    Sphere,
}

struct Overlay {
    key: CString,
    name: CString,
    /// Only allowed to be Some if KHR_composition_layer_color_scale_bias is active
    alpha: Option<f32>,
    width: f32,
    visible: bool,
    kind: OverlayKind,
    z_order: i64,
    bounds: vr::VRTextureBounds_t,
    transform: Option<(vr::ETrackingUniverseOrigin, vr::HmdMatrix34_t)>,
    compositor: Option<SupportedBackend>,
    rect: Option<xr::Rect2Di>,
}

impl Overlay {
    fn new(key: CString, name: CString) -> Self {
        Self {
            key,
            name,
            alpha: None,
            width: 1.0,
            visible: false,
            kind: OverlayKind::Quad,
            z_order: 0,
            bounds: vr::VRTextureBounds_t {
                uMin: 0.0,
                vMin: 0.0,
                uMax: 1.0,
                vMax: 1.0,
            },
            transform: None,
            compositor: None,
            rect: None,
        }
    }

    fn set_texture(
        &mut self,
        key: OverlayKey,
        session_data: RealSessionData<'_>,
        texture: vr::Texture_t,
    ) -> Result<(), vr::EVROverlayError> {
        let backend = match &self.compositor {
            Some(b) => b,
            None => self.compositor.insert(
                SupportedBackend::new(&texture, self.bounds)
                    .ok_or(vr::EVROverlayError::InvalidTexture)?,
            ),
        };

        #[macros::any_graphics(SupportedBackend)]
        fn create_swapchain_map<G: GraphicsBackend>(_: &G) -> AnySwapchainMap
        where
            AnySwapchainMap: From<SwapchainMap<G::Api>>,
        {
            SwapchainMap::<G::Api>::default().into()
        }

        let mut swapchains = session_data.overlay_data.swapchains.lock().unwrap();
        let swapchains =
            swapchains.get_or_insert_with(|| backend.with_any_graphics::<create_swapchain_map>(()));

        #[macros::any_graphics(SupportedBackend)]
        fn set_swapchain_texture<G: GraphicsBackend>(
            backend: &mut G,
            session_data: &SessionData,
            texture_bounds: vr::VRTextureBounds_t,
            map: &mut AnySwapchainMap,
            key: OverlayKey,
            texture: vr::Texture_t,
        ) -> Result<xr::Extent2Di, vr::EVROverlayError>
        where
            for<'a> &'a mut SwapchainMap<G::Api>:
                TryFrom<&'a mut AnySwapchainMap, Error: std::fmt::Display>,
            for<'a> &'a GraphicalSession: TryInto<&'a Session<G::Api>, Error: std::fmt::Display>,
            <G::Api as xr::Graphics>::Format: Eq,
        {
            let map: &mut SwapchainMap<G::Api> = map.try_into().unwrap_or_else(|e| {
                panic!(
                    "Received different texture type for overlay than current ({}) - {e}",
                    std::any::type_name::<G::Api>()
                );
            });
            let Some(b_texture) = G::get_texture(&texture) else {
                debug!("received invalid overlay texture handle");
                return Err(vr::EVROverlayError::InvalidTexture);
            };
            let tex_swapchain_info =
                backend.swapchain_info_for_texture(b_texture, texture_bounds, texture.eColorSpace);
            let mut create_swapchain = || {
                let mut info = backend.swapchain_info_for_texture(
                    b_texture,
                    texture_bounds,
                    texture.eColorSpace,
                );
                let initial_format = info.format;
                session_data.check_format::<G>(&mut info);
                let swapchain = session_data.create_swapchain(&info).unwrap();
                let images = swapchain
                    .enumerate_images()
                    .expect("Couldn't enumerate swapchain images");
                backend.store_swapchain_images(images, info.format);
                SwapchainData {
                    swapchain,
                    info,
                    initial_format,
                }
            };
            let swapchain = {
                let data = map
                    .entry(key)
                    .unwrap()
                    .or_insert_with(&mut create_swapchain);
                if !is_usable_swapchain(&data.info, data.initial_format, &tex_swapchain_info) {
                    *data = create_swapchain();
                }
                &mut data.swapchain
            };
            let idx = swapchain.acquire_image().unwrap();
            swapchain.wait_image(xr::Duration::INFINITE).unwrap();

            let extent = backend.copy_overlay_to_swapchain(b_texture, texture_bounds, idx as usize);
            swapchain.release_image().unwrap();

            Ok(extent)
        }

        let backend = self.compositor.as_mut().unwrap();
        let extent = backend.with_any_graphics_mut::<set_swapchain_texture>((
            &session_data,
            self.bounds,
            swapchains,
            key,
            texture,
        ))?;
        self.rect = Some(xr::Rect2Di {
            extent,
            offset: xr::Offset2Di::default(),
        });
        Ok(())
    }
}

macro_rules! get_overlay {
    (@impl $self:ident, $handle:expr, $overlay:ident, $lock:ident, $get:ident $(,$mut:ident)?) => {
        let $($mut)? overlays = $self.overlays.$lock().unwrap();
        let Some($overlay) = overlays.$get(OverlayKey::from(KeyData::from_ffi($handle))) else {
            return vr::EVROverlayError::UnknownOverlay;
        };
    };
    ($self:ident, $handle:expr, $overlay:ident) => {
        get_overlay!(@impl $self, $handle, $overlay, read, get);
    };
    ($self:ident, $handle:expr, mut $overlay:ident) => {
        get_overlay!(@impl $self, $handle, $overlay, write, get_mut, mut);
    };
}

impl vr::IVROverlay028_Interface for OverlayMan {
    fn CreateOverlay(
        &self,
        key: *const c_char,
        name: *const c_char,
        handle: *mut vr::VROverlayHandle_t,
    ) -> vr::EVROverlayError {
        let key = unsafe { CStr::from_ptr(key) };
        let name = unsafe { CStr::from_ptr(name) };

        if handle.is_null() {
            return vr::EVROverlayError::InvalidParameter;
        }

        let mut overlays = self.overlays.write().unwrap();
        let ret_key = overlays.insert(Overlay::new(key.into(), name.into()));
        let mut key_to_overlay = self.key_to_overlay.write().unwrap();
        key_to_overlay.insert(key.into(), ret_key);

        unsafe {
            handle.write(ret_key.data().as_ffi());
        }

        debug!("created overlay {name:?} with key {key:?}");
        vr::EVROverlayError::None
    }

    fn CreateSubviewOverlay(
        &self,
        _parent_overlay_handle: vr::VROverlayHandle_t,
        _subview_overlay_key: *const ::std::os::raw::c_char,
        _subview_overlay_name: *const ::std::os::raw::c_char,
        _subview_overlay_handle: *mut vr::VROverlayHandle_t,
    ) -> vr::EVROverlayError {
        todo!()
    }

    fn FindOverlay(
        &self,
        key: *const c_char,
        handle: *mut vr::VROverlayHandle_t,
    ) -> vr::EVROverlayError {
        if handle.is_null() {
            return vr::EVROverlayError::InvalidParameter;
        }
        let key = unsafe { CStr::from_ptr(key) };
        let map = self.key_to_overlay.read().unwrap();
        if let Some(key) = map.get(key) {
            unsafe {
                handle.write(key.data().as_ffi());
            }
            vr::EVROverlayError::None
        } else {
            vr::EVROverlayError::UnknownOverlay
        }
    }

    fn ShowOverlay(&self, handle: vr::VROverlayHandle_t) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);

        debug!("showing overlay {:?}", overlay.name);
        overlay.visible = true;
        vr::EVROverlayError::None
    }

    fn HideOverlay(&self, handle: vr::VROverlayHandle_t) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);

        debug!("hiding overlay {:?}", overlay.name);
        overlay.visible = false;
        vr::EVROverlayError::None
    }

    fn SetOverlayAlpha(&self, handle: vr::VROverlayHandle_t, alpha: f32) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);
        if !self
            .openxr
            .enabled_extensions
            .khr_composition_layer_color_scale_bias
        {
            crate::warn_once!(
                "Cannot SetOverlayAlpha on {:?}: Runtime does not support KHR_composition_layer_color_scale_bias",
                overlay.name
            );
            return vr::EVROverlayError::None;
        }

        debug!(
            "overlay {:?} alpha {:.2} → {:.2}",
            overlay.name,
            overlay.alpha.unwrap_or(1.0),
            alpha
        );
        if alpha == 1.0 {
            overlay.alpha = None;
        } else {
            overlay.alpha = Some(alpha);
        }
        vr::EVROverlayError::None
    }

    fn SetOverlayWidthInMeters(
        &self,
        handle: vr::VROverlayHandle_t,
        width: f32,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);

        debug!("setting overlay {:?} width to {width}", overlay.name);
        overlay.width = width;
        vr::EVROverlayError::None
    }

    fn SetOverlayTexture(
        &self,
        handle: vr::VROverlayHandle_t,
        texture: *const vr::Texture_t,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);
        if texture.is_null() {
            vr::EVROverlayError::InvalidParameter
        } else {
            let texture = unsafe { texture.read() };
            let key = OverlayKey::from(KeyData::from_ffi(handle));

            match self.get_real_session_data(&texture, overlay.bounds) {
                Err(e) => {
                    debug!(
                        "failed to get real session data for overlay texture {:?}: {e:?}",
                        overlay.name
                    );
                    e
                }
                Ok(data) => match overlay.set_texture(key, data, texture) {
                    Err(e) => {
                        debug!(
                            "failed to set overlay texture for {:?}: {e:?}",
                            overlay.name
                        );
                        e
                    }
                    Ok(_) => {
                        debug!("set overlay texture for {:?}", overlay.name);
                        vr::EVROverlayError::None
                    }
                },
            }
        }
    }

    fn CloseMessageOverlay(&self) {
        todo!()
    }
    fn ShowMessageOverlay(
        &self,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
    ) -> vr::VRMessageOverlayResponse {
        todo!()
    }
    fn SetKeyboardPositionForOverlay(&self, _: vr::VROverlayHandle_t, _: vr::HmdRect2_t) {
        todo!()
    }
    fn SetKeyboardTransformAbsolute(
        &self,
        _: vr::ETrackingUniverseOrigin,
        _: *const vr::HmdMatrix34_t,
    ) {
        todo!()
    }
    fn HideKeyboard(&self) {
        crate::warn_unimplemented!("HideKeyboard");
    }
    fn GetKeyboardText(&self, _: *mut c_char, _: u32) -> u32 {
        todo!()
    }
    fn ShowKeyboardForOverlay(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::EGamepadTextInputMode,
        _: vr::EGamepadTextInputLineMode,
        _: u32,
        _: *const c_char,
        _: u32,
        _: *const c_char,
        _: u64,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("ShowKeyboardForOverlay");
        vr::EVROverlayError::RequestFailed
    }
    fn ShowKeyboard(
        &self,
        _: vr::EGamepadTextInputMode,
        _: vr::EGamepadTextInputLineMode,
        _: u32,
        _: *const c_char,
        _: u32,
        _: *const c_char,
        _: u64,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("ShowKeyboard");
        vr::EVROverlayError::RequestFailed
    }
    fn GetPrimaryDashboardDevice(&self) -> vr::TrackedDeviceIndex_t {
        todo!()
    }
    fn ShowDashboard(&self, _: *const c_char) {
        todo!()
    }
    fn GetDashboardOverlaySceneProcess(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut u32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetDashboardOverlaySceneProcess(
        &self,
        _: vr::VROverlayHandle_t,
        _: u32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn IsActiveDashboardOverlay(&self, _: vr::VROverlayHandle_t) -> bool {
        todo!()
    }
    fn IsDashboardVisible(&self) -> bool {
        false
    }
    fn CreateDashboardOverlay(
        &self,
        _: *const c_char,
        _: *const c_char,
        _: *mut vr::VROverlayHandle_t,
        _: *mut vr::VROverlayHandle_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayTextureSize(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut u32,
        _: *mut u32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn ReleaseNativeOverlayHandle(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut c_void,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayTexture(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut *mut c_void,
        _: *mut c_void,
        _: *mut u32,
        _: *mut u32,
        _: *mut u32,
        _: *mut vr::ETextureType,
        _: *mut vr::EColorSpace,
        _: *mut vr::VRTextureBounds_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayFromFile(
        &self,
        _: vr::VROverlayHandle_t,
        _: *const c_char,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayRaw(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut c_void,
        _: u32,
        _: u32,
        _: u32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn ClearOverlayTexture(&self, handle: vr::VROverlayHandle_t) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);
        overlay.rect = None;
        vr::EVROverlayError::None
    }
    fn ClearOverlayCursorPositionOverride(&self, _: vr::VROverlayHandle_t) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayCursorPositionOverride(
        &self,
        _: vr::VROverlayHandle_t,
        _: *const vr::HmdVector2_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayCursor(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::VROverlayHandle_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn TriggerLaserMouseHapticVibration(
        &self,
        _: vr::VROverlayHandle_t,
        _: f32,
        _: f32,
        _: f32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayIntersectionMask(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::VROverlayIntersectionMaskPrimitive_t,
        _: u32,
        _: u32,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayIntersectionMask");
        vr::EVROverlayError::None
    }
    fn IsHoverTargetOverlay(&self, _: vr::VROverlayHandle_t) -> bool {
        todo!()
    }
    fn ComputeOverlayIntersection(
        &self,
        _: vr::VROverlayHandle_t,
        _: *const vr::VROverlayIntersectionParams_t,
        _: *mut vr::VROverlayIntersectionResults_t,
    ) -> bool {
        crate::warn_unimplemented!("ComputeOverlayIntersection");
        false
    }
    fn SetOverlayMouseScale(
        &self,
        _: vr::VROverlayHandle_t,
        _: *const vr::HmdVector2_t,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayMouseScale");
        vr::EVROverlayError::RequestFailed
    }
    fn GetOverlayMouseScale(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::HmdVector2_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayInputMethod(
        &self,
        _: vr::VROverlayHandle_t,
        input_method: vr::VROverlayInputMethod,
    ) -> vr::EVROverlayError {
        if input_method == vr::VROverlayInputMethod::Mouse {
            crate::warn_unimplemented!("SetOverlayInputMethod::Mouse");
        } else if input_method == vr::VROverlayInputMethod::None {
            crate::warn_unimplemented!("SetOverlayInputMethod::None");
        }
        vr::EVROverlayError::None
    }
    fn GetOverlayInputMethod(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::VROverlayInputMethod,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn PollNextOverlayEvent(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::VREvent_t,
        _: u32,
    ) -> bool {
        false
    }
    fn WaitFrameSync(&self, _: u32) -> vr::EVROverlayError {
        todo!()
    }
    fn GetTransformForOverlayCoordinates(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::ETrackingUniverseOrigin,
        _: vr::HmdVector2_t,
        _: *mut vr::HmdMatrix34_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn IsOverlayVisible(&self, handle: vr::VROverlayHandle_t) -> bool {
        let overlays = self.overlays.read().unwrap();

        overlays
            .get(OverlayKey::from(KeyData::from_ffi(handle)))
            .map(|overlay| overlay.visible)
            .unwrap_or(false)
    }
    fn SetSubviewPosition(
        &self,
        _overlay_handle: vr::VROverlayHandle_t,
        _x: f32,
        _y: f32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayTransformProjection(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::ETrackingUniverseOrigin,
        _: *const vr::HmdMatrix34_t,
        _: *const vr::VROverlayProjection_t,
        _: vr::EVREye,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayTransformCursor(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::HmdVector2_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayTransformCursor(
        &self,
        _: vr::VROverlayHandle_t,
        _: *const vr::HmdVector2_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayTransformTrackedDeviceComponent(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::TrackedDeviceIndex_t,
        _: *mut c_char,
        _: u32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayTransformTrackedDeviceComponent(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::TrackedDeviceIndex_t,
        _: *const c_char,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayTransformTrackedDeviceRelative(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::TrackedDeviceIndex_t,
        _: *mut vr::HmdMatrix34_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayTransformTrackedDeviceRelative(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::TrackedDeviceIndex_t,
        _: *const vr::HmdMatrix34_t,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayTransformTrackedDeviceRelative");
        vr::EVROverlayError::None
    }
    fn GetOverlayTransformAbsolute(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::ETrackingUniverseOrigin,
        _: *mut vr::HmdMatrix34_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayTransformAbsolute(
        &self,
        handle: vr::VROverlayHandle_t,
        origin: vr::ETrackingUniverseOrigin,
        transform: *const vr::HmdMatrix34_t,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);
        if transform.is_null() {
            vr::EVROverlayError::InvalidParameter
        } else {
            let transform = unsafe { transform.read() };
            let xr_transform: xr::Posef = transform.into();
            let o = xr_transform.orientation;
            let q = Quat::from_xyzw(o.x, o.y, o.z, o.w).normalize();
            let transform = xr::Posef {
                position: xr_transform.position,
                orientation: xr::Quaternionf {
                    x: q.x,
                    y: q.y,
                    z: q.z,
                    w: q.w,
                },
            };
            overlay.transform = Some((origin, transform.into()));
            debug!(
                "set overlay transform origin to {origin:?} for {:?} ({transform:?})",
                overlay.name
            );
            vr::EVROverlayError::None
        }
    }
    fn GetOverlayTransformType(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::VROverlayTransformType,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayTextureBounds(
        &self,
        handle: vr::VROverlayHandle_t,
        bounds: *mut vr::VRTextureBounds_t,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, overlay);
        if bounds.is_null() {
            vr::EVROverlayError::InvalidParameter
        } else {
            unsafe { bounds.write(overlay.bounds) };
            vr::EVROverlayError::None
        }
    }
    fn SetOverlayTextureBounds(
        &self,
        handle: vr::VROverlayHandle_t,
        bounds: *const vr::VRTextureBounds_t,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);
        if bounds.is_null() {
            vr::EVROverlayError::InvalidParameter
        } else {
            overlay.bounds = unsafe { bounds.read() };
            debug!("overlay {:?} {:?}", overlay.name, overlay.bounds);
            vr::EVROverlayError::None
        }
    }
    fn GetOverlayTextureColorSpace(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::EColorSpace,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayTextureColorSpace(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::EColorSpace,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayPreCurvePitch(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut f32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayPreCurvePitch(&self, _: vr::VROverlayHandle_t, _: f32) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayCurvature(
        &self,
        handle: vr::VROverlayHandle_t,
        value: *mut f32,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, overlay);
        unsafe {
            *value = match overlay.kind {
                OverlayKind::Curved { curvature } => curvature,
                _ => 0.0,
            }
        }
        vr::EVROverlayError::None
    }
    fn SetOverlayCurvature(
        &self,
        handle: vr::VROverlayHandle_t,
        value: f32,
    ) -> vr::EVROverlayError {
        // All sanity checks must be made here
        if self
            .openxr
            .enabled_extensions
            .khr_composition_layer_cylinder
        {
            get_overlay!(self, handle, mut overlay);
            overlay.kind = OverlayKind::Curved {
                curvature: value.clamp(0.0, 1.0),
            };
        }
        vr::EVROverlayError::None
    }
    fn GetOverlayWidthInMeters(
        &self,
        handle: vr::VROverlayHandle_t,
        value: *mut f32,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, overlay);
        unsafe {
            *value = overlay.width;
        }
        vr::EVROverlayError::None
    }
    fn GetOverlaySortOrder(
        &self,
        handle: vr::VROverlayHandle_t,
        value: *mut u32,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, overlay);
        unsafe { *value = overlay.z_order as _ };
        vr::EVROverlayError::None
    }
    fn SetOverlaySortOrder(
        &self,
        handle: vr::VROverlayHandle_t,
        value: u32,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, mut overlay);
        debug!(
            "overlay {:?} sort order {} → {}",
            overlay.name, overlay.z_order, value
        );
        overlay.z_order = value as _;
        vr::EVROverlayError::None
    }
    fn GetOverlayTexelAspect(&self, _: vr::VROverlayHandle_t, _: *mut f32) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayTexelAspect(&self, _: vr::VROverlayHandle_t, _: f32) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayTexelAspect");
        vr::EVROverlayError::None
    }
    fn GetOverlayAlpha(
        &self,
        handle: vr::VROverlayHandle_t,
        value: *mut f32,
    ) -> vr::EVROverlayError {
        get_overlay!(self, handle, overlay);
        unsafe { *value = overlay.alpha.unwrap_or(1.0) };
        vr::EVROverlayError::None
    }

    fn GetOverlayColor(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut f32,
        _: *mut f32,
        _: *mut f32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayColor(
        &self,
        _: vr::VROverlayHandle_t,
        _: f32,
        _: f32,
        _: f32,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayColor");
        vr::EVROverlayError::None
    }
    fn GetOverlayFlags(&self, _: vr::VROverlayHandle_t, _: *mut u32) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayFlag(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::VROverlayFlags,
        _: *mut bool,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayFlag(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::VROverlayFlags,
        _: bool,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayFlag");
        vr::EVROverlayError::None
    }
    fn GetOverlayRenderingPid(&self, _: vr::VROverlayHandle_t) -> u32 {
        todo!()
    }
    fn SetOverlayRenderingPid(&self, _: vr::VROverlayHandle_t, _: u32) -> vr::EVROverlayError {
        todo!()
    }
    /// Return the name of the given error enum as a pointer to a static c string.
    fn GetOverlayErrorNameFromEnum(&self, e: vr::EVROverlayError) -> *const c_char {
        let res: &'static CStr = match e {
            vr::EVROverlayError::None => c"None",
            vr::EVROverlayError::UnknownOverlay => c"UnknownOverlay",
            vr::EVROverlayError::InvalidHandle => c"InvalidHandle",
            vr::EVROverlayError::PermissionDenied => c"PermissionDenied",
            vr::EVROverlayError::OverlayLimitExceeded => c"OverlayLimitExceeded",
            vr::EVROverlayError::WrongVisibilityType => c"WrongVisibilityType",
            vr::EVROverlayError::KeyTooLong => c"KeyTooLong",
            vr::EVROverlayError::NameTooLong => c"NameTooLong",
            vr::EVROverlayError::KeyInUse => c"KeyInUse",
            vr::EVROverlayError::WrongTransformType => c"WrongTransformType",
            vr::EVROverlayError::InvalidTrackedDevice => c"InvalidTrackedDevice",
            vr::EVROverlayError::InvalidParameter => c"InvalidParameter",
            vr::EVROverlayError::ThumbnailCantBeDestroyed => c"ThumbnailCantBeDestroyed",
            vr::EVROverlayError::ArrayTooSmall => c"ArrayTooSmall",
            vr::EVROverlayError::RequestFailed => c"RequestFailed",
            vr::EVROverlayError::InvalidTexture => c"InvalidTexture",
            vr::EVROverlayError::UnableToLoadFile => c"UnableToLoadFile",
            vr::EVROverlayError::KeyboardAlreadyInUse => c"KeyboardAlreadyInUse",
            vr::EVROverlayError::NoNeighbor => c"NoNeighbor",
            vr::EVROverlayError::TooManyMaskPrimitives => c"TooManyMaskPrimitives",
            vr::EVROverlayError::BadMaskPrimitive => c"BadMaskPrimitive",
            vr::EVROverlayError::TextureAlreadyLocked => c"TextureAlreadyLocked",
            vr::EVROverlayError::TextureLockCapacityReached => c"TextureLockCapacityReached",
            vr::EVROverlayError::TextureNotLocked => c"TextureNotLocked",
            vr::EVROverlayError::TimedOut => c"TimedOut",
        };
        res.as_ptr()
    }
    fn GetOverlayImageData(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut c_void,
        _: u32,
        _: *mut u32,
        _: *mut u32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayName(&self, _: vr::VROverlayHandle_t, _: *const c_char) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayName(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut c_char,
        _: u32,
        _: *mut vr::EVROverlayError,
    ) -> u32 {
        todo!()
    }
    fn GetOverlayKey(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut c_char,
        _: u32,
        _: *mut vr::EVROverlayError,
    ) -> u32 {
        todo!()
    }
    fn DestroyOverlay(&self, handle: vr::VROverlayHandle_t) -> vr::EVROverlayError {
        let key = OverlayKey::from(KeyData::from_ffi(handle));

        let mut overlays = self.overlays.write().unwrap();
        if let Some(overlay) = overlays.remove(key) {
            let mut map = self.key_to_overlay.write().unwrap();
            map.remove(&overlay.key);
        }
        vr::EVROverlayError::None
    }
}

impl vr::IVROverlay026On027 for OverlayMan {
    fn SetOverlayTransformOverlayRelative(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::VROverlayHandle_t,
        _: *const vr::HmdMatrix34_t,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayTransformOverlayRelative");
        vr::EVROverlayError::None
    }
    fn GetOverlayTransformOverlayRelative(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::VROverlayHandle_t,
        _: *mut vr::HmdMatrix34_t,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("GetOverlayTransformOverlayRelative");
        vr::EVROverlayError::None
    }
}

impl vr::IVROverlay021On024 for OverlayMan {
    fn ShowKeyboardForOverlay(
        &self,
        handle: vr::VROverlayHandle_t,
        input_mode: vr::EGamepadTextInputMode,
        line_input_mode: vr::EGamepadTextInputLineMode,
        description: *const c_char,
        char_max: u32,
        existing_text: *const c_char,
        use_minimal_mode: bool,
        user_value: u64,
    ) -> vr::EVROverlayError {
        <Self as vr::IVROverlay024_Interface>::ShowKeyboardForOverlay(
            self,
            handle,
            input_mode,
            line_input_mode,
            if use_minimal_mode {
                vr::EKeyboardFlags::Minimal.0
            } else {
                0
            },
            description,
            char_max,
            existing_text,
            user_value,
        )
    }
    fn ShowKeyboard(
        &self,
        input_mode: vr::EGamepadTextInputMode,
        input_line_mode: vr::EGamepadTextInputLineMode,
        description: *const c_char,
        char_max: u32,
        existing_text: *const c_char,
        use_minimal_mode: bool,
        user_value: u64,
    ) -> vr::EVROverlayError {
        <Self as vr::IVROverlay024_Interface>::ShowKeyboard(
            self,
            input_mode,
            input_line_mode,
            if use_minimal_mode {
                vr::EKeyboardFlags::Minimal.0
            } else {
                0
            },
            description,
            char_max,
            existing_text,
            user_value,
        )
    }
    fn GetOverlayDualAnalogTransform(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::EDualAnalogWhich,
        _: *mut vr::HmdVector2_t,
        _: *mut f32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayDualAnalogTransform(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::EDualAnalogWhich,
        _: *const vr::HmdVector2_t,
        _: f32,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayRenderModel(
        &self,
        _: vr::VROverlayHandle_t,
        _: *const c_char,
        _: *const vr::HmdColor_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn GetOverlayRenderModel(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut c_char,
        _: u32,
        _: *mut vr::HmdColor_t,
        _: *mut vr::EVROverlayError,
    ) -> u32 {
        todo!()
    }
}

impl vr::IVROverlay020On021 for OverlayMan {
    fn MoveGamepadFocusToNeighbor(
        &self,
        _: vr::EOverlayDirection,
        _: vr::VROverlayHandle_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetOverlayNeighbor(
        &self,
        _: vr::EOverlayDirection,
        _: vr::VROverlayHandle_t,
        _: vr::VROverlayHandle_t,
    ) -> vr::EVROverlayError {
        todo!()
    }
    fn SetGamepadFocusOverlay(&self, _: vr::VROverlayHandle_t) -> vr::EVROverlayError {
        todo!()
    }
    fn GetGamepadFocusOverlay(&self) -> vr::VROverlayHandle_t {
        todo!()
    }
    fn GetOverlayAutoCurveDistanceRangeInMeters(
        &self,
        _handle: vr::VROverlayHandle_t,
        _min_distance: *mut f32,
        _max_distance: *mut f32,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("GetOverlayAutoCurveDistanceRangeInMeters");
        vr::EVROverlayError::None
    }
    fn SetOverlayAutoCurveDistanceRangeInMeters(
        &self,
        _handle: vr::VROverlayHandle_t,
        _min_distance: f32,
        _max_distance: f32,
    ) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetOverlayAutoCurveDistanceRangeInMeters");
        vr::EVROverlayError::None
    }
}

// The OpenVR commit messages mention that these functions just go through the standard overlay
// rendering path now.
impl vr::IVROverlay019On020 for OverlayMan {
    fn GetHighQualityOverlay(&self) -> vr::VROverlayHandle_t {
        crate::warn_unimplemented!("GetHighQualityOverlay");
        vr::k_ulOverlayHandleInvalid
    }
    fn SetHighQualityOverlay(&self, _: vr::VROverlayHandle_t) -> vr::EVROverlayError {
        crate::warn_unimplemented!("SetHighQualityOverlay");
        vr::EVROverlayError::None
    }
}

impl vr::IVROverlay017On018 for OverlayMan {
    fn HandleControllerOverlayInteractionAsMouse(
        &self,
        _: vr::VROverlayHandle_t,
        _: vr::TrackedDeviceIndex_t,
    ) -> bool {
        todo!()
    }
}

impl vr::IVROverlay013On014 for OverlayMan {
    fn GetOverlayTexture(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut *mut c_void,
        _: *mut c_void,
        _: *mut u32,
        _: *mut u32,
        _: *mut u32,
        _: *mut vr::EGraphicsAPIConvention,
        _: *mut vr::EColorSpace,
    ) -> vr::EVROverlayError {
        todo!()
    }
}

impl vr::IVROverlay007On010 for OverlayMan {
    fn PollNextOverlayEvent(
        &self,
        _: vr::VROverlayHandle_t,
        _: *mut vr::vr_0_9_12::VREvent_t,
    ) -> bool {
        todo!()
    }
}
