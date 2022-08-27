use image::{Pixel, Rgba};
use std::num::NonZeroU32;
use wgpu::util::DeviceExt;

use crate::silica::BlendingMode;

const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

#[derive(Debug)]
pub struct LogicalDevice {
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub adapter: wgpu::Adapter,
    pub queue: wgpu::Queue,
}

impl LogicalDevice {
    const ADAPTER_OPTIONS: wgpu::RequestAdapterOptions<'static> = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    };

    pub async fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let adapter = instance.request_adapter(&Self::ADAPTER_OPTIONS).await?;
        Self::from_adapter(instance, adapter).await
    }

    pub async fn with_window(window: &winit::window::Window) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Self::ADAPTER_OPTIONS
            })
            .await?;
        Self::from_adapter(instance, adapter).await
    }

    async fn from_adapter(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Option<Self> {
        dbg!(adapter.get_info());
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::default() | wgpu::Features::TEXTURE_BINDING_ARRAY | wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY | wgpu::Features::BUFFER_BINDING_ARRAY,
                    ..Default::default()
                },
                None,
            )
            .await
            .ok()?;

        Some(Self {
            instance,
            device,
            adapter,
            queue,
        })
    }
}

#[derive(Debug)]
pub struct GpuTexture {
    pub size: wgpu::Extent3d,
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl GpuTexture {
    pub fn empty(device: &wgpu::Device, width: u32, height: u32, label: Option<&str>) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // Canvas texture
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            label,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture,
            view,
            size,
        }
    }

    pub fn replace(
        &self,
        queue: &wgpu::Queue,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            &data,
            // The layout of the texture
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(4 * width),
                rows_per_image: NonZeroU32::new(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BufferDimensions {
    pub width: u32,
    pub height: u32,
    pub unpadded_bytes_per_row: u32,
    pub padded_bytes_per_row: u32,
}

impl BufferDimensions {
    fn new(width: u32, height: u32) -> Self {
        // It is a WebGPU requirement that ImageCopyBuffer.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
        // So we calculate padded_bytes_per_row by rounding unpadded_bytes_per_row
        // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
        // https://en.wikipedia.org/wiki/Data_structure_alignment#Computing_padding
        let bytes_per_pixel =
            (usize::from(Rgba::<u8>::CHANNEL_COUNT) * std::mem::size_of::<u8>()) as u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct Vertex {
    position: [f32; 3],
    bg_coords: [f32; 2],
    fg_coords: [f32; 2],
}

const SQUARE_VERTICES: [Vertex; 4] = [
    Vertex {
        position: [-1.0, 1.0, 0.0],
        bg_coords: [0.0, 0.0],
        fg_coords: [0.0, 1.0],
    },
    Vertex {
        position: [-1.0, -1.0, 0.0],
        bg_coords: [0.0, 1.0],
        fg_coords: [0.0, 0.0],
    },
    Vertex {
        position: [1.0, 1.0, 0.0],
        bg_coords: [1.0, 0.0],
        fg_coords: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0, 0.0],
        bg_coords: [1.0, 1.0],
        fg_coords: [1.0, 0.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 3];

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LayerContext {
    opacity: f32,
    blend: u32,
    _padding: [f32; 2]
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2 + 3]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

pub struct CompositeLayer<'a> {
    pub texture: &'a GpuTexture,
    pub clipped: Option<usize>,
    pub opacity: f32,
    pub blend: BlendingMode,
    pub name: Option<&'a str>,
}

pub struct RenderState<'device> {
    pub handle: &'device LogicalDevice,
    pub buffer_dimensions: BufferDimensions,
    pub texture_extent: wgpu::Extent3d,
    vertices: [Vertex; 4],
    background: Option<[f32; 4]>,
    constant_bind_group: wgpu::BindGroup,
    blending_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    filled_clipping_mask: wgpu::Texture,
    max_layers: u32,
}

impl<'device> RenderState<'device> {
    fn texture_bind_group_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                view_dimension: wgpu::TextureViewDimension::D2,
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
            },
            count: None,
        }
    }

    pub fn flip_vertices(&mut self, flip_hv: (bool, bool)) {
        for v in &mut self.vertices {
            v.fg_coords = [
                if flip_hv.0 {
                    1.0 - v.fg_coords[0]
                } else {
                    v.fg_coords[0]
                },
                if flip_hv.1 {
                    1.0 - v.fg_coords[1]
                } else {
                    v.fg_coords[1]
                },
            ];
        }
        self.reload_vertices_buffer();
    }

    pub fn rotate_vertices(&mut self, ccw: bool) {
        let temp = self.vertices[0].fg_coords;
        if ccw {
            self.vertices[0].fg_coords = self.vertices[1].fg_coords;
            self.vertices[1].fg_coords = self.vertices[3].fg_coords;
            self.vertices[3].fg_coords = self.vertices[2].fg_coords;
            self.vertices[2].fg_coords = temp;
        } else {
            self.vertices[0].fg_coords = self.vertices[2].fg_coords;
            self.vertices[2].fg_coords = self.vertices[3].fg_coords;
            self.vertices[3].fg_coords = self.vertices[1].fg_coords;
            self.vertices[1].fg_coords = temp;
        }
        self.reload_vertices_buffer();
    }

    pub fn tranpose_dimensions(&mut self) {
        let buffer_dimensions =
            BufferDimensions::new(self.buffer_dimensions.height, self.buffer_dimensions.width);
        // The output buffer lets us retrieve the data as an array

        let texture_extent = wgpu::Extent3d {
            width: buffer_dimensions.width,
            height: buffer_dimensions.height,
            depth_or_array_layers: 1,
        };

        let filled_clipping_mask = {
            let texture = self.handle.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("filled_clipping_mask"),
                size: texture_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TEX_DIM,
                format: TEX_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.handle.queue.submit(Some({
                let mut encoder = self
                    .handle
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                encoder.finish()
            }));

            texture
        };

        self.buffer_dimensions = buffer_dimensions;
        self.texture_extent = texture_extent;
        self.filled_clipping_mask = filled_clipping_mask;
    }

    pub fn reload_vertices_buffer(&mut self) {
        // self.vertex_buffer.slice(..).get_mapped_range_mut()[..self.vertices.len()]
        //     .copy_from_slice(bytemuck::cast_slice(&self.vertices));
        // self.vertex_buffer.unmap();
        self.vertex_buffer =
            self.handle
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vertex_buffer"),
                    contents: bytemuck::cast_slice(&self.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
    }

    pub fn new(
        width: u32,
        height: u32,
        background: Option<[f32; 4]>,
        handle: &'device LogicalDevice,
        max_layers: u32,
    ) -> Self {
        let LogicalDevice {
            ref device,
            ref queue,
            ..
        } = handle;

        let vertices = SQUARE_VERTICES;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index_buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        let (constant_bind_group_layout, constant_bind_group) = {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                }],
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("constant_bind_group"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                }],
            });

            (layout, bind_group)
        };

        let blending_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("blending_group_layout"),
                entries: &[
                    // Self::texture_bind_group_layout_entry(0),
                    Self::texture_bind_group_layout_entry(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: NonZeroU32::new(1),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: NonZeroU32::new(1),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: NonZeroU32::new(1),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&constant_bind_group_layout, &blending_group_layout],
                push_constant_ranges: &[],
            });

        // wgpu::include_wgsl!("shader.wgsl")
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Dynamically loaded shader module"),
            source: wgpu::ShaderSource::Wgsl({
                use std::fs::OpenOptions;
                use std::io::Read;
                let mut file = OpenOptions::new()
                    .read(true)
                    .open("./src/shader.wgsl")
                    .unwrap();

                let mut buf = String::new();
                file.read_to_string(&mut buf).unwrap();
                buf.into()
            }),
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: TEX_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let buffer_dimensions = BufferDimensions::new(width, height);
        // The output buffer lets us retrieve the data as an array

        let texture_extent = wgpu::Extent3d {
            width: buffer_dimensions.width,
            height: buffer_dimensions.height,
            depth_or_array_layers: 1,
        };

        let filled_clipping_mask = {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("filled_clipping_mask"),
                size: texture_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TEX_DIM,
                format: TEX_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            queue.submit(Some({
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                encoder.finish()
            }));

            texture
        };

        Self {
            handle,
            buffer_dimensions,
            texture_extent,
            constant_bind_group,
            blending_group_layout,
            background,
            render_pipeline,
            vertices,
            vertex_buffer,
            index_buffer,
            filled_clipping_mask,
            max_layers
        }
    }

    fn new_output_texture(&self) -> GpuTexture {
        // The render pipeline renders data into this texture
        let output_texture = self.handle.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output_texture"),
            size: self.texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
        });

        let output_texture_view =
            output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        GpuTexture {
            texture: output_texture,
            view: output_texture_view,
            size: self.texture_extent,
        }
    }

    pub fn base_composite_texture(&self) -> wgpu::Texture {
        let texture = self.handle.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output_texture"),
            size: self.texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
        });

        if let Some([r, g, b, a]) = self.background {
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.handle.queue.submit(Some({
                let mut encoder = self
                    .handle
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: f64::from(r),
                                g: f64::from(g),
                                b: f64::from(b),
                                a: f64::from(a),
                            }),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                encoder.finish()
            }));
        }

        texture
    }

    // pub fn render(&self, layers: &[CompositeLayer]) -> wgpu::Texture {
    //     let mut composite_texture = self.base_composite_texture();
    //     for layer in layers.iter() {
    //         composite_texture = self.render_layer(
    //             composite_texture,
    //             &layer.texture.view,
    //             LayerContext {
    //                 opacity: layer.opacity,
    //                 blend: layer.blend.to_u32(),
    //             },
    //             if let Some(index) = layer.clipped {
    //                 &layers[index].texture.view
    //             } else {
    //                 &self.filled_clipping_mask_view
    //             },
    //         );

    //         // eprintln!("Finished layer {:?}: {}", layer.name, layer.blend);
    //     }
    //     composite_texture
    // }

    pub fn second_gen_render(&self, layers: &[CompositeLayer]) -> wgpu::Texture {
        let mut composite_texture = self.base_composite_texture();

        self.handle.queue.submit(Some({
            let mut encoder = self
                .handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            for layer in layers.iter() {
                let prev_texture_view =
                    composite_texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        ..Default::default()
                    });

                let ctx_buffer =
                    self.handle
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Context"),
                            contents: bytemuck::cast_slice(&[LayerContext {
                                opacity: layer.opacity,
                                blend: layer.blend.to_u32(),
                                _padding: [0.0; 2]
                            }]),
                            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        });

                let count_buffer =
                self.handle
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Context"),
                        contents: bytemuck::cast_slice(&[1i32]),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });

                let GpuTexture {
                    texture: output_texture,
                    view: output_texture_view,
                    ..
                } = self.new_output_texture();

                let mixing_bind_group =
                    self.handle
                        .device
                        .create_bind_group(&wgpu::BindGroupDescriptor {
                            layout: &self.blending_group_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(
                                        &prev_texture_view,
                                    ),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::TextureViewArray(&[
                                        &if let Some(index) = layer.clipped {
                                            &layers[index].texture.texture
                                        } else {
                                            &self.filled_clipping_mask
                                        }
                                        .create_view(
                                            &wgpu::TextureViewDescriptor {
                                                dimension: Some(
                                                    wgpu::TextureViewDimension::D2Array,
                                                ),
                                                ..Default::default()
                                            },
                                        ),
                                    ]),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 2,
                                    resource: wgpu::BindingResource::TextureViewArray(&[&layer
                                        .texture.texture.create_view(
                                            &wgpu::TextureViewDescriptor {
                                                dimension: Some(
                                                    wgpu::TextureViewDimension::D2Array,
                                                ),
                                                ..Default::default()
                                            },
                                        )]),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 3,
                                    resource: ctx_buffer.as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 4,
                                    resource: count_buffer.as_entire_binding(),
                                },
                            ],
                            label: Some("mixing_bind_group"),
                        });

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                render_pass.set_pipeline(&self.render_pipeline);
                render_pass.set_bind_group(0, &self.constant_bind_group, &[]);
                render_pass.set_bind_group(1, &mixing_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
                drop(render_pass);

                composite_texture = output_texture
            }
            encoder.finish()
        }));
        composite_texture
    }

    // fn render_layer(
    //     &self,
    //     composite_texture: wgpu::Texture,
    //     layer_texture_view: &wgpu::TextureView,
    //     layer_ctx: LayerContext,
    //     mask_texture_view: &wgpu::TextureView,
    // ) -> wgpu::Texture {
    //     let prev_texture_view =
    //         composite_texture.create_view(&wgpu::TextureViewDescriptor::default());

    //     let ctx_buffer = self
    //         .handle
    //         .device
    //         .create_buffer_init(&wgpu::util::BufferInitDescriptor {
    //             label: Some("Context"),
    //             contents: bytemuck::cast_slice(&[layer_ctx]),
    //             usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    //         });

    //     let GpuTexture {
    //         texture: output_texture,
    //         view: output_texture_view,
    //         ..
    //     } = self.new_output_texture();

    //     let mixing_bind_group = self
    //         .handle
    //         .device
    //         .create_bind_group(&wgpu::BindGroupDescriptor {
    //             layout: &self.blending_group_layout,
    //             entries: &[
    //                 wgpu::BindGroupEntry {
    //                     binding: 0,
    //                     resource: wgpu::BindingResource::TextureView(&prev_texture_view),
    //                 },
    //                 wgpu::BindGroupEntry {
    //                     binding: 1,
    //                     resource: wgpu::BindingResource::TextureView(&mask_texture_view),
    //                 },
    //                 wgpu::BindGroupEntry {
    //                     binding: 2,
    //                     resource: wgpu::BindingResource::TextureView(&layer_texture_view),
    //                 },
    //                 wgpu::BindGroupEntry {
    //                     binding: 3,
    //                     resource: ctx_buffer.as_entire_binding(),
    //                 },
    //             ],
    //             label: Some("mixing_bind_group"),
    //         });

    //     self.handle.queue.submit(Some({
    //         let mut encoder = self
    //             .handle
    //             .device
    //             .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    //         let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    //             label: None,
    //             color_attachments: &[Some(wgpu::RenderPassColorAttachment {
    //                 view: &output_texture_view,
    //                 resolve_target: None,
    //                 ops: wgpu::Operations {
    //                     load: wgpu::LoadOp::Load,
    //                     store: true,
    //                 },
    //             })],
    //             depth_stencil_attachment: None,
    //         });

    //         render_pass.set_pipeline(&self.render_pipeline);
    //         render_pass.set_bind_group(0, &self.constant_bind_group, &[]);
    //         render_pass.set_bind_group(1, &mixing_bind_group, &[]);
    //         render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
    //         render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    //         render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
    //         drop(render_pass);

    //         encoder.finish()
    //     }));

    //     output_texture
    // }
}
