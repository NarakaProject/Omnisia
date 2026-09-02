use glam::{IVec3, Vec3};

use omnisia::material::MaterialId;
use omnisia::physics::{DynamicBody, DynamicBodyId, DynamicBodyState, PhysicsConfig};
use omnisia::structure::aggregate::DetachedAggregate;
use omnisia::voxel::{VoxelBlock, VOXEL_SIZE};

// ============================================================================
// 8A.1 DYNAMIC BODY DATA MODEL TESTS
// ============================================================================

#[test]
fn test_dynamic_body_data_model_construction() {
    let voxels = vec![
        (IVec3::new(10, 20, 30), VoxelBlock::new(MaterialId::STONE)),
        (IVec3::new(10, 21, 30), VoxelBlock::new(MaterialId::DIRT)),
    ];
    let aggregate = DetachedAggregate::from_world_voxels(1, &voxels).expect("Valid aggregate");

    let body_id = DynamicBodyId(101);
    let body = DynamicBody::from_detached_aggregate(body_id, aggregate);

    assert_eq!(body.id, body_id);
    assert_eq!(body.state, DynamicBodyState::Active);
    assert_eq!(body.velocity, Vec3::ZERO);
    assert_eq!(body.gravity_scale, 1.0);
    assert_eq!(body.ticks_stationary, 0);
    assert!(!body.is_grounded);

    // Posisi awal harus tepat sama dengan min_voxel dalam meter (10 * 0.5, 20 * 0.5, 30 * 0.5)
    assert_eq!(body.position, Vec3::new(5.0, 10.0, 15.0));
    assert_eq!(body.voxel_count(), 2);
}

#[test]
fn test_dynamic_body_state_transitions() {
    let voxels = vec![(IVec3::ZERO, VoxelBlock::new(MaterialId::STONE))];
    let aggregate = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let mut body = DynamicBody::from_detached_aggregate(DynamicBodyId(1), aggregate);

    assert_eq!(body.state, DynamicBodyState::Active);

    body.ticks_stationary = 10;
    body.set_state(DynamicBodyState::Sleeping);
    assert_eq!(body.state, DynamicBodyState::Sleeping);
    assert_eq!(body.ticks_stationary, 10);

    body.set_state(DynamicBodyState::Settled);
    assert_eq!(body.state, DynamicBodyState::Settled);

    // Kembali ke active mereset counter diam
    body.set_state(DynamicBodyState::Active);
    assert_eq!(body.state, DynamicBodyState::Active);
    assert_eq!(body.ticks_stationary, 0);
}

#[test]
fn test_dynamic_body_bounds_and_voxel_count() {
    // Balok 2x3x4 voxel dari (0,0,0) sampai (1,2,3)
    let mut voxels = Vec::new();
    for x in 0..2 {
        for y in 0..3 {
            for z in 0..4 {
                voxels.push((IVec3::new(x, y, z), VoxelBlock::new(MaterialId::STONE)));
            }
        }
    }
    let aggregate = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(42), aggregate);

    assert_eq!(body.voxel_count(), 24);
    assert_eq!(body.voxel_dimensions(), IVec3::new(2, 3, 4));

    let (min_bound, max_bound) = body.world_bounds();
    assert_eq!(min_bound, Vec3::ZERO);
    assert_eq!(
        max_bound,
        Vec3::new(2.0 * VOXEL_SIZE, 3.0 * VOXEL_SIZE, 4.0 * VOXEL_SIZE)
    );

    let (min_v, max_v) = body.world_voxel_bounds();
    assert_eq!(min_v, IVec3::new(0, 0, 0));
    assert_eq!(max_v, IVec3::new(1, 2, 3));
}

#[test]
fn test_physics_config_defaults() {
    let config = PhysicsConfig::default();
    assert_eq!(config.world_gravity, Vec3::new(0.0, -9.81, 0.0));
    assert_eq!(config.fixed_timestep_hz, 30.0);
    assert_eq!(config.fixed_dt, 1.0 / 30.0);
    assert_eq!(config.sleep_velocity_threshold, 0.05);
    assert_eq!(config.sleep_ticks_required, 15);
    assert_eq!(config.max_substeps_per_frame, 5);
}

// ============================================================================
// 8A.2 DETACHED AGGREGATE -> DYNAMIC BODY (MOVE SEMANTICS & INTEGRITY)
// ============================================================================

#[test]
fn test_detached_aggregate_to_dynamic_body_move_semantics() {
    let voxels = vec![
        (IVec3::new(-5, 12, -8), VoxelBlock::new(MaterialId::STONE)),
        (IVec3::new(-5, 13, -8), VoxelBlock::new(MaterialId::DIRT)),
        (IVec3::new(-4, 12, -8), VoxelBlock::new(MaterialId::GRASS)),
    ];
    let agg = DetachedAggregate::from_world_voxels(99, &voxels).expect("Valid aggregate");

    // Move semantics: agg berpindah langsung ke dalam DynamicBody
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(99), agg)
        .with_gravity_scale(0.5)
        .with_velocity(Vec3::new(0.0, -2.0, 0.0));

    assert_eq!(body.id, DynamicBodyId(99));
    assert_eq!(body.gravity_scale, 0.5);
    assert_eq!(body.velocity, Vec3::new(0.0, -2.0, 0.0));
    assert_eq!(body.voxel_count(), 3);
    assert!(body.validate_integrity());

    // Verifikasi identitas material & posisi dunia asal tetap 100% utuh
    let world_voxels: Vec<(IVec3, VoxelBlock)> = body.iter_world_voxels().collect();
    assert_eq!(world_voxels.len(), 3);
    assert!(world_voxels.contains(&(IVec3::new(-5, 12, -8), VoxelBlock::new(MaterialId::STONE))));
    assert!(world_voxels.contains(&(IVec3::new(-5, 13, -8), VoxelBlock::new(MaterialId::DIRT))));
    assert!(world_voxels.contains(&(IVec3::new(-4, 12, -8), VoxelBlock::new(MaterialId::GRASS))));
}

#[test]
fn test_detached_aggregate_multi_material_and_topology_preservation() {
    // Struktur berbentuk L melintasi kuadran negatif
    let voxels = vec![
        (IVec3::new(-33, 10, 0), VoxelBlock::new(MaterialId::STONE)),
        (IVec3::new(-32, 10, 0), VoxelBlock::new(MaterialId::SAND)),
        (IVec3::new(-32, 11, 0), VoxelBlock::new(MaterialId::DIRT)),
    ];
    let agg = DetachedAggregate::from_world_voxels(10, &voxels).unwrap();
    let body = DynamicBody::from_detached_aggregate(DynamicBodyId(10), agg);

    assert_eq!(body.voxel_count(), 3);
    assert_eq!(body.voxel_dimensions(), IVec3::new(2, 2, 1));
    assert!(body.validate_integrity());

    let (min_b, max_b) = body.world_voxel_bounds();
    assert_eq!(min_b, IVec3::new(-33, 10, 0));
    assert_eq!(max_b, IVec3::new(-32, 11, 0));
}
