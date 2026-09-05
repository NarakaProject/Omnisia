use glam::Vec3;
use omnisia::console::font::{glyph_index_for_char, glyph_uv};
use omnisia::console::parser::{parse_command, MAX_CONSOLE_INPUT_BYTES};
use omnisia::console::{
    create_default_registry, CameraMode, CommandResult, ConsoleState, DeveloperCameraContext,
    DeveloperExecutionContext,
};
use omnisia::environment::time::{EnvironmentClock, MoonPhase};
use omnisia::environment::EnvironmentState;
use omnisia::player::PlayerController;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;

const TOLERANCE: f32 = 1e-4;

// ============================================================================
// 1. SINGLE AUTHORITY FOR ENVIRONMENT TIME CONTROL (Amendments 1, 7, 8)
// ============================================================================

#[test]
fn test_environment_clock_authority_pause_resume() {
    let mut clock = EnvironmentClock::new(0.25, 1200.0);
    assert!(!clock.is_paused());

    // Advance 60 seconds (1 minute)
    clock.advance(60.0);
    let day_frac_before = clock.day_fraction;
    assert!((day_frac_before - (0.25 + 60.0 / 1200.0)).abs() < TOLERANCE);

    // Pause the clock
    clock.pause();
    assert!(clock.is_paused());

    // Advance 120 seconds while paused
    clock.advance(120.0);
    assert_eq!(
        clock.day_fraction, day_frac_before,
        "Day fraction must not advance while paused"
    );

    // Resume the clock
    clock.resume();
    assert!(!clock.is_paused());

    // Advance 60 seconds after resume
    clock.advance(60.0);
    assert!(clock.day_fraction > day_frac_before);
    assert!((clock.day_fraction - (day_frac_before + 60.0 / 1200.0)).abs() < TOLERANCE);
}

#[test]
fn test_environment_clock_delegation_no_divergence() {
    let mut env = EnvironmentState::new();
    assert!(!env.is_paused());

    // Pause through EnvironmentState delegation
    env.pause();
    assert!(env.is_paused());
    assert!(env.clock.is_paused(), "Delegated pause must update clock");

    let initial_sun = env.celestial.sun_direction;
    env.advance(100.0);
    assert_eq!(
        env.celestial.sun_direction, initial_sun,
        "Derived celestial state must not advance while paused"
    );

    // Resume through EnvironmentState delegation
    env.resume();
    assert!(!env.is_paused());
    assert!(!env.clock.is_paused());

    env.advance(100.0);
    assert_ne!(
        env.celestial.sun_direction, initial_sun,
        "Derived celestial state must advance after resume"
    );
}

#[test]
fn test_environment_time_scale_bounds() {
    let mut clock = EnvironmentClock::new(0.0, 1200.0);

    // Valid scales in (0, 1000]
    assert!(clock.set_time_scale(0.5).is_ok());
    assert!((clock.time_scale - 0.5).abs() < TOLERANCE);

    assert!(clock.set_time_scale(10.0).is_ok());
    assert!((clock.time_scale - 10.0).abs() < TOLERANCE);

    assert!(clock.set_time_scale(1000.0).is_ok());
    assert!((clock.time_scale - 1000.0).abs() < TOLERANCE);

    // Invalid scales: <= 0, > 1000, NaN, Infinity
    assert!(clock.set_time_scale(0.0).is_err());
    assert!(clock.set_time_scale(-1.0).is_err());
    assert!(clock.set_time_scale(1000.1).is_err());
    assert!(clock.set_time_scale(f32::NAN).is_err());
    assert!(clock.set_time_scale(f32::INFINITY).is_err());
    assert!(clock.set_time_scale(f32::NEG_INFINITY).is_err());

    // Unchanged after failed attempt
    assert_eq!(clock.time_scale, 1000.0);
}

#[test]
fn test_time_pause_does_not_pause_simulation() {
    // Proves Amendment 8: time pause freezes environment time progression ONLY,
    // while unrelated systems (e.g. kinematic player controller) remain fully capable of updating.
    let mut clock = EnvironmentClock::new(0.5, 1200.0);
    clock.pause();

    let mut world = World::with_seed(WorldSeed(123));
    let mut chunk = omnisia::chunk::Chunk::new(glam::IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            for vy in 0..4 {
                chunk.set_voxel(
                    vx,
                    vy,
                    vz,
                    omnisia::voxel::VoxelBlock::new(omnisia::material::MaterialId::STONE),
                );
            }
        }
    }
    world.store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(4.0, 6.0, 4.0));
    assert!(!player.state.grounded);
    let initial_y = player.state.position.y;

    // Simulate 0.1s step
    clock.advance(0.1);
    for _ in 0..10 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
    }

    // Environment did not advance
    assert_eq!(clock.day_fraction, 0.5);

    // Player simulation DID advance (gravity pulled player down)
    assert!(
        player.state.position.y < initial_y,
        "Player simulation must continue when environment time is paused"
    );
}

// ============================================================================
// 2. DO NOT DUPLICATE CAMERA & PLAYER READ-ONLY (Amendments 2, 3, 9)
// ============================================================================

#[test]
fn test_developer_camera_decoupled_from_player() {
    let player_spawn = Vec3::new(10.0, 20.0, 30.0);
    let dev_spawn = Vec3::new(10.0, 21.6, 30.0);
    let mut cam_ctx = DeveloperCameraContext::new(player_spawn, dev_spawn);

    assert_eq!(cam_ctx.mode(), CameraMode::Player);
    assert_eq!(cam_ctx.player_position(), player_spawn);

    // Switch to developer mode
    cam_ctx.set_mode(CameraMode::Developer);
    assert_eq!(cam_ctx.mode(), CameraMode::Developer);

    // Move developer camera far away
    cam_ctx.set_dev_position(Vec3::new(500.0, 100.0, -250.0));
    assert_eq!(cam_ctx.dev_camera.position, Vec3::new(500.0, 100.0, -250.0));

    // Player position remains completely untouched
    assert_eq!(
        cam_ctx.player_position(),
        player_spawn,
        "Developer camera mutation must NEVER alter player position"
    );

    // Switch back to player mode
    cam_ctx.set_mode(CameraMode::Player);
    assert_eq!(cam_ctx.mode(), CameraMode::Player);
    assert_eq!(cam_ctx.player_position(), player_spawn);
}

#[test]
fn test_developer_camera_speed_bounds() {
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::ZERO, Vec3::ZERO);

    assert!(cam_ctx.set_dev_speed(50.0).is_ok());
    assert_eq!(cam_ctx.dev_camera.speed, 50.0);

    // Rejection of invalid speeds
    assert!(cam_ctx.set_dev_speed(0.0).is_err());
    assert!(cam_ctx.set_dev_speed(-10.0).is_err());
    assert!(cam_ctx.set_dev_speed(f32::NAN).is_err());
    assert!(cam_ctx.set_dev_speed(f32::INFINITY).is_err());
    assert!(cam_ctx.set_dev_speed(5000.0).is_err()); // exceeds max 2000 m/s
}

// ============================================================================
// 3. PARSER, WHITESPACE, QUOTING & UTF-8 SAFETY (Amendments 4, 5, 13)
// ============================================================================

#[test]
fn test_parser_normal_and_quoted_arguments() {
    let parsed = parse_command("time set 0.5").unwrap().unwrap();
    assert_eq!(parsed.command, "time");
    assert_eq!(parsed.args, vec!["set", "0.5"]);

    let quoted = parse_command(r#"camera position "100.5" 200.0 "300.25""#)
        .unwrap()
        .unwrap();
    assert_eq!(quoted.command, "camera");
    assert_eq!(quoted.args, vec!["position", "100.5", "200.0", "300.25"]);

    // Collapsing multiple whitespace
    let ws = parse_command("   env    status   ").unwrap().unwrap();
    assert_eq!(ws.command, "env");
    assert_eq!(ws.args, vec!["status"]);
}

#[test]
fn test_parser_unclosed_quote_error() {
    let err = parse_command(r#"some_command "unclosed string"#);
    assert!(err.is_err());
    assert!(err
        .unwrap_err()
        .to_string()
        .contains("unmatched quotation mark"));
}

#[test]
fn test_parser_utf8_and_unicode_safety() {
    // Proves Amendment 5: input with multi-byte UTF-8, accents, or emojis must never panic
    let unicode_input = "help 測試 こんにちは 🚀 🌟";
    let parsed = parse_command(unicode_input).unwrap().unwrap();
    assert_eq!(parsed.command, "help");
    assert_eq!(parsed.args, vec!["測試", "こんにちは", "🚀", "🌟"]);
}

#[test]
fn test_parser_hard_maximum_input_length() {
    // Proves Amendment 13: input exceeding MAX_CONSOLE_INPUT_BYTES (4096) is rejected
    let huge_input = "a".repeat(MAX_CONSOLE_INPUT_BYTES + 10);
    let res = parse_command(&huge_input);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("exceeds maximum"));
}

#[test]
fn test_font_non_ascii_glyph_fallback() {
    // Proves Amendment 5: ASCII glyphs map to 0..95; non-ASCII maps safely to '?' (31)
    let space_idx = glyph_index_for_char(' ');
    assert_eq!(space_idx, 0);

    let tilde_idx = glyph_index_for_char('~');
    assert_eq!(tilde_idx, 94);

    let q_idx = glyph_index_for_char('?');
    assert_eq!(q_idx, (b'?' - 32) as usize);

    // Non-ASCII codepoint falls back to '?'
    let unicode_char = '🚀';
    assert_eq!(glyph_index_for_char(unicode_char), q_idx);

    let uv = glyph_uv('🚀');
    let uv_q = glyph_uv('?');
    assert_eq!(uv, uv_q);
    assert!(uv[0] >= 0.0 && uv[2] <= 1.0);
    assert!(uv[1] >= 0.0 && uv[3] <= 1.0);
}

// ============================================================================
// 4. COMMAND REGISTRY, AUTO-HELP & CAMERA SEMANTICS (Amendments 9, 10, 14)
// ============================================================================

#[test]
fn test_help_auto_generated_from_registry() {
    let registry = create_default_registry();
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::ZERO, Vec3::ZERO);
    let mut env = EnvironmentState::new();

    let mut ctx = DeveloperExecutionContext {
        camera: &mut cam_ctx,
        environment: &mut env,
        resident_chunks: 100,
        fps: 60.0,
        frame_time_ms: 16.6,
    };

    // Global help
    let parsed_help = parse_command("help").unwrap().unwrap();
    match registry.dispatch(&parsed_help, &mut ctx) {
        CommandResult::Success(text) => {
            assert!(text.contains("time"));
            assert!(text.contains("camera"));
            assert!(text.contains("env"));
            assert!(text.contains("clear"));
            assert!(text.contains("status"));
        }
        _ => panic!("Expected Success for help"),
    }

    // Specific command help
    let parsed_help_time = parse_command("help time").unwrap().unwrap();
    match registry.dispatch(&parsed_help_time, &mut ctx) {
        CommandResult::Success(text) => {
            assert!(text.contains("time get"));
            assert!(text.contains("time pause"));
            assert!(text.contains("time resume"));
            assert!(text.contains("time scale"));
            assert!(text.contains("time set"));
        }
        _ => panic!("Expected Success for help time"),
    }
}

#[test]
fn test_camera_commands_dev_mode_enforcement() {
    let registry = create_default_registry();
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::ZERO, Vec3::ZERO);
    let mut env = EnvironmentState::new();

    let mut ctx = DeveloperExecutionContext {
        camera: &mut cam_ctx,
        environment: &mut env,
        resident_chunks: 100,
        fps: 60.0,
        frame_time_ms: 16.6,
    };

    // Mode is currently Player: attempting to move camera should error (Amendment 9)
    let move_cmd = parse_command("camera position 100 50 -20")
        .unwrap()
        .unwrap();
    match registry.dispatch(&move_cmd, &mut ctx) {
        CommandResult::Error(err) => {
            assert!(err.contains("Developer camera is not active"));
        }
        _ => panic!("Expected error when moving developer camera while in player mode"),
    }

    // Switch to free camera
    let free_cmd = parse_command("camera free").unwrap().unwrap();
    assert!(matches!(
        registry.dispatch(&free_cmd, &mut ctx),
        CommandResult::Success(_)
    ));
    assert_eq!(ctx.camera.mode(), CameraMode::Developer);

    // Now moving developer camera succeeds
    match registry.dispatch(&move_cmd, &mut ctx) {
        CommandResult::Success(msg) => {
            assert!(msg.contains("teleported to"));
            assert_eq!(
                ctx.camera.dev_camera.position,
                Vec3::new(100.0, 50.0, -20.0)
            );
        }
        _ => panic!("Expected success when moving developer camera in developer mode"),
    }
}

#[test]
fn test_time_commands_end_to_end() {
    let registry = create_default_registry();
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::ZERO, Vec3::ZERO);
    let mut env = EnvironmentState::new();

    let mut ctx = DeveloperExecutionContext {
        camera: &mut cam_ctx,
        environment: &mut env,
        resident_chunks: 100,
        fps: 60.0,
        frame_time_ms: 16.6,
    };

    // Set time to noon (0.50)
    let set_noon = parse_command("time set 0.5").unwrap().unwrap();
    match registry.dispatch(&set_noon, &mut ctx) {
        CommandResult::Success(msg) => {
            assert!(msg.contains("12:00"));
            assert!((ctx.environment.clock.day_fraction - 0.5).abs() < TOLERANCE);
        }
        _ => panic!("Expected success for time set 0.5"),
    }

    // Pause time
    let pause_cmd = parse_command("time pause").unwrap().unwrap();
    assert!(matches!(
        registry.dispatch(&pause_cmd, &mut ctx),
        CommandResult::Success(_)
    ));
    assert!(ctx.environment.is_paused());

    // Scale time
    let scale_cmd = parse_command("time scale 10").unwrap().unwrap();
    assert!(matches!(
        registry.dispatch(&scale_cmd, &mut ctx),
        CommandResult::Success(_)
    ));
    assert_eq!(ctx.environment.clock.time_scale, 10.0);

    // Resume time
    let resume_cmd = parse_command("time resume").unwrap().unwrap();
    assert!(matches!(
        registry.dispatch(&resume_cmd, &mut ctx),
        CommandResult::Success(_)
    ));
    assert!(!ctx.environment.is_paused());
}

#[test]
fn test_env_commands_and_moon_control() {
    let registry = create_default_registry();
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::ZERO, Vec3::ZERO);
    let mut env = EnvironmentState::new();

    let mut ctx = DeveloperExecutionContext {
        camera: &mut cam_ctx,
        environment: &mut env,
        resident_chunks: 100,
        fps: 60.0,
        frame_time_ms: 16.6,
    };

    // Status
    let env_status = parse_command("env status").unwrap().unwrap();
    assert!(matches!(
        registry.dispatch(&env_status, &mut ctx),
        CommandResult::Success(_)
    ));

    // Moon phase modification
    let moon_cmd = parse_command("env moon set full").unwrap().unwrap();
    match registry.dispatch(&moon_cmd, &mut ctx) {
        CommandResult::Success(msg) => {
            assert!(msg.contains("Full Moon"));
            assert_eq!(
                ctx.environment.clock.named_moon_phase(),
                MoonPhase::FullMoon
            );
        }
        _ => panic!("Expected success for env moon set full"),
    }
}

#[test]
fn test_clear_command_result_decoupling() {
    // Proves Amendment 10: clear returns CommandResult::Clear without coupling engine to console
    let registry = create_default_registry();
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::ZERO, Vec3::ZERO);
    let mut env = EnvironmentState::new();

    let mut ctx = DeveloperExecutionContext {
        camera: &mut cam_ctx,
        environment: &mut env,
        resident_chunks: 100,
        fps: 60.0,
        frame_time_ms: 16.6,
    };

    let clear_cmd = parse_command("clear").unwrap().unwrap();
    assert_eq!(
        registry.dispatch(&clear_cmd, &mut ctx),
        CommandResult::Clear
    );

    // Submit through ConsoleState clears output_lines
    let mut state = ConsoleState::new();
    state.print("Line 1");
    state.print("Line 2");
    assert_eq!(state.output_lines.len(), 3); // initial message + 2 lines

    state.input_buffer = "clear".to_string();
    state.submit(&registry, &mut ctx);
    assert_eq!(state.output_lines.len(), 0);
}

#[test]
fn test_console_state_unicode_input_navigation() {
    let registry = create_default_registry();
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::ZERO, Vec3::ZERO);
    let mut env = EnvironmentState::new();

    let mut ctx = DeveloperExecutionContext {
        camera: &mut cam_ctx,
        environment: &mut env,
        resident_chunks: 100,
        fps: 60.0,
        frame_time_ms: 16.6,
    };

    let mut state = ConsoleState::new();
    state.open();

    // Insert Unicode characters (UTF-8 multi-byte)
    state.insert_char('日');
    state.insert_char('本');
    state.insert_char('語');
    assert_eq!(state.input_buffer, "日本語");
    assert_eq!(state.cursor_pos, 3);

    // Backspace from end deletes '語'
    state.backspace();
    assert_eq!(state.input_buffer, "日本");
    assert_eq!(state.cursor_pos, 2);

    // Cursor navigation
    state.cursor_left();
    assert_eq!(state.cursor_pos, 1);

    // Submit input
    state.submit(&registry, &mut ctx);
    assert_eq!(state.history.len(), 1);
    assert_eq!(state.history[0], "日本");
    assert!(state.input_buffer.is_empty());
}

#[test]
fn test_camera_relative_mouse_look_and_pitch_clamping() {
    // Phase 10.5.x+ Objective A: Player FPS camera and Developer camera both support
    // direct relative mouse-look without click-and-drag, with strict pitch clamping.
    use omnisia::camera::Camera;

    let mut player_camera = Camera::new(Vec3::ZERO, -90.0, 0.0);
    assert!(!player_camera.is_mouse_dragging);

    let initial_yaw = player_camera.yaw_deg;
    let initial_pitch = player_camera.pitch_deg;

    // Moving mouse on player camera without dragging MUST update yaw/pitch directly
    player_camera.handle_mouse_motion(20.0, 15.0);
    assert_ne!(player_camera.yaw_deg, initial_yaw);
    assert_ne!(player_camera.pitch_deg, initial_pitch);
    assert!((player_camera.yaw_deg - (initial_yaw + 20.0 * 0.15)).abs() < TOLERANCE);
    assert!((player_camera.pitch_deg - (initial_pitch - 15.0 * 0.15)).abs() < TOLERANCE);

    // Extreme pitch movements must be clamped strictly to [-89.0, 89.0]
    player_camera.handle_mouse_motion(0.0, 10000.0); // Look far down
    assert_eq!(player_camera.pitch_deg, -89.0);

    player_camera.handle_mouse_motion(0.0, -20000.0); // Look far up
    assert_eq!(player_camera.pitch_deg, 89.0);

    // Developer camera context
    let mut cam_ctx = DeveloperCameraContext::new(Vec3::new(10.0, 20.0, 30.0), Vec3::ZERO);
    assert_eq!(cam_ctx.mode(), CameraMode::Player);
    assert!(!cam_ctx.dev_camera.is_mouse_dragging);

    let dev_initial_yaw = cam_ctx.dev_camera.yaw_deg;
    let dev_initial_pitch = cam_ctx.dev_camera.pitch_deg;

    // Moving mouse on Developer camera with NO mouse dragging MUST update yaw/pitch immediately
    cam_ctx.dev_camera.handle_mouse_motion(30.0, -20.0);
    assert_ne!(cam_ctx.dev_camera.yaw_deg, dev_initial_yaw);
    assert_ne!(cam_ctx.dev_camera.pitch_deg, dev_initial_pitch);

    // Developer camera pose synchronization test: switching modes preserves view orientation
    let active_player_cam = Camera::new(Vec3::new(100.0, 64.0, 200.0), -45.0, 15.0);
    cam_ctx.sync_dev_camera_pose(&active_player_cam);
    assert_eq!(cam_ctx.dev_camera.position, active_player_cam.position);
    assert_eq!(cam_ctx.dev_camera.yaw_deg, -45.0);
    assert_eq!(cam_ctx.dev_camera.pitch_deg, 15.0);
}
