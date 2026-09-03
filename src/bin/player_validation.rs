use std::time::Instant;

use glam::{IVec3, Vec3};
use omnisia::chunk::Chunk;
use omnisia::material::MaterialId;
use omnisia::player::{PlayerController, PlayerInput};
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::VoxelBlock;

fn main() {
    println!("================================================================================");
    println!("           OMNISIA — PHASE 8B PLAYER CONTROLLER VALIDATION                      ");
    println!("================================================================================");

    let start_all = Instant::now();

    // ------------------------------------------------------------------------
    // STAGE 1: SPAWN / FALL & GROUND LANDING
    // ------------------------------------------------------------------------
    print!("Stage 1: Spawn / Fall & Ground Landing ... ");
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai solid di y = 0..2 (permukaan atas y = 3 * 0.5 = 1.5m)
    for vx in 0..10 {
        for vz in 0..10 {
            for vy in 0..3 {
                chunk.set_voxel(vx, vy, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
    }
    // Udara di atasnya termuat
    store.insert(chunk);

    // Spawn pemain di ketinggian y = 5.0m di atas lantai
    let mut player = PlayerController::new(Vec3::new(2.5, 5.0, 2.5));
    player.config.walk_speed = 5.0;
    player.config.sprint_speed = 9.0;
    assert!(!player.state.grounded);

    // Jalankan simulasi hingga mendarat (maks 60 tick = 2.0s)
    let mut landed = false;
    for _ in 0..60 {
        player.step_simulation(1.0 / 30.0, &store, 0.0);
        if player.state.grounded {
            landed = true;
            break;
        }
    }
    assert!(landed, "Pemain harus mendarat dengan gravitasi!");
    assert!(
        (player.state.position.y - 1.5).abs() < 1e-3,
        "Pemain harus berhenti tepat di permukaan tanah (1.5m)!"
    );
    assert_eq!(player.state.velocity.y, 0.0);
    println!(
        "PASS (landed at y = {:.2}m, grounded = true)",
        player.state.position.y
    );

    // ------------------------------------------------------------------------
    // STAGE 2: KINEMATIC WALK (5.0 m/s)
    // ------------------------------------------------------------------------
    print!("Stage 2: Kinematic Walk Movement (5.0 m/s) ... ");
    player.set_input(PlayerInput::from_raw(
        true, false, false, false, false, false, false,
    ));
    let intent = player.compute_horizontal_intent(0.0);
    let target_speed = player.current_target_speed();
    assert!((target_speed - 5.0).abs() < 1e-4);
    assert!((intent.length() - 1.0).abs() < 1e-4);
    assert!((intent.x - 1.0).abs() < 1e-4);
    println!(
        "PASS (walk_speed = {:.1} m/s, intent = ({:.1}, {:.1}, {:.1}))",
        target_speed, intent.x, intent.y, intent.z
    );

    // ------------------------------------------------------------------------
    // STAGE 3: DIAGONAL NORMALIZATION (W+D == W == 5.0 m/s)
    // ------------------------------------------------------------------------
    print!("Stage 3: Diagonal Normalization (W+D speed == W speed) ... ");
    player.set_input(PlayerInput::from_raw(
        true, false, false, true, false, false, false,
    ));
    let intent_diag = player.compute_horizontal_intent(0.0);
    let diag_speed = intent_diag.length() * player.current_target_speed();
    assert!(
        (diag_speed - 5.0).abs() < 1e-4,
        "Kecepatan diagonal tidak ternormalisasi! speed: {}",
        diag_speed
    );
    println!(
        "PASS (speed = {:.2} m/s, vector length = {:.4})",
        diag_speed,
        intent_diag.length()
    );

    // ------------------------------------------------------------------------
    // STAGE 4: SPRINT MOVEMENT (9.0 m/s)
    // ------------------------------------------------------------------------
    print!("Stage 4: Sprint Movement (9.0 m/s) ... ");
    player.set_input(PlayerInput::from_raw(
        true, false, false, false, true, false, false,
    ));
    player.update_movement_states();
    assert!(player.state.sprinting);
    let sprint_speed = player.current_target_speed();
    assert!((sprint_speed - 9.0).abs() < 1e-4);
    println!("PASS (sprinting = true, speed = {:.1} m/s)", sprint_speed);

    // ------------------------------------------------------------------------
    // STAGE 5: CROUCH & CEILING CLEARANCE CHECK
    // ------------------------------------------------------------------------
    print!("Stage 5: Crouch & Ceiling Clearance Check ... ");
    let mut ceiling_store = ChunkStore::new();
    let mut c_chunk = Chunk::new(IVec3::ZERO);
    // Lantai di y = 0 -> permukaan y = 0.5m
    for vx in 0..10 {
        for vz in 0..10 {
            c_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    // Langit-langit rendah di y = 4 (dasar y = 2.0m). Ruang bebas = 1.5m
    for vx in 0..10 {
        for vz in 0..10 {
            c_chunk.set_voxel(vx, 4, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    ceiling_store.insert(c_chunk);

    let mut crouch_player = PlayerController::new(Vec3::new(2.5, 0.5, 2.5));
    crouch_player.state.grounded = true;

    // 1. Masuk posisi jongkok di bawah langit-langit
    crouch_player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, true, false,
    ));
    crouch_player.update_crouch_state(&ceiling_store);
    assert!(crouch_player.state.crouching);
    assert_eq!(crouch_player.current_capsule().height, 1.2);

    // 2. Lepas tombol jongkok saat masih di bawah langit-langit rendah
    crouch_player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, false,
    ));
    crouch_player.update_crouch_state(&ceiling_store);
    assert!(
        crouch_player.state.crouching,
        "Pemain harus tetap jongkok karena terhalang langit-langit!"
    );
    assert!(
        crouch_player.state.forced_crouch,
        "forced_crouch harus aktif!"
    );

    // 3. Keluar dari bawah langit-langit
    let mut clear_store = ChunkStore::new();
    let mut clear_chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..10 {
        for vz in 0..10 {
            clear_chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    clear_store.insert(clear_chunk);

    crouch_player.update_crouch_state(&clear_store);
    assert!(
        !crouch_player.state.crouching,
        "Pemain harus berhasil berdiri setelah keluar!"
    );
    assert!(!crouch_player.state.forced_crouch);
    assert_eq!(crouch_player.current_capsule().height, 1.8);
    println!("PASS (crouch 1.2m -> forced_crouch blocked -> stand 1.8m success)");

    // ------------------------------------------------------------------------
    // STAGE 6: JUMP CONTROLLER & EDGE TRIGGER (NO REPEATED JUMP)
    // ------------------------------------------------------------------------
    print!("Stage 6: Jump Controller & Single-Consumption Edge Trigger ... ");
    let mut jump_player = PlayerController::new(Vec3::new(2.5, 0.5, 2.5));
    jump_player.state.grounded = true;

    // Frame 1: Tekan space
    jump_player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    assert!(jump_player.try_execute_jump());
    assert_eq!(jump_player.state.velocity.y, 6.0);
    assert!(!jump_player.state.grounded);

    // Frame 2..5: Space terus ditahan saat mendarat
    jump_player.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    jump_player.state.grounded = true; // Simulasikan mendarat
    assert!(
        !jump_player.try_execute_jump(),
        "Menahan space tidak boleh memicu lompatan berulang!"
    );
    println!("PASS (jump 6.0 m/s executed, space held suppressed repeated jumps)");

    // ------------------------------------------------------------------------
    // STAGE 7: HIGH-SPEED ANTI-TUNNELING (50 m/s & 100 m/s)
    // ------------------------------------------------------------------------
    print!("Stage 7: High-Speed Swept Anti-Tunneling (50 m/s & 100 m/s) ... ");
    let mut wall_store = ChunkStore::new();
    let mut wall_chunk = Chunk::new(IVec3::ZERO);
    // Dinding 1 voxel (0.5m) di x = 2.0..2.5 (vx = 4)
    for vy in 0..4 {
        for vz in 0..5 {
            wall_chunk.set_voxel(4, vy, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    wall_store.insert(wall_chunk);

    // Uji 50 m/s
    let mut p_50 = PlayerController::new(Vec3::new(1.0, 0.0, 1.0));
    p_50.state.velocity = Vec3::new(50.0, 0.0, 0.0);
    p_50.step_simulation(1.0 / 30.0, &wall_store, 0.0);
    let front_50 = p_50.state.position.x + p_50.config.capsule_radius;
    assert!(front_50 <= 2.001, "50 m/s tunnel!");

    // Uji 100 m/s (>6 ketebalan voxel dalam 1 tick)
    let mut p_100 = PlayerController::new(Vec3::new(0.5, 0.0, 1.0));
    p_100.state.velocity = Vec3::new(100.0, 0.0, 0.0);
    p_100.step_simulation(1.0 / 30.0, &wall_store, 0.0);
    let front_100 = p_100.state.position.x + p_100.config.capsule_radius;
    assert!(front_100 <= 2.001, "100 m/s tunnel!");
    println!(
        "PASS (stopped at x = {:.3}m, wall = 2.0m, zero tunneling)",
        front_100
    );

    // ------------------------------------------------------------------------
    // STAGE 8: UNLOADED BOUNDARY GUARD (UNKNOWN != AIR)
    // ------------------------------------------------------------------------
    print!("Stage 8: Unloaded Boundary Guard (Unknown != Air) ... ");
    let mut stream_store = ChunkStore::new();
    // Hanya chunk (0, 0, 0) yang dimuat (0..16m). Chunk (1, 0, 0) [16..32m] UNLOADED.
    stream_store.insert(Chunk::new(IVec3::ZERO));

    let mut border_player = PlayerController::new(Vec3::new(15.5, 0.0, 5.0));
    border_player.state.velocity = Vec3::new(15.0, 0.0, 0.0);
    border_player.step_simulation(1.0 / 30.0, &stream_store, 0.0);

    let border_front = border_player.state.position.x + border_player.config.capsule_radius;
    assert!(
        border_front <= 16.001,
        "Pemain menembus batas chunk belum dimuat! front: {}",
        border_front
    );
    assert!(
        border_player.unknown_blocked_total > 0,
        "unknown_blocked_total harus bertambah!"
    );
    println!(
        "PASS (boundary stopped at {:.3}m <= 16.0m, Unknown != Air preserved)",
        border_front
    );

    let total_ms = start_all.elapsed().as_secs_f64() * 1000.0;
    println!("================================================================================");
    println!(
        "ALL 8 PLAYER CONTROLLER VALIDATION STAGES PASSED in {:.2} ms!",
        total_ms
    );
    println!("================================================================================");
}
