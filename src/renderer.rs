use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::IVec3;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::camera::CameraUniform;
use crate::console::font::{generate_font_atlas_pixels, glyph_uv, ATLAS_HEIGHT, ATLAS_WIDTH};
use crate::console::{ConsoleLineKind, ConsoleState};
use crate::environment::SkyUniform;
use crate::mesh::types::{MeshData, VoxelVertex};

/// Screen-size uniform untuk transformasi koordinat overlay 2D konsol (Phase 10.5.x)
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ConsoleScreenUniform {
    pub screen_size: [f32; 2],
    pub _pad: [f32; 2],
}

/// Vertex 2D untuk rendering teks dan panel Developer Console (Phase 10.5.x)
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ConsoleVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl ConsoleVertex {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ConsoleVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Uniform struct untuk pencahayaan global
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LightUniform {
    pub sun_direction: [f32; 3],
    pub _pad1: f32,
    pub sun_color: [f32; 3],
    pub _pad2: f32,
    pub ambient_color: [f32; 3],
    pub _pad3: f32,
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            // Arah sinar matahari diagonal ke bawah
            sun_direction: [-0.5, -0.8, -0.4],
            _pad1: 0.0,
            // Warna matahari hangat lembut
            sun_color: [1.0, 0.96, 0.90],
            _pad2: 0.0,
            // Cahaya ambient pastel langit
            ambient_color: [0.45, 0.50, 0.58],
            _pad3: 0.0,
        }
    }
}

/// Buffer GPU untuk mesh dari satu chunk.
///
/// INVARIANT 6: GPU mesh adalah cache grafis turunan (derived), bukan data dunia otoritatif.
pub struct GpuChunkMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

/// Renderer wgpu utama untuk engine Omnisia
pub struct Renderer {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,

    pub render_pipeline: wgpu::RenderPipeline,
    pub depth_texture_view: wgpu::TextureView,

    // Uniform Buffers & Bind Group
    pub camera_buffer: wgpu::Buffer,
    pub light_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,

    // Procedural Sky Pass Resources (Phase 10.5)
    pub sky_pipeline: wgpu::RenderPipeline,
    pub sky_buffer: wgpu::Buffer,
    pub sky_bind_group: wgpu::BindGroup,

    // Developer Console Overlay Resources (Phase 10.5.x, Amendments 11 & 12)
    pub console_pipeline: wgpu::RenderPipeline,
    pub console_screen_buffer: wgpu::Buffer,
    pub console_bind_group: wgpu::BindGroup,
    pub console_vertex_buffer: wgpu::Buffer,
    pub console_vertex_capacity: usize,
    pub console_vertex_cache: Vec<ConsoleVertex>,

    // GPU Mesh Cache (keyed by Chunk Coordinate)
    pub chunk_meshes: HashMap<IVec3, GpuChunkMesh>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        // 1. Inisialisasi Instance wgpu (Metal pada macOS)
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // 2. Buat Surface
        let surface = instance.create_surface(window.clone())?;

        // 3. Request Adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap_or_else(|| panic!("Gagal menemukan GPU adapter yang kompatibel"));

        log::info!("GPU Adapter Terpilih: {:?}", adapter.get_info().name);
        log::info!("Backend Terpilih: {:?}", adapter.get_info().backend);

        // 4. Request Device & Queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Omnisia Primary Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await?;

        // 5. Konfigurasi Surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync, // Mencegah uncapped FPS & thermal throttling
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // 6. Buat Depth Buffer (Depth32Float)
        let depth_texture_view = Self::create_depth_view(&device, &config);

        // 7. Buat Uniform Buffers & Bind Groups
        let camera_uniform = CameraUniform::default();
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Uniform Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let light_uniform = LightUniform::default();
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light Uniform Buffer"),
            contents: bytemuck::cast_slice(&[light_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
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

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
        });

        // 8. Load WGSL Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Omnisia Half-Lambert Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // 9. Buat Render Pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Voxel Render Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Voxel Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[VoxelVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 10. Procedural Sky Pass Setup (Phase 10.5, Amendment 1 & 10)
        let sky_uniform = SkyUniform::default();
        let sky_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sky Uniform Buffer"),
            contents: bytemuck::cast_slice(&[sky_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sky_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Sky Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let sky_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sky Bind Group"),
            layout: &sky_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sky_buffer.as_entire_binding(),
            }],
        });

        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Omnisia Procedural Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });

        let sky_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Render Pipeline Layout"),
            bind_group_layouts: &[&sky_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Render Pipeline"),
            layout: Some(&sky_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_sky"),
                buffers: &[], // Fullscreen triangle without vertex buffer allocations
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 11. Developer Console Overlay Setup (Phase 10.5.x, Amendments 11 & 12)
        let font_pixels = generate_font_atlas_pixels();
        let font_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Console Font Atlas Texture"),
            size: wgpu::Extent3d {
                width: ATLAS_WIDTH as u32,
                height: ATLAS_HEIGHT as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &font_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &font_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_WIDTH as u32),
                rows_per_image: Some(ATLAS_HEIGHT as u32),
            },
            wgpu::Extent3d {
                width: ATLAS_WIDTH as u32,
                height: ATLAS_HEIGHT as u32,
                depth_or_array_layers: 1,
            },
        );

        let font_texture_view = font_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let font_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Console Font Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let console_screen_uniform = ConsoleScreenUniform {
            screen_size: [width as f32, height as f32],
            _pad: [0.0, 0.0],
        };
        let console_screen_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Console Screen Uniform Buffer"),
            contents: bytemuck::cast_slice(&[console_screen_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let console_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Console Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let console_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Console Bind Group"),
            layout: &console_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: console_screen_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&font_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&font_sampler),
                },
            ],
        });

        let console_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Omnisia Developer Console Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("console.wgsl").into()),
        });

        let console_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Console Render Pipeline Layout"),
                bind_group_layouts: &[&console_bind_group_layout],
                push_constant_ranges: &[],
            });

        let console_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Console Render Pipeline"),
            layout: Some(&console_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &console_shader,
                entry_point: Some("vs_main"),
                buffers: &[ConsoleVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &console_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let console_vertex_capacity = 16384;
        let console_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Console Vertex Buffer"),
            size: (console_vertex_capacity * std::mem::size_of::<ConsoleVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            depth_texture_view,
            camera_buffer,
            light_buffer,
            uniform_bind_group,
            sky_pipeline,
            sky_buffer,
            sky_bind_group,
            console_pipeline,
            console_screen_buffer,
            console_bind_group,
            console_vertex_buffer,
            console_vertex_capacity,
            console_vertex_cache: Vec::with_capacity(console_vertex_capacity),
            chunk_meshes: HashMap::new(),
        })
    }

    fn create_depth_view(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        depth_texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_texture_view = Self::create_depth_view(&self.device, &self.config);
        }
    }

    pub fn update_camera(&self, camera_uniform: &CameraUniform) {
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[*camera_uniform]),
        );
    }

    pub fn update_light(&self, light_uniform: &LightUniform) {
        self.queue.write_buffer(
            &self.light_buffer,
            0,
            bytemuck::cast_slice(&[*light_uniform]),
        );
    }

    pub fn update_sky(&self, sky_uniform: &SkyUniform) {
        self.queue
            .write_buffer(&self.sky_buffer, 0, bytemuck::cast_slice(&[*sky_uniform]));
    }

    /// Mengunggah data mesh CPU ke GPU buffer cache
    pub fn upload_chunk_mesh(&mut self, chunk_coord: IVec3, mesh_data: &MeshData) {
        if mesh_data.is_empty() {
            self.chunk_meshes.remove(&chunk_coord);
            return;
        }

        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Chunk {:?} Vertex Buffer", chunk_coord)),
                contents: bytemuck::cast_slice(&mesh_data.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Chunk {:?} Index Buffer", chunk_coord)),
                contents: bytemuck::cast_slice(&mesh_data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        self.chunk_meshes.insert(
            chunk_coord,
            GpuChunkMesh {
                vertex_buffer,
                index_buffer,
                index_count: mesh_data.indices.len() as u32,
            },
        );
    }
}

/// Telemetri dan metrik rendering runtime yang akurat dan ringan tanpa alokasi heap per frame
#[derive(Debug, Clone, Default)]
pub struct RenderMetrics {
    pub cpu_resident_chunks: usize,
    pub gpu_mesh_count: usize,
    pub render_eligible_chunks: usize,
    pub frustum_visible_chunks: usize,
    pub frustum_culled_chunks: usize,
    pub submitted_indices: usize,
    pub uploads_this_frame: usize,
    pub upload_backlog: usize,
    pub pending_mesh_jobs: usize,
    pub frame_time_ms: f32,
    pub fps: f32,
}

impl Renderer {
    pub fn remove_chunk_mesh(&mut self, chunk_coord: &IVec3) {
        self.chunk_meshes.remove(chunk_coord);
    }

    /// Menghapus semua GPU mesh yang tidak lagi berada dalam set chunk koordinat aktif
    pub fn retain_only(&mut self, active_coords: &std::collections::HashSet<IVec3>) {
        self.chunk_meshes
            .retain(|coord, _| active_coords.contains(coord));
    }

    /// Melakukan render frame lengkap dengan Frustum Culling, Render-Distance filtering, dan Developer Console overlay
    pub fn render(
        &mut self,
        frustum: &crate::camera::Frustum,
        camera_chunk: IVec3,
        render_radius: i32,
        console: Option<&ConsoleState>,
    ) -> Result<RenderMetrics, wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Tahap Awal: Persiapkan Developer Console Overlay (Phase 10.5.x, Amendment 12)
        // Jika konsol tertutup (is_open == false atau None), return 0:
        // ZERO alokasi vertex, ZERO transfer buffer GPU, ZERO overhead!
        let console_vertex_count = if let Some(c) = console {
            if c.is_open() {
                self.prepare_console_overlay(c)
            } else {
                0
            }
        } else {
            0
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Primary Render Encoder"),
            });

        let mut render_eligible = 0;
        let mut frustum_visible = 0;
        let mut frustum_culled = 0;
        let mut submitted_indices = 0;

        {
            // Warna langit pastel lembut untuk background clear
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primary Voxel Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.72,
                            g: 0.82,
                            b: 0.92,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for (&coord, gpu_mesh) in &self.chunk_meshes {
                if gpu_mesh.index_count == 0 {
                    continue;
                }

                // 1. Filter Tahap: Render-Distance Eligible (<= render_radius horizontal, <= 2 vertical)
                let dx = (coord.x - camera_chunk.x).abs();
                let dz = (coord.z - camera_chunk.z).abs();
                let dy = (coord.y - camera_chunk.y).abs();
                if dx > render_radius || dz > render_radius || dy > 2 {
                    continue;
                }
                render_eligible += 1;

                // 2. Filter Tahap: Frustum Visible
                if !frustum.intersects_chunk(coord) {
                    frustum_culled += 1;
                    continue;
                }

                // 3. Aksi: Draw Submission
                frustum_visible += 1;
                submitted_indices += gpu_mesh.index_count as usize;

                render_pass.set_vertex_buffer(0, gpu_mesh.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(gpu_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..1);
            }

            // 4. Procedural Sky Pass (Phase 10.5, Amendment 1 & 10)
            // The sky is depth-tested against already-rendered opaque terrain so pixels whose depth
            // is already less than 1.0 are rejected by the depth test. GPU early/hierarchical depth
            // optimization may reduce fragment work, but exact fragment execution behavior is implementation-dependent.
            render_pass.set_pipeline(&self.sky_pipeline);
            render_pass.set_bind_group(0, &self.sky_bind_group, &[]);
            render_pass.draw(0..3, 0..1);

            // 5. Developer Console Overlay Pass (Phase 10.5.x, Amendments 11 & 12)
            if console_vertex_count > 0 {
                render_pass.set_pipeline(&self.console_pipeline);
                render_pass.set_bind_group(0, &self.console_bind_group, &[]);
                let byte_size =
                    (console_vertex_count * std::mem::size_of::<ConsoleVertex>()) as u64;
                render_pass.set_vertex_buffer(0, self.console_vertex_buffer.slice(0..byte_size));
                render_pass.draw(0..console_vertex_count as u32, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(RenderMetrics {
            cpu_resident_chunks: 0,
            gpu_mesh_count: self.chunk_meshes.len(),
            render_eligible_chunks: render_eligible,
            frustum_visible_chunks: frustum_visible,
            frustum_culled_chunks: frustum_culled,
            submitted_indices,
            uploads_this_frame: 0,
            upload_backlog: 0,
            pending_mesh_jobs: 0,
            frame_time_ms: 0.0,
            fps: 0.0,
        })
    }

    /// Menghasilkan mesh 2D konsol dan mengunggah ke GPU buffer (Phase 10.5.x, Amendment 11 & 12).
    /// Hanya dieksekusi jika konsol sedang terbuka (`is_open == true`).
    fn prepare_console_overlay(&mut self, console: &ConsoleState) -> usize {
        self.console_vertex_cache.clear();
        let screen_w = self.config.width as f32;
        let screen_h = self.config.height as f32;
        let console_h = (screen_h * 0.48).clamp(240.0, 520.0);

        let scale = if screen_w >= 1024.0 { 2.0 } else { 1.0 };
        let char_w = 8.0 * scale;
        let char_h = 8.0 * scale;
        let line_h = 10.0 * scale;
        let pad = 10.0 * scale;

        let push_quad = |cache: &mut Vec<ConsoleVertex>,
                         x0: f32,
                         y0: f32,
                         x1: f32,
                         y1: f32,
                         u0: f32,
                         v0: f32,
                         u1: f32,
                         v1: f32,
                         color: [f32; 4]| {
            cache.push(ConsoleVertex {
                pos: [x0, y0],
                uv: [u0, v0],
                color,
            });
            cache.push(ConsoleVertex {
                pos: [x1, y0],
                uv: [u1, v0],
                color,
            });
            cache.push(ConsoleVertex {
                pos: [x1, y1],
                uv: [u1, v1],
                color,
            });
            cache.push(ConsoleVertex {
                pos: [x0, y0],
                uv: [u0, v0],
                color,
            });
            cache.push(ConsoleVertex {
                pos: [x1, y1],
                uv: [u1, v1],
                color,
            });
            cache.push(ConsoleVertex {
                pos: [x0, y1],
                uv: [u0, v1],
                color,
            });
        };

        // 1. Background Panel (Translucent dark slate)
        push_quad(
            &mut self.console_vertex_cache,
            0.0,
            0.0,
            screen_w,
            console_h,
            -1.0,
            -1.0,
            -1.0,
            -1.0,
            [0.04, 0.05, 0.08, 0.88],
        );

        // 2. Bottom Accent Border
        push_quad(
            &mut self.console_vertex_cache,
            0.0,
            console_h - (2.0 * scale),
            screen_w,
            console_h,
            -1.0,
            -1.0,
            -1.0,
            -1.0,
            [0.25, 0.55, 0.90, 0.95],
        );

        let render_text =
            |cache: &mut Vec<ConsoleVertex>, text: &str, mut x: f32, y: f32, color: [f32; 4]| {
                for c in text.chars() {
                    if x + char_w > screen_w - pad {
                        break;
                    }
                    let uv = glyph_uv(c);
                    push_quad(
                        cache,
                        x,
                        y,
                        x + char_w,
                        y + char_h,
                        uv[0],
                        uv[1],
                        uv[2],
                        uv[3],
                        color,
                    );
                    x += char_w;
                }
            };

        // 3. Header Banner
        let header_text =
            "--- OMNISIA DEVELOPER CONSOLE --- (Type 'help' for commands, '`' or 'F1' to toggle)";
        render_text(
            &mut self.console_vertex_cache,
            header_text,
            pad,
            pad,
            [0.35, 0.75, 1.0, 1.0],
        );

        // Header divider line
        let div_y = pad + line_h + 2.0;
        push_quad(
            &mut self.console_vertex_cache,
            pad,
            div_y,
            screen_w - pad,
            div_y + 1.0,
            -1.0,
            -1.0,
            -1.0,
            -1.0,
            [0.2, 0.3, 0.45, 0.5],
        );

        // Scroll indicator if scrolled
        if console.scroll_offset > 0 {
            let scroll_text = format!("[SCROLLED +{}]", console.scroll_offset);
            let scroll_x = screen_w - pad - (scroll_text.len() as f32 * char_w);
            render_text(
                &mut self.console_vertex_cache,
                &scroll_text,
                scroll_x,
                pad,
                [1.0, 0.8, 0.2, 1.0],
            );
        }

        // 4. Output / Scrollback History
        let input_line_y = console_h - line_h - pad;
        let out_top_y = div_y + 4.0;
        let available_out_h = input_line_y - out_top_y - 4.0;
        let max_visible_lines = ((available_out_h / line_h).floor() as usize).max(1);

        let total_lines = console.output_lines.len();
        let end_idx = total_lines.saturating_sub(console.scroll_offset);
        let start_idx = end_idx.saturating_sub(max_visible_lines);

        let mut line_draw_y = out_top_y;
        for i in start_idx..end_idx {
            if let Some(line) = console.output_lines.get(i) {
                let color = match line.kind {
                    ConsoleLineKind::Input => [0.85, 0.92, 1.0, 1.0],
                    ConsoleLineKind::Output => [0.78, 0.82, 0.88, 1.0],
                    ConsoleLineKind::Info => [0.35, 0.75, 1.0, 1.0],
                    ConsoleLineKind::Error => [1.0, 0.35, 0.35, 1.0],
                };
                render_text(
                    &mut self.console_vertex_cache,
                    &line.text,
                    pad,
                    line_draw_y,
                    color,
                );
            }
            line_draw_y += line_h;
        }

        // 5. Input Prompt Line Background
        push_quad(
            &mut self.console_vertex_cache,
            pad,
            input_line_y - 2.0,
            screen_w - pad,
            input_line_y + line_h + 2.0,
            -1.0,
            -1.0,
            -1.0,
            -1.0,
            [0.08, 0.10, 0.14, 0.85],
        );

        // Prompt symbol "> "
        render_text(
            &mut self.console_vertex_cache,
            "> ",
            pad + 4.0,
            input_line_y,
            [0.3, 0.85, 1.0, 1.0],
        );

        // Current input buffer
        let input_text_x = pad + 4.0 + 2.0 * char_w;
        render_text(
            &mut self.console_vertex_cache,
            &console.input_buffer,
            input_text_x,
            input_line_y,
            [1.0, 1.0, 1.0, 1.0],
        );

        // Cursor indicator
        let cursor_x = input_text_x + (console.cursor_pos as f32 * char_w);
        push_quad(
            &mut self.console_vertex_cache,
            cursor_x,
            input_line_y,
            cursor_x + (2.0 * scale),
            input_line_y + char_h,
            -1.0,
            -1.0,
            -1.0,
            -1.0,
            [0.3, 0.85, 1.0, 0.95],
        );

        let vertex_count = self
            .console_vertex_cache
            .len()
            .min(self.console_vertex_capacity);
        if vertex_count > 0 {
            let screen_uniform = ConsoleScreenUniform {
                screen_size: [screen_w, screen_h],
                _pad: [0.0, 0.0],
            };
            self.queue.write_buffer(
                &self.console_screen_buffer,
                0,
                bytemuck::cast_slice(&[screen_uniform]),
            );
            self.queue.write_buffer(
                &self.console_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.console_vertex_cache[0..vertex_count]),
            );
        }
        vertex_count
    }
}
