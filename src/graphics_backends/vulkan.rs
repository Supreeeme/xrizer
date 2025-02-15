use super::GraphicsBackend;
use ash::vk::{self, Handle};
use log::warn;
use openvr as vr;
use openxr as xr;
use std::collections::HashSet;
use std::ffi::{c_char, CString};
use std::sync::{LazyLock, Mutex};

struct RealSessionData {
    images: Vec<vk::Image>,
    format: vk::Format,
    pool: vk::CommandPool,
    bufs: Vec<vk::CommandBuffer>,
    overlay_pipeline: Option<PipelineData>,
}

pub struct VulkanData {
    _entry: ash::Entry,
    pub instance: ash::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue: vk::Queue,
    pub queue_family_index: u32,
    real_data: Option<RealSessionData>,
}

impl Drop for VulkanData {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
        }
        match &self.real_data {
            // Temporary session - we created these handles, so let's destroy them
            None => unsafe {
                self.device.destroy_device(None);
                self.instance.destroy_instance(None);
            },
            // Real session - the handles come from the app, only destroy the command pool we created
            Some(data) => unsafe {
                self.device.destroy_command_pool(data.pool, None);
                if let Some(data) = &data.overlay_pipeline {
                    self.device.destroy_pipeline(data.pipeline, None);
                    self.device.destroy_pipeline_layout(data.layout, None);
                    self.device.destroy_render_pass(data.renderpass, None);
                    self.device.destroy_descriptor_pool(data.pool, None);
                    self.device.destroy_sampler(data.sampler, None);
                }
            },
        }
    }
}

impl GraphicsBackend for VulkanData {
    type Api = xr::Vulkan;
    type OpenVrTexture = *const vr::VRVulkanTextureData_t;
    type NiceFormat = vk::Format;

    #[inline]
    fn to_nice_format(format: u32) -> Self::NiceFormat {
        vk::Format::from_raw(format as _)
    }

    fn session_create_info(&self) -> <Self::Api as openxr::Graphics>::SessionCreateInfo {
        let queue_families = unsafe {
            self.instance
                .get_physical_device_queue_family_properties(self.physical_device)
        };
        let family = queue_families[self.queue_family_index as usize];
        let queue_index = (0..family.queue_count)
            .find(|idx| {
                let queue = unsafe { self.device.get_device_queue(self.queue_family_index, *idx) };
                queue == self.queue
            })
            .unwrap_or_else(|| {
                panic!(
                    "Could not find queue index for queue {:?} in family {}",
                    self.queue, self.queue_family_index
                )
            });

        xr::vulkan::SessionCreateInfo {
            instance: self.instance.handle().as_raw() as _,
            physical_device: self.physical_device.as_raw() as _,
            device: self.device.handle().as_raw() as _,
            queue_family_index: self.queue_family_index,
            queue_index,
        }
    }

    fn get_texture(texture: &vr::Texture_t) -> Self::OpenVrTexture {
        texture.handle.cast()
    }
    fn store_swapchain_images(&mut self, images: Vec<u64>, format: u32) {
        let images: Vec<vk::Image> = images.into_iter().map(vk::Image::from_raw).collect();
        let pool = unsafe {
            self.device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                        .queue_family_index(self.queue_family_index),
                    None,
                )
                .unwrap()
        };
        let bufs = unsafe {
            self.device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        // We have to copy 2 eyes per swapchain image
                        .command_buffer_count(images.len() as u32 * 2),
                )
                .unwrap()
        };

        if let Some(data) = self.real_data.replace(RealSessionData {
            images,
            format: vk::Format::from_raw(format as _),
            pool,
            bufs,
            overlay_pipeline: Default::default(),
        }) {
            unsafe {
                self.device.destroy_command_pool(data.pool, None);
            }
        }
    }

    fn swapchain_info_for_texture(
        &self,
        texture: *const vr::VRVulkanTextureData_t,
        bounds: vr::VRTextureBounds_t,
        color_space: vr::EColorSpace,
    ) -> xr::SwapchainCreateInfo<Self::Api> {
        let texture = unsafe { texture.as_ref() }.unwrap();
        let (extent, _) = texture_extent_from_bounds(texture, bounds);
        xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                | xr::SwapchainUsageFlags::TRANSFER_DST,
            format: get_colorspace_corrected_format(
                vk::Format::from_raw(texture.m_nFormat as _),
                color_space,
            )
            .as_raw() as _,
            sample_count: texture.m_nSampleCount,
            width: extent.width,
            height: extent.height,
            face_count: 1,
            array_size: 2,
            mip_count: 1,
        }
    }

    fn copy_texture_to_swapchain(
        &self,
        eye: vr::EVREye,
        texture: *const vr::VRVulkanTextureData_t,
        color_space: vr::EColorSpace,
        bounds: vr::VRTextureBounds_t,
        image_index: usize,
        submit_flags: vr::EVRSubmitFlags,
    ) -> xr::Extent2Di {
        let (texture, array_data) =
            if (submit_flags & vr::EVRSubmitFlags::VulkanTextureWithArrayData).0 > 0 {
                let data = unsafe { &*texture.cast::<vr::VRVulkanTextureArrayData_t>() };
                (&data._base, Some(data))
            } else {
                (unsafe { &*texture }, None)
            };

        let data = self.real_data.as_ref().unwrap();
        let swapchain_image = data.images[image_index];
        let buf = data.bufs[2 * image_index + eye as usize];

        let (extent, offset) = texture_extent_from_bounds(texture, bounds);
        log::trace!("{:?} extent: {:?} | bounds: {:?}", eye, extent, bounds);

        self.record_commands(buf, || unsafe {
            // transition swapchain image to TRANSFER_DST
            let swapchain_res = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: eye as u32,
                layer_count: 1,
            };

            self.device.cmd_pipeline_barrier(
                buf,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    dst_access_mask: vk::AccessFlags::TRANSFER_READ,
                    old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    image: swapchain_image,
                    subresource_range: swapchain_res,
                    ..Default::default()
                }],
            );

            // transfer image
            let subresource = vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            };

            let game_image = vk::Image::from_raw(texture.m_nImage);
            let game_layer = array_data.map(|d| d.m_unArrayIndex).unwrap_or(0);

            let copy = vk::ImageResolve {
                src_subresource: vk::ImageSubresourceLayers {
                    base_array_layer: game_layer,
                    ..subresource
                },
                src_offset: offset,
                dst_subresource: vk::ImageSubresourceLayers {
                    base_array_layer: eye as u32,
                    ..subresource
                },
                dst_offset: vk::Offset3D::default(),
                extent,
            };

            let game_format = get_colorspace_corrected_format(
                vk::Format::from_raw(texture.m_nFormat as _),
                color_space,
            );
            if texture.m_nSampleCount > 1 {
                self.device.cmd_resolve_image(
                    buf,
                    game_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    swapchain_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy],
                );
            } else if data.format != game_format {
                let end_img_offset = vk::Offset3D {
                    x: extent.width as _,
                    y: extent.height as _,
                    z: 1,
                };
                self.device.cmd_blit_image(
                    buf,
                    game_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    swapchain_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageBlit {
                        src_subresource: copy.src_subresource,
                        src_offsets: [copy.src_offset, end_img_offset],
                        dst_subresource: copy.dst_subresource,
                        dst_offsets: [copy.dst_offset, end_img_offset],
                    }],
                    vk::Filter::NEAREST,
                );
            } else {
                self.device.cmd_copy_image(
                    buf,
                    game_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    swapchain_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[
                        // SAFETY: ImageResolve and ImageCopy have the same fields and layout.
                        #[allow(unused_unsafe, clippy::missing_transmute_annotations)]
                        unsafe {
                            std::mem::transmute(copy)
                        },
                    ],
                );
            }

            // transition swapchain image back to OPTIMAL
            self.device.cmd_pipeline_barrier(
                buf,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags::empty(),
                    old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    image: swapchain_image,
                    subresource_range: swapchain_res,
                    ..Default::default()
                }],
            );
        });

        xr::Extent2Di {
            width: extent.width as _,
            height: extent.height as _,
        }
    }

    fn copy_overlay_to_swapchain(
        &mut self,
        texture: *const vr::VRVulkanTextureData_t,
        bounds: vr::VRTextureBounds_t,
        image_index: usize,
        alpha: f32,
    ) -> xr::Extent2Di {
        let mut data = self.real_data.as_ref().unwrap();
        let buf = data.bufs[image_index];
        let texture = unsafe { texture.as_ref() }.unwrap();
        let (extent, offset) = texture_extent_from_bounds(texture, bounds);
        let rect = vk::Rect2D {
            offset: vk::Offset2D {
                x: offset.x,
                y: offset.y,
            },
            extent: vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
        };
        let pipeline_data = match &data.overlay_pipeline {
            Some(d) => {
                assert_eq!(
                    d.image_format, data.format,
                    "Overlay image format unexpectedly changed"
                );
                d
            }
            None => {
                self.real_data.as_mut().unwrap().overlay_pipeline = Some(PipelineData::new(
                    &self.device,
                    vk::Format::from_raw(texture.m_nFormat as _),
                    data.format,
                    texture.m_nSampleCount,
                    &data.images,
                ));
                data = self.real_data.as_ref().unwrap();
                data.overlay_pipeline.as_ref().unwrap()
            }
        };

        let swapchain_view = pipeline_data.image_views[image_index];
        let game_view = unsafe {
            self.device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(vk::Image::from_raw(texture.m_nImage))
                        .format(vk::Format::from_raw(texture.m_nFormat as _))
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .components(vk::ComponentMapping::default())
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .unwrap()
        };
        let fb = unsafe {
            self.device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(pipeline_data.renderpass)
                        .attachments(&[game_view, swapchain_view])
                        .width(texture.m_nWidth)
                        .height(texture.m_nHeight)
                        .layers(1),
                    None,
                )
                .unwrap()
        };

        unsafe {
            self.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(pipeline_data.set)
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&[vk::DescriptorImageInfo {
                        sampler: pipeline_data.sampler,
                        image_view: game_view,
                        image_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    }])],
                &[],
            )
        }

        self.record_commands(buf, || unsafe {
            self.device.cmd_bind_pipeline(
                buf,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_data.pipeline,
            );
            self.device.cmd_set_viewport(
                buf,
                0,
                &[vk::Viewport {
                    width: extent.width as f32,
                    height: extent.width as f32,
                    x: 0.0,
                    y: 0.0,
                    min_depth: 0.0,
                    max_depth: 0.0,
                }],
            );
            self.device.cmd_set_scissor(buf, 0, &[rect]);
            self.device.cmd_bind_descriptor_sets(
                buf,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_data.layout,
                0,
                &[pipeline_data.set],
                &[],
            );
            let pc = [bounds.uMin, bounds.uMax, bounds.vMin, bounds.vMax];
            self.device.cmd_push_constants(
                buf,
                pipeline_data.layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                pc.align_to().1,
            );
            self.device.cmd_push_constants(
                buf,
                pipeline_data.layout,
                vk::ShaderStageFlags::FRAGMENT,
                std::mem::size_of_val(&pc) as u32,
                std::slice::from_ref(&alpha).align_to().1,
            );
            self.device.cmd_begin_render_pass(
                buf,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(pipeline_data.renderpass)
                    .framebuffer(fb)
                    .render_area(rect),
                vk::SubpassContents::INLINE,
            );
            self.device.cmd_draw(buf, 4, 1, 0, 0);

            self.device.cmd_end_render_pass(buf);
        });

        unsafe {
            self.device.destroy_framebuffer(fb, None);
            self.device.destroy_image_view(game_view, None);
        }

        xr::Extent2Di {
            width: extent.width as _,
            height: extent.height as _,
        }
    }
}
impl VulkanData {
    pub fn record_commands(&self, buf: vk::CommandBuffer, cmds: impl FnOnce()) {
        unsafe {
            self.device
                .begin_command_buffer(
                    buf,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
        }

        cmds();

        unsafe {
            self.device.end_command_buffer(buf).unwrap();

            self.device
                .queue_submit(
                    self.queue,
                    &[vk::SubmitInfo::default().command_buffers(&[buf])],
                    vk::Fence::null(),
                )
                .unwrap();
        }
    }

    pub fn new(data: &vr::VRVulkanTextureData_t) -> Self {
        let entry = new_entry();
        let instance = unsafe {
            ash::Instance::load(
                entry.static_fn(),
                vk::Instance::from_raw(data.m_pInstance as _),
            )
        };
        let device = unsafe {
            ash::Device::load(
                instance.fp_v1_0(),
                vk::Device::from_raw(data.m_pDevice as _),
            )
        };

        Self {
            _entry: entry,
            instance,
            physical_device: vk::PhysicalDevice::from_raw(data.m_pPhysicalDevice as _),
            device,
            queue: vk::Queue::from_raw(data.m_pQueue as _),
            queue_family_index: data.m_nQueueFamilyIndex,
            real_data: Default::default(),
        }
    }

    pub fn new_temporary(xr_instance: &xr::Instance, system_id: xr::SystemId) -> Self {
        let entry = new_entry();

        let inst_exts = xr_instance
            .vulkan_legacy_instance_extensions(system_id)
            .unwrap();
        let inst_exts: Vec<CString> = inst_exts
            .split_ascii_whitespace()
            .map(|ext| CString::new(ext).unwrap())
            .collect();
        let inst_exts: Vec<*const c_char> = inst_exts.iter().map(|ext| ext.as_ptr()).collect();

        let instance = unsafe {
            entry
                .create_instance(
                    &vk::InstanceCreateInfo::default()
                        .application_info(
                            &vk::ApplicationInfo::default()
                                .api_version(vk::API_VERSION_1_0)
                                .application_name(c"XRizer temporary session"),
                        )
                        .enabled_extension_names(&inst_exts),
                    None,
                )
                .expect("Failed to create temporary Vulkan instance")
        };

        let physical_device = vk::PhysicalDevice::from_raw(unsafe {
            xr_instance
                .vulkan_graphics_device(system_id, instance.handle().as_raw() as _)
                .expect("Failed to get temporary Vulkan physical device") as _
        });
        let dev_exts = xr_instance
            .vulkan_legacy_device_extensions(system_id)
            .unwrap();
        let dev_exts: Vec<CString> = dev_exts
            .split_ascii_whitespace()
            .map(|ext| CString::new(ext).unwrap())
            .collect();
        let dev_exts: Vec<*const c_char> = dev_exts.iter().map(|ext| ext.as_ptr()).collect();

        // find whatever graphics queue family
        let queue_family_index =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
                .into_iter()
                .enumerate()
                .find_map(|(idx, family)| {
                    (family.queue_flags.contains(vk::QueueFlags::GRAPHICS)).then_some(idx)
                })
                .unwrap() as u32;

        let device = unsafe {
            instance
                .create_device(
                    physical_device,
                    &vk::DeviceCreateInfo::default()
                        .queue_create_infos(std::slice::from_ref(
                            &vk::DeviceQueueCreateInfo::default()
                                .queue_family_index(queue_family_index)
                                .queue_priorities(&[1.0]),
                        ))
                        .enabled_extension_names(&dev_exts),
                    None,
                )
                .expect("Could not create temporary vulkan device")
        };

        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        Self {
            _entry: entry,
            instance,
            physical_device,
            device,
            queue,
            queue_family_index,
            real_data: Default::default(),
        }
    }
}

struct PipelineData {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    renderpass: vk::RenderPass,
    image_views: Vec<vk::ImageView>,
    image_format: vk::Format,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    sampler: vk::Sampler,
}

impl PipelineData {
    fn new(
        device: &ash::Device,
        source_format: vk::Format,
        target_format: vk::Format,
        sample_count: u32,
        images: &[vk::Image],
    ) -> Self {
        let samples = match sample_count {
            1 => vk::SampleCountFlags::TYPE_1,
            2 => vk::SampleCountFlags::TYPE_2,
            4 => vk::SampleCountFlags::TYPE_4,
            8 => vk::SampleCountFlags::TYPE_8,
            16 => vk::SampleCountFlags::TYPE_16,
            32 => vk::SampleCountFlags::TYPE_32,
            64 => vk::SampleCountFlags::TYPE_64,
            other => {
                warn!("unexpected sample count {other} for pipeline - using 1");
                vk::SampleCountFlags::TYPE_1
            }
        };
        let attachments = [
            // game image
            vk::AttachmentDescription {
                format: source_format,
                samples,
                load_op: vk::AttachmentLoadOp::LOAD,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                final_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ..Default::default()
            },
            // swapchain image
            vk::AttachmentDescription {
                format: target_format,
                samples,
                load_op: vk::AttachmentLoadOp::DONT_CARE,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ..Default::default()
            },
        ];

        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .input_attachments(&[vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }])
            .color_attachments(&[vk::AttachmentReference {
                attachment: 1,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }]);
        let info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(std::slice::from_ref(&subpass));

        let renderpass = unsafe { device.create_render_pass(&info, None).unwrap() };

        let load_module = |stage, bytes| {
            struct ShaderModule<'a>(&'a ash::Device, vk::ShaderModule);
            impl Drop for ShaderModule<'_> {
                fn drop(&mut self) {
                    unsafe {
                        self.0.destroy_shader_module(self.1, None);
                    }
                }
            }

            let module = unsafe {
                device
                    .create_shader_module(
                        &vk::ShaderModuleCreateInfo::default()
                            .code(&ash::util::read_spv(&mut std::io::Cursor::new(bytes)).unwrap()),
                        None,
                    )
                    .unwrap()
            };

            (
                ShaderModule(device, module),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(stage)
                    .module(module)
                    .name(c"main"),
            )
        };

        let (_vert_module, vert_stage) = load_module(
            vk::ShaderStageFlags::VERTEX,
            &include_bytes!(concat!(env!("OUT_DIR"), "/vert_overlay.spv"))[..],
        );
        let (_frag_module, frag_stage) = load_module(
            vk::ShaderStageFlags::FRAGMENT,
            &include_bytes!(concat!(env!("OUT_DIR"), "/frag_overlay.spv"))[..],
        );

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(std::slice::from_ref(&binding)),
                    None,
                )
                .unwrap()
        };
        let pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&[vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                            descriptor_count: 1,
                        }]),
                    None,
                )
                .unwrap()
        };
        let set = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(pool)
                        .set_layouts(&[set_layout]),
                )
                .unwrap()[0]
        };

        let texture_coordinates_pc = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: std::mem::size_of::<[f32; 4]>() as u32,
        };
        let alpha_pc = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
            offset: texture_coordinates_pc.size,
            size: std::mem::size_of::<f32>() as u32,
        };
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&set_layout))
                        .push_constant_ranges(&[texture_coordinates_pc, alpha_pc]),
                    None,
                )
                .unwrap()
        };

        let stages = [vert_stage, frag_stage];
        let input_state = Default::default();
        let assembly_state = vk::PipelineInputAssemblyStateCreateInfo {
            topology: vk::PrimitiveTopology::TRIANGLE_STRIP,
            ..Default::default()
        };
        let tess_state = Default::default();
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let rast_state = vk::PipelineRasterizationStateCreateInfo::default()
            .cull_mode(vk::CullModeFlags::NONE)
            .line_width(1.0)
            .depth_bias_enable(false);
        let multi_state = vk::PipelineMultisampleStateCreateInfo::default();
        let depth_state = vk::PipelineDepthStencilStateCreateInfo::default();
        let blend = vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::TRUE,
            src_color_blend_factor: vk::BlendFactor::ONE,
            dst_color_blend_factor: vk::BlendFactor::ZERO,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ONE,
            dst_alpha_blend_factor: vk::BlendFactor::ZERO,
            alpha_blend_op: vk::BlendOp::ADD,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        };
        let blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&blend));
        let d_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&d_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&input_state)
            .input_assembly_state(&assembly_state)
            .tessellation_state(&tess_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rast_state)
            .multisample_state(&multi_state)
            .depth_stencil_state(&depth_state)
            .color_blend_state(&blend_state)
            .dynamic_state(&dynamic)
            .render_pass(renderpass)
            .subpass(0)
            .layout(pipeline_layout)
            .base_pipeline_handle(vk::Pipeline::null())
            .base_pipeline_index(-1);

        let pipeline = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&pipeline_info),
                    None,
                )
                .unwrap()[0]
        };

        let image_views = images
            .iter()
            .copied()
            .map(|img| unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(target_format)
                            .components(vk::ComponentMapping::default())
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
                        None,
                    )
                    .unwrap()
            })
            .collect();

        let sampler = unsafe {
            device
                .create_sampler(&vk::SamplerCreateInfo::default(), None)
                .unwrap()
        };

        Self {
            pipeline,
            layout: pipeline_layout,
            renderpass,
            image_views,
            image_format: target_format,
            pool,
            set,
            sampler,
        }
    }
}

#[inline]
fn get_colorspace_corrected_format(format: vk::Format, color_space: vr::EColorSpace) -> vk::Format {
    static UNSUPPORTED: LazyLock<Mutex<HashSet<vk::Format>>> = LazyLock::new(Mutex::default);
    // https://github.com/ValveSoftware/openvr/wiki/Vulkan#image-formats
    match color_space {
        vr::EColorSpace::Auto | vr::EColorSpace::Gamma => match format {
            vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => vk::Format::R8G8B8A8_SRGB,
            vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => vk::Format::B8G8R8A8_SRGB,
            vk::Format::BC3_SRGB_BLOCK => format,
            _ => {
                if UNSUPPORTED.lock().unwrap().insert(format) {
                    warn!("Unhandled texture format: {format:?}");
                }
                format
            }
        },
        vr::EColorSpace::Linear => todo!("Linear colorspace not implemented yet"),
    }
}

fn texture_extent_from_bounds(
    texture: &vr::VRVulkanTextureData_t,
    bounds: vr::VRTextureBounds_t,
) -> (vk::Extent3D, vk::Offset3D) {
    let width_min = bounds.uMin * texture.m_nWidth as f32;
    let width_max = bounds.uMax * texture.m_nWidth as f32;
    let height_min = bounds.vMin * texture.m_nHeight as f32;
    let height_max = bounds.vMax * texture.m_nHeight as f32;

    (
        vk::Extent3D {
            width: (width_max - width_min).abs() as u32,
            height: (height_max - height_min).abs() as u32,
            depth: 1,
        },
        vk::Offset3D {
            x: width_min.min(width_max) as i32,
            y: height_min.min(height_max) as i32,
            z: 0,
        },
    )
}

fn new_entry() -> ash::Entry {
    #[cfg(not(test))]
    unsafe {
        ash::Entry::load().unwrap()
    }

    #[cfg(test)]
    unsafe {
        ash::Entry::from_static_fn(ash::StaticFn {
            get_instance_proc_addr: fakexr::vulkan::get_instance_proc_addr,
        })
    }
}
