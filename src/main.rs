use std::sync::Arc;
use std::time::Instant;

use glam::IVec3;
use omnisia::camera::Camera;
use omnisia::coord::{world_pos_to_world_voxel, CHUNK_SIZE};
use omnisia::modding::runtime::ContentRuntime;
use omnisia::renderer::{LightUniform, Renderer};
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct AppState {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    world: World,
    camera: Camera,
    last_frame_time: Instant,
    fps_timer: Instant,
    frame_count: u32,
}

impl AppState {
    fn new(world: World) -> Self {
        Self {
            window: None,
            renderer: None,
            world,
            camera: Camera::new(
                glam::Vec3::new(0.0, 35.0, 0.0), // Spawn sedikit di atas elevasi permukaan awal
                -90.0,                           // Hadap ke arah -Z awal
                -10.0,
            ),
            last_frame_time: Instant::now(),
            fps_timer: Instant::now(),
            frame_count: 0,
        }
    }
}

impl ApplicationHandler for AppState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("Omnisia Voxel Engine - Phase 6")
                .with_inner_size(PhysicalSize::new(1280, 720));

            let window = Arc::new(
                event_loop
                    .create_window(window_attributes)
                    .expect("Gagal membuat window winit"),
            );

            let renderer = pollster::block_on(Renderer::new(window.clone()))
                .expect("Gagal menginisialisasi renderer wgpu Metal");

            // Setup initial lighting
            renderer.update_light(&LightUniform::default());

            self.window = Some(window);
            self.renderer = Some(renderer);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Menutup aplikasi Omnisia.");
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(physical_size);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.camera.handle_keyboard(&event);
            }
            WindowEvent::MouseInput { button, state, .. } => {
                self.camera.handle_mouse_button(button, state);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame_time).as_secs_f32().min(0.1);
                self.last_frame_time = now;

                self.camera.update(dt);

                // Update Streaming World & Integrasi Mesh GPU
                self.world
                    .update(self.camera.position, dt, self.renderer.as_mut());

                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let aspect = renderer.size.width as f32 / renderer.size.height.max(1) as f32;
                    let camera_uniform = self.camera.build_uniform(aspect);
                    renderer.update_camera(&camera_uniform);

                    let frustum = self.camera.extract_frustum(aspect);
                    let camera_voxel = world_pos_to_world_voxel(self.camera.position);
                    let center_chunk = IVec3::new(
                        camera_voxel.x.div_euclid(CHUNK_SIZE),
                        camera_voxel.y.div_euclid(CHUNK_SIZE),
                        camera_voxel.z.div_euclid(CHUNK_SIZE),
                    );

                    let render_result =
                        renderer.render(&frustum, center_chunk, self.world.render_radius);

                    match render_result {
                        Ok(mut metrics) => {
                            self.frame_count += 1;
                            let elapsed = self.fps_timer.elapsed().as_secs_f32();
                            if elapsed >= 0.5 {
                                let fps = self.frame_count as f32 / elapsed;
                                let frame_ms = 1000.0 / fps.max(1.0);
                                metrics.cpu_resident_chunks = self.world.store.resident_count();
                                metrics.uploads_this_frame = self.world.last_uploads_count;
                                metrics.upload_backlog = self.world.upload_backlog();
                                metrics.pending_mesh_jobs = self.world.pending_jobs_count();
                                metrics.frame_time_ms = frame_ms;
                                metrics.fps = fps;

                                let mem = self.world.store.memory_usage(0);
                                let title = format!(
                                    "Omnisia [Phase 6] | FPS: {:.1} ({:.2}ms) | CPU Resident: {} | GPU: {} | Vis: {}/{} | Culled: {} | Indices: {} | Uploads: +{}/backlog:{} | Mem: {:.1}MB",
                                    fps,
                                    frame_ms,
                                    metrics.cpu_resident_chunks,
                                    metrics.gpu_mesh_count,
                                    metrics.frustum_visible_chunks,
                                    metrics.render_eligible_chunks,
                                    metrics.frustum_culled_chunks,
                                    metrics.submitted_indices,
                                    metrics.uploads_this_frame,
                                    metrics.upload_backlog,
                                    mem.total_megabytes(),
                                );
                                window.set_title(&title);
                                self.frame_count = 0;
                                self.fps_timer = Instant::now();
                            }
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            renderer.resize(renderer.size);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            log::error!("GPU Out of Memory!");
                            event_loop.exit();
                        }
                        Err(e) => {
                            log::warn!("Render warning: {:?}", e);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.camera.handle_mouse_motion(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--validate-mods") {
        log::info!("Menjalankan validasi Core Content dan mod Omnisia...");
        let resolved = ContentRuntime::build_runtime("content/core", "mods")
            .expect("Validasi Core Content & Mod gagal");

        println!("============================================================");
        println!("           OMNISIA CONTENT & MOD VALIDATION REPORT          ");
        println!("============================================================");
        println!(
            "[OK] core (Built-in Core: {} materials, {} blocks)",
            resolved.materials.len(),
            resolved.blocks.len()
        );
        println!(
            "\nMod Eksternal Ditemukan: {}",
            resolved.report.total_discovered
        );
        for (mod_id, summary) in &resolved.report.loaded_mods {
            println!(
                "[OK] {} (Materials: {}, Blocks: {}, Overrides: {})",
                mod_id, summary.materials_loaded, summary.blocks_loaded, summary.overrides_applied
            );
        }
        if !resolved.report.applied_overrides.is_empty() {
            println!("\nExplicit Overrides Diterapkan:");
            for ov_msg in &resolved.report.applied_overrides {
                println!("  [OVERRIDE] {}", ov_msg);
            }
        }
        println!("\nTotal Registry Terdaftar:");
        println!("  Materials: {}", resolved.materials.len());
        println!("  Blocks:    {}", resolved.blocks.len());
        println!("============================================================");
        return;
    }

    log::info!("Memulai engine Omnisia (Phase 6 - Vegetation & Performance)...");

    let event_loop = EventLoop::new().expect("Gagal menginisialisasi EventLoop winit");
    event_loop.set_control_flow(ControlFlow::Poll);

    let seed = WorldSeed::default();
    let world = World::with_seed(seed);
    let mut app = AppState::new(world);

    event_loop.run_app(&mut app).expect("Aplikasi crashed");
}
