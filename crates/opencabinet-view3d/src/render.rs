// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Draws a [`Mesh`] with wgpu into an image in memory, which the window
//! shows like any other picture (ADR-0016). The window's own drawing is
//! untouched, so the rest of the program works the same on every computer;
//! only this view needs a graphics adapter, and without a usable one wgpu
//! draws on the processor (WARP on Windows, lavapipe or llvmpipe
//! elsewhere), slower but the same picture.

use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::math::Mat4;
use crate::scene::{LineVertex, Mesh, Vertex};

/// What the shaders read for a whole frame.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    options: [f32; 4],
}

/// The colors are written as sRGB; the shaders work in linear light.
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// How a frame is drawn, besides its mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The camera's view and projection.
    pub view_projection: Mat4,
    /// Where the camera is.
    pub eye: [f32; 3],
    /// Whether wood grain, plies and planks are drawn.
    pub textured: bool,
    /// How opaque seen-through surfaces are, from 0 to 1.
    pub ghost_opacity: f32,
}

/// The GPU buffers of a mesh.
struct Buffers {
    vertices: Option<wgpu::Buffer>,
    solid: Option<(wgpu::Buffer, u32)>,
    ghost: Option<(wgpu::Buffer, u32)>,
    lines: Option<(wgpu::Buffer, u32)>,
    overlay: Option<(wgpu::Buffer, u32)>,
}

/// The images a frame is drawn into, for one size.
struct Targets {
    size: (u32, u32),
    /// What the pass draws into.
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// Where a multisampled `color` resolves.
    resolve: Option<wgpu::TextureView>,
    /// The single-sample picture read back.
    source: wgpu::Texture,
    readback: wgpu::Buffer,
    padded_row: u32,
}

/// A renderer with its own GPU device.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    samples: u32,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    sky: wgpu::RenderPipeline,
    solid: wgpu::RenderPipeline,
    ghost: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    overlay: wgpu::RenderPipeline,
    buffers: Buffers,
    targets: Option<Targets>,
    /// The adapter, for the user: its name and whether it is the
    /// processor drawing.
    pub adapter: String,
    /// Whether the picture is drawn on the processor rather than a
    /// graphics card.
    pub software: bool,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("adapter", &self.adapter)
            .field("samples", &self.samples)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// A renderer on the best graphics adapter, or drawing on the
    /// processor when there is none.
    ///
    /// # Errors
    /// When not even the processor can draw: a message for the user.
    pub fn new() -> Result<Self, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let options = |fallback| wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: fallback,
            compatible_surface: None,
            ..Default::default()
        };
        // OPENCABINET_3D_SOFTWARE asks for the processor first, for a
        // graphics driver that draws wrongly, and to measure the fallback.
        let software_first = std::env::var_os("OPENCABINET_3D_SOFTWARE").is_some();
        let adapter = pollster::block_on(instance.request_adapter(&options(software_first)))
            .or_else(|_| pollster::block_on(instance.request_adapter(&options(!software_first))))
            .map_err(|e| format!("No graphics adapter can draw the 3D view: {e}"))?;
        let info = adapter.get_info();
        let software = info.device_type == wgpu::DeviceType::Cpu;
        let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("OpenCabinet 3D"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| format!("The graphics adapter cannot draw the 3D view: {e}"))?;
        let samples = if adapter
            .get_texture_format_features(COLOR)
            .flags
            .sample_count_supported(4)
        {
            4
        } else {
            1
        };

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("OpenCabinet 3D"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipelines = Pipelines {
            device: &device,
            shader: &shader,
            layout: &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("OpenCabinet 3D"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            }),
            samples,
        };
        Ok(Self {
            sky: pipelines.sky(),
            solid: pipelines.surfaces(false),
            ghost: pipelines.surfaces(true),
            lines: pipelines.lines(false),
            overlay: pipelines.lines(true),
            device,
            queue,
            samples,
            globals,
            bind_group,
            buffers: Buffers {
                vertices: None,
                solid: None,
                ghost: None,
                lines: None,
                overlay: None,
            },
            targets: None,
            adapter: format!("{} ({:?})", info.name, info.backend),
            software,
        })
    }

    /// Sends `mesh` to the GPU, for the frames that follow.
    pub fn upload(&mut self, mesh: &Mesh) {
        let device = &self.device;
        let buffer = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let indices = |label, list: &[u32]| {
            (!list.is_empty()).then(|| {
                let count = u32::try_from(list.len()).unwrap_or(u32::MAX);
                (
                    buffer(label, bytemuck::cast_slice(list), wgpu::BufferUsages::INDEX),
                    count,
                )
            })
        };
        let lines = |label, list: &[LineVertex]| {
            (!list.is_empty()).then(|| {
                let count = u32::try_from(list.len()).unwrap_or(u32::MAX);
                (
                    buffer(
                        label,
                        bytemuck::cast_slice(list),
                        wgpu::BufferUsages::VERTEX,
                    ),
                    count,
                )
            })
        };
        self.buffers = Buffers {
            vertices: (!mesh.vertices.is_empty()).then(|| {
                buffer(
                    "vertices",
                    bytemuck::cast_slice::<Vertex, u8>(&mesh.vertices),
                    wgpu::BufferUsages::VERTEX,
                )
            }),
            solid: indices("solid", &mesh.solid),
            ghost: indices("ghost", &mesh.ghost),
            lines: lines("lines", &mesh.lines),
            overlay: lines("overlay", &mesh.overlay),
        };
    }

    /// Makes the images a frame is drawn into, when their size changed.
    fn make_targets(&mut self, width: u32, height: u32) {
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.size == (width, height))
        {
            return;
        }
        let device = &self.device;
        let texture = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let view =
            |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        let attach = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let copy = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        let depth = view(&texture("depth", DEPTH, self.samples, attach));
        // The picture is read from a single-sample image: the one drawn
        // into, or the one a multisampled image resolves into.
        let source = texture("picture", COLOR, 1, copy);
        let (color, resolve) = if self.samples > 1 {
            let color = view(&texture("color", COLOR, self.samples, attach));
            (color, Some(view(&source)))
        } else {
            (view(&source), None)
        };
        let padded_row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.targets = Some(Targets {
            size: (width, height),
            color,
            depth,
            resolve,
            source,
            readback,
            padded_row,
        });
    }

    /// Draws the uploaded mesh as `frame` says, `width` by `height`
    /// pixels, into `pixels`: four bytes a pixel, RGBA, sRGB, row by row
    /// from the top.
    ///
    /// # Errors
    /// When the GPU fails: a message for the user.
    pub fn draw(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        pixels: &mut [u8],
    ) -> Result<(), String> {
        let (width, height) = (width.clamp(1, 8192), height.clamp(1, 8192));
        let globals = Globals {
            view_projection: frame.view_projection.0,
            eye: [frame.eye[0], frame.eye[1], frame.eye[2], 1.0],
            options: [
                if frame.textured { 1.0 } else { 0.0 },
                frame.ghost_opacity,
                0.0,
                0.0,
            ],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.make_targets(width, height);
        let Some(targets) = self.targets.as_ref() else {
            return Err("The 3D view has nothing to draw into".into());
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        self.encode(&mut encoder, targets);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &targets.source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &targets.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(targets.padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        self.read_back(targets, pixels)
    }

    /// The frame's drawing: the sky, the opaque triangles, those seen
    /// through, the edges and the selected part's edges over everything.
    fn encode(&self, encoder: &mut wgpu::CommandEncoder, targets: &Targets) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.color,
                depth_slice: None,
                resolve_target: targets.resolve.as_ref(),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..3, 0..1);
        let b = &self.buffers;
        if let Some(vertices) = &b.vertices {
            pass.set_vertex_buffer(0, vertices.slice(..));
            for (pipeline, indices) in [(&self.solid, &b.solid), (&self.ghost, &b.ghost)] {
                if let Some((buffer, count)) = indices {
                    pass.set_pipeline(pipeline);
                    pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
            }
        }
        for (pipeline, lines) in [(&self.lines, &b.lines), (&self.overlay, &b.overlay)] {
            if let Some((buffer, count)) = lines {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }
    }

    /// Waits for the frame and copies it into `pixels`, without the
    /// padding the GPU adds to each row.
    fn read_back(&self, targets: &Targets, pixels: &mut [u8]) -> Result<(), String> {
        let slice = targets.readback.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("The 3D view could not be drawn: {e}"))?;
        receiver
            .recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("The 3D view could not be read back: {e}"))?;
        {
            let data = slice
                .get_mapped_range()
                .map_err(|e| format!("The 3D view could not be read back: {e}"))?;
            let (width, height) = targets.size;
            let row = width as usize * 4;
            let padded = targets.padded_row as usize;
            for (y, out) in pixels
                .chunks_exact_mut(row)
                .take(height as usize)
                .enumerate()
            {
                out.copy_from_slice(&data[y * padded..y * padded + row]);
            }
        }
        targets.readback.unmap();
        Ok(())
    }
}

/// What every pipeline is built from.
struct Pipelines<'a> {
    device: &'a wgpu::Device,
    shader: &'a wgpu::ShaderModule,
    layout: &'a wgpu::PipelineLayout,
    samples: u32,
}

const SURFACE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32x4
];
const LINE_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

impl Pipelines<'_> {
    fn pipeline(
        &self,
        label: &str,
        (vertex, fragment): (&str, &str),
        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
        topology: wgpu::PrimitiveTopology,
        depth: wgpu::DepthStencilState,
        blend: bool,
    ) -> wgpu::RenderPipeline {
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(self.layout),
                vertex: wgpu::VertexState {
                    module: self.shader,
                    entry_point: Some(vertex),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(depth),
                multisample: wgpu::MultisampleState {
                    count: self.samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: self.shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR,
                        blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
    }

    fn depth(write: bool, compare: wgpu::CompareFunction, bias: i32) -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: bias,
                slope_scale: if bias == 0 { 0.0 } else { 1.0 },
                clamp: 0.0,
            },
        }
    }

    fn sky(&self) -> wgpu::RenderPipeline {
        self.pipeline(
            "sky",
            ("sky_vertex", "sky_fragment"),
            &[],
            wgpu::PrimitiveTopology::TriangleList,
            Self::depth(false, wgpu::CompareFunction::Always, 0),
            false,
        )
    }

    fn surfaces(&self, ghost: bool) -> wgpu::RenderPipeline {
        let layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SURFACE_ATTRIBUTES,
        };
        self.pipeline(
            if ghost { "ghost" } else { "solid" },
            ("surface_vertex", "surface_fragment"),
            &[Some(layout)],
            wgpu::PrimitiveTopology::TriangleList,
            Self::depth(
                !ghost,
                wgpu::CompareFunction::LessEqual,
                if ghost { 0 } else { 2 },
            ),
            ghost,
        )
    }

    fn lines(&self, overlay: bool) -> wgpu::RenderPipeline {
        let layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &LINE_ATTRIBUTES,
        };
        let compare = if overlay {
            wgpu::CompareFunction::Always
        } else {
            wgpu::CompareFunction::LessEqual
        };
        self.pipeline(
            if overlay { "overlay" } else { "lines" },
            ("line_vertex", "line_fragment"),
            &[Some(layout)],
            wgpu::PrimitiveTopology::LineList,
            Self::depth(false, compare, 0),
            true,
        )
    }
}
