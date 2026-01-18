use super::GraphicsBackend;
use derive_more::Deref;
use glutin_glx_sys::{
    Success,
    glx::{self, Glx},
};
use libc::{dlerror, dlopen, dlsym};
use log::warn;
use openvr as vr;
use openxr as xr;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, LazyLock, Once};

static GLX: LazyLock<Library> = LazyLock::new(|| Library::new(c"libGLX.so.0"));

pub struct GlData {
    session_data: Arc<SessionCreateInfo>,
    images: Vec<u32>,
    format: u32,
    read_fbo: u32,
    draw_fbo: u32,
}

#[derive(Deref)]
struct SessionCreateInfo(xr::opengl::SessionCreateInfo);
// SAFETY: SessionCreateInfo is only not Send + Sync because of the pointer next field.
// We don't even use this field so it's fine.
unsafe impl Send for SessionCreateInfo {}
unsafe impl Sync for SessionCreateInfo {}

unsafe fn get_fbconfig(
    glx: &Glx,
    display: *mut glx::types::Display,
    glx_context: glx::types::GLXContext,
) -> Option<glx::types::GLXFBConfig> {
    unsafe {
        let mut config_id = 0;
        let ret = glx.QueryContext(display, glx_context, glx::FBCONFIG_ID as _, &mut config_id);
        if ret != Success as i32 {
            warn!("Failed to get fbconfig id from context (error code {ret})");
            return None;
        }

        let mut screen = 0;
        let ret = glx.QueryContext(display, glx_context, glx::SCREEN as _, &mut screen);
        if ret != Success as i32 {
            warn!("Failed to get GLX screen for context (error code {ret})");
            return None;
        }

        let attrs = [glx::FBCONFIG_ID, config_id as _, glx::NONE];
        let mut items = 0;
        let cfgs = glx.ChooseFBConfig(display, screen, attrs.as_ptr() as _, &mut items);
        (!cfgs.is_null() && items >= 0).then(|| std::slice::from_raw_parts(cfgs, items as usize)[0])
    }
}

unsafe fn get_visualid(
    glx: &Glx,
    display: *mut glx::types::Display,
    cfg: Option<glx::types::GLXFBConfig>,
) -> u32 {
    unsafe {
        let Some(cfg) = cfg else {
            return 0;
        };
        let visual = glx.GetVisualFromFBConfig(display, cfg);
        if visual.is_null() {
            warn!("No visual available from fbconfig.");
            0
        } else {
            (&raw const (*visual).visualid).read() as u32
        }
    }
}

impl GlData {
    // Returns None if we couldn't get the display.
    pub(crate) fn new() -> Option<Self> {
        let glx = Glx::load_with(|func| {
            let func = unsafe { CString::from_vec_unchecked(func.as_bytes().to_vec()) };
            GLX.get(&func)
        });

        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            gl::load_with(|f| {
                let f = unsafe { CString::from_vec_unchecked(f.as_bytes().to_vec()) };
                unsafe { glx.GetProcAddress(f.as_ptr().cast()) }.cast()
            });

            if log::log_enabled!(log::Level::Debug) {
                unsafe {
                    gl::DebugMessageCallback(Some(debug_message), std::ptr::null());
                    gl::Enable(gl::DEBUG_OUTPUT);
                }
            }
        });

        // Grab the session info on creation - this makes us resilient against session restarts,
        // which could result in us trying to grab the context from a different thread
        let session_info = unsafe {
            let x_display = glx.GetCurrentDisplay();
            if x_display.is_null() {
                warn!("X display is null!");
                return None;
            }
            let glx_context = glx.GetCurrentContext();
            if glx_context.is_null() {
                warn!("GLX context was null!");
                return None;
            }

            let glx_drawable = glx.GetCurrentDrawable();
            let fbconfig = get_fbconfig(&glx, x_display, glx_context);
            let visualid = get_visualid(&glx, x_display, fbconfig);

            xr::opengl::SessionCreateInfo::Xlib {
                x_display: x_display.cast(),
                glx_fb_config: fbconfig.map(|p| p.cast_mut()).unwrap_or_else(|| {
                    warn!("No fbconfig found.");
                    std::ptr::null_mut()
                }),
                visualid,
                glx_drawable,
                glx_context: glx_context.cast_mut(),
            }
        };

        let mut fbos = [0; 2];
        unsafe {
            gl::GenFramebuffers(fbos.len() as i32, fbos.as_mut_ptr());
        }

        Some(GlData {
            session_data: Arc::new(SessionCreateInfo(session_info)),
            images: Default::default(),
            format: 0,
            read_fbo: fbos[0],
            draw_fbo: fbos[1],
        })
    }
}

impl GraphicsBackend for GlData {
    type Api = xr::OpenGL;
    type OpenVrTexture = gl::types::GLuint;
    type NiceFormat = u32;

    fn to_nice_format(format: u32) -> Self::NiceFormat {
        format
    }

    fn session_create_info(&self) -> <Self::Api as openxr::Graphics>::SessionCreateInfo {
        // SAFETY: SessionCreateInfo should be Copy anyway but doesn't work right
        // https://github.com/Ralith/openxrs/issues/183
        unsafe { std::ptr::read(&**self.session_data) }
    }

    #[inline]
    fn get_texture(texture: &vr::Texture_t) -> Option<Self::OpenVrTexture> {
        Some(texture.handle as _)
    }

    #[inline]
    fn store_swapchain_images(
        &mut self,
        images: Vec<<Self::Api as xr::Graphics>::SwapchainImage>,
        format: u32,
    ) {
        self.images = images;
        self.format = format;
    }

    #[inline]
    fn swapchain_info_for_texture(
        &self,
        texture: Self::OpenVrTexture,
        bounds: vr::VRTextureBounds_t,
        color_space: vr::EColorSpace,
    ) -> xr::SwapchainCreateInfo<Self::Api> {
        let mut fmt = 0;
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_INTERNAL_FORMAT, &mut fmt);
        }
        let xr::Rect2Di { extent, .. } = texture_rect_from_bounds(texture, bounds);

        let fmt = match color_space {
            vr::EColorSpace::Linear => fmt,
            vr::EColorSpace::Gamma | vr::EColorSpace::Auto => match fmt as gl::types::GLenum {
                gl::RGBA8 => gl::SRGB8_ALPHA8 as gl::types::GLint,
                gl::RGB8 => gl::SRGB8 as gl::types::GLint,
                _ => fmt,
            },
        };

        xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::TRANSFER_DST,
            format: fmt as u32,
            sample_count: 1,
            width: extent.width as u32,
            height: extent.height as u32,
            face_count: 1,
            array_size: 2,
            mip_count: 1,
        }
    }

    fn copy_texture_to_swapchain(
        &self,
        eye: vr::EVREye,
        texture: Self::OpenVrTexture,
        _color_space: vr::EColorSpace,
        bounds: vr::VRTextureBounds_t,
        image_index: usize,
        _submit_flags: vr::EVRSubmitFlags,
    ) -> xr::Extent2Di {
        let swapchain_texture = self.images[image_index];

        let xr::Rect2Di { extent, offset } = texture_rect_from_bounds(texture, bounds);

        let mut fmt = 0;
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_INTERNAL_FORMAT, &mut fmt);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        if self.format == fmt as u32 {
            unsafe {
                gl::CopyImageSubData(
                    texture,
                    gl::TEXTURE_2D,
                    0, // level
                    offset.x,
                    offset.y,
                    0, // z
                    swapchain_texture,
                    gl::TEXTURE_2D_ARRAY,
                    0, // x
                    0, // y
                    0, // z
                    eye as i32,
                    extent.width,
                    extent.height,
                    1,
                );
            }
        } else {
            unsafe {
                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.read_fbo);
                gl::FramebufferTexture2D(
                    gl::READ_FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0,
                    gl::TEXTURE_2D,
                    texture,
                    0,
                );
                gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, self.draw_fbo);
                gl::FramebufferTextureLayer(
                    gl::DRAW_FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0,
                    swapchain_texture,
                    0,
                    eye as i32,
                );

                gl::BlitFramebuffer(
                    offset.x,
                    offset.y,
                    offset.x + extent.width,
                    offset.y + extent.height,
                    0,
                    0,
                    extent.width,
                    extent.height,
                    gl::COLOR_BUFFER_BIT,
                    gl::NEAREST,
                );

                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
                gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
            }
        }

        extent
    }

    fn copy_overlay_to_swapchain(
        &mut self,
        texture: Self::OpenVrTexture,
        bounds: openvr::VRTextureBounds_t,
        image_index: usize,
    ) -> openxr::Extent2Di {
        self.copy_texture_to_swapchain(
            vr::EVREye::Left,
            texture,
            vr::EColorSpace::Auto,
            bounds,
            image_index,
            vr::EVRSubmitFlags::Default,
        )
    }
}

fn texture_rect_from_bounds(
    texture: glx::types::GLuint,
    bounds: vr::VRTextureBounds_t,
) -> xr::Rect2Di {
    let [mut height, mut width] = Default::default();
    unsafe {
        gl::BindTexture(gl::TEXTURE_2D, texture);
        gl::GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut width);
        gl::GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut height);
        gl::BindTexture(gl::TEXTURE_2D, 0);
    }
    let width_min = bounds.uMin * width as f32;
    let width_max = bounds.uMax * width as f32;
    let height_min = bounds.vMin * height as f32;
    let height_max = bounds.vMax * height as f32;

    xr::Rect2Di {
        extent: xr::Extent2Di {
            width: (width_max - width_min).abs() as i32,
            height: (height_max - height_min).abs() as i32,
        },
        offset: xr::Offset2Di {
            x: width_min.min(width_max) as i32,
            y: height_min.min(height_max) as i32,
        },
    }
}

extern "system" fn debug_message(
    source: gl::types::GLenum,
    ty: gl::types::GLenum,
    id: gl::types::GLuint,
    severity: gl::types::GLenum,
    _: gl::types::GLsizei,
    message: *const c_char,
    _: *mut c_void,
) {
    let source = match source {
        gl::DEBUG_SOURCE_API => "OpenGL Api",
        gl::DEBUG_SOURCE_OTHER => "Other",
        _ => "<unknown>",
    };

    let ty = match ty {
        gl::DEBUG_TYPE_ERROR => "Error",
        gl::DEBUG_TYPE_DEPRECATED_BEHAVIOR => "Deprecated Behavior",
        gl::DEBUG_TYPE_UNDEFINED_BEHAVIOR => "Undefined Behavior",
        gl::DEBUG_TYPE_PORTABILITY => "Portability Issue",
        gl::DEBUG_TYPE_PERFORMANCE => "Performance Issue",
        gl::DEBUG_TYPE_OTHER => "Other",
        _ => "<unknown>",
    };

    let severity = match severity {
        gl::DEBUG_SEVERITY_HIGH => "High",
        gl::DEBUG_SEVERITY_MEDIUM => "Medium",
        gl::DEBUG_SEVERITY_LOW => "Low",
        gl::DEBUG_SEVERITY_NOTIFICATION => "Notification",
        _ => "<unknown>",
    };
    let message = unsafe { CStr::from_ptr(message) };
    log::debug!("(severity: {severity}, id: {id}) {ty} message from {source}: {message:?}");
}

struct Library(*mut c_void);
unsafe impl Send for Library {}
unsafe impl Sync for Library {}
impl Library {
    fn new(name: &CStr) -> Self {
        let handle = unsafe { dlopen(name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        if handle.is_null() {
            let err = unsafe { CStr::from_ptr(dlerror()) };
            panic!("Failed to load {name:?}: {err:?}");
        }

        Self(handle)
    }

    fn get(&self, function: &CStr) -> *const c_void {
        // clear old error
        unsafe {
            dlerror();
        }

        let symbol = unsafe { dlsym(self.0, function.as_ptr()) };
        if symbol.is_null() {
            let err = unsafe { dlerror() };
            if !err.is_null() {
                panic!("Failed to get symbol {function:?}: {:?}", unsafe {
                    CStr::from_ptr(err)
                });
            }
        }
        symbol
    }
}
