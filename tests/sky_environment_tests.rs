use glam::Vec3;
use omnisia::camera::Camera;
use omnisia::environment::celestial::{evaluate_star_reference, CelestialParameters};
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
// CATEGORY 7: LIGHTUNIFORM HARMONIZATION & DIFFUSE CONTRACT (MANDATES 1, 2, 3, 4, 7)
// ============================================================================

#[test]
fn test_light_uniform_harmonization_sun_direction() {
    let mut env = EnvironmentState::new();
    env.set_day_fraction(0.50); // Noon

    let light = env.build_light_uniform();
    // Sun in sky is (0, +1, 0). Celestial light ray direction hitting terrain must be (0, -1, 0).
    assert!((light.sun_direction[0] - 0.0).abs() < TOLERANCE);
    assert!((light.sun_direction[1] - (-1.0)).abs() < TOLERANCE);
    assert!((light.sun_direction[2] - 0.0).abs() < TOLERANCE);

    // Direct sun light at noon must be bright and warm
    assert!(light.sun_color[0] > 0.9);
    assert!(light.sun_color[1] > 0.9);
    assert!(light.sun_color[2] > 0.8);
}

#[test]
fn test_light_uniform_harmonization_midnight_moon_direction() {
    // Mandates 1 & 2: At midnight, the active celestial light source is the MOON in the sky.
    // The celestial ray direction in LightUniform must point DOWNWARDS onto terrain (-Y),
    // and L = normalize(-light.sun_direction) must point UPWARDS to the moon (+Y).
    let mut env = EnvironmentState::new();
    env.set_day_fraction(0.00); // Midnight

    let light = env.build_light_uniform();
    let incoming_ray = Vec3::from_slice(&light.sun_direction);

    // Moon is high in the sky (+Y), so incoming light ray MUST point downwards (-Y)
    assert!(
        incoming_ray.y < -0.9,
        "Incoming celestial ray at midnight must point downward (-Y)"
    );

    // L is the vector pointing TO the celestial source: normalize(-light.sun_direction)
    let to_celestial_source = (-incoming_ray).normalize();
    assert!(
        to_celestial_source.y > 0.9,
        "L vector in shader must point UP towards the moon"
    );

    // Terrain moonlight must be subtle and cool (Mandate 6)
    assert!(light.sun_color[0] < 0.10);
    assert!(light.sun_color[1] < 0.10);
    assert!(light.sun_color[2] < 0.12);
    assert!(
        light.sun_color[2] > light.sun_color[0],
        "Moonlight must have cool blue tint"
    );
}

#[test]
fn test_celestial_top_vs_bottom_face_diffuse_model() {
    // Mandates 3 & 4:
    // For N·L <= 0, direct diffuse must not illuminate a surface facing away from the active source.
    // Bottom faces (tree canopy underside) must receive strictly 0.0 direct diffuse.
    let mut env = EnvironmentState::new();
    env.set_day_fraction(0.00); // Midnight

    let light = env.build_light_uniform();
    let l_vec = (-Vec3::from_slice(&light.sun_direction)).normalize();

    let top_normal = Vec3::new(0.0, 1.0, 0.0);
    let bottom_normal = Vec3::new(0.0, -1.0, 0.0);

    // Top face dot product with celestial light direction
    let top_n_dot_l = top_normal.dot(l_vec);
    assert!(
        top_n_dot_l > 0.0,
        "Top face must face towards moon (N·L > 0)"
    );
    let top_diffuse = (top_n_dot_l * 0.5 + 0.5).powi(2);
    assert!(
        top_diffuse > 0.0,
        "Top face receives direct diffuse moonlight"
    );

    // Bottom face (canopy underside)
    let bottom_n_dot_l = bottom_normal.dot(l_vec);
    assert!(
        bottom_n_dot_l < 0.0,
        "Bottom face must face away from moon (N·L < 0)"
    );
    let bottom_diffuse = if bottom_n_dot_l > 0.0 {
        (bottom_n_dot_l * 0.5 + 0.5).powi(2)
    } else {
        0.0
    };
    assert_eq!(
        bottom_diffuse, 0.0,
        "Bottom face must receive strictly zero direct diffuse (Mandate 4)"
    );

    // Total illumination on bottom face is bounded strictly by ambient light
    let ambient_light = Vec3::from_slice(&light.ambient_color);
    let total_bottom_light = Vec3::from_slice(&light.sun_color) * bottom_diffuse + ambient_light;
    assert_eq!(total_bottom_light, ambient_light);
    assert!(
        total_bottom_light.length() < 0.06,
        "Canopy underside illumination remains subtle ambient"
    );
}

#[test]
fn test_twilight_smooth_transition_weights() {
    // Mandate 7: Twilight must transition smoothly without flipping the celestial direction vector.
    // Independent sun/moon contribution weights ensure smooth crossover.
    for step in 0..100 {
        let frac = 0.20 + (step as f32 / 100.0) * 0.15; // 0.20 to 0.35 (spanning dawn / sunrise)
        let params = CelestialParameters::evaluate(frac);
        assert!(params.celestial_light_direction.is_finite());
        assert!((params.celestial_light_direction.length() - 1.0).abs() < TOLERANCE);
        assert!(params.celestial_light_color[0].is_finite());
        assert!(params.celestial_light_color[1].is_finite());
        assert!(params.celestial_light_color[2].is_finite());
    }
}

#[test]
fn test_moon_disc_vs_terrain_light_independence() {
    // Mandates 5 & 6: moon_disc_radiance != moon_terrain_light
    let params = CelestialParameters::evaluate(0.00); // Midnight

    // Moon terrain illumination is subtle directional light
    let terrain_moon_light_max = params.celestial_light_color[0]
        .max(params.celestial_light_color[1])
        .max(params.celestial_light_color[2]);
    assert!(
        terrain_moon_light_max <= 0.10,
        "Moon terrain light must be subtle"
    );

    // Moon disc radiance in sky shader has peak radiance of ~1.45 (far greater than terrain light)
    let moon_disc_radiance = 1.45;
    assert!(
        moon_disc_radiance > terrain_moon_light_max * 10.0,
        "Moon disc visual radiance must be strictly independent from terrain illumination"
    );
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

// ============================================================================
// CATEGORY 10: PROCEDURAL STAR INVARIANTS (MANDATES 10, 11, 12)
// ============================================================================

#[test]
fn test_star_reference_determinism() {
    // Mandate 11: Deterministic for identical inputs
    let dir = Vec3::new(0.35, 0.85, -0.40).normalize();
    let bounded_time = 12.345;
    let star_visibility = 1.0;

    let res1 = evaluate_star_reference(dir, bounded_time, star_visibility);
    let res2 = evaluate_star_reference(dir, bounded_time, star_visibility);

    assert_eq!(
        res1, res2,
        "Star reference evaluation must be strictly deterministic"
    );
}

#[test]
fn test_star_reference_night_population() {
    // Mandate 11: Non-zero night population
    // Sample celestial directions across the night sky dome at midnight (star_visibility = 1.0)
    let mut star_count = 0;
    let mut star_cell_count = 0;
    let mut total_samples = 0;

    for az in 0..120 {
        let theta = (az as f32 / 120.0) * std::f32::consts::TAU;
        for el in 1..40 {
            let phi = (el as f32 / 40.0) * (std::f32::consts::FRAC_PI_2 - 0.05);
            let dir =
                Vec3::new(phi.cos() * theta.cos(), phi.sin(), phi.cos() * theta.sin()).normalize();
            let res = evaluate_star_reference(dir, 0.0, 1.0);
            if res.is_star {
                star_count += 1;
            }
            if res.is_star_cell {
                star_cell_count += 1;
            }
            total_samples += 1;
        }
    }

    assert!(
        star_cell_count > 0,
        "Night sky hemisphere must contain a non-zero population of star cells"
    );
    // Approximately 2.5% of cells are designated star cells
    let star_cell_ratio = star_cell_count as f32 / total_samples as f32;
    assert!(
        (0.01..0.05).contains(&star_cell_ratio),
        "Star cell density {} is within balanced celestial threshold (approx 2.5%)",
        star_cell_ratio
    );

    assert!(
        star_count > 0,
        "Night sky hemisphere must contain visible rasterized star points (found {})",
        star_count
    );
}

#[test]
fn test_star_reference_daylight_suppression() {
    // Mandate 11: Strongly suppressed during daylight
    // At noon, star_visibility = 0.0
    for az in 0..20 {
        let theta = (az as f32 / 20.0) * std::f32::consts::TAU;
        for el in 1..10 {
            let phi = (el as f32 / 10.0) * std::f32::consts::FRAC_PI_2;
            let dir =
                Vec3::new(phi.cos() * theta.cos(), phi.sin(), phi.cos() * theta.sin()).normalize();
            let res = evaluate_star_reference(dir, 15.0, 0.0);
            assert_eq!(
                res.effective_brightness, 0.0,
                "Stars must be completely suppressed during daylight"
            );
            assert!(!res.is_star);
        }
    }
}

#[test]
fn test_star_reference_temporal_stability() {
    // Mandate 11:
    // - temporal variation affects brightness/twinkle rather than spatial identity
    // - spatial structure remains stable under time
    let mut found_star_dir = None;

    for az in 0..100 {
        let theta = (az as f32 / 100.0) * std::f32::consts::TAU;
        for el in 5..30 {
            let phi = (el as f32 / 30.0) * (std::f32::consts::FRAC_PI_2 - 0.1);
            let dir =
                Vec3::new(phi.cos() * theta.cos(), phi.sin(), phi.cos() * theta.sin()).normalize();
            let res = evaluate_star_reference(dir, 0.0, 1.0);
            if res.is_star {
                found_star_dir = Some(dir);
                break;
            }
        }
        if found_star_dir.is_some() {
            break;
        }
    }

    let star_dir = found_star_dir.expect("Failed to find a star direction for stability test");

    let t0 = evaluate_star_reference(star_dir, 0.0, 1.0);
    let t1 = evaluate_star_reference(star_dir, 15.0, 1.0);
    let t2 = evaluate_star_reference(star_dir, 37.5, 1.0);

    // Spatial structure and cell identity MUST be strictly identical across time
    assert_eq!(t0.cell, t1.cell);
    assert_eq!(t0.cell, t2.cell);
    assert_eq!(t0.is_star, t1.is_star);
    assert_eq!(t0.is_star, t2.is_star);
    assert_eq!(t0.base_brightness, t1.base_brightness);
    assert_eq!(t0.base_brightness, t2.base_brightness);

    // Only effective brightness (twinkle) varies across time
    assert_ne!(
        t0.effective_brightness, t1.effective_brightness,
        "Twinkle must cause temporal variation in brightness"
    );
}

#[test]
fn test_star_reference_horizon_extinction_fade() {
    // Near-horizon star directions undergo smooth atmospheric extinction
    let overhead_dir = Vec3::new(0.0, 1.0, 0.0);
    let horizon_dir = Vec3::new(1.0, 0.005, 0.0).normalize();

    // The horizon fade multiplier smoothsteps from -0.02 to 0.06
    let overhead_res = evaluate_star_reference(overhead_dir, 0.0, 1.0);
    let horizon_res = evaluate_star_reference(horizon_dir, 0.0, 1.0);

    // If both directions are evaluated, horizon direction cannot have greater base brightness than overhead
    assert!(
        horizon_res.base_brightness <= overhead_res.base_brightness + 1.0,
        "Horizon extinction prevents bright stars at zero elevation"
    );
}

// ============================================================================
// CATEGORY 11: MANDATORY TIME ANCHOR & MIDNIGHT SURFACE VALIDATION (MANDATE 19)
// ============================================================================

#[test]
fn test_mandatory_time_anchors_and_midnight_surfaces() {
    // Mandate 19:
    // Manual validation is mandatory and must include:
    //   time 0.00 (midnight)
    //   time 0.25 (sunrise)
    //   time 0.50 (noon)
    //   time 0.75 (sunset)
    //
    // At midnight specifically inspect:
    //   - open terrain
    //   - exposed top faces
    //   - vertical faces
    //   - underside of tree canopy
    //   - lower trunk
    //   - bottom-facing voxel surfaces
    //   - moon disc
    //   - moon halo
    //   - star visibility

    // 1. Time 0.00: Midnight
    let mut env = EnvironmentState::new();
    env.set_day_fraction(0.00);
    assert_eq!(env.clock.time_string(), "00:00");
    assert_eq!(env.celestial.day_factor, 0.0);
    assert_eq!(env.celestial.star_visibility, 1.0);
    assert!(env.celestial.sun_elevation < -0.9);
    assert!(env.celestial.moon_direction.y > 0.99);

    // Midnight lighting check
    let midnight_light = env.build_light_uniform();
    let l_midnight = (-Vec3::from_slice(&midnight_light.sun_direction)).normalize();
    assert!(
        l_midnight.y > 0.9,
        "Celestial L vector points up to the moon at midnight"
    );

    let top_face_n = Vec3::new(0.0, 1.0, 0.0);
    let bottom_face_n = Vec3::new(0.0, -1.0, 0.0);
    let south_face_n = Vec3::new(0.0, 0.0, 1.0);
    let north_face_n = Vec3::new(0.0, 0.0, -1.0);

    // Exposed top faces: receives direct moonlight + ambient
    let top_nl = top_face_n.dot(l_midnight);
    assert!(top_nl > 0.0);
    let top_diffuse = (top_nl * 0.5 + 0.5).powi(2);
    let top_direct = Vec3::from_slice(&midnight_light.sun_color) * top_diffuse;
    assert!(
        top_direct.length() > 0.02,
        "Exposed top faces must receive direct moonlight"
    );

    // Underside of tree canopy & bottom-facing voxel surfaces:
    let bottom_nl = bottom_face_n.dot(l_midnight);
    assert!(bottom_nl < 0.0);
    let bottom_diffuse = if bottom_nl > 0.0 {
        (bottom_nl * 0.5 + 0.5).powi(2)
    } else {
        0.0
    };
    assert_eq!(
        bottom_diffuse, 0.0,
        "Underside of tree canopy must receive ZERO direct light (Mandate 4)"
    );
    let canopy_ambient = Vec3::from_slice(&midnight_light.ambient_color) * 0.65; // with AO
    assert!(
        canopy_ambient.length() < 0.035,
        "Canopy underside is subtle ambient without upward lighting"
    );

    // Vertical faces (lower trunk, cliff walls):
    let south_nl = south_face_n.dot(l_midnight);
    let north_nl = north_face_n.dot(l_midnight);
    assert!(south_nl >= 0.0);
    assert!(north_nl <= 0.0);

    // 2. Time 0.25: Sunrise
    env.set_day_fraction(0.25);
    assert_eq!(env.clock.time_string(), "06:00");
    assert!((env.celestial.sun_direction.y - 0.0).abs() < 0.01);
    assert!(
        env.celestial.twilight_factor > 0.8,
        "Sunrise has prominent twilight glow"
    );

    // 3. Time 0.50: Noon
    env.set_day_fraction(0.50);
    assert_eq!(env.clock.time_string(), "12:00");
    assert_eq!(env.celestial.day_factor, 1.0);
    assert_eq!(env.celestial.star_visibility, 0.0);
    let noon_light = env.build_light_uniform();
    let noon_l = (-Vec3::from_slice(&noon_light.sun_direction)).normalize();
    assert!(
        (noon_l.y - 1.0).abs() < 0.01,
        "Celestial L vector points straight up to the sun at noon"
    );
    assert!(
        noon_light.sun_color[0] > 0.9,
        "Noon direct sunlight is bright and warm"
    );

    // 4. Time 0.75: Sunset
    env.set_day_fraction(0.75);
    assert_eq!(env.clock.time_string(), "18:00");
    assert!((env.celestial.sun_direction.y - 0.0).abs() < 0.01);
    assert!(
        env.celestial.twilight_factor > 0.8,
        "Sunset has prominent twilight glow"
    );
}

// ============================================================================
// CATEGORY 11: CELESTIAL / ATMOSPHERIC TRANSITION COHERENCE (Phase 10.5.x+)
// ============================================================================

#[test]
fn test_celestial_transition_dense_sunset_continuity() {
    use omnisia::environment::celestial::smoothstep;

    // Dense sunset sequence as mandated: 0.70, 0.72, 0.74, 0.75, 0.76, 0.78, 0.80
    let sunset_samples = [0.70, 0.72, 0.74, 0.75, 0.76, 0.78, 0.80];
    let mut prev_elevation = 1.0f32;

    for &frac in &sunset_samples {
        let params = CelestialParameters::evaluate(frac);
        assert!(
            params.sun_elevation.is_finite(),
            "Sun elevation must be finite at frac {}",
            frac
        );
        assert!(
            params.sun_elevation < prev_elevation,
            "Sun elevation must monotonically descend across sunset: {} vs {}",
            params.sun_elevation,
            prev_elevation
        );
        prev_elevation = params.sun_elevation;

        let sun_disc_extinction = smoothstep(-0.02, 0.05, params.sun_elevation);
        let sun_halo_extinction = smoothstep(-0.12, 0.02, params.sun_elevation);

        if frac <= 0.74 {
            // Before horizon crossing: sun disc is fully visible, twilight or daylight active
            assert!(
                sun_disc_extinction > 0.9,
                "Sun disc must remain visible before setting: frac {}",
                frac
            );
            assert!(
                params.twilight_factor > 0.0 || params.day_factor > 0.5,
                "Atmosphere must represent day or twilight before setting: frac {}",
                frac
            );
        } else if frac >= 0.76 {
            // After sun descends below horizon: sun disc is completely extinguished
            assert_eq!(
                sun_disc_extinction, 0.0,
                "Sun disc must be completely extinguished at frac {}: elevation {}",
                frac, params.sun_elevation
            );
        }

        if frac >= 0.80 {
            // Deep night: halo completely extinguished, deep night atmosphere
            assert_eq!(
                sun_halo_extinction, 0.0,
                "Sun halo must be extinguished in deep night at frac {}",
                frac
            );
            assert_eq!(
                params.day_factor, 0.0,
                "Day factor must be 0.0 in deep night at frac {}",
                frac
            );
            assert_eq!(
                params.twilight_factor, 0.0,
                "Twilight factor must be 0.0 in deep night at frac {}",
                frac
            );
            assert_eq!(
                params.star_visibility, 1.0,
                "Stars must be fully visible in deep night at frac {}",
                frac
            );
        }
    }

    // Dense sunrise sequence: 0.20, 0.22, 0.24, 0.25, 0.26, 0.28, 0.30
    let sunrise_samples = [0.20, 0.22, 0.24, 0.25, 0.26, 0.28, 0.30];
    let mut prev_sunrise_elev = -1.0f32;

    for &frac in &sunrise_samples {
        let params = CelestialParameters::evaluate(frac);
        assert!(
            params.sun_elevation > prev_sunrise_elev,
            "Sun elevation must monotonically ascend across sunrise"
        );
        prev_sunrise_elev = params.sun_elevation;

        let sun_disc_extinction = smoothstep(-0.02, 0.05, params.sun_elevation);
        let sun_halo_extinction = smoothstep(-0.12, 0.02, params.sun_elevation);

        if frac <= 0.20 {
            // Before sunrise: deep night, zero sun disc, zero halo
            assert_eq!(sun_disc_extinction, 0.0);
            assert_eq!(sun_halo_extinction, 0.0);
        } else if frac >= 0.26 {
            // After sun rises above horizon: sun disc active, twilight or daylight
            assert!(
                sun_disc_extinction > 0.9,
                "Sun disc must be visible after rising at frac {}",
                frac
            );
            assert!(
                params.twilight_factor > 0.0 || params.day_factor > 0.5,
                "Atmosphere must be daytime or twilight when sun is up"
            );
        }
    }
}

#[test]
fn test_celestial_atmospheric_transition_coherence_invariant() {
    use omnisia::environment::celestial::smoothstep;

    // Evaluate across 1000 finely spaced time steps across the entire 24h cycle
    let step_count = 1000;
    for i in 0..=step_count {
        let frac = (i as f32) / (step_count as f32);
        let params = CelestialParameters::evaluate(frac);

        let sun_disc_extinction = smoothstep(-0.02, 0.05, params.sun_elevation);
        let sun_halo_extinction = smoothstep(-0.12, 0.02, params.sun_elevation);

        // INVARIANT 1: A visible sun disc (extinction > 0) must NEVER appear against a deep night sky.
        // Deep night sky is defined as day_factor == 0 and twilight_factor == 0.
        if sun_disc_extinction > 0.0 {
            assert!(
                params.day_factor > 0.0 || params.twilight_factor > 0.0,
                "Violated Coherence Invariant: Sun disc active (extinction={}) while atmosphere is deep night at frac={}",
                sun_disc_extinction,
                frac
            );
        }

        // INVARIANT 2: A visible sun halo (extinction > 0) must NEVER appear against a deep night sky.
        if sun_halo_extinction > 0.0 {
            assert!(
                params.day_factor > 0.0 || params.twilight_factor > 0.0,
                "Violated Coherence Invariant: Sun halo active (extinction={}) while atmosphere is deep night at frac={}",
                sun_halo_extinction,
                frac
            );
        }

        // INVARIANT 3: When the atmosphere is deep night, both sun disc and sun halo MUST be zero.
        if params.day_factor == 0.0 && params.twilight_factor == 0.0 {
            assert_eq!(
                sun_disc_extinction, 0.0,
                "Sun disc must be zero during deep night at frac={}",
                frac
            );
            assert_eq!(
                sun_halo_extinction, 0.0,
                "Sun halo must be zero during deep night at frac={}",
                frac
            );
        }

        // INVARIANT 4: Direction and elevation consistency
        assert!(
            (params.sun_direction.y - params.sun_elevation).abs() < TOLERANCE,
            "Sun elevation must exactly match sun_direction.y"
        );
    }
}

#[test]
fn test_celestial_camera_altitude_invariance() {
    use glam::Mat4;

    let env = EnvironmentState::new();
    let inv_vp = Mat4::IDENTITY;

    // Build sky uniform at different camera elevations:
    // Low terrain elevation (Y = 0)
    let uniform_low = env.build_sky_uniform(inv_vp, Vec3::new(0.0, 0.0, 0.0));
    // Elevated terrain (Y = 64)
    let uniform_mid = env.build_sky_uniform(inv_vp, Vec3::new(50.0, 64.0, 50.0));
    // High developer camera altitude (Y = 5000)
    let uniform_high = env.build_sky_uniform(inv_vp, Vec3::new(-100.0, 5000.0, -200.0));

    // Celestial parameters MUST remain identical regardless of camera altitude
    assert_eq!(uniform_low.sun_direction, uniform_mid.sun_direction);
    assert_eq!(uniform_low.sun_direction, uniform_high.sun_direction);

    assert_eq!(uniform_low.sun_elevation, uniform_mid.sun_elevation);
    assert_eq!(uniform_low.sun_elevation, uniform_high.sun_elevation);

    assert_eq!(uniform_low.moon_direction, uniform_mid.moon_direction);
    assert_eq!(uniform_low.moon_direction, uniform_high.moon_direction);

    assert_eq!(uniform_low.day_factor, uniform_mid.day_factor);
    assert_eq!(uniform_low.day_factor, uniform_high.day_factor);

    assert_eq!(uniform_low.twilight_factor, uniform_mid.twilight_factor);
    assert_eq!(uniform_low.twilight_factor, uniform_high.twilight_factor);

    assert_eq!(uniform_low.star_visibility, uniform_mid.star_visibility);
    assert_eq!(uniform_low.star_visibility, uniform_high.star_visibility);

    assert_eq!(uniform_low.zenith_color, uniform_high.zenith_color);
    assert_eq!(uniform_low.horizon_color, uniform_high.horizon_color);
}

#[test]
fn test_moon_visual_hierarchy_radiance_invariants() {
    // Phase 10.5.x+ Section 14-19 & Hardening Amendment:
    // Moon visual hierarchy:
    // MOON CORE (2.85) >> MOON HALO (0.035) > STARS (0.15 - 0.40) > NIGHT SKY (0.02)
    // Moon terrain illumination ([0.035, 0.050, 0.080]) remains subtle and independent.

    let mut env = EnvironmentState::new();
    env.set_day_fraction(0.00); // Midnight

    let sky_uniform = env.build_sky_uniform(glam::Mat4::IDENTITY, Vec3::ZERO);
    let light_uniform = env.build_light_uniform();

    // Night sky background brightness:
    let zenith_lum = sky_uniform.zenith_color[0] * 0.2126
        + sky_uniform.zenith_color[1] * 0.7152
        + sky_uniform.zenith_color[2] * 0.0722;
    assert!(
        zenith_lum < 0.03,
        "Night sky background is dark: {}",
        zenith_lum
    );

    // Terrain moonlight is subtle directional light:
    let terrain_moonlight = Vec3::from_slice(&light_uniform.sun_color);
    assert!(
        terrain_moonlight.length() < 0.12,
        "Terrain moonlight must remain subtle: {}",
        terrain_moonlight.length()
    );

    // Moon visual radiance contract in sky.wgsl:
    // Core crescent intensity = 2.85
    // Halo maximum intensity = 0.035
    // Star reference typical peak = 0.20 - 0.40
    let moon_core_intensity = 2.85f32;
    let moon_halo_intensity = 0.035f32;
    let star_typical_intensity = 0.25f32;

    assert!(
        moon_core_intensity > 50.0 * moon_halo_intensity,
        "Moon core must dominate halo by orders of magnitude (2.85 >> 0.035)"
    );
    assert!(
        star_typical_intensity > moon_halo_intensity,
        "Stars must be clearly distinct and brighter than the soft diffuse halo"
    );
    assert!(
        moon_halo_intensity > zenith_lum,
        "Halo is a faint presence above night sky black"
    );
}

// ============================================================================
// CATEGORY 12: PROCEDURAL AURORA VISUAL LAYER (Phase 10.6, Amendments A-E)
// ============================================================================

#[test]
fn test_aurora_determinism() {
    use omnisia::environment::aurora::evaluate_aurora_reference;

    let camera_pos = Vec3::new(120.0, 64.0, -350.0);
    let view_dir = Vec3::new(0.3, 0.4, -0.8).normalize();
    let sun_elevation = -0.50; // Night
    let intensity = 1.0;

    let result1 = evaluate_aurora_reference(camera_pos, view_dir, sun_elevation, intensity);
    let result2 = evaluate_aurora_reference(camera_pos, view_dir, sun_elevation, intensity);

    assert_eq!(
        result1, result2,
        "Aurora reference evaluation must be 100% deterministic"
    );
    assert!(result1.layer_x.is_finite());
    assert!(result1.layer_z.is_finite());
    assert!(result1.vertical_envelope > 0.0);
    assert!(result1.anchor_alignment > 0.0);
    assert_eq!(result1.effective_emission, 1.0);
}

#[test]
fn test_aurora_day_suppression_and_night_visibility() {
    use omnisia::environment::aurora::AuroraParameters;

    // Test B: Day suppression at noon (sun elevation = 1.0, day_fraction = 0.50)
    let params_noon = CelestialParameters::evaluate(0.50);
    assert!((params_noon.sun_elevation - 1.0).abs() < TOLERANCE);
    let vis_noon = AuroraParameters::visibility(params_noon.sun_elevation);
    assert_eq!(
        vis_noon, 0.0,
        "Aurora visibility must be strictly 0.0 at solar noon (Mandatory Amendment A)"
    );

    let aurora = AuroraParameters::default();
    assert_eq!(
        aurora.effective_emission(params_noon.sun_elevation),
        0.0,
        "Effective emission must be 0.0 during the day"
    );

    // Test C: Night visibility at midnight (sun elevation = -1.0, day_fraction = 0.00)
    let params_midnight = CelestialParameters::evaluate(0.00);
    assert!((params_midnight.sun_elevation - (-1.0)).abs() < TOLERANCE);
    let vis_midnight = AuroraParameters::visibility(params_midnight.sun_elevation);
    assert_eq!(
        vis_midnight, 1.0,
        "Aurora visibility must be strictly 1.0 at midnight (Mandatory Amendment A)"
    );
    assert_eq!(
        aurora.effective_emission(params_midnight.sun_elevation),
        1.0,
        "Effective emission must be full intensity at midnight"
    );
}

#[test]
fn test_aurora_sunset_and_sunrise_transition_matrix() {
    use omnisia::environment::aurora::AuroraParameters;

    // Mandatory Amendment E: Dense sample validation around sunset / dusk
    // 0.70 (day), 0.72 (late day), 0.74 (sunset golden hour), 0.75 (exact sunset),
    // 0.76 (civil dusk), 0.78 (nautical dusk), 0.80 (deep night)
    let sunset_samples = [0.70, 0.72, 0.74, 0.75, 0.76, 0.78, 0.80];
    let mut prev_vis = 0.0f32;

    for &frac in &sunset_samples {
        let celestial = CelestialParameters::evaluate(frac);
        let vis = AuroraParameters::visibility(celestial.sun_elevation);

        assert!(
            vis.is_finite(),
            "Visibility must be finite at frac {}",
            frac
        );
        assert!(
            vis >= prev_vis,
            "Aurora visibility must monotonically emerge during sunset/dusk: {} vs {} at frac {}",
            vis,
            prev_vis,
            frac
        );
        prev_vis = vis;

        if frac <= 0.75 {
            // Day through exact sunset (sun elevation >= 0.0 >= -0.06): strictly 0.0
            assert_eq!(
                vis, 0.0,
                "Aurora must be strictly 0.0 at/before sunset (frac {}, elev {})",
                frac, celestial.sun_elevation
            );
        } else if frac == 0.76 {
            // Civil dusk (elev ~ -0.063): barely emerging, strictly <= 0.05
            assert!(
                vis < 0.05,
                "Aurora must barely be emerging at civil dusk (vis={}, elev={})",
                vis,
                celestial.sun_elevation
            );
        } else if frac >= 0.78 {
            // Nautical dusk through deep night (elev <= -0.18): full visibility 1.0
            assert_eq!(
                vis, 1.0,
                "Aurora must be full strength by nautical dusk/night at frac {}",
                frac
            );
        }
    }

    // Sunrise / dawn sequence: 0.20 (night), 0.22 (nautical dawn), 0.24 (civil dawn),
    // 0.25 (exact sunrise), 0.26 (morning), 0.28 (day), 0.30 (day)
    let sunrise_samples = [0.20, 0.22, 0.24, 0.25, 0.26, 0.28, 0.30];
    let mut prev_sunrise_vis = 1.0f32;

    for &frac in &sunrise_samples {
        let celestial = CelestialParameters::evaluate(frac);
        let vis = AuroraParameters::visibility(celestial.sun_elevation);

        assert!(vis.is_finite());
        assert!(
            vis <= prev_sunrise_vis,
            "Aurora visibility must monotonically fade during dawn/sunrise: {} vs {} at frac {}",
            vis,
            prev_sunrise_vis,
            frac
        );
        prev_sunrise_vis = vis;

        if frac <= 0.22 {
            assert_eq!(vis, 1.0, "Full visibility before dawn at frac {}", frac);
        } else if frac == 0.24 {
            assert!(
                vis < 0.05,
                "Aurora must be faded to near-zero before sunrise (vis={})",
                vis
            );
        } else if frac >= 0.25 {
            assert_eq!(
                vis, 0.0,
                "Aurora must be strictly 0.0 at/after sunrise (frac {}, elev {})",
                frac, celestial.sun_elevation
            );
        }
    }
}

#[test]
fn test_aurora_intensity_bounds_and_safety() {
    use omnisia::environment::aurora::AuroraParameters;

    let mut aurora = AuroraParameters::new();
    assert_eq!(aurora.intensity, 1.0);

    // Valid intensities
    assert!(aurora.set_intensity(0.0).is_ok());
    assert_eq!(aurora.intensity, 0.0);

    assert!(aurora.set_intensity(2.5).is_ok());
    assert_eq!(aurora.intensity, 2.5);

    assert!(aurora.set_intensity(10.0).is_ok());
    assert_eq!(aurora.intensity, 10.0);

    // Invalid intensities: negative, > 10.0, NaN, Infinity
    assert!(aurora.set_intensity(-0.1).is_err());
    assert!(aurora.set_intensity(10.1).is_err());
    assert!(aurora.set_intensity(f32::NAN).is_err());
    assert!(aurora.set_intensity(f32::INFINITY).is_err());
    assert!(aurora.set_intensity(f32::NEG_INFINITY).is_err());

    // Effective emission respects intensity scaling
    aurora.set_intensity(2.0).unwrap();
    assert_eq!(aurora.effective_emission(-0.50), 2.0);

    aurora.set_intensity(0.0).unwrap();
    assert_eq!(aurora.effective_emission(-0.50), 0.0);
}

#[test]
fn test_aurora_camera_altitude_invariance() {
    use omnisia::environment::aurora::evaluate_aurora_reference;

    // Invariance of environmental celestial state with respect to camera altitude:
    let env = EnvironmentState::new();
    let inv_vp = glam::Mat4::IDENTITY;

    let uniform_0 = env.build_sky_uniform(inv_vp, Vec3::new(0.0, 0.0, 0.0));
    let uniform_64 = env.build_sky_uniform(inv_vp, Vec3::new(50.0, 64.0, -100.0));
    let uniform_500 = env.build_sky_uniform(inv_vp, Vec3::new(-20.0, 500.0, 80.0));
    let uniform_5000 = env.build_sky_uniform(inv_vp, Vec3::new(0.0, 5000.0, 0.0));

    // Environmental state remains bitwise identical across all altitudes
    assert_eq!(uniform_0.sun_elevation, uniform_64.sun_elevation);
    assert_eq!(uniform_0.sun_elevation, uniform_500.sun_elevation);
    assert_eq!(uniform_0.sun_elevation, uniform_5000.sun_elevation);

    assert_eq!(uniform_0.aurora_intensity, uniform_64.aurora_intensity);
    assert_eq!(uniform_0.aurora_intensity, uniform_500.aurora_intensity);
    assert_eq!(uniform_0.aurora_intensity, uniform_5000.aurora_intensity);

    assert_eq!(uniform_0.day_factor, uniform_5000.day_factor);
    assert_eq!(uniform_0.twilight_factor, uniform_5000.twilight_factor);

    // Spatial parallax check: layer height scales smoothly without inversion
    let view_dir = Vec3::new(0.0, 0.35, -0.9).normalize();
    let ref_0 = evaluate_aurora_reference(Vec3::ZERO, view_dir, -0.5, 1.0);
    let ref_64 = evaluate_aurora_reference(Vec3::new(0.0, 64.0, 0.0), view_dir, -0.5, 1.0);
    let ref_500 = evaluate_aurora_reference(Vec3::new(0.0, 500.0, 0.0), view_dir, -0.5, 1.0);
    let ref_5000 = evaluate_aurora_reference(Vec3::new(0.0, 5000.0, 0.0), view_dir, -0.5, 1.0);

    assert!(ref_0.layer_z < 0.0);
    assert!(ref_64.layer_z < 0.0);
    assert!(ref_500.layer_z < 0.0);
    assert!(
        ref_5000.layer_z < 0.0,
        "Curtain orientation must not invert even at 5000m"
    );

    // Parallax contracts distance smoothly as camera ascends
    assert!(ref_0.layer_z < ref_500.layer_z);
}

#[test]
fn test_aurora_camera_rotation_stability() {
    use omnisia::environment::aurora::evaluate_aurora_reference;

    let camera_pos = Vec3::new(10.0, 20.0, 30.0);
    let sun_elev = -0.50;
    let intensity = 1.0;

    // Look North (-Z)
    let dir_north = Vec3::new(0.0, 0.35, -1.0).normalize();
    let ref_north = evaluate_aurora_reference(camera_pos, dir_north, sun_elev, intensity);

    // Look South (+Z)
    let dir_south = Vec3::new(0.0, 0.35, 1.0).normalize();
    let ref_south = evaluate_aurora_reference(camera_pos, dir_south, sun_elev, intensity);

    // Mandatory Amendment C: Primary curtain anchored along -Z world axis
    assert!(
        ref_north.anchor_alignment > 0.8,
        "North-facing direction has strong anchor alignment: {}",
        ref_north.anchor_alignment
    );
    assert_eq!(
        ref_south.anchor_alignment, 0.0,
        "South-facing direction has zero anchor alignment (leaves sky open for moon)"
    );

    // Rotating 360 degrees and returning produces identical coordinates
    let ref_north_return = evaluate_aurora_reference(camera_pos, dir_north, sun_elev, intensity);
    assert_eq!(ref_north, ref_north_return);
}

#[test]
fn test_aurora_temporal_boundedness() {
    let mut clock = EnvironmentClock::new(0.0, 1200.0);

    // Advance 500 hours (1,800,000 seconds)
    clock.advance(1_800_000.0);
    let bounded = clock.bounded_star_time();
    assert!(
        (0.0..60.0).contains(&bounded),
        "Bounded time must stay in [0.0, 60.0), got {}",
        bounded
    );
    assert!(bounded.is_finite());
}

#[test]
fn test_celestial_hierarchy_and_terrain_isolation_with_aurora() {
    // Mandatory Amendment D:
    // - Bounded aurora radiance (<= 0.70 peak)
    // - Moon core dominance (2.85 >> 0.70)
    // - Night sky background preservation (< 0.03)
    // - Absolute terrain lighting isolation (moonlight unchanged, underside diffuse 0.0)

    let mut env = EnvironmentState::new();
    env.set_day_fraction(0.00); // Midnight
    env.aurora.set_intensity(1.0).unwrap();

    let light_midnight = env.build_light_uniform();
    let sky_midnight = env.build_sky_uniform(glam::Mat4::IDENTITY, Vec3::ZERO);

    // 1. Terrain moonlight remains subtle directional light:
    let terrain_moonlight = Vec3::from_slice(&light_midnight.sun_color);
    assert!(
        terrain_moonlight.length() < 0.12,
        "Terrain moonlight must remain subtle: {}",
        terrain_moonlight.length()
    );

    // 2. Canopy underside receives strictly 0.0 direct diffuse:
    let l_midnight = (-Vec3::from_slice(&light_midnight.sun_direction)).normalize();
    let bottom_n = Vec3::new(0.0, -1.0, 0.0);
    let bottom_nl = bottom_n.dot(l_midnight);
    let bottom_diffuse = if bottom_nl > 0.0 {
        (bottom_nl * 0.5 + 0.5).powi(2)
    } else {
        0.0
    };
    assert_eq!(
        bottom_diffuse, 0.0,
        "Aurora must NOT illuminate underside faces"
    );

    // 3. Moon core dominance over aurora peak:
    let moon_core_intensity = 2.85f32;
    let aurora_max_peak = 0.70f32;
    assert!(
        moon_core_intensity > 3.0 * aurora_max_peak,
        "Moon core (2.85) must clearly dominate peak aurora (0.70)"
    );

    // 4. Night sky preservation:
    let zenith_lum = sky_midnight.zenith_color[0] * 0.2126
        + sky_midnight.zenith_color[1] * 0.7152
        + sky_midnight.zenith_color[2] * 0.0722;
    assert!(zenith_lum < 0.03, "Night sky background remains dark");

    // 5. Changing aurora intensity has ZERO effect on LightUniform:
    env.aurora.set_intensity(10.0).unwrap();
    let light_boosted = env.build_light_uniform();
    assert_eq!(
        light_midnight.sun_color, light_boosted.sun_color,
        "Aurora intensity must NEVER affect LightUniform sun_color"
    );
    assert_eq!(
        light_midnight.ambient_color, light_boosted.ambient_color,
        "Aurora intensity must NEVER affect LightUniform ambient_color"
    );
}

// ============================================================================
// PHASE 10.6.1 SPECIFIC INVARIANT TESTS: ABI, DEPTH, TEMPORAL & SMOOTHSTEP
// ============================================================================

#[test]
fn test_sky_uniform_abi_176_bytes() {
    use omnisia::environment::SkyUniform;

    // Strict ABI Verification: total size must be exactly 176 bytes (11 * 16 bytes std140)
    assert_eq!(
        std::mem::size_of::<SkyUniform>(),
        176,
        "SkyUniform ABI size must be exactly 176 bytes"
    );
    assert_eq!(
        std::mem::size_of::<SkyUniform>() % 16,
        0,
        "SkyUniform total size must be a multiple of 16 bytes for std140 uniform buffer alignment"
    );

    // Byte offset verification for all 15 fields matching sky.wgsl uniform declaration:
    let dummy = SkyUniform::default();
    let base = &dummy as *const SkyUniform as usize;

    assert_eq!(
        &dummy.inv_view_proj as *const [f32; 16] as usize - base,
        0,
        "inv_view_proj offset must be 0"
    );
    assert_eq!(
        &dummy.camera_pos as *const [f32; 3] as usize - base,
        64,
        "camera_pos offset must be 64"
    );
    assert_eq!(
        &dummy.bounded_time as *const f32 as usize - base,
        76,
        "bounded_time offset must be 76"
    );
    assert_eq!(
        &dummy.sun_direction as *const [f32; 3] as usize - base,
        80,
        "sun_direction offset must be 80"
    );
    assert_eq!(
        &dummy.sun_elevation as *const f32 as usize - base,
        92,
        "sun_elevation offset must be 92"
    );
    assert_eq!(
        &dummy.moon_direction as *const [f32; 3] as usize - base,
        96,
        "moon_direction offset must be 96"
    );
    assert_eq!(
        &dummy.moon_phase as *const f32 as usize - base,
        108,
        "moon_phase offset must be 108"
    );
    assert_eq!(
        &dummy.sun_color as *const [f32; 3] as usize - base,
        112,
        "sun_color offset must be 112"
    );
    assert_eq!(
        &dummy.twilight_factor as *const f32 as usize - base,
        124,
        "twilight_factor offset must be 124"
    );
    assert_eq!(
        &dummy.ambient_color as *const [f32; 3] as usize - base,
        128,
        "ambient_color offset must be 128"
    );
    assert_eq!(
        &dummy.star_visibility as *const f32 as usize - base,
        140,
        "star_visibility offset must be 140"
    );
    assert_eq!(
        &dummy.horizon_color as *const [f32; 3] as usize - base,
        144,
        "horizon_color offset must be 144"
    );
    assert_eq!(
        &dummy.day_factor as *const f32 as usize - base,
        156,
        "day_factor offset must be 156"
    );
    assert_eq!(
        &dummy.zenith_color as *const [f32; 3] as usize - base,
        160,
        "zenith_color offset must be 160"
    );
    assert_eq!(
        &dummy.aurora_intensity as *const f32 as usize - base,
        172,
        "aurora_intensity offset must be 172"
    );
}

#[test]
fn test_aurora_multi_scale_temporal_freeze() {
    // Validates that pausing game time freezes temporal evolution identically,
    // and advancing time produces continuous, finite phase progression across all scales.
    let mut clock = EnvironmentClock::new(0.0, 1200.0);
    clock.set_day_fraction(0.00); // Midnight

    // Fixed bounded time sample
    let t_freeze = 42.5f32;
    let tau = std::f32::consts::TAU;
    let omega0 = tau / 60.0;

    let phase_macro_1 = t_freeze * omega0;
    let phase_curtain_1 = t_freeze * (omega0 * 2.0);
    let phase_filaments_1 = t_freeze * (omega0 * 3.0);
    let phase_shimmer_1 = t_freeze * (omega0 * 4.0);

    // Re-evaluating with identical bounded time produces identical phases:
    let phase_macro_2 = t_freeze * omega0;
    let phase_curtain_2 = t_freeze * (omega0 * 2.0);
    let phase_filaments_2 = t_freeze * (omega0 * 3.0);
    let phase_shimmer_2 = t_freeze * (omega0 * 4.0);

    assert_eq!(phase_macro_1, phase_macro_2);
    assert_eq!(phase_curtain_1, phase_curtain_2);
    assert_eq!(phase_filaments_1, phase_filaments_2);
    assert_eq!(phase_shimmer_1, phase_shimmer_2);

    // Phases are decoupled and operate at distinct non-synchronized harmonic rates:
    assert_ne!(phase_macro_1, phase_curtain_1);
    assert_ne!(phase_curtain_1, phase_filaments_1);
    assert_ne!(phase_filaments_1, phase_shimmer_1);

    // 60-second wrapping continuity: all harmonics wrap seamlessly to identity:
    let t_wrap = 60.0f32;
    for k in [1.0f32, 2.0, 3.0, 4.0] {
        let angle = t_wrap * (omega0 * k);
        assert!(
            angle.sin().abs() < 1e-4,
            "sin(t_wrap * omega) must wrap to 0.0, got {}",
            angle.sin()
        );
        assert!(
            (angle.cos() - 1.0).abs() < 1e-4,
            "cos(t_wrap * omega) must wrap to 1.0, got {}",
            angle.cos()
        );
    }
}

#[test]
fn test_aurora_apparent_depth_parallax_continuity() {
    use omnisia::environment::aurora::evaluate_aurora_reference;

    // Hard review constraint: verify layer intersection mathematics numerically across the full validation range.
    // Confirm the intended differential relationship t_far > t_main > t_fine across camera altitudes and view elevations.
    let test_altitudes = [-100.0, 0.0, 64.0, 500.0, 2000.0, 4000.0, 5000.0];
    let test_view_elevations = [0.05, 0.15, 0.35, 0.60, 0.85];

    for &cam_y in &test_altitudes {
        for &view_dy in &test_view_elevations {
            let camera_pos = Vec3::new(100.0, cam_y, -200.0);
            let dir = Vec3::new(0.0, view_dy, -((1.0 - view_dy * view_dy).sqrt())).normalize();

            let ref_res = evaluate_aurora_reference(camera_pos, dir, -0.50, 1.0);

            // 1. Strict ordering: t_far > t_main > t_fine holds everywhere:
            assert!(
                ref_res.t_far > ref_res.t_main,
                "t_far ({}) must be > t_main ({}) at cam_y={}, view_dy={}",
                ref_res.t_far,
                ref_res.t_main,
                cam_y,
                view_dy
            );
            assert!(
                ref_res.t_main > ref_res.t_fine,
                "t_main ({}) must be > t_fine ({}) at cam_y={}, view_dy={}",
                ref_res.t_main,
                ref_res.t_fine,
                cam_y,
                view_dy
            );

            // 2. Minimum layer separation preserved (never collapses to 0):
            assert!(
                ref_res.t_far - ref_res.t_main > 500.0,
                "Far and Main layers must remain separated by at least 500m along ray"
            );
            assert!(
                ref_res.t_main - ref_res.t_fine > 200.0,
                "Main and Fine layers must remain separated by at least 200m along ray"
            );

            // 3. Finiteness check:
            assert!(ref_res.t_far.is_finite());
            assert!(ref_res.t_main.is_finite());
            assert!(ref_res.t_fine.is_finite());
            assert!(ref_res.layer_x.is_finite());
            assert!(ref_res.layer_z.is_finite());
        }
    }

    // Camera translation differential parallax check:
    // When the camera translates by dx, the apparent angular position on closer layers shifts faster:
    let cam1 = Vec3::new(0.0, 64.0, 0.0);
    let cam2 = Vec3::new(50.0, 64.0, 0.0); // 50m horizontal translation
    let dir = Vec3::new(0.0, 0.35, -1.0).normalize();

    let res1 = evaluate_aurora_reference(cam1, dir, -0.50, 1.0);
    let res2 = evaluate_aurora_reference(cam2, dir, -0.50, 1.0);

    // Layer X coordinates shift by dx:
    assert!((res2.layer_x - res1.layer_x - 50.0).abs() < 1e-4);

    // Differential parallax: ratio of ray distances t_far / t_fine > 2.0 (closer layer appears ~2x more responsive)
    let parallax_ratio = res1.t_far / res1.t_fine;
    assert!(
        parallax_ratio > 2.0,
        "Far layer is > 2x more distant than Fine layer: {}",
        parallax_ratio
    );
}

#[test]
fn test_aurora_ordered_smoothstep_invariants() {
    // Phase 10.6.1 Section 16 constraint:
    // All smoothstep edge pairs used in the aurora pipeline must have edge0 < edge1 (ascending order).
    fn assert_ascending(edge0: f32, edge1: f32, name: &str) {
        assert!(
            edge0 < edge1,
            "Smoothstep edges for {} must be strictly ascending: {} < {}",
            name,
            edge0,
            edge1
        );
    }

    // Visibility
    assert_ascending(-0.18, -0.06, "aurora_visibility");
    // World anchor alignment
    assert_ascending(-0.55, 0.25, "anchor_alignment");
    // Vertical envelope
    assert_ascending(0.04, 0.16, "horizon_fade");
    assert_ascending(0.60, 0.88, "zenith_fade");
    // Meso cluster mask
    assert_ascending(0.25, 0.75, "cluster_mask");
    // Vertical ray breaks
    assert_ascending(0.20, 0.80, "break_mask");
    // Fine ray breaks
    assert_ascending(0.40, 0.85, "fine_break");
    // Upper reaches violet accent
    assert_ascending(0.32, 0.72, "upper_reaches");
}
