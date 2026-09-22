//! The board, stone, and shadow passes.
//!
//! The board is one full-screen triangle: the shader works out the board point
//! for every pixel, so the wood grain belongs to the board plane and does not
//! slide when the view moves. The stones are one lens mesh, drawn instanced.
//! Nothing overlaps, so there is no depth buffer: the passes run in order.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt as _;

/// The uniform block, shared by all three shaders. Every member is a `vec4`, so
/// the Rust and WGSL layouts cannot drift apart.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    /// Centre in cells, pixels per cell, flipped (0.0 or 1.0).
    pub view: [f32; 4],
    /// Framebuffer size in physical pixels.
    pub window: [f32; 4],
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
}

/// One stone.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct StoneInstance {
    /// The intersection, in cells.
    pub centre: [f32; 2],
    /// Linear albedo rgb and roughness.
    pub colour: [f32; 4],
    /// Texture seed, material kind, unused, unused.
    pub params: [f32; 4],
}

/// A stone material.
#[derive(Debug, Clone, Copy)]
pub struct StoneMaterial {
    /// Linear albedo.
    pub albedo: [f32; 3],
    /// Base roughness.
    pub roughness: f32,
    /// 0.0 for slate, 1.0 for shell.
    pub kind: f32,
}

/// The slate stone: dark, matte, faint veins.
pub const SLATE: StoneMaterial = StoneMaterial {
    albedo: [0.055, 0.058, 0.068],
    roughness: 0.42,
    kind: 0.0,
};

/// The shell stone: milky, cool, a tighter highlight.
pub const SHELL: StoneMaterial = StoneMaterial {
    albedo: [0.900, 0.920, 0.955],
    roughness: 0.19,
    kind: 1.0,
};

/// A wood or marble preset.
#[derive(Debug, Clone, Copy)]
pub struct BoardMaterial {
    /// The lighter grain tone, or the vein colour for marble.
    pub light: [f32; 3],
    /// The darker grain tone.
    pub dark: [f32; 3],
    /// The pore colour.
    pub pore: [f32; 3],
    /// Grain frequency across the board.
    pub grain: f32,
    /// How strongly the rings modulate the colour.
    pub contrast: f32,
    /// How strongly the pores show.
    pub pore_depth: f32,
    /// Sheen strength.
    pub sheen: f32,
}

/// The default board: a warm medium brown, a little darker than new kaya.
pub const AGED_WOOD: BoardMaterial = BoardMaterial {
    light: [0.480, 0.223, 0.050],
    dark: [0.180, 0.060, 0.014],
    pore: [0.042, 0.018, 0.005],
    grain: 5.0,
    contrast: 0.85,
    pore_depth: 0.55,
    sheen: 0.35,
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
/// The height of a stone at its apex, in cells.
const STONE_HEIGHT: f32 = 0.175;
/// How many times the lens profile is sampled.
const RINGS: usize = 14;
/// How many segments the lens is revolved with.
const SEGMENTS: usize = 56;

/// The lens profile, as (radius, height) pairs from the base to the apex.
fn profile() -> Vec<[f32; 2]> {
    let control = [
        [0.40, 0.0],
        [STONE_RADIUS, 0.085],
        [0.36, 0.150],
        [0.0, STONE_HEIGHT],
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
fn lens_mesh() -> (Vec<Vertex>, Vec<u32>) {
    let points = profile();
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

/// The most stones that can be on the board at once.
pub const MAX_STONES: usize = 225;

/// Everything needed to draw a frame, apart from the targets.
pub struct Renderer {
    board_pipeline: wgpu::RenderPipeline,
    stone_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipelines"),
            bind_group_layouts: &[Some(&layout)],
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

        let (mesh_vertices, mesh_indices) = lens_mesh();
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

    /// The materials and view for an unzoomed, unpanned window of the given size.
    pub fn default_globals(width: u32, height: u32) -> Globals {
        let board = AGED_WOOD;
        let fit = (width.min(height) as f32) / 16.3;
        Globals {
            view: [7.0, 7.0, fit, 0.0],
            window: [width as f32, height as f32, 0.0, 0.0],
            light: [LIGHT[0], LIGHT[1], LIGHT[2], 0.0],
            wood_a: [board.light[0], board.light[1], board.light[2], board.grain],
            wood_b: [board.dark[0], board.dark[1], board.dark[2], board.contrast],
            wood_c: [
                board.pore[0],
                board.pore[1],
                board.pore[2],
                board.pore_depth,
            ],
            wood_d: [0.28, 0.55, board.sheen, 0.0],
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
        }
    }

    /// How many samples each pixel takes.
    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Upload the view and the materials.
    pub fn set_globals(&mut self, queue: &wgpu::Queue, globals: &Globals) {
        let _ = &self.globals;
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

    #[test]
    fn the_lens_mesh_is_closed_and_inside_its_bounds() {
        let (vertices, indices) = lens_mesh();
        assert_eq!(vertices.len(), (RINGS + 1) * (SEGMENTS + 1));
        assert_eq!(indices.len(), RINGS * SEGMENTS * 6);
        for vertex in &vertices {
            let radius = (vertex.position[0].powi(2) + vertex.position[1].powi(2)).sqrt();
            assert!(radius <= STONE_RADIUS + 1e-5, "radius {radius} is too wide");
            assert!(
                (0.0..=STONE_HEIGHT + 1e-5).contains(&vertex.position[2]),
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
