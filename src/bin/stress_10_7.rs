//! OMNISIA — PHASE 10.7 INTEGRATION, PERFORMANCE & VISUAL STRESS HARNESS
//!
//! Autonomous, deterministic stress test harness and performance characterization
//! across the full Phase 10 engine stack:
//! - Deterministic 65-second gameplay stress route:
//!   * Phase 1 (0–5s):   Idle / Static World (Establish quiet baseline)
//!   * Phase 2 (5–10s):  Smooth Camera Movement
//!   * Phase 3 (10–20s): Fast Camera Movement across chunk boundaries
//!   * Phase 4 (20–35s): Localized Voxel Destruction (Small ~10, Medium ~100, Large ~1000 voxels)
//!   * Phase 5 (35–45s): Direction Reversal & Return through recently visited/destroyed chunks
//!   * Phase 6 (45–55s): Vertical Camera Movement (Y in [-100, 4000]m respecting clamp)
//!   * Phase 7 (55–65s): Continuous Environment & Celestial Transition (Day -> Dusk -> Midnight -> Aurora -> Dawn)
//!
//! Measures exact frame times, P50, P90, P95, P99, Max, frames > 16.67ms, frames > 20ms, frames > 33.33ms,
//! and subsystem breakdowns.

use std::time::Instant;

use glam::{IVec3, Vec3};
use omnisia::camera::Camera;
use omnisia::csg::crater::CraterGenerator;
use omnisia::csg::policy::DefaultDestructionPolicy;
use omnisia::environment::sky::SkyUniform;
use omnisia::environment::EnvironmentState;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;
use wgpu::util::DeviceExt;

/// Summary statistics for a collection of frame times in milliseconds.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    pub count: usize,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub over_16_ms: usize,
    pub over_20_ms: usize,
    pub over_33_ms: usize,
}

impl FrameStats {
    pub fn compute(mut samples: Vec<f64>) -> Self {
        if samples.is_empty() {
            return Self::default();
        }

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let count = samples.len();
        let sum: f64 = samples.iter().sum();
        let mean_ms = sum / count as f64;

        let p50_idx = ((count as f64) * 0.50).min((count - 1) as f64) as usize;
        let p90_idx = ((count as f64) * 0.90).min((count - 1) as f64) as usize;
        let p95_idx = ((count as f64) * 0.95).min((count - 1) as f64) as usize;
        let p99_idx = ((count as f64) * 0.99).min((count - 1) as f64) as usize;

        let max_ms = *samples.last().unwrap_or(&0.0);
        let over_16_ms = samples.iter().filter(|&&s| s > 16.67).count();
        let over_20_ms = samples.iter().filter(|&&s| s > 20.0).count();
        let over_33_ms = samples.iter().filter(|&&s| s > 33.33).count();

        Self {
            count,
            mean_ms,
            p50_ms: samples[p50_idx],
            p90_ms: samples[p90_idx],
            p95_ms: samples[p95_idx],
            p99_ms: samples[p99_idx],
            max_ms,
            over_16_ms,
            over_20_ms,
            over_33_ms,
        }
    }

    pub fn print_row(&self, scenario_name: &str) {
        println!(
            "  {:<24} | {:>6.2} | {:>6.2} | {:>6.2} | {:>6.2} | {:>6.2} | {:>6.2} | {:>8} | {:>8}",
            scenario_name,
            self.mean_ms,
            self.p50_ms,
            self.p90_ms,
            self.p95_ms,
            self.p99_ms,
            self.max_ms,
            self.over_16_ms,
            self.over_33_ms,
        );
    }
}

/// Headless GPU context for actual sky and voxel pass submission timing.
struct HeadlessGpuHarness {
    device: wgpu::Device,
    queue: wgpu::Queue,
    sky_pipeline: wgpu::RenderPipeline,
    sky_buffer: wgpu::Buffer,
    sky_bind_group: wgpu::BindGroup,
    render_target: wgpu::TextureView,
}

impl HeadlessGpuHarness {
    fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Stress Harness Headless Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .ok()?;

        let texture_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Target 1280x720"),
            size: wgpu::Extent3d {
                width: 1280,
                height: 720,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let render_target = texture.create_view(&wgpu::TextureViewDescriptor::default());

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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Pipeline Layout"),
            bind_group_layouts: &[&sky_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sky_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sky Uniform Buffer"),
            size: std::mem::size_of::<SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
            label: Some("Procedural Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../sky.wgsl").into()),
        });

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_sky"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
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
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Some(Self {
            device,
            queue,
            sky_pipeline,
            sky_buffer,
            sky_bind_group,
            render_target,
        })
    }

    fn render_sky_pass(&self, sky_uniform: &SkyUniform) -> f64 {
        self.queue
            .write_buffer(&self.sky_buffer, 0, bytemuck::cast_slice(&[*sky_uniform]));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Harness Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Harness Sky Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.render_target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.sky_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        let start = Instant::now();
        self.queue.submit(std::iter::once(encoder.finish()));
        self.device.poll(wgpu::Maintain::Wait);
        start.elapsed().as_secs_f64() * 1000.0
    }
}

fn main() {
    println!("================================================================================");
    println!("     OMNISIA — PHASE 10.7 INTEGRATION, PERFORMANCE & STRESS HARNESS             ");
    println!("     Deterministic 65-Second Real-Workload Route (Target: 60 FPS / <=16.67 ms)  ");
    println!("================================================================================");

    let gpu_harness = HeadlessGpuHarness::new();
    if gpu_harness.is_some() {
        println!("  [GPU Backend] Headless Metal/Vulkan GPU pipeline active (1280x720)");
    } else {
        println!("  [GPU Backend] Software/CPU-only fallback active");
    }

    let mut world = World::with_seed(WorldSeed(42));
    let mut env = EnvironmentState::new();
    let mut camera = Camera::new(Vec3::new(0.0, 35.0, 0.0), -90.0, 0.0);
    let policy = DefaultDestructionPolicy;

    // Warm up initial spawn region (radius = 5)
    println!("  [Pre-Warm] Dispatching initial world generation around spawn...");
    let spawn_pos = Vec3::new(0.0, 35.0, 0.0);
    for _ in 0..60 {
        world.update(spawn_pos, 0.016, None);
    }
    println!(
        "  [Pre-Warm] Ready: {} chunks resident, {} pending jobs",
        world.store.resident_count(),
        world.scheduler.pending_jobs_count()
    );

    let dt = 0.0166667; // 60 FPS fixed simulation step (16.67ms)
    let total_frames = 65 * 60; // 3900 frames for 65 seconds

    let mut all_frame_times = Vec::with_capacity(total_frames);
    let mut idle_times = Vec::new();
    let mut smooth_cam_times = Vec::new();
    let mut fast_cam_times = Vec::new();
    let mut destruction_times = Vec::new();
    let mut reverse_times = Vec::new();
    let mut vertical_times = Vec::new();
    let mut env_times = Vec::new();

    // Subsystem timing accumulators (microseconds)
    let mut sub_camera_us = 0.0;
    let mut sub_env_us = 0.0;
    let mut sub_world_us = 0.0;
    let mut sub_csg_us = 0.0;
    let mut sub_gpu_sky_us = 0.0;

    let harness_start = Instant::now();

    for frame in 0..total_frames {
        let sim_time = frame as f32 * dt;
        let frame_start = Instant::now();

        // --------------------------------------------------------------------
        // 1. INPUT & CAMERA UPDATE
        // --------------------------------------------------------------------
        let t_cam = Instant::now();
        if sim_time < 5.0 {
            // Phase 1: Idle (0–5s)
            // Camera stationary at spawn
        } else if sim_time < 10.0 {
            // Phase 2: Smooth Camera Movement (5–10s)
            // Walk forward at 5 m/s along Z
            camera.position.z -= 5.0 * dt;
            camera.yaw_deg = -90.0 + (sim_time - 5.0) * 4.0;
        } else if sim_time < 20.0 {
            // Phase 3: Fast Camera Movement (10–20s)
            // Move at 25 m/s crossing multiple chunk boundaries
            camera.position.x += 15.0 * dt;
            camera.position.z -= 20.0 * dt;
        } else if sim_time < 35.0 {
            // Phase 4: Localized Destruction Sequence (20–35s)
            // Camera circles around crater sites
            let circle_angle = (sim_time - 20.0) * 0.4;
            camera.position.x = 100.0 + circle_angle.cos() * 25.0;
            camera.position.z = -150.0 + circle_angle.sin() * 25.0;
            camera.yaw_deg = circle_angle * 57.2958;
        } else if sim_time < 45.0 {
            // Phase 5: Reverse Traversal (35–45s)
            // Return back toward origin
            camera.position.x -= 15.0 * dt;
            camera.position.z += 20.0 * dt;
        } else if sim_time < 55.0 {
            // Phase 6: Vertical Camera Traverse (45–55s)
            // Sweep Y from 35m up to 3950m (respecting clamp [-100, 4000])
            let progress = (sim_time - 45.0) / 10.0;
            camera.position.y = 35.0 + progress * (3950.0 - 35.0);
            camera.pitch_deg = -20.0 + progress * 40.0;
        } else {
            // Phase 7: Environment & Sky Transition (55–65s)
            // Camera looks north at sky while celestial clock rapidly transitions
            camera.position.y = 120.0;
            camera.pitch_deg = 35.0;
            camera.yaw_deg = -90.0; // Facing North (-Z)
        }
        sub_camera_us += t_cam.elapsed().as_secs_f64() * 1_000_000.0;

        // --------------------------------------------------------------------
        // 2. ENVIRONMENT & CELESTIAL CLOCK UPDATE
        // --------------------------------------------------------------------
        let t_env = Instant::now();
        if sim_time >= 55.0 {
            // Fast-forward cycle: Day (0.25) -> Dusk (0.45) -> Midnight (0.0) -> Aurora -> Dawn
            let env_speed = 0.08; // advance multiple hours per second
            env.advance(dt * env_speed * 120.0);
        } else {
            env.advance(dt);
        }
        sub_env_us += t_env.elapsed().as_secs_f64() * 1_000_000.0;

        // --------------------------------------------------------------------
        // 3. DESTRUCTION / VOXEL CSG EDITS
        // --------------------------------------------------------------------
        let t_csg = Instant::now();
        // Triggers at specific frames during Phase 4:
        // Frame 1320 (T=22s): Small crater (~10 voxels, r=0.7m)
        // Frame 1560 (T=26s): Medium crater (~100 voxels, r=1.5m)
        // Frame 1800 (T=30s): Large crater (~1000 voxels, r=3.2m)
        if frame == 1320 {
            let center = Vec3::new(camera.position.x, 0.0, camera.position.z);
            let _ = world.apply_crater(center, 0.7);
        } else if frame == 1560 {
            let center = Vec3::new(camera.position.x + 2.0, 0.0, camera.position.z);
            let _ = world.apply_crater(center, 1.5);
        } else if frame == 1800 {
            let center = Vec3::new(camera.position.x - 3.0, 0.0, camera.position.z);
            let _ = world.apply_crater(center, 3.2);
        }
        sub_csg_us += t_csg.elapsed().as_secs_f64() * 1_000_000.0;

        // --------------------------------------------------------------------
        // 4. WORLD STREAMING, CHUNK SCHEDULER & EVICTION UPDATE
        // --------------------------------------------------------------------
        let t_world = Instant::now();
        world.update(camera.position, dt, None);
        sub_world_us += t_world.elapsed().as_secs_f64() * 1_000_000.0;

        // --------------------------------------------------------------------
        // 5. GPU SKY PASS TIMING
        // --------------------------------------------------------------------
        let mut gpu_ms = 0.0;
        if let Some(ref harness) = gpu_harness {
            let sky_vp = camera.build_sky_view_projection_matrix(16.0 / 9.0);
            let inv_sky_vp = sky_vp.inverse();
            let sky_uniform = env.build_sky_uniform(inv_sky_vp, camera.position);
            gpu_ms = harness.render_sky_pass(&sky_uniform);
            sub_gpu_sky_us += gpu_ms * 1_000.0;
        }

        // Total effective frame time in milliseconds (CPU frame + GPU sky pass)
        let frame_ms = (frame_start.elapsed().as_secs_f64() * 1000.0) + gpu_ms;
        all_frame_times.push(frame_ms);

        if sim_time < 5.0 {
            idle_times.push(frame_ms);
        } else if sim_time < 10.0 {
            smooth_cam_times.push(frame_ms);
        } else if sim_time < 20.0 {
            fast_cam_times.push(frame_ms);
        } else if sim_time < 35.0 {
            destruction_times.push(frame_ms);
        } else if sim_time < 45.0 {
            reverse_times.push(frame_ms);
        } else if sim_time < 55.0 {
            vertical_times.push(frame_ms);
        } else {
            env_times.push(frame_ms);
        }
    }

    let total_wall_time = harness_start.elapsed();

    // Compute statistics across all phases
    let overall_stats = FrameStats::compute(all_frame_times);
    let idle_stats = FrameStats::compute(idle_times);
    let smooth_stats = FrameStats::compute(smooth_cam_times);
    let fast_stats = FrameStats::compute(fast_cam_times);
    let destruction_stats = FrameStats::compute(destruction_times);
    let reverse_stats = FrameStats::compute(reverse_times);
    let vertical_stats = FrameStats::compute(vertical_times);
    let env_stats = FrameStats::compute(env_times);

    println!("\n================================================================================");
    println!("     OMNISIA PHASE 10.7 — INTEGRATED FRAME-TIME PERFORMANCE MATRIX              ");
    println!("================================================================================");
    println!(
        "  {:<24} | {:>6} | {:>6} | {:>6} | {:>6} | {:>6} | {:>6} | {:>8} | {:>8}",
        "Scenario", "Mean", "P50", "P90", "P95", "P99", "Max", ">16.67ms", ">33.33ms"
    );
    println!("  --------------------------------------------------------------------------------------------------");
    idle_stats.print_row("1. Idle / Static");
    smooth_stats.print_row("2. Smooth Camera");
    fast_stats.print_row("3. Fast Streaming");
    destruction_stats.print_row("4. Destruction");
    reverse_stats.print_row("5. Reverse / Return");
    vertical_stats.print_row("6. Vertical Traverse");
    env_stats.print_row("7. Env Transition");
    println!("  --------------------------------------------------------------------------------------------------");
    overall_stats.print_row("OVERALL (65s / 3900f)");
    println!("================================================================================\n");

    let total_f = total_frames as f64;
    println!("================================================================================");
    println!("     SUBSYSTEM CPU / GPU BREAKDOWN (Average per frame)                          ");
    println!("================================================================================");
    println!(
        "  Camera & Input:        {:>7.3} ms/frame ({:>5.1}%)",
        (sub_camera_us / total_f) / 1000.0,
        ((sub_camera_us / total_f) / 1000.0 / overall_stats.mean_ms) * 100.0
    );
    println!(
        "  Environment & Clock:   {:>7.3} ms/frame ({:>5.1}%)",
        (sub_env_us / total_f) / 1000.0,
        ((sub_env_us / total_f) / 1000.0 / overall_stats.mean_ms) * 100.0
    );
    println!(
        "  World, Streaming, Evict: {:>5.3} ms/frame ({:>5.1}%)",
        (sub_world_us / total_f) / 1000.0,
        ((sub_world_us / total_f) / 1000.0 / overall_stats.mean_ms) * 100.0
    );
    println!(
        "  Destruction / CSG:     {:>7.3} ms/frame ({:>5.1}%)",
        (sub_csg_us / total_f) / 1000.0,
        ((sub_csg_us / total_f) / 1000.0 / overall_stats.mean_ms) * 100.0
    );
    println!(
        "  GPU Fullscreen Sky:    {:>7.3} ms/frame ({:>5.1}%)",
        (sub_gpu_sky_us / total_f) / 1000.0,
        ((sub_gpu_sky_us / total_f) / 1000.0 / overall_stats.mean_ms) * 100.0
    );
    println!("================================================================================");
    println!(
        "  Total 3900 frames executed in {:.2}s wall time",
        total_wall_time.as_secs_f64()
    );
    println!("================================================================================\n");

    // ------------------------------------------------------------------------
    // FORENSIC DIAGNOSTIC SUITE (HYPOTHESES H1, H2, H5 MEASUREMENTS)
    // ------------------------------------------------------------------------
    println!("================================================================================");
    println!("     PHASE 10.7 FORENSIC DIAGNOSTIC SUITE (HYPOTHESES H1 - H6)                   ");
    println!("================================================================================");

    // H1: GPU Buffer Allocation Burst Measurement (4 vs 8 vs 16 vs 32 uploads)
    if let Some(ref harness) = gpu_harness {
        println!("\n  --- [H1] GPU Buffer Allocation Burst Cost (device.create_buffer_init) ---");
        // Dummy chunk mesh data: 800 vertices (32 bytes each) + 1200 indices (4 bytes each)
        let vertex_bytes = vec![0u8; 800 * 32];
        let index_bytes = vec![0u8; 1200 * 4];

        for &batch_size in &[1, 4, 8, 16, 32] {
            let iters = 20;
            let mut total_ms = 0.0;
            for _ in 0..iters {
                let t_start = Instant::now();
                let mut buffers = Vec::with_capacity(batch_size * 2);
                for _ in 0..batch_size {
                    let vb = harness
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Diag Vertex Buffer"),
                            contents: &vertex_bytes,
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                    let ib = harness
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Diag Index Buffer"),
                            contents: &index_bytes,
                            usage: wgpu::BufferUsages::INDEX,
                        });
                    buffers.push((vb, ib));
                }
                total_ms += t_start.elapsed().as_secs_f64() * 1000.0;
            }
            let avg_ms = total_ms / iters as f64;
            let per_chunk_ms = avg_ms / batch_size as f64;
            println!(
                "    Burst Size: {:>2} chunks ({:>2} GPU buffers) | Total: {:>6.3} ms | Per chunk: {:>5.3} ms",
                batch_size, batch_size * 2, avg_ms, per_chunk_ms
            );
        }
    }

    // H2: Redundant Per-Frame Work Measurement
    println!("\n  --- [H2] Redundant Per-Frame CPU Operations ---");
    // 1. active_set allocation & traversal across 500 chunks
    {
        use std::collections::HashSet;
        let mut resident_map = std::collections::HashMap::new();
        for x in -5..=5 {
            for z in -5..=5 {
                for y in -2..=2 {
                    resident_map.insert(IVec3::new(x, y, z), ());
                }
            }
        }
        let resident_count = resident_map.len();
        let iters = 10_000;
        let t_start = Instant::now();
        for _ in 0..iters {
            let active_set: HashSet<IVec3> = resident_map.keys().cloned().collect();
            std::hint::black_box(active_set);
        }
        let dur = t_start.elapsed().as_secs_f64() * 1_000_000.0 / iters as f64;
        println!(
            "    `resident.keys().cloned().collect()` ({} chunks): {:.2} µs/frame",
            resident_count, dur
        );
    }

    // 2. upload_queue drain & sort across 32 queued meshes
    {
        let iters = 5_000;
        let mut queue: std::collections::VecDeque<(IVec3, omnisia::mesh::types::MeshData)> =
            std::collections::VecDeque::new();
        for x in 0..32 {
            queue.push_back((IVec3::new(x, 0, 0), omnisia::mesh::types::MeshData::new()));
        }
        let cam_pos = Vec3::ZERO;
        let t_start = Instant::now();
        for _ in 0..iters {
            let mut q_clone = queue.clone();
            let mut items: Vec<(IVec3, omnisia::mesh::types::MeshData)> =
                q_clone.drain(..).collect();
            items.sort_unstable_by(|(c1, _), (c2, _)| {
                let p1 = Vec3::new(c1.x as f32, c1.y as f32, c1.z as f32);
                let p2 = Vec3::new(c2.x as f32, c2.y as f32, c2.z as f32);
                cam_pos
                    .distance_squared(p1)
                    .partial_cmp(&cam_pos.distance_squared(p2))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            std::hint::black_box(items);
        }
        let dur = t_start.elapsed().as_secs_f64() * 1_000_000.0 / iters as f64;
        println!(
            "    `upload_queue.drain(..).collect()` + sort (32 items): {:.2} µs/frame",
            dur
        );
    }

    // 3. 605-element radius scan lookups
    {
        let iters = 5_000;
        let t_start = Instant::now();
        let r = 5;
        let center_chunk = IVec3::ZERO;
        for _ in 0..iters {
            let mut needed = 0;
            for dy in -2..=2 {
                for dz in -r..=r {
                    for dx in -r..=r {
                        let chunk_coord = center_chunk + IVec3::new(dx, dy, dz);
                        if !world.store.contains(&chunk_coord)
                            && !world.store.is_in_flight(&chunk_coord)
                        {
                            needed += 1;
                        }
                    }
                }
            }
            std::hint::black_box(needed);
        }
        let dur = t_start.elapsed().as_secs_f64() * 1_000_000.0 / iters as f64;
        println!(
            "    605-cell streaming discovery loop (unconditioned): {:.2} µs/frame",
            dur
        );
    }

    // H5: Destruction Scaling (10 vs 100 vs 1000 voxels)
    println!("\n  --- [H5] Destruction / CSG Crater Scaling ---");
    let mut diag_store = omnisia::streaming::store::ChunkStore::new();
    for cx in -1..=1 {
        for cy in -1..=1 {
            for cz in -1..=1 {
                let mut chunk = omnisia::chunk::Chunk::new(IVec3::new(cx, cy, cz));
                chunk.fill_material(omnisia::material::MaterialId::STONE);
                diag_store.insert(chunk);
            }
        }
    }
    for &(label, radius, expected_vox) in &[
        ("Small Crater (~10 voxels)", 0.7, 10),
        ("Medium Crater (~100 voxels)", 1.5, 100),
        ("Large Crater (~1000 voxels)", 3.2, 1000),
    ] {
        let center = Vec3::new(0.0, 0.0, 0.0);
        let iters = 100;
        let t_start = Instant::now();
        let mut edit_count = 0;
        let mut affected_chunks = 0;
        for _ in 0..iters {
            if let Ok(tx) =
                CraterGenerator::generate(center, radius, &policy, &world.materials, &diag_store)
            {
                edit_count = tx.len();
                if let Ok(delta) = tx.validate(&diag_store) {
                    affected_chunks = delta.mesh_invalidation_chunks.len();
                }
            }
        }
        let dur_us = t_start.elapsed().as_secs_f64() * 1_000_000.0 / iters as f64;
        println!(
            "    {:<28} | Radius: {:>4.1}m | Edits: {:>4} (target ~{:>4}) | Invalidated Chunks: {:>2} | Cost: {:>6.2} µs",
            label, radius, edit_count, expected_vox, affected_chunks, dur_us
        );
    }
    println!("================================================================================\n");
}
