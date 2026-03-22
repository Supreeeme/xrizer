use super::GraphicsBackend;
use derive_more::Deref;
use openvr as vr;
use openxr as xr;
use std::sync::Arc;

pub struct HeadlessData {
    session_data: Arc<SessionCreateInfo>,
}

#[derive(Deref)]
struct SessionCreateInfo(xr::headless::SessionCreateInfo);
// SAFETY: SessionCreateInfo is only not Send + Sync because of the pointer next field.
// We don't even use this field so it's fine.
unsafe impl Send for SessionCreateInfo {}
unsafe impl Sync for SessionCreateInfo {}

impl HeadlessData {
    pub(crate) fn new() -> Self {
        let session_info = xr::headless::SessionCreateInfo {};
        HeadlessData {
            session_data: Arc::new(SessionCreateInfo(session_info)),
        }
    }
}

impl GraphicsBackend for HeadlessData {
    type Api = xr::Headless;
    type OpenVrTexture = *const vr::VRVulkanTextureData_t;
    type Format = u32;
    type NiceFormat = u32;

    fn from_openxr_format(_format: xr::headless::HeadlessFormat) -> Self::Format {
        0u32
    }

    fn to_openxr_format(_format: Self::Format) -> <Self::Api as xr::Graphics>::Format {
        panic!()
    }

    fn to_nice_format(format: Self::Format) -> Self::NiceFormat {
        format
    }

    fn session_create_info(&self) -> <Self::Api as openxr::Graphics>::SessionCreateInfo {
        // SAFETY: SessionCreateInfo should be Copy anyway but doesn't work right
        // https://github.com/Ralith/openxrs/issues/183
        unsafe { std::ptr::read(&**self.session_data) }
    }

    #[inline]
    fn get_texture(texture: &openvr::Texture_t) -> Option<Self::OpenVrTexture> {
        if !texture.handle.is_null() {
            Some(texture.handle.cast())
        } else {
            None
        }
    }

    #[inline]
    fn store_swapchain_images(
        &mut self,
        _images: Vec<<Self::Api as xr::Graphics>::SwapchainImage>,
        _format: Self::Format,
    ) {
    }

    #[inline]
    fn swapchain_info_for_texture(
        &self,
        _texture: Self::OpenVrTexture,
        _bounds: vr::VRTextureBounds_t,
        _color_space: vr::EColorSpace,
    ) -> xr::SwapchainCreateInfo<Self::Api> {
        /*let raw_format = 0u32;
        let format = *unsafe { &raw_format as &xr::headless::HeadlessFormat };
        xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::TRANSFER_DST,
            format,
            sample_count: 1,
            width: 0u32,
            height: 0u32,
            face_count: 1,
            array_size: 2,
            mip_count: 1,
        }*/
        panic!();
    }

    fn copy_texture_to_swapchain(
        &self,
        _eye: vr::EVREye,
        _texture: Self::OpenVrTexture,
        _color_space: vr::EColorSpace,
        _bounds: vr::VRTextureBounds_t,
        _image_index: usize,
        _submit_flags: vr::EVRSubmitFlags,
    ) -> xr::Extent2Di {
        Default::default()
    }

    fn copy_overlay_to_swapchain(
        &mut self,
        _texture: Self::OpenVrTexture,
        _bounds: openvr::VRTextureBounds_t,
        _image_index: usize,
    ) -> openxr::Extent2Di {
        Default::default()
    }
}
