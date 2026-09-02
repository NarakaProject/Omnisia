use glam::Vec3;

use omnisia::player::collider::Capsule;
use omnisia::player::config::PlayerConfig;
use omnisia::player::state::PlayerState;

// ============================================================================
// 8B.1 PLAYER CAPSULE / COLLIDER & DATA MODEL
// ============================================================================

#[test]
fn test_player_config_defaults_and_validation() {
    let config = PlayerConfig::default();

    assert_eq!(config.standing_height, 1.8);
    assert_eq!(config.crouching_height, 1.2);
    assert_eq!(config.capsule_radius, 0.30);
    assert_eq!(config.walk_speed, 5.0);
    assert_eq!(config.sprint_speed, 9.0);
    assert_eq!(config.crouch_speed, 2.5);
    assert_eq!(config.jump_velocity, 6.0);
    assert_eq!(config.gravity, -9.81);
    assert_eq!(config.ground_contact_epsilon, 0.05);
    assert_eq!(config.step_height, 0.55);
    assert_eq!(config.fixed_timestep, 1.0 / 30.0);
    assert_eq!(config.max_substeps_per_frame, 5);
    assert_eq!(config.max_dt_clamp, 0.25);
    assert_eq!(config.eye_height_standing, 1.62);
    assert_eq!(config.eye_height_crouching, 1.08);

    // Verifikasi panjang segmen
    // Standing: 1.8 - 2 * 0.3 = 1.2m
    assert!((config.standing_segment_length() - 1.2).abs() < 1e-5);
    // Crouching: 1.2 - 2 * 0.3 = 0.6m
    assert!((config.crouching_segment_length() - 0.6).abs() < 1e-5);
}

#[test]
fn test_capsule_dimensions_and_feet_reference() {
    // Posisi dasar kaki (y_feet) di (10.0, 2.0, 10.0)
    let feet_pos = Vec3::new(10.0, 2.0, 10.0);
    let capsule = Capsule::new(feet_pos, 0.30, 1.8);

    assert_eq!(capsule.base, feet_pos);
    assert_eq!(capsule.radius, 0.30);
    assert_eq!(capsule.height, 1.8);

    // Belahan bola bawah: feet + radius
    assert_eq!(capsule.lower_sphere_center(), Vec3::new(10.0, 2.30, 10.0));

    // Belahan bola atas: feet + height - radius
    assert_eq!(capsule.upper_sphere_center(), Vec3::new(10.0, 3.50, 10.0));

    // Puncak kepala: feet + height
    assert_eq!(capsule.top(), Vec3::new(10.0, 3.80, 10.0));

    // Bounding box AABB terluar
    let (aabb_min, aabb_max) = capsule.aabb();
    assert_eq!(aabb_min, Vec3::new(9.70, 2.0, 9.70));
    assert_eq!(aabb_max, Vec3::new(10.30, 3.80, 10.30));
}

#[test]
fn test_capsule_narrow_phase_vs_aabb_true_geometric_intersection() {
    // Kapsul berdiri dengan dasar kaki di (0.0, 0.0, 0.0), radius 0.3, tinggi 1.8
    let capsule = Capsule::new(Vec3::ZERO, 0.30, 1.8);

    // 1. Kasus: Kotak voxel berada di (1.0..1.5, 0.0..0.5, 0.0..0.5)
    // Jarak horizontal dx = 1.0 - 0.0 = 1.0 > radius (0.3)
    let far_box_min = Vec3::new(1.0, 0.0, 0.0);
    let far_box_max = Vec3::new(1.5, 0.5, 0.5);
    assert!(!capsule.intersects_aabb(far_box_min, far_box_max));

    // 2. Kasus: Kotak voxel tepat bersinggungan di sumbu X pada x=0.3
    // Jarak dx = 0.3 - 0.0 = 0.3 <= radius -> HARUS INTERSECT!
    let touching_box_min = Vec3::new(0.30, 0.0, 0.0);
    let touching_box_max = Vec3::new(0.80, 0.5, 0.5);
    assert!(capsule.intersects_aabb(touching_box_min, touching_box_max));

    // 3. Kasus Sudut Diagonal:
    // Box di (0.25, 0.0, 0.25) -> dx = 0.25, dz = 0.25
    // dist^2 = 0.25^2 + 0.25^2 = 0.0625 + 0.0625 = 0.125
    // radius^2 = 0.30^2 = 0.09
    // Karena 0.125 > 0.09, kapsul lingkaran TIDAK boleh bertabrakan secara sembarangan!
    // Sedangkan AABB naif kapsul (max_x=0.3, max_z=0.3) akan SALAH MENGKLAIM TABRAKAN jika bukan geometric narrow phase!
    let corner_box_min = Vec3::new(0.25, 0.0, 0.25);
    let corner_box_max = Vec3::new(0.75, 0.5, 0.75);
    assert!(
        !capsule.intersects_aabb(corner_box_min, corner_box_max),
        "Narrow-phase harus menghitung irisan kapsul sejati, bukan overlap AABB kotak!"
    );

    // 4. Kasus di atas kepala:
    // Box di y=2.0..2.5 (kapsul puncak y=1.8).
    let ceiling_box_min = Vec3::new(-0.2, 2.0, -0.2);
    let ceiling_box_max = Vec3::new(0.2, 2.5, 0.2);
    assert!(!capsule.intersects_aabb(ceiling_box_min, ceiling_box_max));

    // Box menyentuh puncak kepala di y=1.7..2.2
    let hit_ceiling_box_min = Vec3::new(-0.2, 1.7, -0.2);
    let hit_ceiling_box_max = Vec3::new(0.2, 2.2, 0.2);
    assert!(capsule.intersects_aabb(hit_ceiling_box_min, hit_ceiling_box_max));
}

#[test]
fn test_capsule_crouch_height_transition_feet_fixed() {
    let feet_pos = Vec3::new(5.0, 1.0, 5.0);

    // Kapsul berdiri
    let standing_capsule = Capsule::new(feet_pos, 0.30, 1.8);
    // Kapsul jongkok
    let crouching_capsule = Capsule::new(feet_pos, 0.30, 1.2);

    // INVARIAN KANONIKAL: Posisi kaki telapak tidak berubah (zero foot teleportation)!
    assert_eq!(standing_capsule.base, crouching_capsule.base);
    assert_eq!(
        standing_capsule.lower_sphere_center(),
        crouching_capsule.lower_sphere_center()
    );

    // Tinggi dan bola atas turun
    assert!((standing_capsule.top().y - 2.8).abs() < 1e-5);
    assert!((crouching_capsule.top().y - 2.2).abs() < 1e-5);

    assert!((standing_capsule.upper_sphere_center().y - 2.5).abs() < 1e-5);
    assert!((crouching_capsule.upper_sphere_center().y - 1.9).abs() < 1e-5);
}

#[test]
fn test_player_state_defaults() {
    let state = PlayerState::default();
    assert_eq!(state.position, Vec3::ZERO);
    assert_eq!(state.velocity, Vec3::ZERO);
    assert!(!state.grounded);
    assert_eq!(state.ground_normal, Vec3::Y);
    assert_eq!(state.ground_distance, 0.0);
    assert!(!state.crouching);
    assert!(!state.sprinting);
    assert!(!state.forced_crouch);
    assert!(!state.jump_requested);
    assert_eq!(state.ticks_stationary, 0);
    assert_eq!(state.speed(), 0.0);
    assert_eq!(state.horizontal_speed(), 0.0);
}

// ============================================================================
// 8B.2 GROUND DETECTION & SURFACE SUPPORT
// ============================================================================

#[test]
fn test_ground_detection_standing_on_solid_ground() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::check_ground_support;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Letakkan balok solid di (10, 2, 10) -> permukaan atas y = (2 + 1) * 0.5 = 1.5m
    chunk.set_voxel(10, 2, 10, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain berdiri tepat di permukaan y = 1.5m
    let feet_pos = Vec3::new(5.25, 1.50, 5.25);
    let result = check_ground_support(feet_pos, 0.30, 0.05, &store);

    assert!(
        result.grounded,
        "Pemain harus terdeteksi grounded di atas balok solid!"
    );
    assert_eq!(result.ground_normal, Vec3::Y);
    assert!((result.ground_distance - 0.0).abs() < 1e-5);
    assert_eq!(result.support_voxel, Some(IVec3::new(10, 2, 10)));
    assert_eq!(result.ground_y_surface, Some(1.5));

    // Berdiri sedikit di atas lantai (toleransi 0.03m <= epsilon 0.05m)
    let feet_pos_slight_gap = Vec3::new(5.25, 1.53, 5.25);
    let result_gap = check_ground_support(feet_pos_slight_gap, 0.30, 0.05, &store);
    assert!(result_gap.grounded);
    assert!((result_gap.ground_distance - 0.03).abs() < 1e-4);
}

#[test]
fn test_ground_detection_airborne_even_if_velocity_is_zero() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::check_ground_support;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    // Lantai di y = 0 -> permukaan y = 0.5m
    chunk.set_voxel(10, 0, 10, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain berada di udara y = 5.0m (4.5m di atas tanah)
    // Section 6: Grounded BUKAN velocity.y == 0!
    let feet_airborne = Vec3::new(5.25, 5.0, 5.25);
    let result = check_ground_support(feet_airborne, 0.30, 0.05, &store);

    assert!(
        !result.grounded,
        "Pemain di udara bebas tidak boleh dianggap grounded meskipun kecepatan vertikal nol!"
    );
    assert_eq!(result.support_voxel, None);
}

#[test]
fn test_ground_detection_chunk_boundary_and_negative_coordinates() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::check_ground_support;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    // Chunk negatif (-1, 0, -1)
    let mut neg_chunk = Chunk::new(IVec3::new(-1, 0, -1));
    // Voxel lokal 31, 2, 31 => koordinat dunia: -1 * 32 + 31 = -1
    // Permukaan atas y = (2 + 1) * 0.5 = 1.5m
    neg_chunk.set_voxel(31, 2, 31, VoxelBlock::new(MaterialId::STONE));
    store.insert(neg_chunk);

    // Posisi telapak kaki di koordinat negatif: x = -0.25m (voxel x = -1), z = -0.25m, y = 1.50m
    let feet_neg = Vec3::new(-0.25, 1.50, -0.25);
    let result = check_ground_support(feet_neg, 0.30, 0.05, &store);

    assert!(
        result.grounded,
        "Ground detection harus bekerja presisi di koordinat negatif!"
    );
    assert_eq!(result.support_voxel, Some(IVec3::new(-1, 2, -1)));
    assert_eq!(result.ground_y_surface, Some(1.5));
}

#[test]
fn test_ground_detection_unloaded_chunk_is_not_falsely_grounded() {
    use omnisia::player::check_ground_support;
    use omnisia::streaming::store::ChunkStore;

    // ChunkStore kosong (semua chunk unloaded / unknown)
    let store = ChunkStore::new();
    let feet_pos = Vec3::new(10.0, 2.0, 10.0);
    let result = check_ground_support(feet_pos, 0.30, 0.05, &store);

    // Unknown chunk tidak boleh dipalsukan menjadi tumpuan solid
    assert!(
        !result.grounded,
        "Chunk yang belum dimuat tidak boleh menghasilkan status grounded!"
    );
}

// ============================================================================
// 8B.3 KINEMATIC WALK MOVEMENT & HORIZONTAL PLANAR PROJECTION
// ============================================================================

#[test]
fn test_walk_movement_camera_relative_forward_backward_strafe() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::ZERO);

    // 1. Arah hadap kamera: yaw = 0 deg (menghadap ke +X)
    // Input W (maju) -> arah hadap harus (+1, 0, 0)
    controller.set_input(PlayerInput::from_raw(
        true, false, false, false, false, false, false,
    ));
    let intent_w = controller.compute_horizontal_intent(0.0);
    assert!((intent_w.x - 1.0).abs() < 1e-4);
    assert!(intent_w.y.abs() < 1e-5);
    assert!(intent_w.z.abs() < 1e-4);

    // Input S (mundur) -> arah hadap harus (-1, 0, 0)
    controller.set_input(PlayerInput::from_raw(
        false, true, false, false, false, false, false,
    ));
    let intent_s = controller.compute_horizontal_intent(0.0);
    assert!((intent_s.x - (-1.0)).abs() < 1e-4);

    // Input D (strafe kanan) -> arah hadap harus (+Z)
    controller.set_input(PlayerInput::from_raw(
        false, false, false, true, false, false, false,
    ));
    let intent_d = controller.compute_horizontal_intent(0.0);
    assert!(intent_d.x.abs() < 1e-4);
    assert!((intent_d.z - 1.0).abs() < 1e-4);

    // Input A (strafe kiri) -> arah hadap harus (-Z)
    controller.set_input(PlayerInput::from_raw(
        false, false, true, false, false, false, false,
    ));
    let intent_a = controller.compute_horizontal_intent(0.0);
    assert!((intent_a.z - (-1.0)).abs() < 1e-4);

    // 2. Arah hadap kamera: yaw = 90 deg (menghadap ke +Z)
    // Input W (maju) -> harus (+Z)
    controller.set_input(PlayerInput::from_raw(
        true, false, false, false, false, false, false,
    ));
    let intent_w_90 = controller.compute_horizontal_intent(90.0);
    assert!(intent_w_90.x.abs() < 1e-4);
    assert!((intent_w_90.z - 1.0).abs() < 1e-4);
}

#[test]
fn test_diagonal_movement_normalized_not_sqrt2() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::ZERO);

    // Tekan W saja
    controller.set_input(PlayerInput::from_raw(
        true, false, false, false, false, false, false,
    ));
    let intent_w = controller.compute_horizontal_intent(0.0);
    let speed_w = intent_w.length() * controller.current_target_speed();
    assert!((speed_w - 5.0).abs() < 1e-4);

    // Tekan W + D (diagonal)
    controller.set_input(PlayerInput::from_raw(
        true, false, false, true, false, false, false,
    ));
    let intent_diagonal = controller.compute_horizontal_intent(0.0);
    let speed_diagonal = intent_diagonal.length() * controller.current_target_speed();

    // INVARIAN KANONIKAL: Kecepatan diagonal TIDAK BOLEH melebihi walk_speed (tidak ada speed exploit 5 * sqrt(2))!
    assert!(
        (speed_diagonal - 5.0).abs() < 1e-4,
        "Kecepatan diagonal harus dinormalisasi menjadi tepat 5.0 m/s, terukur: {}",
        speed_diagonal
    );
    assert!(
        (intent_diagonal.length() - 1.0).abs() < 1e-4,
        "Panjang vektor niat gerak diagonal harus tepat 1.0!"
    );
}

#[test]
fn test_camera_pitch_does_not_cause_vertical_movement() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::ZERO);
    controller.set_input(PlayerInput::from_raw(
        true, false, false, false, false, false, false,
    ));

    // Evaluasi niat gerak horizontal
    let intent = controller.compute_horizontal_intent(45.0);

    // Sumbu vertikal Y dari niat gerak horizontal HARUS NOL MUTLAK
    assert_eq!(
        intent.y, 0.0,
        "Niat gerak horizontal pemain tidak boleh mengandung komponen vertikal!"
    );
}

// ============================================================================
// 8B.4 SPRINT MOVEMENT & STATE PRECEDENCE (CROUCHING > SPRINTING)
// ============================================================================

#[test]
fn test_sprint_speed_and_activation() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::ZERO);

    // 1. Shift + W -> Sprint aktif (9.0 m/s)
    controller.set_input(PlayerInput::from_raw(
        true, false, false, false, true, false, false,
    ));
    controller.update_movement_states();
    assert!(
        controller.state.sprinting,
        "Shift + W harus mengaktifkan sprint!"
    );
    assert_eq!(controller.current_target_speed(), 9.0);

    // 2. Shift saja tanpa WASD -> TIDAK boleh sprint (Section 13: Shift alone does not move)
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, true, false, false,
    ));
    controller.update_movement_states();
    assert!(
        !controller.state.sprinting,
        "Shift tanpa input gerak tidak boleh mengaktifkan sprint!"
    );
    assert_eq!(controller.current_target_speed(), 5.0);
    assert_eq!(controller.compute_horizontal_intent(0.0), Vec3::ZERO);
}

#[test]
fn test_crouching_suppresses_sprint_precedence() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::ZERO);

    // Input menekan W + Shift (sprint) + C (crouch)
    controller.set_input(PlayerInput::from_raw(
        true, false, false, false, true, true, false,
    ));
    // Aktifkan status jongkok
    controller.state.crouching = true;
    controller.update_movement_states();

    // INVARIAN KANONIKAL: Crouching > Sprinting!
    assert!(
        !controller.state.sprinting,
        "Status jongkok harus menonaktifkan status sprint!"
    );
    assert_eq!(
        controller.current_target_speed(),
        2.5,
        "Kecepatan target saat jongkok harus crouch_speed (2.5 m/s), bukan sprint!"
    );
}

#[test]
fn test_sprint_does_not_alter_capsule_dimensions() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::new(10.0, 5.0, 10.0));
    controller.set_input(PlayerInput::from_raw(
        true, false, false, false, true, false, false,
    ));
    controller.update_movement_states();

    let capsule = controller.current_capsule();
    // Sprint tidak boleh mengubah collider
    assert_eq!(capsule.height, 1.8);
    assert_eq!(capsule.radius, 0.30);
    assert_eq!(capsule.base, Vec3::new(10.0, 5.0, 10.0));
}

// ============================================================================
// 8B.5 CROUCH HEIGHT TRANSITION & CEILING CLEARANCE CHECK
// ============================================================================

#[test]
fn test_crouch_height_and_feet_stability() {
    use omnisia::player::{PlayerController, PlayerInput};
    use omnisia::streaming::store::ChunkStore;

    let store = ChunkStore::new();
    let feet_pos = Vec3::new(15.0, 4.0, 15.0);
    let mut controller = PlayerController::new(feet_pos);

    // Default berdiri
    assert_eq!(controller.current_capsule().height, 1.8);
    assert_eq!(controller.current_capsule().base, feet_pos);

    // Input crouch aktif
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, true, false,
    ));
    controller.update_crouch_state(&store);

    assert!(controller.state.crouching);
    assert!(!controller.state.forced_crouch);
    // Tinggi kapsul berkurang menjadi 1.2m
    assert_eq!(controller.current_capsule().height, 1.2);
    // INVARIAN KANONIKAL: Telapak kaki tidak boleh bergerak/teleportasi!
    assert_eq!(controller.current_capsule().base, feet_pos);
    assert_eq!(controller.state.position, feet_pos);

    // Eye position turun mengikuti tinggi jongkok
    let standing_eye = feet_pos.y + 1.62;
    let crouching_eye = feet_pos.y + 1.08;
    assert!((controller.eye_position().y - crouching_eye).abs() < 1e-4);
    assert!(controller.eye_position().y < standing_eye);
}

#[test]
fn test_crouch_stand_clearance_blocked_by_low_ceiling() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::{PlayerController, PlayerInput};
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);

    // Lantai di y = 0 -> permukaan atas y = 0.5m
    chunk.set_voxel(10, 0, 10, VoxelBlock::new(MaterialId::STONE));

    // Langit-langit rendah di y = 4 -> dasar bawah langit-langit y = 4 * 0.5 = 2.0m
    // Jarak bebas dari lantai (0.5m) ke langit-langit (2.0m) adalah 1.5m!
    // Kapsul berdiri butuh 1.8m (0.5m + 1.8m = 2.3m > 2.0m -> MENEMBUS!)
    // Kapsul jongkok butuh 1.2m (0.5m + 1.2m = 1.7m < 2.0m -> MUAT!)
    chunk.set_voxel(10, 4, 10, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    // Pemain berdiri di lantai (y = 0.5m)
    let feet_pos = Vec3::new(5.25, 0.50, 5.25);
    let mut controller = PlayerController::new(feet_pos);

    // 1. Pemain jongkok di bawah langit-langit rendah
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, true, false,
    ));
    controller.update_crouch_state(&store);
    assert!(controller.state.crouching);
    assert_eq!(controller.current_capsule().height, 1.2);

    // 2. Pemain melepas tombol crouch (crouch = false), mencoba berdiri!
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, false,
    ));
    controller.update_crouch_state(&store);

    // INVARIAN KANONIKAL: Berdiri harus DITOLAK karena terhalang langit-langit!
    assert!(
        controller.state.crouching,
        "Pemain harus tetap berjongkok saat berada di bawah langit-langit rendah!"
    );
    assert!(
        controller.state.forced_crouch,
        "Flag forced_crouch harus aktif!"
    );
    assert_eq!(controller.current_capsule().height, 1.2);
    // Kaki tidak boleh berpindah
    assert_eq!(controller.state.position, feet_pos);
}

#[test]
fn test_crouch_stand_clearance_success_after_clear() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::{PlayerController, PlayerInput};
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(10, 0, 10, VoxelBlock::new(MaterialId::STONE));
    // Langit-langit tinggi di y = 6 (dasar y = 3.0m). Kapsul berdiri butuh 0.5 + 1.8 = 2.3m <= 3.0m -> BEBAS!
    chunk.set_voxel(10, 6, 10, VoxelBlock::new(MaterialId::STONE));
    store.insert(chunk);

    let feet_pos = Vec3::new(5.25, 0.50, 5.25);
    let mut controller = PlayerController::new(feet_pos);

    // Jongkok
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, true, false,
    ));
    controller.update_crouch_state(&store);
    assert!(controller.state.crouching);

    // Lepas tombol jongkok
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, false,
    ));
    controller.update_crouch_state(&store);

    // Langit-langit cukup tinggi -> berhasil berdiri!
    assert!(
        !controller.state.crouching,
        "Pemain harus berhasil berdiri saat clearance mencukupi!"
    );
    assert!(!controller.state.forced_crouch);
    assert_eq!(controller.current_capsule().height, 1.8);
    assert_eq!(controller.state.position, feet_pos);
}

// ============================================================================
// 8B.6 JUMP CONTROLLER & SINGLE-CONSUMPTION EDGE TRIGGER
// ============================================================================

#[test]
fn test_grounded_jump_success_and_consumption() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::new(0.0, 1.0, 0.0));
    // Simulasikan pemain sedang grounded
    controller.state.grounded = true;

    // Input menekan Space (jump = true)
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    assert!(controller.state.jump_requested);

    // Eksekusi lompatan
    let jumped = controller.try_execute_jump();

    // INVARIAN KANONIKAL: Lompatan sukses dan jump_requested langsung dikonsumsi!
    assert!(jumped, "Lompatan saat grounded harus berhasil!");
    assert_eq!(controller.state.velocity.y, 6.0);
    assert!(
        !controller.state.grounded,
        "Pemain seketika lepas landas (grounded = false)!"
    );
    assert!(
        !controller.state.jump_requested,
        "jump_requested harus segera dikonsumsi menjadi false!"
    );
}

#[test]
fn test_airborne_jump_rejected() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::new(0.0, 10.0, 0.0));
    // Pemain di udara bebas (grounded = false)
    controller.state.grounded = false;
    controller.state.velocity.y = -2.0;

    // Coba tekan Space di udara (jump = true)
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));

    // Eksekusi lompatan
    let jumped = controller.try_execute_jump();

    // INVARIAN KANONIKAL: Airborne jump ditolak (tidak ada double jump)!
    assert!(!jumped, "Lompat saat di udara harus ditolak!");
    assert_eq!(
        controller.state.velocity.y, -2.0,
        "Kecepatan vertikal tidak boleh berubah!"
    );
    assert!(!controller.state.jump_requested);
}

#[test]
fn test_jump_holding_space_no_repeated_jumps() {
    use omnisia::player::{PlayerController, PlayerInput};

    let mut controller = PlayerController::new(Vec3::ZERO);
    controller.state.grounded = true;

    // Frame 1: Tombol Space ditekan (rising edge false -> true)
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    assert!(controller.try_execute_jump());
    assert_eq!(controller.state.velocity.y, 6.0);
    assert!(!controller.state.grounded);

    // Frame 2..10: Tombol Space TERUS DITAHAN (held: true -> true)
    // Pemain berada di udara
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    assert!(!controller.state.jump_requested);

    // Frame 11: Pemain mendarat kembali di tanah (grounded = true), namun Space MASIH DITAHAN
    controller.state.grounded = true;
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));

    // Eksekusi jump tick
    let repeat_jump = controller.try_execute_jump();

    // INVARIAN KANONIKAL: Menahan Space TIDAK BOLEH memicu lompatan berulang!
    assert!(
        !repeat_jump,
        "Menahan tombol Space tidak boleh memicu repeated jumps saat mendarat!"
    );
    assert!(controller.state.grounded);

    // Frame 12: Tombol Space dilepas (true -> false)
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, false,
    ));

    // Frame 13: Tombol Space ditekan KEMBALI (false -> true)
    controller.set_input(PlayerInput::from_raw(
        false, false, false, false, false, false, true,
    ));
    let new_jump = controller.try_execute_jump();
    assert!(
        new_jump,
        "Setelah dilepas dan ditekan kembali, lompat harus berhasil!"
    );
}

// ============================================================================
// 8B.7 PLAYER GRAVITY & FIXED TIMESTEP SIMULATION LOOP
// ============================================================================

#[test]
fn test_player_gravity_airborne_acceleration() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::player::PlayerController;
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    // Muat chunk kolom y dari y=0 hingga y=4 (ketinggian 0..80m) sebagai LoadedAir
    for cy in 0..5 {
        store.insert(Chunk::new(IVec3::new(0, cy, 0)));
    }

    let spawn_pos = Vec3::new(8.0, 50.0, 8.0);
    let mut controller = PlayerController::new(spawn_pos);
    controller.state.grounded = false;

    // Simulasikan 30 ticks (tepat 1.0 detik) jatuh bebas
    for _ in 0..30 {
        controller.step_simulation(1.0 / 30.0, &store, 0.0);
    }

    // Kecepatan vertikal setelah 1 detik: v = g * t = -9.81 * 1.0 = -9.81 m/s
    assert!(
        (controller.state.velocity.y - (-9.81)).abs() < 1e-4,
        "Kecepatan akhir jatuh bebas harus mendekati -9.81 m/s, terukur: {}",
        controller.state.velocity.y
    );
    // Ketinggian harus berkurang secara signifikan dari posisi awal (50.0m)
    assert!(controller.state.position.y < 50.0);
    assert!(!controller.state.grounded);
}

#[test]
fn test_fixed_timestep_frame_rate_invariance_30_60_120_fps() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::player::{PlayerController, PlayerInput};
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    // Muat area chunk udara di sekitar pergerakan
    for cx in 0..3 {
        for cy in 0..8 {
            store.insert(Chunk::new(IVec3::new(cx, cy, 0)));
        }
    }

    let initial_pos = Vec3::new(8.0, 100.0, 8.0);

    // Setup input konstan: berjalan maju W (yaw = 0 deg -> arah +X)
    let forward_input = PlayerInput::from_raw(true, false, false, false, false, false, false);

    // 1. Jalankan selama 1.0 detik pada 30 FPS (30 frame, masing-masing dt = 1/30 detik)
    let mut controller_30fps = PlayerController::new(initial_pos);
    controller_30fps.set_input(forward_input);
    for _ in 0..30 {
        controller_30fps.update_fixed_time(1.0 / 30.0, &store, 0.0);
    }

    // 2. Jalankan selama 1.0 detik pada 60 FPS (60 frame, masing-masing dt = 1/60 detik)
    let mut controller_60fps = PlayerController::new(initial_pos);
    controller_60fps.set_input(forward_input);
    for _ in 0..60 {
        controller_60fps.update_fixed_time(1.0 / 60.0, &store, 0.0);
    }

    // 3. Jalankan selama 1.0 detik pada 120 FPS (120 frame, masing-masing dt = 1/120 detik)
    let mut controller_120fps = PlayerController::new(initial_pos);
    controller_120fps.set_input(forward_input);
    for _ in 0..120 {
        controller_120fps.update_fixed_time(1.0 / 120.0, &store, 0.0);
    }

    // INVARIAN KANONIKAL: Ketiga cadence render harus menghasilkan trajektori identik!
    let dist_30_60 = controller_30fps
        .state
        .position
        .distance(controller_60fps.state.position);
    let dist_60_120 = controller_60fps
        .state
        .position
        .distance(controller_120fps.state.position);

    assert!(
        dist_30_60 < 1e-4,
        "Trajektori 30 FPS vs 60 FPS harus identik! Selisih jarak: {}",
        dist_30_60
    );
    assert!(
        dist_60_120 < 1e-4,
        "Trajektori 60 FPS vs 120 FPS harus identik! Selisih jarak: {}",
        dist_60_120
    );

    // Kecepatan linier juga harus identik
    assert!((controller_30fps.state.velocity.x - controller_60fps.state.velocity.x).abs() < 1e-4);
    assert!((controller_30fps.state.velocity.y - controller_60fps.state.velocity.y).abs() < 1e-4);
}

#[test]
fn test_pathological_frame_stall_bounded_catchup() {
    use omnisia::player::PlayerController;
    use omnisia::streaming::store::ChunkStore;

    let store = ChunkStore::new();
    let mut controller = PlayerController::new(Vec3::ZERO);

    // Kirim stall besar 2.0 detik (seperti saat garbage collection atau window resize)
    controller.update_fixed_time(2.0, &store, 0.0);

    // INVARIAN KANONIKAL: Akumulator harus dibuang / tidak boleh memicu spiral of death
    assert!(
        controller.time_accumulator < controller.config.fixed_timestep,
        "Akumulator harus di-clamp dan tidak boleh menumpuk substep tak terhingga!"
    );
}

// ============================================================================
// 8B.8 SWEPT VOXEL COLLISION RESOLUTION, ANTI-TUNNELING & UNLOADED BOUNDARY GUARD
// ============================================================================

#[test]
fn test_high_speed_anti_tunneling_50_and_100_mps() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::PlayerController;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);

    // Dinding setebal 1 voxel (0.5m) pada x = 2.0..2.5m (voxel x = 4)
    // Tinggi dinding 3 blok (y = 0, 1, 2)
    for vy in 0..3 {
        for vz in 0..5 {
            chunk.set_voxel(4, vy, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    // 1. Uji Kecepatan 50 m/s:
    // delta_x dalam 1 tick (1/30 detik) = 50.0 / 30.0 = 1.667m > tebal dinding 0.5m!
    let mut controller_50 = PlayerController::new(Vec3::new(1.0, 0.0, 1.0));
    controller_50.state.velocity = Vec3::new(50.0, 0.0, 0.0);
    controller_50.step_simulation(1.0 / 30.0, &store, 0.0);

    // Dinding berada di x = 2.0. Kapsul ber-radius 0.3.
    // Titik depan kapsul: position.x + radius HARUS <= 2.0m!
    let front_x_50 = controller_50.state.position.x + controller_50.config.capsule_radius;
    assert!(
        front_x_50 <= 2.001,
        "Player menembus dinding pada kecepatan 50 m/s! Ujung depan: {}",
        front_x_50
    );
    assert_eq!(controller_50.state.velocity.x, 0.0);
    assert!(controller_50.collision_hits_total > 0);

    // 2. Uji Kecepatan Ekstrim 100 m/s:
    // delta_x dalam 1 tick = 100.0 / 30.0 = 3.333m (menempuh >6 ketebalan voxel sekaligus)!
    let mut controller_100 = PlayerController::new(Vec3::new(0.5, 0.0, 1.0));
    controller_100.state.velocity = Vec3::new(100.0, 0.0, 0.0);
    controller_100.step_simulation(1.0 / 30.0, &store, 0.0);

    let front_x_100 = controller_100.state.position.x + controller_100.config.capsule_radius;
    assert!(
        front_x_100 <= 2.001,
        "Player menembus dinding pada kecepatan 100 m/s! Ujung depan: {}",
        front_x_100
    );
    assert_eq!(controller_100.state.velocity.x, 0.0);
}

#[test]
fn test_swept_collision_one_voxel_floor() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::PlayerController;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);

    // Lantai setebal 1 voxel (0.5m) di y = 0 -> permukaan atas y = 0.5m
    for vx in 0..5 {
        for vz in 0..5 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    // Pemain jatuh dengan kecepatan vertikal tinggi -50 m/s dari ketinggian y = 2.0m
    let mut controller = PlayerController::new(Vec3::new(1.0, 2.0, 1.0));
    controller.state.velocity = Vec3::new(0.0, -50.0, 0.0);
    controller.step_simulation(1.0 / 30.0, &store, 0.0);

    // INVARIAN KANONIKAL: Pemain harus berhenti tepat di atas lantai (y = 0.5m) tanpa menembus ke void!
    assert!(
        (controller.state.position.y - 0.5).abs() < 1e-3,
        "Pemain jatuh menembus lantai 1 voxel! Posisi y: {}",
        controller.state.position.y
    );
    assert!(controller.state.grounded);
    assert_eq!(controller.state.velocity.y, 0.0);
}

#[test]
fn test_corner_collision_no_diagonal_leak() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::PlayerController;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(IVec3::ZERO);

    // Buat sudut dinding L:
    // Dinding 1: x = 2.0..2.5 (vx = 4)
    // Dinding 2: z = 2.0..2.5 (vz = 4)
    for vy in 0..3 {
        for vz in 0..5 {
            chunk.set_voxel(4, vy, vz, VoxelBlock::new(MaterialId::STONE));
        }
        for vx in 0..5 {
            chunk.set_voxel(vx, vy, 4, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(chunk);

    // Pemain bergerak diagonal cepat menuju sudut (+X, +Z)
    let mut controller = PlayerController::new(Vec3::new(1.2, 0.0, 1.2));
    controller.state.velocity = Vec3::new(30.0, 0.0, 30.0);
    controller.step_simulation(1.0 / 30.0, &store, 0.0);

    // Ujung kapsul tidak boleh bocor menembus diagonal (x <= 2.0m dan z <= 2.0m)
    let front_x = controller.state.position.x + controller.config.capsule_radius;
    let front_z = controller.state.position.z + controller.config.capsule_radius;
    assert!(
        front_x <= 2.001,
        "Bocor diagonal di sumbu X! front_x: {}",
        front_x
    );
    assert!(
        front_z <= 2.001,
        "Bocor diagonal di sumbu Z! front_z: {}",
        front_z
    );
}

#[test]
fn test_chunk_boundary_and_negative_coordinates_collision() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::material::MaterialId;
    use omnisia::player::PlayerController;
    use omnisia::streaming::store::ChunkStore;
    use omnisia::voxel::VoxelBlock;

    let mut store = ChunkStore::new();
    // Chunk negatif (-1, 0, 0)
    let mut neg_chunk = Chunk::new(IVec3::new(-1, 0, 0));
    // Dinding di voxel lokal 31 (koordinat dunia -1 * 32 + 31 = -1 -> x = -0.5m .. 0.0m)
    for vy in 0..3 {
        for vz in 0..5 {
            neg_chunk.set_voxel(31, vy, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    store.insert(neg_chunk);

    // Chunk (0, 0, 0) dimuat
    let pos_chunk = Chunk::new(IVec3::ZERO);
    store.insert(pos_chunk);

    // Pemain di chunk positif x = 0.5m bergerak ke kiri (-X) menuju dinding di batas chunk negatif
    let mut controller = PlayerController::new(Vec3::new(0.5, 0.0, 1.0));
    controller.state.velocity = Vec3::new(-20.0, 0.0, 0.0);
    controller.step_simulation(1.0 / 30.0, &store, 0.0);

    // Dinding di [-0.5, 0.0]. Ujung kiri kapsul (pos.x - radius) harus tertahan di >= 0.0m
    let left_x = controller.state.position.x - controller.config.capsule_radius;
    assert!(
        left_x >= -0.001,
        "Tabrakan di batas koordinat negatif tembus! Ujung kiri: {}",
        left_x
    );
    assert_eq!(controller.state.velocity.x, 0.0);
}

#[test]
fn test_unloaded_chunk_blocks_movement_unknown_not_air() {
    use glam::IVec3;
    use omnisia::chunk::Chunk;
    use omnisia::player::PlayerController;
    use omnisia::streaming::store::ChunkStore;

    let mut store = ChunkStore::new();
    // Hanya muat chunk (0, 0, 0) [x = 0..16m]
    let loaded_chunk = Chunk::new(IVec3::ZERO);
    store.insert(loaded_chunk);
    // Chunk (1, 0, 0) [x = 16..32m] TIDAK DIMUAT (UNLOADED / UNKNOWN)

    // Pemain berada di x = 15.5m (dekat batas chunk) bergerak ke kanan (+X) menuju chunk yang belum dimuat
    let mut controller = PlayerController::new(Vec3::new(15.5, 0.0, 5.0));
    controller.state.velocity = Vec3::new(10.0, 0.0, 0.0);
    controller.step_simulation(1.0 / 30.0, &store, 0.0);

    // INVARIAN KANONIKAL: Batas chunk belum dimuat (x = 16.0m) harus menahan gerakan pemain!
    let front_x = controller.state.position.x + controller.config.capsule_radius;
    assert!(
        front_x <= 16.001,
        "Pemain tidak boleh memasuki chunk yang belum dimuat! front_x: {}",
        front_x
    );
    assert!(
        controller.unknown_blocked_total > 0,
        "unknown_blocked_total harus mencatat blokir chunk belum dimuat!"
    );
    assert_eq!(controller.state.velocity.x, 0.0);
}
