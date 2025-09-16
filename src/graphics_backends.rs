mod gl;
mod vulkan;

use derive_more::{From, TryInto};
pub use gl::GlData;
use openvr as vr;
use openxr as xr;
pub use vulkan::VulkanData;

pub trait GraphicsBackend: Into<SupportedBackend> {
    type Api: xr::Graphics + 'static;
    type OpenVrTexture: Copy;
    type NiceFormat: std::fmt::Debug;

    fn to_nice_format(format: <Self::Api as xr::Graphics>::Format) -> Self::NiceFormat;

    fn session_create_info(&self) -> <Self::Api as xr::Graphics>::SessionCreateInfo;

    /// Returns None if the texture is invalid.
    fn get_texture(texture: &vr::Texture_t) -> Option<Self::OpenVrTexture>;

    fn swapchain_info_for_texture(
        &self,
        texture: Self::OpenVrTexture,
        bounds: vr::VRTextureBounds_t,
        color_space: vr::EColorSpace,
    ) -> xr::SwapchainCreateInfo<Self::Api>;

    fn store_swapchain_images(
        &mut self,
        images: Vec<<Self::Api as xr::Graphics>::SwapchainImage>,
        format: <Self::Api as xr::Graphics>::Format,
    );

    fn copy_texture_to_swapchain(
        &self,
        eye: vr::EVREye,
        texture: Self::OpenVrTexture,
        color_space: vr::EColorSpace,
        bounds: vr::VRTextureBounds_t,
        image_index: usize,
        submit_flags: vr::EVRSubmitFlags,
    ) -> xr::Extent2Di;

    fn copy_overlay_to_swapchain(
        &mut self,
        texture: Self::OpenVrTexture,
        bounds: vr::VRTextureBounds_t,
        image_index: usize,
    ) -> xr::Extent2Di;
}

#[derive(macros::Backends, TryInto, From)]
#[try_into(owned, ref)]
#[allow(clippy::large_enum_variant)]
pub enum SupportedBackend {
    Vulkan(VulkanData),
    OpenGL(GlData),
    #[cfg(test)]
    Fake(crate::compositor::FakeGraphicsData),
}

impl<B> GraphicsEnum<B> for SupportedBackend
where
    B: GraphicsBackend + TryFrom<Self>,
{
    type Inner = B;
}

#[allow(clippy::single_component_path_imports)]
pub(crate) use supported_apis_enum;
#[allow(clippy::single_component_path_imports)]
pub(crate) use supported_backends_enum;

// These traits are used to commit some type crimes, see macros::any_graphics
// None of them should be manually implemented
pub trait GraphicsEnum<G>: Sized {
    type Inner: TryFrom<Self>;
}

pub trait WithAnyGraphicsParams {
    type Args;
    type Ret;
}

pub trait WithAnyGraphics<G>: WithAnyGraphicsParams {
    type GraphicsEnum: GraphicsEnum<G>;
    fn with_any_graphics(
        inner: &<Self::GraphicsEnum as GraphicsEnum<G>>::Inner,
        args: Self::Args,
    ) -> Self::Ret;
}

pub trait WithAnyGraphicsMut<G>: WithAnyGraphicsParams {
    type GraphicsEnum: GraphicsEnum<G>;
    fn with_any_graphics(
        inner: &mut <Self::GraphicsEnum as GraphicsEnum<G>>::Inner,
        args: Self::Args,
    ) -> Self::Ret;
}

pub trait WithAnyGraphicsOwned<G>: WithAnyGraphicsParams {
    type GraphicsEnum: GraphicsEnum<G>;
    fn with_any_graphics(
        inner: <Self::GraphicsEnum as GraphicsEnum<G>>::Inner,
        args: Self::Args,
    ) -> Self::Ret;
}

impl SupportedBackend {
    pub fn new(texture: &vr::Texture_t, _bounds: vr::VRTextureBounds_t) -> Option<Self> {
        match texture.eType {
            vr::ETextureType::Vulkan => {
                let vk_texture = unsafe { &*(texture.handle as *const vr::VRVulkanTextureData_t) };
                Some(Self::Vulkan(VulkanData::new(vk_texture)))
            }
            vr::ETextureType::OpenGL => GlData::new().map(Self::OpenGL),
            #[cfg(test)]
            vr::ETextureType::Reserved => Some(Self::Fake(
                crate::compositor::FakeGraphicsData::new(texture),
            )),
            other => panic!("Unsupported texture type: {other:?}"),
        }
    }
}
