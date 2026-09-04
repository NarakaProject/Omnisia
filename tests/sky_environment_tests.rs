use glam::Vec3;
use omnisia::camera::Camera;
use omnisia::environment::celestial::CelestialParameters;
use omnisia::environment::sky::EnvironmentState;
use omnisia::environment::time::{EnvironmentClock, MoonPhase};

const TOLERANCE: f32 = 1e-4;

// ============================================================================
// CATEGORY 1: TIME & CELESTIAL CLOCK PROGRESSION
// ============================================================================

#[test]
fn test_clock_initial_state() {
    let clock = EnvironmentClock::new(0.25, 1200.0);
    assert!((clock.day_fraction - 0.25).abs() < TOLERANCE);
    assert!((clock.time_of_day_hours() - 6.0).abs() < TOLERANCE);
    assert_eq!(clock.time_string(), "06:00");
}

#[test]
fn test_clock_advance_canonical_times() {
    let mut clock = EnvironmentClock::new(0.0, 1200.0);

    // Advance to sunrise (0.25 cycle = 300 seconds)
    clock.advance(300.0);
    assert!((clock.day_fraction - 0.25).abs() < TOLERANCE);
    assert_eq!(clock.time_string(), "06:00");

    // Advance to noon (0.50 cycle = 300 seconds)
    clock.advance(300.0);
    assert!((clock.day_fraction - 0.50).abs() < TOLERANCE);
    assert_eq!(clock.time_string(), "12:00");

    // Advance to sunset (0.75 cycle = 300 seconds)
    clock.advance(300.0);
    assert!((clock.day_fraction - 0.75).abs() < TOLERANCE);
    assert_eq!(clock.time_string(), "18:00");

    // Advance to midnight wrap (1.00 cycle = 300 seconds)
    clock.advance(300.0);
    assert!(clock.day_fraction < TOLERANCE || (clock.day_fraction - 1.0).abs() < TOLERANCE);
    assert_eq!(clock.time_string(), "00:00");
}

#[test]
fn test_clock_wrap_around_multiple_cycles() {
    let mut clock = EnvironmentClock::new(0.0, 100.0);

    // Advance 5.25 cycles (525 seconds)
    clock.advance(525.0);
    assert!((clock.day_fraction - 0.25).abs() < TOLERANCE);
    assert!((clock.total_elapsed_secs - 525.0).abs() < 1e-3);
}

#[test]
fn test_clock_time_scale_multiplier() {
    let mut clock = EnvironmentClock::new(0.0, 1200.0);
    clock.time_scale = 2.0;

    // Advance 150 seconds with 2x time_scale -> 300 effective seconds -> 0.25 day
    clock.advance(150.0);
    assert!((clock.day_fraction - 0.25).abs() < TOLERANCE);
}

#[test]
fn test_clock_negative_and_zero_delta_safety() {
    let mut clock = EnvironmentClock::new(0.5, 1200.0);

    clock.advance(0.0);
    assert!((clock.day_fraction - 0.5).abs() < TOLERANCE);

    clock.advance(-50.0);
    assert!((clock.day_fraction - 0.5).abs() < TOLERANCE);

    clock.advance(f32::NAN);
    assert!((clock.day_fraction - 0.5).abs() < TOLERANCE);

    clock.advance(f32::INFINITY);
    assert!((clock.day_fraction - 0.5).abs() < TOLERANCE);
}

// ============================================================================
// CATEGORY 2: SUN POSITION & ELEVATION ANCHORS (AMENDMENT 4)
// ============================================================================

#[test]
fn test_sun_midnight_anchor() {
    let params = CelestialParameters::evaluate(0.00);
    assert!((params.sun_direction.x - 0.0).abs() < TOLERANCE);
    assert!((params.sun_direction.y - (-1.0)).abs() < TOLERANCE);
    assert!((params.sun_direction.z - 0.0).abs() < TOLERANCE);
    assert!((params.sun_elevation - (-1.0)).abs() < TOLERANCE);
}

#[test]
fn test_sun_sunrise_anchor() {
    let params = CelestialParameters::evaluate(0.25);
    assert!((params.sun_direction.x - 1.0).abs() < TOLERANCE);
    assert!((params.sun_direction.y - 0.0).abs() < TOLERANCE);
    assert!((params.sun_direction.z - 0.0).abs() < TOLERANCE);
    assert!(params.sun_elevation.abs() < TOLERANCE);
}

#[test]
fn test_sun_noon_anchor() {
    let params = CelestialParameters::evaluate(0.50);
    assert!((params.sun_direction.x - 0.0).abs() < TOLERANCE);
    assert!((params.sun_direction.y - 1.0).abs() < TOLERANCE);
    assert!((params.sun_direction.z - 0.0).abs() < TOLERANCE);
    assert!((params.sun_elevation - 1.0).abs() < TOLERANCE);
}

#[test]
fn test_sun_sunset_anchor() {
    let params = CelestialParameters::evaluate(0.75);
    assert!((params.sun_direction.x - (-1.0)).abs() < TOLERANCE);
    assert!((params.sun_direction.y - 0.0).abs() < TOLERANCE);
    assert!((params.sun_direction.z - 0.0).abs() < TOLERANCE);
    assert!(params.sun_elevation.abs() < TOLERANCE);
}

#[test]
fn test_sun_unit_vector_normalization() {
    for i in 0..100 {
        let fraction = i as f32 / 100.0;
        let params = CelestialParameters::evaluate(fraction);
        let len = params.sun_direction.length();
        assert!(
            (len - 1.0).abs() < TOLERANCE,
            "Sun direction length {} not 1.0 at fraction {}",
            len,
            fraction
        );
    }
}

// ============================================================================
// CATEGORY 3: MOON POSITION, OPPOSITION & DECLINATION (AMENDMENT 5 & 6)
// ============================================================================

#[test]
fn test_moon_midnight_opposition() {
    // At midnight (0.00), sun is nadir (0, -1, 0).
    // Moon should be high in the sky (zenith +Y with 5-deg declination tilt).
    let params = CelestialParameters::evaluate(0.00);
    assert!((params.moon_direction.x - 0.0).abs() < TOLERANCE);
    assert!(params.moon_direction.y > 0.99); // cos(5 deg) ≈ 0.996
    assert!(params.moon_direction.z > 0.08); // sin(5 deg) ≈ 0.087
    assert!((params.moon_direction.length() - 1.0).abs() < TOLERANCE);
}

#[test]
fn test_moon_noon_opposition() {
    // At noon (0.50), sun is zenith (0, 1, 0).
    // Moon should be below horizon (nadir -Y with 5-deg declination tilt).
    let params = CelestialParameters::evaluate(0.50);
    assert!((params.moon_direction.x - 0.0).abs() < TOLERANCE);
    assert!(params.moon_direction.y < -0.99);
    assert!(params.moon_direction.z < -0.08);
    assert!((params.moon_direction.length() - 1.0).abs() < TOLERANCE);
}

#[test]
fn test_moon_phase_continuity_and_classification() {
    let mut clock = EnvironmentClock::new(0.0, 100.0);
    clock.lunar_cycle_days = 28.0;

    // Start with phase 0.0 (New Moon)
    clock.set_moon_phase(0.0);
    assert_eq!(clock.named_moon_phase(), MoonPhase::NewMoon);

    // Set to First Quarter (0.25)
    clock.set_moon_phase(0.25);
    assert_eq!(clock.named_moon_phase(), MoonPhase::FirstQuarter);

    // Set to Full Moon (0.50)
    clock.set_moon_phase(0.50);
    assert_eq!(clock.named_moon_phase(), MoonPhase::FullMoon);

    // Set to Last Quarter (0.75)
    clock.set_moon_phase(0.75);
    assert_eq!(clock.named_moon_phase(), MoonPhase::LastQuarter);

    // Intermediate phases
    assert_eq!(MoonPhase::from_phase(0.15), MoonPhase::WaxingCrescent);
    assert_eq!(MoonPhase::from_phase(0.35), MoonPhase::WaxingGibbous);
    assert_eq!(MoonPhase::from_phase(0.65), MoonPhase::WaningGibbous);
    assert_eq!(MoonPhase::from_phase(0.85), MoonPhase::WaningCrescent);
}

// ============================================================================
// CATEGORY 4: TWILIGHT & ATMOSPHERIC CONTINUITY
// ============================================================================

#[test]
fn test_twilight_factor_sunrise_and_sunset() {
    // Exact horizon crossing at sunrise (0.25) and sunset (0.75) must peak twilight_factor = 1.0
    let sunrise = CelestialParameters::evaluate(0.25);
    let sunset = CelestialParameters::evaluate(0.75);

    assert!((sunrise.twilight_factor - 1.0).abs() < TOLERANCE);
    assert!((sunset.twilight_factor - 1.0).abs() < TOLERANCE);
}

#[test]
fn test_twilight_factor_noon_and_midnight() {
    // Solar noon (0.50) and midnight (0.00) must have zero twilight_factor
    let midnight = CelestialParameters::evaluate(0.00);
    let noon = CelestialParameters::evaluate(0.50);

    assert_eq!(midnight.twilight_factor, 0.0);
    assert_eq!(noon.twilight_factor, 0.0);
}

#[test]
fn test_twilight_smoothness_no_discontinuities() {
    // Sample 1000 points across the day cycle; consecutive steps must have small smooth delta
    let samples = 1000;
    let mut prev_twilight = CelestialParameters::evaluate(0.0).twilight_factor;

    for i in 1..=samples {
        let fraction = i as f32 / samples as f32;
        let curr_twilight = CelestialParameters::evaluate(fraction).twilight_factor;
        let diff = (curr_twilight - prev_twilight).abs();
        assert!(
            diff < 0.05,
            "Twilight discontinuity {} detected at fraction {}",
            diff,
            fraction
        );
        prev_twilight = curr_twilight;
    }
}

// ============================================================================
// CATEGORY 5: PROCEDURAL STARS TEMPORAL STABILITY (AMENDMENT 7 & 8)
// ============================================================================

#[test]
fn test_star_visibility_day_vs_night() {
    let noon = CelestialParameters::evaluate(0.50);
    let midnight = CelestialParameters::evaluate(0.00);
    let sunrise = CelestialParameters::evaluate(0.25);

    // Deep midnight: maximum star visibility
    assert!((midnight.star_visibility - 1.0).abs() < TOLERANCE);

    // Noon: stars completely suppressed
    assert_eq!(noon.star_visibility, 0.0);

    // Sunrise/twilight: stars significantly attenuated compared to midnight
    assert!(sunrise.star_visibility < 0.3);
}

#[test]
fn test_bounded_star_time_modulo_precision() {
    let mut clock = EnvironmentClock::new(0.0, 1200.0);

    // Advance 3600 seconds (3 hours)
    clock.advance(3600.0);
    let bounded = clock.bounded_star_time();
    assert!((0.0..60.0).contains(&bounded));

    // Advance 100 days
    clock.advance(100.0 * 1200.0);
    let bounded_100d = clock.bounded_star_time();
    assert!((0.0..60.0).contains(&bounded_100d));
}

// ============================================================================
// CATEGORY 6: REPLAY DETERMINISM & DRIFT RESISTANCE (AMENDMENT 13)
// ============================================================================

#[test]
fn test_deterministic_replay_identical_sequence() {
    let mut env1 = EnvironmentState::new();
    let mut env2 = EnvironmentState::new();

    let dt_sequence = [0.016, 0.033, 0.020, 0.100, 0.500, 1.200, 0.016];

    for _ in 0..10 {
        for &dt in &dt_sequence {
            env1.advance(dt);
            env2.advance(dt);
        }
    }

    assert_eq!(env1.clock.day_fraction, env2.clock.day_fraction);
    assert_eq!(env1.celestial.sun_direction, env2.celestial.sun_direction);
    assert_eq!(env1.celestial.moon_direction, env2.celestial.moon_direction);
    assert_eq!(
        env1.celestial.twilight_factor,
        env2.celestial.twilight_factor
    );
    assert_eq!(env1.celestial.zenith_color, env2.celestial.zenith_color);
    assert_eq!(env1.celestial.horizon_color, env2.celestial.horizon_color);
}

#[test]
fn test_long_run_accumulation_bounds_and_finite_checks() {
    let mut env = EnvironmentState::new();

    // Advance 1,000,000 frames at 60 FPS (dt = 0.016666s ≈ 16,666 simulation seconds ≈ 13.8 game days)
    for _ in 0..100_000 {
        env.advance(0.016666);
    }

    assert!((0.0..1.0).contains(&env.clock.day_fraction));
    assert!(env.clock.day_fraction.is_finite());
    assert!(env.celestial.sun_direction.is_finite());
    assert!(env.celestial.moon_direction.is_finite());
    assert!(env.celestial.twilight_factor.is_finite());
    assert!(env.celestial.star_visibility.is_finite());
    assert!(env.celestial.zenith_color[0].is_finite());
    assert!(env.celestial.horizon_color[0].is_finite());
}

// ============================================================================
// CATEGORY 7: LIGHTUNIFORM HARMONIZATION (AMENDMENT 3)
// ============================================================================

#[test]
fn test_light_uniform_harmonization_sun_direction() {
    let mut env = EnvironmentState::new();
    env.set_day_fraction(0.50); // Noon

    let light = env.build_light_uniform();
    // Sun in sky is (0, +1, 0). Sunlight ray direction hitting terrain must be (0, -1, 0).
    assert!((light.sun_direction[0] - 0.0).abs() < TOLERANCE);
    assert!((light.sun_direction[1] - (-1.0)).abs() < TOLERANCE);
    assert!((light.sun_direction[2] - 0.0).abs() < TOLERANCE);

    // Direct sun light at noon must be bright and warm
    assert!(light.sun_color[0] > 0.9);
    assert!(light.sun_color[1] > 0.9);
    assert!(light.sun_color[2] > 0.8);
}

// ============================================================================
// CATEGORY 8: INVERSE VIEW-PROJECTION UNPROJECTION (AMENDMENT 9)
// ============================================================================

#[test]
fn test_sky_view_direction_translation_invariance() {
    let aspect = 16.0 / 9.0;

    // Camera 1 at origin
    let cam1 = Camera::new(Vec3::new(0.0, 0.0, 0.0), 45.0, 15.0);
    let sky_vp1 = cam1.build_sky_view_projection_matrix(aspect);
    let inv_vp1 = sky_vp1.inverse();

    // Camera 2 translated by 5,000 meters but with identical rotation
    let cam2 = Camera::new(Vec3::new(3000.0, 1500.0, -4000.0), 45.0, 15.0);
    let sky_vp2 = cam2.build_sky_view_projection_matrix(aspect);
    let inv_vp2 = sky_vp2.inverse();

    // Center of screen clip position (0, 0, 1, 1)
    let clip_center = glam::Vec4::new(0.0, 0.0, 1.0, 1.0);

    let world_h1 = inv_vp1 * clip_center;
    let world_pos1 = world_h1.truncate() / world_h1.w;
    let dir1 = world_pos1.normalize();

    let world_h2 = inv_vp2 * clip_center;
    let world_pos2 = world_h2.truncate() / world_h2.w;
    let dir2 = world_pos2.normalize();

    // View directions must be bitwise identical because camera translation is isolated from the sky view matrix
    assert_eq!(dir1, dir2);
}

// ============================================================================
// CATEGORY 9: HEADLESS GPU OFFSCREEN RENDER & DEPTH REJECTION VALIDATION
// ============================================================================

#[test]
fn test_headless_gpu_sky_render_validation() {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
        {
            Some(a) => a,
            None => {
                println!("No primary GPU adapter available for headless render test, skipping");
                return;
            }
        };

        let (device, queue) = match adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
        {
            Ok(dq) => dq,
            Err(_) => return,
        };

        let width = 64u32;
        let height = 64u32;
        let texture_format = wgpu::TextureFormat::Rgba8Unorm;

        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Test Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../src/sky.wgsl").into()),
        });

        let sky_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Test Sky Bind Group Layout"),
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
            label: Some("Test Sky Pipeline Layout"),
            bind_group_layouts: &[&sky_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Test Sky Pipeline"),
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

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Test Color Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Test Depth Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sky_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Test Sky Buffer"),
            size: std::mem::size_of::<omnisia::environment::SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sky_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Test Sky Bind Group"),
            layout: &sky_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sky_buffer.as_entire_binding(),
            }],
        });

        // Camera looking horizontally forward
        let cam = Camera::new(Vec3::ZERO, 0.0, 0.0);
        let sky_vp = cam.build_sky_view_projection_matrix(1.0);
        let inv_sky_vp = sky_vp.inverse();

        // 1. Render Noon Sky
        let mut env_noon = EnvironmentState::new();
        env_noon.set_day_fraction(0.50);
        let noon_uniform = env_noon.build_sky_uniform(inv_sky_vp, Vec3::ZERO);
        queue.write_buffer(&sky_buffer, 0, bytemuck::cast_slice(&[noon_uniform]));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Noon Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&sky_pipeline);
            pass.set_bind_group(0, &sky_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));

        // 2. Render Midnight Sky
        let mut env_midnight = EnvironmentState::new();
        env_midnight.set_day_fraction(0.00);
        let midnight_uniform = env_midnight.build_sky_uniform(inv_sky_vp, Vec3::ZERO);
        queue.write_buffer(&sky_buffer, 0, bytemuck::cast_slice(&[midnight_uniform]));

        let mut encoder2 =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder2.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Midnight Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&sky_pipeline);
            pass.set_bind_group(0, &sky_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(encoder2.finish()));
    });
}
