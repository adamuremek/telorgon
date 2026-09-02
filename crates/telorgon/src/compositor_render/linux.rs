use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::sync::Arc;

use crate::compositor_wayland::{
    BufferTransform, DmaBufFormat, DmaBufImage, ShmFormat, ShmImage, ShmImageRegion,
    ViewportSource, ViewportState, WaylandBufferId,
};
use crate::core::{RectI, SizeI};
use crate::render::{
    ImageAlphaMode, ImageColorEncoding, ImageId, ImagePixelFormat, ImageResource,
    ImageResourceUpdate,
};
use crate::renderer_vulkan::{
    HostedImageUse, VulkanDevice, VulkanDmaBufImport, VulkanDmaBufPlane, VulkanExternalImageLease,
    VulkanExternalImageOrigin, VulkanScene,
};
use ash::vk;

use crate::compositor_render::CompositorRenderError;

pub fn imported_image_id(buffer: WaylandBufferId) -> ImageId {
    ImageId(buffer.get())
}

pub fn shm_image_resource(
    buffer: WaylandBufferId,
    content_version: u64,
    image: ShmImage,
) -> Result<ImageResource, CompositorRenderError> {
    if content_version == 0 {
        return Err(CompositorRenderError::new(
            "SHM content version must be nonzero",
        ));
    }
    let descriptor = image.descriptor;
    let width = usize::try_from(descriptor.size.width)
        .map_err(|_| CompositorRenderError::new("invalid SHM width"))?;
    let height = usize::try_from(descriptor.size.height)
        .map_err(|_| CompositorRenderError::new("invalid SHM height"))?;
    let source_pixel_bytes = descriptor
        .format
        .bytes_per_pixel()
        .ok_or_else(|| CompositorRenderError::new("unsupported SHM pixel format"))?
        as usize;
    let row_bytes = width
        .checked_mul(source_pixel_bytes)
        .ok_or_else(|| CompositorRenderError::new("SHM row size overflow"))?;
    let stride = descriptor.stride as usize;
    if stride < row_bytes
        || image.pixels.len()
            < stride
                .checked_mul(height)
                .ok_or_else(|| CompositorRenderError::new("SHM extent overflow"))?
    {
        return Err(CompositorRenderError::new(
            "SHM bytes do not cover the declared image",
        ));
    }

    let (pixel_format, pixels) =
        convert_shm_rows(descriptor.format, width, height, stride, image.pixels)?;
    Ok(ImageResource {
        image: imported_image_id(buffer),
        content_version,
        extent: descriptor.size,
        color_encoding: ImageColorEncoding::Srgb,
        alpha_mode: shm_alpha_mode(descriptor.format),
        pixel_format,
        pixels: Arc::from(pixels),
    })
}

pub fn shm_image_update(
    buffer: WaylandBufferId,
    content_version: u64,
    image: ShmImageRegion,
) -> Result<ImageResourceUpdate, CompositorRenderError> {
    if content_version == 0 {
        return Err(CompositorRenderError::new(
            "SHM content version must be nonzero",
        ));
    }
    let width = image.rect.width as usize;
    let height = image.rect.height as usize;
    let (pixel_format, pixels) = convert_shm_rows(
        image.descriptor.format,
        width,
        height,
        image.row_bytes,
        image.pixels,
    )?;
    Ok(ImageResourceUpdate {
        image: imported_image_id(buffer),
        content_version,
        extent: image.descriptor.size,
        rect: image.rect,
        row_bytes: width * 4,
        color_encoding: ImageColorEncoding::Srgb,
        alpha_mode: shm_alpha_mode(image.descriptor.format),
        pixel_format,
        pixels: Arc::from(pixels),
    })
}

pub fn shm_image_metadata(
    format: ShmFormat,
) -> Result<(ImagePixelFormat, ImageAlphaMode), CompositorRenderError> {
    if format.bytes_per_pixel().is_none() {
        return Err(CompositorRenderError::new("unsupported SHM pixel format"));
    }
    let pixel_format = match format {
        ShmFormat::Argb8888 | ShmFormat::Xrgb8888 => ImagePixelFormat::Bgra8,
        ShmFormat::Abgr8888 | ShmFormat::Xbgr8888 | ShmFormat::Rgb565 => ImagePixelFormat::Rgba8,
        ShmFormat::Other(_) => unreachable!("bytes-per-pixel rejected unknown format"),
    };
    Ok((pixel_format, shm_alpha_mode(format)))
}

fn shm_alpha_mode(format: ShmFormat) -> ImageAlphaMode {
    match format {
        ShmFormat::Argb8888 | ShmFormat::Abgr8888 => ImageAlphaMode::Premultiplied,
        _ => ImageAlphaMode::Opaque,
    }
}

fn convert_shm_rows(
    format: ShmFormat,
    width: usize,
    height: usize,
    source_stride: usize,
    source_pixels: Vec<u8>,
) -> Result<(ImagePixelFormat, Vec<u8>), CompositorRenderError> {
    let source_pixel_bytes = format
        .bytes_per_pixel()
        .ok_or_else(|| CompositorRenderError::new("unsupported SHM pixel format"))?
        as usize;
    let source_row_bytes = width
        .checked_mul(source_pixel_bytes)
        .ok_or_else(|| CompositorRenderError::new("SHM row size overflow"))?;
    let output_row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| CompositorRenderError::new("SHM output row size overflow"))?;
    let output_len = output_row_bytes
        .checked_mul(height)
        .ok_or_else(|| CompositorRenderError::new("SHM image size overflow"))?;
    if source_stride < source_row_bytes
        || source_pixels.len() < source_stride.saturating_mul(height)
    {
        return Err(CompositorRenderError::new(
            "SHM bytes do not cover the declared rows",
        ));
    }
    let (pixel_format, _) = shm_image_metadata(format)?;
    if format != ShmFormat::Rgb565 && source_stride == output_row_bytes {
        return Ok((pixel_format, source_pixels));
    }
    let mut output = vec![0_u8; output_len];
    for row in 0..height {
        let source = &source_pixels[row * source_stride..row * source_stride + source_row_bytes];
        let target = &mut output[row * output_row_bytes..(row + 1) * output_row_bytes];
        if format == ShmFormat::Rgb565 {
            for (source, target) in source.chunks_exact(2).zip(target.chunks_exact_mut(4)) {
                let value = u16::from_le_bytes([source[0], source[1]]);
                let red = ((value >> 11) & 0x1f) as u8;
                let green = ((value >> 5) & 0x3f) as u8;
                let blue = (value & 0x1f) as u8;
                target.copy_from_slice(&[
                    (red << 3) | (red >> 2),
                    (green << 2) | (green >> 4),
                    (blue << 3) | (blue >> 2),
                    255,
                ]);
            }
        } else {
            target.copy_from_slice(source);
        }
    }
    Ok((pixel_format, output))
}

/// Applies `wl_surface` buffer transform/scale and the committed viewporter state to a retained
/// four-channel image. Sampling is deterministic nearest-neighbor so the software reference and a future
/// Vulkan compositor can share the exact surface geometry contract.
pub fn transform_surface_image(
    image: ImageResource,
    buffer_scale: i32,
    transform: BufferTransform,
    viewport: Option<ViewportState>,
) -> Result<ImageResource, CompositorRenderError> {
    if buffer_scale <= 0 {
        return Err(CompositorRenderError::new("buffer scale must be positive"));
    }
    if buffer_scale == 1 && transform == BufferTransform::Normal && viewport.is_none() {
        return Ok(image);
    }
    let input_width = usize::try_from(image.extent.width)
        .map_err(|_| CompositorRenderError::new("invalid image width"))?;
    let input_height = usize::try_from(image.extent.height)
        .map_err(|_| CompositorRenderError::new("invalid image height"))?;
    let expected = input_width
        .checked_mul(input_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| CompositorRenderError::new("surface image size overflow"))?;
    if input_width == 0 || input_height == 0 || image.pixels.len() < expected {
        return Err(CompositorRenderError::new(
            "surface image does not cover its extent",
        ));
    }
    let swap_axes = matches!(
        transform,
        BufferTransform::Rotate90
            | BufferTransform::Rotate270
            | BufferTransform::Flipped90
            | BufferTransform::Flipped270
    );
    let transformed_width = if swap_axes { input_height } else { input_width };
    let transformed_height = if swap_axes { input_width } else { input_height };
    let scale = buffer_scale as usize;
    if transformed_width % scale != 0 || transformed_height % scale != 0 {
        return Err(CompositorRenderError::new(
            "transformed buffer extent is not divisible by its scale",
        ));
    }
    let logical_width = transformed_width / scale;
    let logical_height = transformed_height / scale;
    let logical_len = logical_width
        .checked_mul(logical_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| CompositorRenderError::new("logical surface size overflow"))?;
    let mut logical = vec![0_u8; logical_len];
    for y in 0..logical_height {
        for x in 0..logical_width {
            let transformed_x = (x * scale + scale / 2).min(transformed_width - 1);
            let transformed_y = (y * scale + scale / 2).min(transformed_height - 1);
            let (source_x, source_y) = transformed_coordinate(
                transform,
                transformed_x,
                transformed_y,
                input_width,
                input_height,
            );
            let source = (source_y * input_width + source_x) * 4;
            let target = (y * logical_width + x) * 4;
            logical[target..target + 4].copy_from_slice(&image.pixels[source..source + 4]);
        }
    }

    let viewport = viewport.unwrap_or_default();
    let source = viewport.source.unwrap_or(ViewportSource {
        x: 0.0,
        y: 0.0,
        width: logical_width as f64,
        height: logical_height as f64,
    });
    if !source.x.is_finite()
        || !source.y.is_finite()
        || !source.width.is_finite()
        || !source.height.is_finite()
        || source.x < 0.0
        || source.y < 0.0
        || source.width <= 0.0
        || source.height <= 0.0
        || source.x + source.width > logical_width as f64
        || source.y + source.height > logical_height as f64
    {
        return Err(CompositorRenderError::new(
            "viewport source lies outside the logical image",
        ));
    }
    let destination = viewport.destination.unwrap_or(SizeI {
        width: source.width as i32,
        height: source.height as i32,
    });
    if destination.width <= 0 || destination.height <= 0 {
        return Err(CompositorRenderError::new(
            "viewport destination must be positive",
        ));
    }
    let destination_width = destination.width as usize;
    let destination_height = destination.height as usize;
    let destination_len = destination_width
        .checked_mul(destination_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= 512 * 1024 * 1024)
        .ok_or_else(|| CompositorRenderError::new("viewport destination is too large"))?;
    let mut pixels = vec![0_u8; destination_len];
    for y in 0..destination_height {
        for x in 0..destination_width {
            let sample_x = (source.x + (x as f64 + 0.5) * source.width / destination_width as f64)
                .floor() as usize;
            let sample_y = (source.y + (y as f64 + 0.5) * source.height / destination_height as f64)
                .floor() as usize;
            let sample_x = sample_x.min(logical_width - 1);
            let sample_y = sample_y.min(logical_height - 1);
            let source_index = (sample_y * logical_width + sample_x) * 4;
            let target_index = (y * destination_width + x) * 4;
            pixels[target_index..target_index + 4]
                .copy_from_slice(&logical[source_index..source_index + 4]);
        }
    }
    Ok(ImageResource {
        extent: destination,
        pixels: Arc::from(pixels),
        ..image
    })
}

fn transformed_coordinate(
    transform: BufferTransform,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> (usize, usize) {
    match transform {
        BufferTransform::Normal => (x, y),
        BufferTransform::Rotate90 => (y, height - 1 - x),
        BufferTransform::Rotate180 => (width - 1 - x, height - 1 - y),
        BufferTransform::Rotate270 => (width - 1 - y, x),
        BufferTransform::Flipped => (width - 1 - x, y),
        BufferTransform::Flipped90 => (width - 1 - y, height - 1 - x),
        BufferTransform::Flipped180 => (x, height - 1 - y),
        BufferTransform::Flipped270 => (y, x),
    }
}

pub struct DmaBufImporter {
    capabilities: Vec<crate::renderer_vulkan::VulkanDmaBufFormatCapability>,
    next_generation: BTreeMap<WaylandBufferId, u64>,
}

impl DmaBufImporter {
    pub fn new(device: &VulkanDevice) -> Result<Self, CompositorRenderError> {
        let capabilities = device
            .dma_buf_import_capabilities(vk::ImageUsageFlags::SAMPLED)
            .map_err(render_error)?;
        Ok(Self {
            capabilities,
            next_generation: BTreeMap::new(),
        })
    }

    pub fn advertised_formats(&self) -> Vec<DmaBufFormat> {
        self.capabilities
            .iter()
            .filter(|capability| capability.importable() && capability.plane_count == 1)
            .map(|capability| DmaBufFormat {
                fourcc: capability.drm_fourcc,
                modifier: capability.drm_modifier,
            })
            .collect()
    }

    /// Imports and binds one committed DMA-BUF generation without copying its pixels.
    ///
    /// The protocol runtime has validated the descriptor and owns every FD. The selected Vulkan
    /// capability comes from this exact device, and this method derives allocation bounds from the
    /// DMA-BUF itself before entering the renderer's unsafe host boundary.
    pub fn import_and_bind(
        &mut self,
        device: &VulkanDevice,
        scene: &mut VulkanScene,
        buffer: WaylandBufferId,
        content_version: u64,
        image: DmaBufImage,
        acquire: Option<OwnedFd>,
        damage: Vec<RectI>,
    ) -> Result<(), CompositorRenderError> {
        if content_version == 0 || image.planes.len() != 1 || image.descriptor.planes.len() != 1 {
            return Err(CompositorRenderError::new(
                "DMA-BUF import requires one plane and a nonzero content version",
            ));
        }
        if image.descriptor.flags.interlaced || image.descriptor.flags.bottom_field_first {
            return Err(CompositorRenderError::new(
                "interlaced DMA-BUF content is not supported by Telorgon rendering",
            ));
        }
        let plane_descriptor = image.descriptor.planes[0];
        let capability = self
            .capabilities
            .iter()
            .copied()
            .find(|capability| {
                capability.drm_fourcc == image.descriptor.format
                    && capability.drm_modifier == plane_descriptor.modifier
                    && capability.plane_count == 1
            })
            .ok_or_else(|| {
                CompositorRenderError::new("DMA-BUF tuple was not advertised by this Vulkan device")
            })?;
        let allocation_size = fd_allocation_size(&image.planes[0])?;
        let minimum_size = u64::from(plane_descriptor.stride)
            .checked_mul(image.descriptor.size.height as u64)
            .ok_or_else(|| CompositorRenderError::new("DMA-BUF extent overflow"))?;
        if allocation_size < minimum_size {
            return Err(CompositorRenderError::new(
                "DMA-BUF allocation is smaller than its declared rows",
            ));
        }
        let generation = self.next_generation.entry(buffer).or_insert(0);
        *generation = generation
            .checked_add(1)
            .ok_or_else(|| CompositorRenderError::new("DMA-BUF lease generation exhausted"))?;
        let plane = image.planes.into_iter().next().expect("one plane checked");
        let import = VulkanDmaBufImport {
            planes: vec![VulkanDmaBufPlane {
                memory: plane,
                memory_index: 0,
                offset: u64::from(plane_descriptor.offset),
                size: allocation_size.saturating_sub(u64::from(plane_descriptor.offset)),
                row_pitch: plane_descriptor.stride,
                allocation_size,
            }],
            drm_fourcc: image.descriptor.format,
            drm_modifier: plane_descriptor.modifier,
            format: capability.format,
            extent: vk::Extent2D {
                width: image.descriptor.size.width as u32,
                height: image.descriptor.size.height as u32,
            },
            usage: vk::ImageUsageFlags::SAMPLED,
            content_version,
            lease_generation: *generation,
            color_encoding: capability.color_encoding,
            alpha_mode: capability.alpha_mode,
            origin: if image.descriptor.flags.y_invert {
                VulkanExternalImageOrigin::BottomLeft
            } else {
                VulkanExternalImageOrigin::TopLeft
            },
            initial_use: HostedImageUse::General,
            final_use: HostedImageUse::General,
            acquire,
            damage,
            protected: false,
        };
        let lease: VulkanExternalImageLease =
            unsafe { device.import_dma_buf(import) }.map_err(render_error)?;
        scene
            .bind_external_image(imported_image_id(buffer), lease)
            .map_err(render_error)
    }
}

fn fd_allocation_size(fd: &OwnedFd) -> Result<u64, CompositorRenderError> {
    let file = std::fs::File::from(fd.try_clone().map_err(io_error)?);
    let size = file.metadata().map_err(io_error)?.len();
    if size == 0 {
        Err(CompositorRenderError::new(
            "DMA-BUF did not expose a nonzero allocation size",
        ))
    } else {
        Ok(size)
    }
}

fn io_error(error: std::io::Error) -> CompositorRenderError {
    CompositorRenderError::new(error.to_string())
}

fn render_error(error: crate::render::RenderError) -> CompositorRenderError {
    CompositorRenderError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor_wayland::{ShmBuffer, ShmFormat};
    use crate::core::SizeI;

    #[test]
    fn xrgb_shm_retains_native_bgra_bytes_and_is_forced_opaque() {
        let buffer = WaylandBufferId::from_raw(7).unwrap();
        let resource = shm_image_resource(
            buffer,
            1,
            ShmImage {
                descriptor: ShmBuffer {
                    offset: 0,
                    size: SizeI {
                        width: 1,
                        height: 1,
                    },
                    stride: 4,
                    format: ShmFormat::Xrgb8888,
                },
                pixels: vec![3, 2, 1, 0],
            },
        )
        .unwrap();
        assert_eq!(&*resource.pixels, &[3, 2, 1, 0]);
        assert_eq!(resource.pixel_format, ImagePixelFormat::Bgra8);
        assert_eq!(resource.alpha_mode, ImageAlphaMode::Opaque);
    }

    #[test]
    fn damaged_argb_region_stays_tightly_packed_and_native_bgra() {
        let buffer = WaylandBufferId::from_raw(7).unwrap();
        let rect = RectI {
            x: 3,
            y: 4,
            width: 2,
            height: 1,
        };
        let update = shm_image_update(
            buffer,
            2,
            ShmImageRegion {
                descriptor: ShmBuffer {
                    offset: 16,
                    size: SizeI {
                        width: 20,
                        height: 10,
                    },
                    stride: 96,
                    format: ShmFormat::Argb8888,
                },
                rect,
                row_bytes: 8,
                pixels: vec![3, 2, 1, 4, 7, 6, 5, 8],
            },
        )
        .unwrap();

        assert_eq!(update.rect, rect);
        assert_eq!(update.row_bytes, 8);
        assert_eq!(update.pixel_format, ImagePixelFormat::Bgra8);
        assert_eq!(update.alpha_mode, ImageAlphaMode::Premultiplied);
        assert_eq!(&*update.pixels, &[3, 2, 1, 4, 7, 6, 5, 8]);
    }

    #[test]
    fn surface_transform_scale_and_viewport_change_the_retained_extent() {
        let image = ImageResource {
            image: ImageId(7),
            content_version: 1,
            extent: SizeI {
                width: 2,
                height: 2,
            },
            color_encoding: ImageColorEncoding::Srgb,
            alpha_mode: ImageAlphaMode::Opaque,
            pixel_format: ImagePixelFormat::Rgba8,
            pixels: Arc::from([
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ]),
        };
        let scaled = transform_surface_image(image.clone(), 2, BufferTransform::Normal, None)
            .expect("scale is valid");
        assert_eq!(
            scaled.extent,
            SizeI {
                width: 1,
                height: 1
            }
        );
        assert_eq!(&*scaled.pixels, &[255, 255, 255, 255]);

        let cropped = transform_surface_image(
            image,
            1,
            BufferTransform::Rotate90,
            Some(ViewportState {
                source: Some(ViewportSource {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 2.0,
                }),
                destination: Some(SizeI {
                    width: 1,
                    height: 2,
                }),
            }),
        )
        .expect("viewport is valid");
        assert_eq!(
            cropped.extent,
            SizeI {
                width: 1,
                height: 2
            }
        );
        assert_eq!(&cropped.pixels[..4], &[0, 0, 255, 255]);
    }
}
