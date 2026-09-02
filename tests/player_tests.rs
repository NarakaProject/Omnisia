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
