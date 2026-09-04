use std::sync::Arc;
use std::time::Instant;

use glam::IVec3;
use omnisia::camera::Camera;
use omnisia::console::{
    create_default_registry, CameraMode, CommandRegistry, ConsoleState, DeveloperCameraContext,
    DeveloperExecutionContext,
};
use omnisia::coord::{world_pos_to_world_voxel, world_voxel_to_chunk_and_local, CHUNK_SIZE};
use omnisia::environment::EnvironmentState;
use omnisia::modding::runtime::ContentRuntime;
use omnisia::player::{PlayerController, PlayerInput};
use omnisia::renderer::{LightUniform, Renderer};
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Mode kendali kamera dan pergerakan (alias kompatibilitas untuk CameraMode)
pub type ControlMode = CameraMode;

struct AppState {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    world: World,
    player_camera: Camera,
    camera_ctx: DeveloperCameraContext,
    console: ConsoleState,
    command_registry: CommandRegistry,
    player: PlayerController,
    environment: EnvironmentState,

    // Status input keyboard untuk mode player
    key_w: bool,
    key_s: bool,
    key_a: bool,
    key_d: bool,
    key_shift: bool,
    key_crouch: bool,
    key_jump: bool,

    last_frame_time: Instant,
    fps_timer: Instant,
    frame_count: u32,
    current_fps: f32,
    current_frame_ms: f32,
    ignore_next_mouse_motion: bool,
}

impl AppState {
    fn new(world: World) -> Self {
        let spawn_pos = glam::Vec3::new(0.0, 35.0, 0.0);
        let player = PlayerController::new(spawn_pos);
        let player_eye = player.eye_position();
        let player_camera = Camera::new(player_eye, -90.0, -10.0);
        let camera_ctx = DeveloperCameraContext::new(spawn_pos, player_eye);
        let console = ConsoleState::new();
        let command_registry = create_default_registry();

        Self {
            window: None,
            renderer: None,
            world,
            player_camera,
            camera_ctx,
            console,
            command_registry,
            player,
            environment: EnvironmentState::new(),
            key_w: false,
            key_s: false,
            key_a: false,
            key_d: false,
            key_shift: false,
            key_crouch: false,
            key_jump: false,
            last_frame_time: Instant::now(),
            fps_timer: Instant::now(),
            frame_count: 0,
            current_fps: 60.0,
            current_frame_ms: 16.6,
            ignore_next_mouse_motion: false,
        }
    }

    /// Explicit cursor and mouse capture state management (Mandates 13, 14, 15, 16, 17).
    /// - Developer Camera (console closed): locked/hidden cursor for true free-look.
    /// - Player Camera or Console Open: released/visible cursor.
    /// - Resets accumulated mouse delta to avoid sudden camera jumps on mode transitions.
    fn update_cursor_grab(&mut self) {
        if let Some(window) = &self.window {
            if self.camera_ctx.is_developer() && !self.console.is_open() {
                let _ = window
                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(winit::window::CursorGrabMode::Confined));
                window.set_cursor_visible(false);
            } else {
                let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
                window.set_cursor_visible(true);
            }
        }
        self.ignore_next_mouse_motion = true;
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
            self.update_cursor_grab();
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
                let pressed = event.state == ElementState::Pressed;
                let key_code = match event.physical_key {
                    PhysicalKey::Code(code) => Some(code),
                    _ => None,
                };

                // 1. Console Toggle: Backquote (`) canonical, F1 optional alias (Amendment 6)
                if let Some(code) = key_code {
                    if (code == KeyCode::Backquote || code == KeyCode::F1) && pressed {
                        self.console.toggle();
                        if self.console.is_open() {
                            // Clear player input keys to avoid stuck movement
                            self.key_w = false;
                            self.key_s = false;
                            self.key_a = false;
                            self.key_d = false;
                            self.key_shift = false;
                            self.key_crouch = false;
                            self.key_jump = false;
                            self.player.set_input(PlayerInput::default());
                        }
                        self.update_cursor_grab();
                        return;
                    }
                }

                // 2. Intercept keyboard input when developer console is open
                if self.console.is_open() {
                    if pressed {
                        if let Some(code) = key_code {
                            match code {
                                KeyCode::Enter | KeyCode::NumpadEnter => {
                                    let mut ctx = DeveloperExecutionContext {
                                        camera: &mut self.camera_ctx,
                                        environment: &mut self.environment,
                                        resident_chunks: self.world.store.resident_count(),
                                        fps: self.current_fps,
                                        frame_time_ms: self.current_frame_ms,
                                    };
                                    self.console.submit(&self.command_registry, &mut ctx);
                                    self.update_cursor_grab();
                                }
                                KeyCode::Backspace => self.console.backspace(),
                                KeyCode::Delete => self.console.delete(),
                                KeyCode::ArrowLeft => self.console.cursor_left(),
                                KeyCode::ArrowRight => self.console.cursor_right(),
                                KeyCode::Home => self.console.cursor_home(),
                                KeyCode::End => self.console.cursor_end(),
                                KeyCode::ArrowUp => self.console.history_prev(),
                                KeyCode::ArrowDown => self.console.history_next(),
                                KeyCode::PageUp => self.console.scroll_up(5),
                                KeyCode::PageDown => self.console.scroll_down(5),
                                KeyCode::Escape => {
                                    self.console.close();
                                    self.update_cursor_grab();
                                }
                                _ => {
                                    if let Some(text) = &event.text {
                                        for c in text.chars() {
                                            if !c.is_control() && c != '`' {
                                                self.console.insert_char(c);
                                            }
                                        }
                                    }
                                }
                            }
                        } else if let Some(text) = &event.text {
                            for c in text.chars() {
                                if !c.is_control() && c != '`' {
                                    self.console.insert_char(c);
                                }
                            }
                        }
                    }
                    return;
                }

                // 3. Normal Gameplay / Camera Controls when Console is Closed
                if let Some(key) = key_code {
                    match key {
                        KeyCode::KeyP | KeyCode::F3 => {
                            if pressed {
                                match self.camera_ctx.mode() {
                                    CameraMode::Player => {
                                        log::info!(
                                            "Beralih ke Developer Camera Mode (Free Camera)"
                                        );
                                        self.camera_ctx.set_mode(CameraMode::Developer);
                                    }
                                    CameraMode::Developer => {
                                        log::info!(
                                            "Beralih ke Player Mode (Kinematic Capsule Controller)"
                                        );
                                        self.camera_ctx.set_mode(CameraMode::Player);
                                        // Developer camera does NOT mutate player position! (Amendment 2)
                                    }
                                }
                                self.update_cursor_grab();
                            }
                        }
                        KeyCode::KeyW => self.key_w = pressed,
                        KeyCode::KeyS => self.key_s = pressed,
                        KeyCode::KeyA => self.key_a = pressed,
                        KeyCode::KeyD => self.key_d = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => self.key_shift = pressed,
                        KeyCode::KeyC | KeyCode::ControlLeft => self.key_crouch = pressed,
                        KeyCode::Space => self.key_jump = pressed,
                        _ => {}
                    }
                }

                if self.camera_ctx.is_developer() {
                    self.camera_ctx.dev_camera.handle_keyboard(&event);
                } else {
                    self.player.set_input(PlayerInput::from_raw(
                        self.key_w,
                        self.key_s,
                        self.key_a,
                        self.key_d,
                        self.key_shift,
                        self.key_crouch,
                        self.key_jump,
                    ));
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if !self.console.is_open() {
                    if self.camera_ctx.is_developer() {
                        self.camera_ctx
                            .dev_camera
                            .handle_mouse_button(button, state);
                    } else {
                        self.player_camera.handle_mouse_button(button, state);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame_time).as_secs_f32().min(0.1);
                self.last_frame_time = now;

                // Advance visual environment state (derived visual layer, Amendment 1 & 8)
                self.environment.advance(dt);

                // Sync read-only player snapshot to camera context (Amendment 3)
                self.camera_ctx
                    .sync_player_snapshot(self.player.state.position, self.player.eye_position());

                if self.camera_ctx.mode() == CameraMode::Player {
                    self.world
                        .update_player(&mut self.player, dt, self.player_camera.yaw_deg);
                    self.player_camera.position = self.player.eye_position();
                } else {
                    // Developer camera updates independently
                    self.camera_ctx.dev_camera.update(dt);
                    // Crucial: Keep player physics updated without modifying player pos from dev camera (Amendment 2)
                    self.world
                        .update_player(&mut self.player, dt, self.player_camera.yaw_deg);
                }

                // Reference to the active camera consuming the effective transform (Amendment 2)
                let active_camera = match self.camera_ctx.mode() {
                    CameraMode::Player => &self.player_camera,
                    CameraMode::Developer => &self.camera_ctx.dev_camera,
                };

                // Update Streaming World & Integrasi Mesh GPU
                self.world
                    .update(active_camera.position, dt, self.renderer.as_mut());

                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let aspect = renderer.size.width as f32 / renderer.size.height.max(1) as f32;
                    let camera_uniform = active_camera.build_uniform(aspect);
                    renderer.update_camera(&camera_uniform);

                    // Update Procedural Sky & Harmonized Light Uniforms (Phase 10.5, Amendment 2, 3, 9)
                    let sky_vp = active_camera.build_sky_view_projection_matrix(aspect);
                    let inv_sky_vp = sky_vp.inverse();
                    let sky_uniform = self
                        .environment
                        .build_sky_uniform(inv_sky_vp, glam::Vec3::ZERO);
                    renderer.update_sky(&sky_uniform);

                    let light_uniform = self.environment.build_light_uniform();
                    renderer.update_light(&light_uniform);

                    let frustum = active_camera.extract_frustum(aspect);
                    let camera_voxel = world_pos_to_world_voxel(active_camera.position);
                    let center_chunk = IVec3::new(
                        camera_voxel.x.div_euclid(CHUNK_SIZE),
                        camera_voxel.y.div_euclid(CHUNK_SIZE),
                        camera_voxel.z.div_euclid(CHUNK_SIZE),
                    );

                    let render_result = renderer.render(
                        &frustum,
                        center_chunk,
                        self.world.render_radius,
                        Some(&self.console),
                    );

                    match render_result {
                        Ok(mut metrics) => {
                            self.frame_count += 1;
                            let elapsed = self.fps_timer.elapsed().as_secs_f32();
                            if elapsed >= 0.5 {
                                let fps = self.frame_count as f32 / elapsed;
                                let frame_ms = 1000.0 / fps.max(1.0);
                                self.current_fps = fps;
                                self.current_frame_ms = frame_ms;
                                metrics.cpu_resident_chunks = self.world.store.resident_count();
                                metrics.uploads_this_frame = self.world.last_uploads_count;
                                metrics.upload_backlog = self.world.upload_backlog();
                                metrics.pending_mesh_jobs = self.world.pending_jobs_count();
                                metrics.frame_time_ms = frame_ms;
                                metrics.fps = fps;

                                let (chunk_coord, local_voxel) =
                                    world_voxel_to_chunk_and_local(camera_voxel);
                                let mem = self.world.store.memory_usage(0);

                                let title = if self.camera_ctx.mode() == CameraMode::Player {
                                    let crouch_str = if self.player.state.forced_crouch {
                                        "Yes(Forced)"
                                    } else if self.player.state.crouching {
                                        "Yes"
                                    } else {
                                        "No"
                                    };
                                    let active_gravity = if !self.player.state.grounded
                                        && self.player.state.gliding
                                    {
                                        self.player.config.gravity
                                            * self.player.config.glide_gravity_multiplier
                                    } else {
                                        self.player.config.gravity
                                    };
                                    let glide_str = if self.player.state.gliding {
                                        "Active"
                                    } else if self.player.state.airborne_origin
                                        == omnisia::player::AirborneOrigin::SprintJump
                                    {
                                        "Eligible"
                                    } else {
                                        "Ineligible"
                                    };
                                    format!(
                                        "Omnisia [10.5.x: Player] | State: {:?} | Origin: {:?} | Glide: {} | HSpd: {:.2}m/s | VSpd: {:.2}m/s | Feet: ({:.1}, {:.1}, {:.1})m | Grd: {} | Crouch: {} | Grav: {:.2} | Coll[q:{}, h:{}, unk:{}] | Tick: {:.1}µs | FPS: {:.1} ({:.2}ms) | CPU: {} | GPU: {} | Vis: {}/{} | Mem: {:.1}MB",
                                        self.player.state.movement_state,
                                        self.player.state.airborne_origin,
                                        glide_str,
                                        self.player.state.horizontal_speed(),
                                        self.player.state.velocity.y,
                                        self.player.state.position.x,
                                        self.player.state.position.y,
                                        self.player.state.position.z,
                                        self.player.state.grounded,
                                        crouch_str,
                                        active_gravity,
                                        self.player.collision_queries_total,
                                        self.player.collision_hits_total,
                                        self.player.unknown_blocked_total,
                                        self.player.last_tick_duration_us,
                                        fps,
                                        frame_ms,
                                        metrics.cpu_resident_chunks,
                                        metrics.gpu_mesh_count,
                                        metrics.frustum_visible_chunks,
                                        metrics.render_eligible_chunks,
                                        mem.total_megabytes(),
                                    )
                                } else {
                                    format!(
                                        "Omnisia [10.5.x: Developer Camera] | Pos: ({:.1}, {:.1}, {:.1})m | Chunk: ({}, {}, {}) | Vox: ({}, {}, {}) | Speed: {:.0}m/s [{}] | FPS: {:.1} ({:.2}ms) | CPU: {} | GPU: {} | Vis: {}/{} | Culled: {} | Struct[Evt:{}, Pend:{}, Agg:{}] | Mem: {:.1}MB",
                                        self.camera_ctx.dev_camera.position.x,
                                        self.camera_ctx.dev_camera.position.y,
                                        self.camera_ctx.dev_camera.position.z,
                                        chunk_coord.x,
                                        chunk_coord.y,
                                        chunk_coord.z,
                                        local_voxel.x,
                                        local_voxel.y,
                                        local_voxel.z,
                                        self.camera_ctx.dev_camera.speed,
                                        self.camera_ctx.dev_camera.active_preset.name(),
                                        fps,
                                        frame_ms,
                                        metrics.cpu_resident_chunks,
                                        metrics.gpu_mesh_count,
                                        metrics.frustum_visible_chunks,
                                        metrics.render_eligible_chunks,
                                        metrics.frustum_culled_chunks,
                                        self.world.structure.total_events_processed,
                                        self.world.structure.pending_checks.len(),
                                        self.world.structure.total_detached_extracted,
                                        mem.total_megabytes(),
                                    )
                                };
                                window.set_title(&title);

                                // Catat telemetri ke log setiap 2 detik
                                if self.fps_timer.elapsed().as_secs_f32() >= 2.0 {
                                    log::info!(
                                        "[TELEMETRY] Pos: ({:.1}m, {:.1}m, {:.1}m) | Chunk: ({}, {}, {}) | Vox: ({}, {}, {}) | Speed: {:.0}m/s [{}] | FPS: {:.1} | CPU: {} | GPU: {} | Vis: {}/{} | Culled: {} | Struct: [Evt:{}, Pend:{}, Agg:{}] | Mem: {:.1}MB",
                                        self.camera_ctx.dev_camera.position.x, self.camera_ctx.dev_camera.position.y, self.camera_ctx.dev_camera.position.z,
                                        chunk_coord.x, chunk_coord.y, chunk_coord.z,
                                        local_voxel.x, local_voxel.y, local_voxel.z,
                                        self.camera_ctx.dev_camera.speed, self.camera_ctx.dev_camera.active_preset.name(),
                                        fps,
                                        metrics.cpu_resident_chunks, metrics.gpu_mesh_count,
                                        metrics.frustum_visible_chunks, metrics.render_eligible_chunks,
                                        metrics.frustum_culled_chunks,
                                        self.world.structure.total_events_processed,
                                        self.world.structure.pending_checks.len(),
                                        self.world.structure.total_detached_extracted,
                                        mem.total_megabytes(),
                                    );
                                }

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
        if !self.console.is_open() {
            if let DeviceEvent::MouseMotion { delta } = event {
                // Discard synthetic mouse delta on transitions (Mandate 16)
                if self.ignore_next_mouse_motion {
                    self.ignore_next_mouse_motion = false;
                    return;
                }
                if self.camera_ctx.is_developer() {
                    self.camera_ctx
                        .dev_camera
                        .handle_mouse_motion(delta.0, delta.1);
                } else {
                    self.player_camera.handle_mouse_motion(delta.0, delta.1);
                }
            }
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

    if args.iter().any(|arg| arg == "--scale-validation") {
        use omnisia::scale::{HumanScaleReference, ScaleRuler, VegetationDimensionReport};

        println!("============================================================");
        println!("         OMNISIA REAL-WORLD SCALE VALIDATION REPORT         ");
        println!("============================================================");
        println!("Physical Metric Scale Constants:");
        println!("  1 Voxel = 0.50 meters");
        println!("  1 Chunk = 32 voxels = 16.0 meters (Volume: 32³ = 32,768 voxels)");
        println!("\nScale Ruler Standard Intervals:");
        for interval in omnisia::scale::SCALE_RULER_INTERVALS_METERS {
            let vx = ScaleRuler::meters_to_voxels(interval);
            println!("  Ruler {:>5.1}m = {:>5.0} voxels", interval, vx);
        }
        let human = HumanScaleReference::default();
        println!("\nHuman Scale Reference:");
        println!(
            "  Height: {:.2}m ({:.1} voxels)",
            human.height_meters, human.height_voxels
        );
        println!(
            "  Shoulder Width: {:.2}m ({:.1} voxels)",
            human.width_meters, human.width_voxels
        );

        println!("\nVegetation Dimension Verification (Actual vs Ecological Range):");
        let oak = VegetationDimensionReport::measure_oak(5, 2);
        println!(
            "  [{}] Trunk: {:.1}m, Canopy R: {:.1}m, Total H: {:.1}m (Expected: {}) -> Valid: {}",
            oak.name,
            oak.trunk_height_meters,
            oak.canopy_radius_meters,
            oak.total_height_meters,
            oak.expected_range_meters,
            oak.is_ecologically_valid
        );
        let pine = VegetationDimensionReport::measure_pine(7, 2);
        println!(
            "  [{}] Trunk: {:.1}m, Canopy R: {:.1}m, Total H: {:.1}m (Expected: {}) -> Valid: {}",
            pine.name,
            pine.trunk_height_meters,
            pine.canopy_radius_meters,
            pine.total_height_meters,
            pine.expected_range_meters,
            pine.is_ecologically_valid
        );

        println!("\nDeveloper Free-Flight Movement Presets:");
        println!("  [1] Slow:    5.0 m/s  (Micro/Voxel inspection)");
        println!("  [2] Normal: 20.0 m/s  (Running/exploration default)");
        println!("  [3] Fast:  100.0 m/s  (Cross-biome traversal)");
        println!("  [4] Extreme: 500.0 m/s (Large-scale streaming stress-test)");

        println!("\nStreaming Semantics Audit:");
        println!("  render_radius:     5 chunks (80.0m horizontal radius)");
        println!("  simulation_radius: 3 chunks (48.0m active simulation / high priority)");
        println!("  retain_radius:     7 chunks (112.0m memory retention buffer)");
        println!("  vertical_radius:   dy in [-2..=2] (5 vertical layers = 80.0m column)");
        println!("============================================================");
        return;
    }

    log::info!("Memulai engine Omnisia (Phase 7 - Structural Connectivity & Scale Validation)...");
    log::info!("{}", omnisia::scale::ScaleRuler::ruler_summary());
    log::info!("Movement Presets: [1] 5 m/s, [2] 20 m/s, [3] 100 m/s, [4] 500 m/s");

    let event_loop = EventLoop::new().expect("Gagal menginisialisasi EventLoop winit");
    event_loop.set_control_flow(ControlFlow::Poll);

    let seed = WorldSeed::default();
    let world = World::with_seed(seed);
    let mut app = AppState::new(world);

    event_loop.run_app(&mut app).expect("Aplikasi crashed");
}
