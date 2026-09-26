//! The board, stone, and shadow passes.
//!
//! The board is one full-screen triangle: the shader works out the board point
//! for every pixel, so the wood grain belongs to the board plane and does not
//! slide when the view moves. The stones are one lens mesh, drawn instanced.
//! Nothing overlaps, so there is no depth buffer: the passes run in order.

use anyhow::Context as _;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt as _;

/// The board's wood: a photograph of a real board, from the owner's collection.
pub const WOOD_PNG: &[u8] = include_bytes!("../assets/wood-left.png");
/// The engraved coordinate labels, generated offline as a signed-distance field.
pub const GLYPH_PNG: &[u8] = include_bytes!("../assets/glyphs.png");

/// A decoded RGBA8 image, used for the wood photograph and the glyph atlas.
pub struct RgbaTexture {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// The pixels, as RGBA8.
    pub rgba: Vec<u8>,
}

/// The decoded photograph.
pub type WoodTexture = RgbaTexture;
/// The decoded glyph atlas.
pub type GlyphTexture = RgbaTexture;

impl RgbaTexture {
    /// Decode the embedded wood photograph.
    ///
    /// # Errors
    /// An error when the embedded image is not a readable PNG.
    pub fn load_wood() -> anyhow::Result<RgbaTexture> {
        Self::load(WOOD_PNG)
    }

    /// Decode the embedded coordinate-label atlas.
    ///
    /// # Errors
    /// An error when the embedded image is not a readable PNG.
    pub fn load_glyphs() -> anyhow::Result<RgbaTexture> {
        Self::load(GLYPH_PNG)
    }

    /// Decode an embedded PNG photograph or glyph atlas.
    ///
    /// # Errors
    /// An error when the embedded image is not a readable PNG.
    pub fn load(bytes: &[u8]) -> anyhow::Result<RgbaTexture> {
        // A Cursor, because the PNG decoder seeks within its input.
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder
            .read_info()
            .context("the texture could not be read")?;
        let capacity = reader
            .output_buffer_size()
            .context("the texture has no readable size")?;
        let mut buffer = vec![0; capacity];
        let info = reader
            .next_frame(&mut buffer)
            .context("the texture could not be decoded")?;
        buffer.truncate(info.buffer_size());

        let pixels = info.width as usize * info.height as usize;
        let rgba = match info.color_type {
            png::ColorType::Rgba => buffer,
            png::ColorType::Rgb => {
                let mut out = Vec::with_capacity(pixels * 4);
                for pixel in buffer.chunks_exact(3) {
                    out.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
                out
            }
            png::ColorType::Grayscale => {
                let mut out = Vec::with_capacity(pixels * 4);
                for value in buffer {
                    out.extend_from_slice(&[value, value, value, 255]);
                }
                out
            }
            png::ColorType::GrayscaleAlpha => {
                let mut out = Vec::with_capacity(pixels * 4);
                for pixel in buffer.chunks_exact(2) {
                    out.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
                }
                out
            }
            other => anyhow::bail!("the texture has an unsupported colour type: {other:?}"),
        };
        Ok(RgbaTexture {
            width: info.width,
            height: info.height,
            rgba,
        })
    }
}

/// The uniform block, shared by all three shaders. Every member is a `vec4`, so
/// the Rust and WGSL layouts cannot drift apart.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    /// Centre in cells, pixels per cell, flipped (0.0 or 1.0).
    pub view: [f32; 4],
    /// The framebuffer width and height, then the viewport width and height.
    pub frame: [f32; 4],
    /// The viewport origin in the framebuffer.
    pub origin: [f32; 4],
    /// The direction to the light.
    pub light: [f32; 4],
    /// Wood lighter rgb, grain frequency.
    pub wood_a: [f32; 4],
    /// Wood darker rgb, ring contrast.
    pub wood_b: [f32; 4],
    /// Pore rgb, pore depth.
    pub wood_c: [f32; 4],
    /// Roughness along the grain, roughness across, sheen.
    pub wood_d: [f32; 4],
    /// Slate rgb, roughness.
    pub stone_a: [f32; 4],
    /// Shell rgb, roughness.
    pub stone_b: [f32; 4],
    /// The reflected sky rgb, and the ambient share.
    pub env_a: [f32; 4],
    /// The reflected floor rgb, and the exposure.
    pub env_b: [f32; 4],
    /// The last-placed stone for the red glow: .xy = cell col/row, .z = enabled.
    pub last_move: [f32; 4],
    /// Overlay toggles: .x = coordinate labels.
    pub toggles: [f32; 4],
    /// Slate look dials: shine scale, texture contrast scale, albedo scale,
    /// room reflection scale.
    pub slate_knobs: [f32; 4],
    /// Shell look dials, in the same order.
    pub shell_knobs: [f32; 4],
}

/// One stone.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct StoneInstance {
    /// The intersection, in cells.
    pub centre: [f32; 2],
    /// Linear albedo rgb and roughness.
    pub colour: [f32; 4],
    /// Texture seed, material kind, roughness cap (0.0 for the default), unused.
    pub params: [f32; 4],
}

/// A stone material.
#[derive(Debug, Clone, Copy)]
pub struct StoneMaterial {
    /// Linear albedo.
    pub albedo: [f32; 3],
    /// Base roughness. Higher is duller.
    pub roughness: f32,
    /// How dull the stone may go: where the highlight stops spreading. A low cap
    /// keeps a tight sparkle; a high one lets the light smear into a soft sheen.
    pub cap: f32,
    /// How much of the light the surface reflects and how strongly it shines.
    /// A polished shell stone takes the full reflection; slate is duller and
    /// mostly scatters the light, so it takes little.
    pub gloss: f32,
    /// 0.0 for slate, 1.0 for shell.
    pub kind: f32,
}

/// The slate stone: a dark blue-grey solid with a matte, slightly dusty surface.
///
/// Linear albedo, from sRGB (0.28, 0.285, 0.30). Slate is not polished like
/// glass: it scatters most of the light, keeps a broad and weak highlight, and
/// reflects very little of its surroundings. A low gloss is what stops it looking
/// wet, and a higher roughness spreads what highlight there is instead of
/// breaking it into a hard, shiny ring.
///
/// Tuned in the B10 take of `--variants`: a dull, dry stone with no sparkle.
pub const SLATE: StoneMaterial = StoneMaterial {
    albedo: [0.062, 0.065, 0.075],
    roughness: 0.72,
    cap: 0.90,
    gloss: 0.22,
    kind: 0.0,
};

/// The shell stone: milky, faintly cool, and glossier than the slate.
///
/// Linear albedo, from sRGB (0.95, 0.95, 0.93) with a cool tint.
///
/// Tuned in the W9 take of `--variants`: the streaks stand out, and the body is
/// a little darker to give them something to stand against.
pub const SHELL: StoneMaterial = StoneMaterial {
    albedo: [0.760, 0.762, 0.735],
    roughness: 0.20,
    cap: 0.672,
    gloss: 1.0,
    kind: 1.0,
};

/// How the photographed wood is shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WoodLook {
    /// A brightness multiplier on the photograph. The photograph is dark, so a
    /// value above one is normal.
    pub gain: f32,
    /// How strongly the added pores show.
    pub pores: f32,
    /// The strength of the varnish sheen.
    pub sheen: f32,
}

/// The default look.
///
/// The photograph is dark, and a dark board hides dark stones. Measured on this
/// board, a gain of 2.8 is where the black stones reach 3.9 to 1 contrast and the
/// white stones 3.8 to 1: even, and both above the 3 to 1 minimum, so either
/// colour can be placed confidently. Brightening also raises the grain contrast
/// rather than flattening it, because the tone curve has not yet compressed the
/// wood.
pub const WOOD: WoodLook = WoodLook {
    gain: 2.8,
    pores: 0.35,
    sheen: 0.28,
};

/// The light direction: from the upper left of the board, out of the surface.
pub const LIGHT: [f32; 3] = [-0.42, -0.55, 0.72];

const BOARD: &str = concat!(
    include_str!("render/wgsl/common.wgsl"),
    "\n",
    include_str!("render/wgsl/board.wgsl")
);
const STONE: &str = concat!(
    include_str!("render/wgsl/common.wgsl"),
    "\n",
    include_str!("render/wgsl/stone.wgsl")
);
const SHADOW: &str = concat!(
    include_str!("render/wgsl/common.wgsl"),
    "\n",
    include_str!("render/wgsl/shadow.wgsl")
);

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

/// The radius of a stone at its widest, in cells.
const STONE_RADIUS: f32 = 0.47;
/// How many times the lens profile is sampled.
const RINGS: usize = 14;
/// How many segments the lens is revolved with.
const SEGMENTS: usize = 56;

/// The shape of a stone: how tall the lens is, and where its profile bends.
///
/// A real Go or Gomoku stone is about 10 mm thick for a 22 mm diameter, so its
/// height is a little under half its width. A flatter stone reads as a disc,
/// however well it is shaded, so the proportion matters more than the material.
/// The three fractions say where the widest point, the shoulder, and the start
/// of the dome sit between the base and the apex: lower values put more of the
/// stone's height into a flat top, which is the lentil shape of a real stone,
/// and higher values put it into a round dome, which reads as a ball.
#[derive(Debug, Clone, Copy)]
pub struct StoneShape {
    /// The height of a stone at its apex, in cells.
    pub height: f32,
    /// The height of the widest point, as a fraction of the height.
    pub widest: f32,
    /// The height of the shoulder, as a fraction of the height.
    pub shoulder: f32,
    /// The height where the dome begins, as a fraction of the height.
    pub dome: f32,
}

/// The shape the game plays with: a lentil, not a ball. Lower than a tenth of
/// a cell taller than a real stone, with the widest point low and the dome
/// flat, which is the shape of the stones on a real board.
pub const STONE_SHAPE: StoneShape = StoneShape {
    height: 0.30,
    widest: 0.18,
    shoulder: 0.55,
    dome: 0.82,
};

/// The lens profile, as (radius, height) pairs from the base to the apex.
fn profile(shape: &StoneShape) -> Vec<[f32; 2]> {
    // (radius, height) from the base to the apex: a widening foot, the widest
    // point low down, then a dome, as a real stone is shaped.
    let height = shape.height;
    let control = [
        [0.42, 0.0],
        [STONE_RADIUS, shape.widest * height],
        [0.415, shape.shoulder * height],
        [0.30, shape.dome * height],
        [0.0, height],
    ];
    let mut points = Vec::with_capacity(RINGS + 1);
    let segments = control.len() - 1;
    for index in 0..=RINGS {
        let t = index as f32 / RINGS as f32;
        let scaled = t * segments as f32;
        let segment = (scaled as usize).min(segments - 1);
        // Ease within each segment so that the silhouette is round rather than
        // a chain of straight edges.
        let local = scaled - segment as f32;
        let eased = local * local * (3.0 - 2.0 * local);
        let from = control[segment];
        let to = control[segment + 1];
        points.push([
            from[0] + (to[0] - from[0]) * eased,
            from[1] + (to[1] - from[1]) * eased,
        ]);
    }
    points
}

/// Build the lens mesh: a profile revolved around the height axis.
fn lens_mesh(shape: &StoneShape) -> (Vec<Vertex>, Vec<u32>) {
    let points = profile(shape);
    let mut vertices = Vec::with_capacity((RINGS + 1) * (SEGMENTS + 1));

    for ring in 0..=RINGS {
        let [radius, height] = points[ring];
        // The profile tangent, then the outward normal in the (r, y) plane.
        let next = points[(ring + 1).min(RINGS)];
        let previous = points[ring.saturating_sub(1)];
        let mut tangent = [next[0] - previous[0], next[1] - previous[1]];
        let length = (tangent[0] * tangent[0] + tangent[1] * tangent[1]).sqrt();
        if length > 1e-6 {
            tangent = [tangent[0] / length, tangent[1] / length];
        }
        let normal_plane = [tangent[1], -tangent[0]];

        for segment in 0..=SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            let (sin, cos) = angle.sin_cos();
            vertices.push(Vertex {
                position: [radius * cos, radius * sin, height],
                normal: [
                    normal_plane[0] * cos,
                    normal_plane[0] * sin,
                    normal_plane[1],
                ],
            });
        }
    }

    let stride = (SEGMENTS + 1) as u32;
    let mut indices = Vec::with_capacity(RINGS * SEGMENTS * 6);
    for ring in 0..RINGS as u32 {
        for segment in 0..SEGMENTS as u32 {
            let a = ring * stride + segment;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (vertices, indices)
}

fn vertex_layouts() -> Vec<Option<wgpu::VertexBufferLayout<'static>>> {
    vec![
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
            ],
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<StoneInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 4,
                },
            ],
        }),
    ]
}

/// Give egui's textures to its renderer. Every delta must be applied: egui
/// reports the ones that were dropped if any are left over.
pub fn apply_textures(
    renderer: &mut egui_wgpu::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    delta: &egui::TexturesDelta,
) {
    for (id, deltas) in &delta.set {
        for image in deltas {
            renderer.update_texture(device, queue, *id, image);
        }
    }
    for id in &delta.free {
        renderer.free_texture(id);
    }
}

/// The most stones that can be on the board at once.
pub const MAX_STONES: usize = 225;

/// Everything needed to draw a frame, apart from the targets.
pub struct Renderer {
    board_pipeline: wgpu::RenderPipeline,
    stone_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    board_textures_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    instances: wgpu::Buffer,
    stone_count: u32,
    sample_count: u32,
}

impl Renderer {
    /// Build the pipelines, the uniform block, and the lens mesh.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        sample_count: u32,
        wood: &WoodTexture,
        glyphs: &GlyphTexture,
        shape: &StoneShape,
    ) -> Renderer {
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals layout"),
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

        // The photograph, as an sRGB texture, so the GPU converts it to linear
        // on every sample.
        let wood_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wood"),
            size: wgpu::Extent3d {
                width: wood.width,
                height: wood.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &wood_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &wood.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * wood.width),
                rows_per_image: Some(wood.height),
            },
            wgpu::Extent3d {
                width: wood.width,
                height: wood.height,
                depth_or_array_layers: 1,
            },
        );
        let wood_view = wood_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let wood_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("wood"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // The glyph atlas is a signed-distance field in linear greyscale.  It
        // is clamped at its edges so neighbouring cells never bleed into a
        // sample.
        let glyph_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyphs"),
            size: wgpu::Extent3d {
                width: glyphs.width,
                height: glyphs.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &glyph_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &glyphs.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * glyphs.width),
                rows_per_image: Some(glyphs.height),
            },
            wgpu::Extent3d {
                width: glyphs.width,
                height: glyphs.height,
                depth_or_array_layers: 1,
            },
        );
        let glyph_view = glyph_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let glyph_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyphs"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let board_textures_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("board textures layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let board_textures_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("board textures"),
            layout: &board_textures_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&wood_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&wood_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&glyph_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&glyph_sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipelines"),
            bind_group_layouts: &[Some(&layout), Some(&board_textures_layout)],
            immediate_size: 0,
        });

        let multisample = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        let targets = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::REPLACE),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let board_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("board"),
            source: wgpu::ShaderSource::Wgsl(BOARD.into()),
        });
        let stone_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stone"),
            source: wgpu::ShaderSource::Wgsl(STONE.into()),
        });
        let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow"),
            source: wgpu::ShaderSource::Wgsl(SHADOW.into()),
        });

        let board_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("board"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &board_module,
                entry_point: Some("vs_board"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &board_module,
                entry_point: Some("fs_board"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });

        let layouts = vertex_layouts();
        let stone_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stone"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &stone_module,
                entry_point: Some("vs_stone"),
                compilation_options: Default::default(),
                buffers: &layouts,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &stone_module,
                entry_point: Some("fs_stone"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });

        // The shadow only darkens: source zero, destination scaled by one minus
        // the source alpha.
        let darken = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            // Leave the alpha channel alone: the colour channel darkens the
            // board, and the alpha stays as the board wrote it.
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let shadow_targets = [Some(wgpu::ColorTargetState {
            format,
            blend: darken,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                // Only the instance centre is read; the quad comes from the
                // vertex index.
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<StoneInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 2,
                    }],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &shadow_module,
                entry_point: Some("fs_shadow"),
                compilation_options: Default::default(),
                targets: &shadow_targets,
            }),
            multiview_mask: None,
            cache: None,
        });

        let (mesh_vertices, mesh_indices) = lens_mesh(shape);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lens vertices"),
            contents: bytemuck::cast_slice(&mesh_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lens indices"),
            contents: bytemuck::cast_slice(&mesh_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stone instances"),
            size: (MAX_STONES * std::mem::size_of::<StoneInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut renderer = Renderer {
            board_pipeline,
            stone_pipeline,
            shadow_pipeline,
            globals,
            bind_group,
            board_textures_group,
            vertices,
            indices,
            index_count: mesh_indices.len() as u32,
            instances,
            stone_count: 0,
            sample_count,
        };
        renderer.set_globals(queue, &Renderer::default_globals(800, 800));
        renderer
    }

    /// The materials, the room, and the view for a fitted window.
    pub fn default_globals(width: u32, height: u32) -> Globals {
        Renderer::globals_with_wood(crate::camera::Viewport::window(width, height), WOOD)
    }

    /// The same, with a chosen look for the wood.
    pub fn globals_with_wood(viewport: crate::camera::Viewport, look: WoodLook) -> Globals {
        let fit = crate::camera::Camera::fit_scale(viewport);
        Globals {
            view: [7.0, 7.0, fit, 0.0],
            frame: [
                viewport.frame[0],
                viewport.frame[1],
                viewport.width,
                viewport.height,
            ],
            origin: [viewport.x, viewport.y, 0.0, 0.0],
            light: [LIGHT[0], LIGHT[1], LIGHT[2], 0.0],
            // The photograph supplies the colour, so the first field is a
            // brightness multiplier rather than a tone.
            wood_a: [look.gain, look.gain, look.gain, 0.0],
            // How much each stone material shines: slate, then shell.
            wood_b: [SLATE.gloss, SHELL.gloss, 0.0, 0.0],
            // The pore depth is the fourth field; the colour is unused because
            // the pores darken the photograph.
            wood_c: [0.0, 0.0, 0.0, look.pores],
            wood_d: [0.18, 0.48, look.sheen, 0.30],
            stone_a: [
                SLATE.albedo[0],
                SLATE.albedo[1],
                SLATE.albedo[2],
                SLATE.roughness,
            ],
            stone_b: [
                SHELL.albedo[0],
                SHELL.albedo[1],
                SHELL.albedo[2],
                SHELL.roughness,
            ],
            // A cool reflection above the board, a dark one below: this is the
            // room the stones and the varnish reflect.
            env_a: [0.28, 0.31, 0.38, 0.28],
            env_b: [0.045, 0.040, 0.036, 1.05],
            last_move: [0.0; 4],
            toggles: [0.0; 4],
            // The B10 take: a quarter of the shine, the glitter calmed, the body
            // a touch lighter, and little of the room. Dry, not wet.
            slate_knobs: [0.25, 0.60, 1.25, 0.12],
            // The W9 take: the streaks stand well out, and the body is a little
            // darker to give them something to stand against.
            shell_knobs: [1.0, 3.72, 0.874, 1.0],
        }
    }

    /// How many samples each pixel takes.
    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Upload the view and the materials.
    pub fn set_globals(&mut self, queue: &wgpu::Queue, globals: &Globals) {
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(globals));
    }

    /// Upload the stones. At most [`MAX_STONES`].
    pub fn set_stones(&mut self, queue: &wgpu::Queue, stones: &[StoneInstance]) {
        let count = stones.len().min(MAX_STONES);
        self.stone_count = count as u32;
        if count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&stones[..count]));
        }
    }

    /// Record the three passes.
    ///
    /// `target` is the multisampled attachment. `resolve` is the texture the
    /// samples are resolved into, which is the surface texture in a window and
    /// an ordinary texture in a preview.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        resolve: &wgpu::TextureView,
        clear: wgpu::Color,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("board and stones"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: Some(resolve),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, &self.board_textures_group, &[]);

        pass.set_pipeline(&self.board_pipeline);
        pass.draw(0..3, 0..1);

        if self.stone_count > 0 {
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..6, 0..self.stone_count);

            pass.set_pipeline(&self.stone_pipeline);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..self.stone_count);
        }
    }
}

/// The shader sources as the GPU sees them: the shared file in front of each
/// module. Used by the test that validates the WGSL without a GPU.
#[cfg(test)]
fn shader_sources() -> [(&'static str, &'static str); 4] {
    [
        ("common", include_str!("render/wgsl/common.wgsl")),
        ("board", BOARD),
        ("stone", STONE),
        ("shadow", SHADOW),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shader_module_is_valid_wgsl() {
        for (name, source) in shader_sources() {
            let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|err| {
                panic!("{name} is not valid WGSL:\n{}", err.emit_to_string(source))
            });
            let mut validator = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            );
            validator
                .validate(&module)
                .unwrap_or_else(|err| panic!("{name} fails validation: {err:?}"));
        }
    }

    /// Where a point goes through the uniform, as the shaders work it out: a
    /// board point in cells, to framebuffer pixels, by way of clip space.
    fn through_the_shader(globals: &Globals, point: [f32; 2], surface: [f32; 2]) -> [f32; 2] {
        let d = [point[0] - globals.view[0], point[1] - globals.view[1]];
        let screen = [
            globals.origin[0] + globals.frame[2] * 0.5 + d[0] * globals.view[2],
            globals.origin[1] + globals.frame[3] * 0.5 + d[1] * globals.view[2],
        ];
        // To clip space with the size that the uniform claims, then back to the
        // surface with the size the surface really has: that is what the hardware
        // does, and where a wrong size shows up.
        let clip = [
            screen[0] / globals.frame[0] * 2.0 - 1.0,
            1.0 - screen[1] / globals.frame[1] * 2.0,
        ];
        [
            (clip[0] + 1.0) * 0.5 * surface[0],
            (1.0 - clip[1]) * 0.5 * surface[1],
        ]
    }

    #[test]
    fn the_projection_agrees_with_the_camera() {
        // A window with a menu bar at the top and a side panel on the right, so
        // that the board area is not centred in the surface. A projection that
        // guesses the surface size from the board area gets this wrong.
        let surface = [1200.0, 900.0];
        let viewport = crate::camera::Viewport {
            frame: surface,
            x: 0.0,
            y: 28.0,
            width: 950.0,
            height: 872.0,
        };
        let globals = Renderer::globals_with_wood(viewport, WOOD);
        assert_eq!(
            [globals.frame[0], globals.frame[1]],
            surface,
            "the uniform carries the size of the surface"
        );
        let mut camera = crate::camera::Camera::fit(viewport);
        camera.pixels_per_cell = globals.view[2];
        for point in [
            [0.0, 0.0],
            [7.0, 7.0],
            [14.0, 14.0],
            [3.5, 11.25],
            [0.0, 14.0],
        ] {
            let shader = through_the_shader(&globals, point, surface);
            let camera = camera.to_screen(viewport, point);
            assert!(
                (shader[0] - camera[0]).abs() < 0.01 && (shader[1] - camera[1]).abs() < 0.01,
                "at {point:?} the shader says {shader:?} and the camera says {camera:?}"
            );
        }
    }

    #[test]
    fn the_lens_mesh_is_closed_and_inside_its_bounds() {
        let (vertices, indices) = lens_mesh(&STONE_SHAPE);
        assert_eq!(vertices.len(), (RINGS + 1) * (SEGMENTS + 1));
        assert_eq!(indices.len(), RINGS * SEGMENTS * 6);
        for vertex in &vertices {
            let radius = (vertex.position[0].powi(2) + vertex.position[1].powi(2)).sqrt();
            assert!(radius <= STONE_RADIUS + 1e-5, "radius {radius} is too wide");
            assert!(
                (0.0..=STONE_SHAPE.height + 1e-5).contains(&vertex.position[2]),
                "height {} is outside the stone",
                vertex.position[2]
            );
        }
        assert!(
            indices
                .iter()
                .all(|index| (*index as usize) < vertices.len())
        );
    }
}
