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
