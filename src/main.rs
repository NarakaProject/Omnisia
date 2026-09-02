use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use omnisia::camera::Camera;
use omnisia::mesh::generate_culled_mesh;
use omnisia::mesh::types::MeshData;
use omnisia::modding::validate_mods_directory;
use omnisia::renderer::Renderer;
use omnisia::world::World;

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    world: World,
    last_frame_time: Instant,
    fps_timer: Instant,
    frame_count: u32,
    total_vertices: usize,
    total_indices: usize,
}

impl Default for App {
    fn default() -> Self {
        // Posisi kamera awal menghadap bukit dan struktur melayang
        let camera = Camera::new(
            Vec3::new(16.0, 18.0, 38.0),
            -90.0, // Yaw menghadap sumbu -Z
            -15.0, // Pitch melihat sedikit ke bawah
        );

        let mut world = World::new();

        // Bangun demo world
        world.generate_demo_world();

        Self {
            window: None,
            renderer: None,
            camera,
            world,
            last_frame_time: Instant::now(),
            fps_timer: Instant::now(),
            frame_count: 0,
            total_vertices: 0,
            total_indices: 0,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attrs = WindowAttributes::default()
            .with_title("Omnisia - Micro-Voxel Engine [Phase 2.5: Core Content Boundary Active]")
            .with_inner_size(PhysicalSize::new(1280, 720));

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("Gagal membuat window winit"),
        );

        let mut renderer = pollster::block_on(Renderer::new(window.clone()))
            .expect("Gagal menginisialisasi renderer wgpu");

        // Generate mesh untuk semua chunk di demo world dan unggah ke GPU cache
        let mut total_verts = 0;
        let mut total_inds = 0;
        let mut mesh_buffer = MeshData::new();

        for (&coord, chunk) in &self.world.chunks {
            generate_culled_mesh(chunk, &self.world.materials, &mut mesh_buffer);
            total_verts += mesh_buffer.vertex_count();
            total_inds += mesh_buffer.index_count();
            renderer.upload_chunk_mesh(coord, &mesh_buffer);
        }

        self.total_vertices = total_verts;
        self.total_indices = total_inds;

        log::info!(
            "Mesh diunggah ke GPU: {} Chunks, {} Vertices, {} Indices ({} Quads)",
            self.world.chunks.len(),
            self.total_vertices,
            self.total_indices,
            self.total_indices / 6
        );

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last_frame_time = Instant::now();
        self.fps_timer = Instant::now();
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

                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let aspect = renderer.size.width as f32 / renderer.size.height.max(1) as f32;
                    let camera_uniform = self.camera.build_uniform(aspect);
                    renderer.update_camera(&camera_uniform);

                    match renderer.render() {
                        Ok(_) => {}
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

                    // FPS & Frame Timing Metrics
                    self.frame_count += 1;
                    if self.fps_timer.elapsed().as_secs_f32() >= 1.0 {
                        let fps = self.frame_count as f32 / self.fps_timer.elapsed().as_secs_f32();
                        let frame_ms = 1000.0 / fps.max(1.0);
                        let title = format!(
                            "Omnisia Micro-Voxel Engine | FPS: {:.1} ({:.2} ms) | Materials: {} | Blocks: {} | Verts: {}",
                            fps,
                            frame_ms,
                            self.world.materials.len(),
                            self.world.blocks.len(),
                            self.total_vertices
                        );
                        window.set_title(&title);
                        self.frame_count = 0;
                        self.fps_timer = Instant::now();
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
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|arg| arg == "--validate-mods" || arg == "-v")
    {
        log::info!("Menjalankan validasi Core Content dan mod Omnisia...");
        let report = validate_mods_directory("content/core", "mods");
        report.print_summary();
        if !report.is_all_ok() {
            std::process::exit(1);
        }
        return;
    }

    log::info!("Memulai Omnisia Micro-Voxel Engine [Phase 2.5: Core Boundary Active]...");

    let event_loop = EventLoop::new().expect("Gagal membuat winit EventLoop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .expect("Terjadi error pada EventLoop");
}
