use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::IVec3;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::camera::CameraUniform;
use crate::environment::SkyUniform;
use crate::mesh::types::{MeshData, VoxelVertex};

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

    /// Melakukan render frame lengkap dengan Frustum Culling dan Render-Distance filtering
    pub fn render(
        &mut self,
        frustum: &crate::camera::Frustum,
        camera_chunk: IVec3,
        render_radius: i32,
    ) -> Result<RenderMetrics, wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

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
}
