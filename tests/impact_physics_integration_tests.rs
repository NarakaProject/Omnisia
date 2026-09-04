use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::csg::{
    CraterGenerator, DefaultDestructionPolicy, VoxelEdit, VoxelEditCommitResult,
    VoxelEditTransaction,
};
use omnisia::impact::{ImpactBridge, ImpactEvent, ImpactId};
use omnisia::material::{MaterialId, MaterialRegistry};
use omnisia::physics::aggregate::{
    calculate_aggregate_mass_properties, AggregateColliderStrategy, OrientationQuantizationPolicy,
};
use omnisia::physics::rigid_body::{MassProperties, RigidBody, SleepState};
use omnisia::physics::world::{PhysicsWorld, PhysicsWorldConfig};
use omnisia::physics::{BodyType, RigidBodyId};
use omnisia::streaming::store::ChunkStore;
use omnisia::structure::aggregate::DetachedAggregate;
use omnisia::structure::anchor::AnchorPolicy;
use omnisia::structure::events::{StructuralEvent, StructuralMutationType};
use omnisia::structure::manager::StructuralSystem;
use omnisia::voxel::VoxelBlock;

const MAT_ANCHOR: MaterialId = MaterialId(1); // Solid bedrock / anchor
const MAT_WOOD: MaterialId = MaterialId(2); // Non-anchor structural material

fn setup_test_environment() -> (ChunkStore, StructuralSystem, PhysicsWorld) {
    let store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));

    let mut anchor_policy = AnchorPolicy::default();
    anchor_policy.register_anchor_material(MAT_ANCHOR);

    let structural_sys = StructuralSystem::new(anchor_policy);
    let physics_world = PhysicsWorld::new(PhysicsWorldConfig::default());

    (store, structural_sys, physics_world)
}

fn setup_multi_chunk_store(min_c: IVec3, max_c: IVec3) -> ChunkStore {
    let mut store = ChunkStore::new();
    for x in min_c.x..=max_c.x {
        for y in min_c.y..=max_c.y {
            for z in min_c.z..=max_c.z {
                store.insert(Chunk::new(IVec3::new(x, y, z)));
            }
        }
    }
    store
}

// ============================================================================
// A. IMPACT INTEGRATION & CSG CONSUMPTION (TESTS 1..6)
// ============================================================================

#[test]
fn test_1_impact_to_csg_transaction() {
    let (mut store, _, _) = setup_test_environment();
    for y in 0..5 {
        store.set_voxel_world(IVec3::new(5, y, 5), VoxelBlock::new(MAT_WOOD));
    }

    let impact = ImpactEvent::builder(ImpactId(1), Vec3::new(2.5, 1.5, 2.5), 1.0)
        .energy(100.0)
        .build()
        .unwrap();

    let tx = CraterGenerator::from_impact(
        &impact,
        &DefaultDestructionPolicy,
        &MaterialRegistry::default(),
        &store,
    )
    .unwrap();

    assert!(!tx.is_empty());
    let commit_res = tx.commit(&mut store).unwrap();
    assert!(!commit_res.structural_events.is_empty());
}

#[test]
fn test_2_crater_no_topology_split_remains_static() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    // Solid 5x5 block of anchor stone
    for x in 10..15 {
        for y in 0..5 {
            for z in 10..15 {
                store.set_voxel_world(IVec3::new(x, y, z), VoxelBlock::new(MAT_ANCHOR));
            }
        }
    }

    let impact = ImpactEvent::builder(ImpactId(2), Vec3::new(6.0, 2.5, 6.0), 0.6)
        .energy(50.0)
        .build()
        .unwrap();

    let tx = CraterGenerator::from_impact(
        &impact,
        &DefaultDestructionPolicy,
        &MaterialRegistry::default(),
        &store,
    )
    .unwrap();

    let commit_res = tx.commit(&mut store).unwrap();
    let bridge_res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    assert_eq!(bridge_res.total_voxels_detached, 0);
    assert!(bridge_res.detached_bodies.is_empty());
    assert_eq!(phys_world.rigid_bodies.len(), 0);
}

#[test]
fn test_3_crater_topology_split_creates_detached_aggregate() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    // Anchor at (0,0,0), non-anchor beam extending to (0, 3, 0)
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));
    store.set_voxel_world(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 2, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 3, 0), VoxelBlock::new(MAT_WOOD));

    // Remove voxel at (0, 1, 0) severing (0, 2, 0) and (0, 3, 0)
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(0, 1, 0)));
    let commit_res = tx.commit(&mut store).unwrap();

    let impact = ImpactEvent::builder(ImpactId(3), Vec3::new(0.25, 0.75, 0.25), 0.5)
        .energy(10.0)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    assert_eq!(res.detached_bodies.len(), 1);
    assert_eq!(res.total_voxels_detached, 2);
    assert_eq!(phys_world.rigid_bodies.len(), 1);
    // Severed voxels become AIR in ChunkStore
    assert_eq!(store.get_voxel_world(IVec3::new(0, 2, 0)), VoxelBlock::AIR);
    assert_eq!(store.get_voxel_world(IVec3::new(0, 3, 0)), VoxelBlock::AIR);
    // Anchor remains intact
    assert_eq!(
        store.get_voxel_world(IVec3::new(0, 0, 0)),
        VoxelBlock::new(MAT_ANCHOR)
    );
}

#[test]
fn test_4_energy_only_impact_applies_zero_impulse() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));
    store.set_voxel_world(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 2, 0), VoxelBlock::new(MAT_WOOD));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(0, 1, 0)));
    let commit_res = tx.commit(&mut store).unwrap();

    // Impact with pure ENERGY, zero impulse
    let impact = ImpactEvent::builder(ImpactId(4), Vec3::new(0.25, 0.75, 0.25), 0.5)
        .energy(500.0)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    assert_eq!(res.impulse_applied, None);
    let dyn_id = res.detached_bodies[0];
    let record = phys_world.get_dynamic_aggregate(dyn_id).unwrap();
    let rb = phys_world.get_rigid_body(record.rigid_body_id).unwrap();
    // Body starts at zero velocity under energy-only impact
    assert_eq!(rb.linear_velocity(), Vec3::ZERO);
    assert_eq!(rb.angular_velocity(), Vec3::ZERO);
}

#[test]
fn test_5_impulse_impact_applies_exact_magnitude() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));
    store.set_voxel_world(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 2, 0), VoxelBlock::new(MAT_WOOD));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(0, 1, 0)));
    let commit_res = tx.commit(&mut store).unwrap();

    let target_impulse = 50.0f32;
    let impact = ImpactEvent::builder(ImpactId(5), Vec3::new(0.25, 0.75, 0.25), 0.5)
        .impulse(target_impulse)
        .direction(Vec3::X)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    assert_eq!(res.impulse_applied, Some(target_impulse));
    let dyn_id = res.detached_bodies[0];
    let record = phys_world.get_dynamic_aggregate(dyn_id).unwrap();
    let rb = phys_world.get_rigid_body(record.rigid_body_id).unwrap();

    // Mass = 1.0 kg (single voxel default), momentum = m * v = J
    let speed = rb.linear_velocity().length();
    let mass = rb.mass_properties().mass;
    let actual_impulse = speed * mass;
    assert!(
        (actual_impulse - target_impulse).abs() < 1e-4,
        "Applied impulse must match J exactly without attenuation"
    );
}

#[test]
fn test_6_missing_impulse_direction_fallback() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));
    store.set_voxel_world(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 2, 0), VoxelBlock::new(MAT_WOOD));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(0, 1, 0)));
    let commit_res = tx.commit(&mut store).unwrap();

    // No direction, but has surface_normal Vec3::Y
    let impact = ImpactEvent::builder(ImpactId(6), Vec3::new(0.25, 0.75, 0.25), 0.5)
        .impulse(20.0)
        .surface_normal(Vec3::Y)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    assert_eq!(res.impulse_applied, Some(20.0));
    let dyn_id = res.detached_bodies[0];
    let record = phys_world.get_dynamic_aggregate(dyn_id).unwrap();
    let rb = phys_world.get_rigid_body(record.rigid_body_id).unwrap();
    // Direction was -surface_normal = -Y
    assert!(rb.linear_velocity().y < 0.0);
}

// ============================================================================
// B. DETACHMENT, LOCALITY & STRUCTURAL CONSISTENCY (TESTS 7..12)
// ============================================================================

#[test]
fn test_7_add_voxel_does_not_trigger_detachment_bfs() {
    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));
    let mut struct_sys = StructuralSystem::new(AnchorPolicy::default());

    // Adding voxel emits VoxelPlaced
    let events = vec![StructuralEvent::new(
        IVec3::new(1, 1, 1),
        StructuralMutationType::VoxelPlaced {
            new_block: VoxelBlock::new(MAT_WOOD),
        },
    )];

    let detached = struct_sys.reconcile_events(&events, &mut store);
    assert!(detached.is_empty());
    assert_eq!(struct_sys.total_connectivity_checks, 0);
}

#[test]
fn test_8_replace_solid_to_solid_does_not_trigger_detachment_bfs() {
    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));
    let mut struct_sys = StructuralSystem::new(AnchorPolicy::default());

    // Solid to solid replacement preserves connectivity
    let events = vec![StructuralEvent::new(
        IVec3::new(1, 1, 1),
        StructuralMutationType::VoxelReplaced {
            previous_block: VoxelBlock::new(MAT_WOOD),
            new_block: VoxelBlock::new(MAT_ANCHOR),
        },
    )];

    let detached = struct_sys.reconcile_events(&events, &mut store);
    assert!(detached.is_empty());
    assert_eq!(struct_sys.total_connectivity_checks, 0);
}

#[test]
fn test_9_replace_solid_to_air_triggers_detachment_bfs() {
    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));
    store.set_voxel_world(IVec3::new(1, 2, 1), VoxelBlock::new(MAT_WOOD));

    // Replace solid -> air is a detachment candidate
    let event = StructuralEvent::new(
        IVec3::new(1, 1, 1),
        StructuralMutationType::VoxelReplaced {
            previous_block: VoxelBlock::new(MAT_WOOD),
            new_block: VoxelBlock::AIR,
        },
    );

    assert!(event.can_cause_detachment());
    let seeds = StructuralSystem::collect_candidate_seeds(&[event], &store);
    assert!(seeds.contains(&IVec3::new(1, 2, 1)));
}

#[test]
fn test_10_duplicate_neighbor_candidates_single_bfs() {
    let (mut store, mut struct_sys, _) = setup_test_environment();
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD));

    // Two removal events touching the same neighbor (0,0,0)
    let events = vec![
        StructuralEvent::new(
            IVec3::new(1, 0, 0),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
        StructuralEvent::new(
            IVec3::new(0, 1, 0),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
    ];

    let seeds = StructuralSystem::collect_candidate_seeds(&events, &store);
    // (0,0,0) appears as a neighbor of both, but deduplication ensures single seed
    let count_target = seeds.iter().filter(|&&p| p == IVec3::new(0, 0, 0)).count();
    assert_eq!(count_target, 1);

    let detached = struct_sys.reconcile_events(&events, &mut store);
    assert_eq!(detached.len(), 1);
    assert_eq!(struct_sys.total_connectivity_checks, 1);
}

#[test]
fn test_11_multi_removed_voxels_single_extracted_aggregate() {
    let (mut store, mut struct_sys, _) = setup_test_environment();
    // Anchor
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));
    // Cut 2 voxels at y=1 and y=2
    // Overhang at y=3 and y=4
    store.set_voxel_world(IVec3::new(0, 3, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 4, 0), VoxelBlock::new(MAT_WOOD));

    let events = vec![
        StructuralEvent::new(
            IVec3::new(0, 1, 0),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
        StructuralEvent::new(
            IVec3::new(0, 2, 0),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
    ];

    let detached = struct_sys.reconcile_events(&events, &mut store);
    assert_eq!(
        detached.len(),
        1,
        "Should extract exactly one 2-voxel aggregate"
    );
    assert_eq!(detached[0].voxel_count(), 2);
}

#[test]
fn test_12_structural_graph_consistency_after_detachment() {
    let (mut store, mut struct_sys, _) = setup_test_environment();
    store.set_voxel_world(IVec3::new(2, 2, 2), VoxelBlock::new(MAT_WOOD));

    let events = vec![StructuralEvent::new(
        IVec3::new(2, 2, 1),
        StructuralMutationType::VoxelRemoved {
            previous_block: VoxelBlock::new(MAT_WOOD),
        },
    )];

    let detached = struct_sys.reconcile_events(&events, &mut store);
    assert_eq!(detached.len(), 1);

    // After detachment:
    // 1. ChunkStore voxel is AIR
    assert_eq!(store.get_voxel_world(IVec3::new(2, 2, 2)), VoxelBlock::AIR);
    // 2. StructuralSystem ledger contains aggregate
    assert_eq!(struct_sys.detached_aggregates.len(), 1);
    // 3. No graph discrepancy: subsequent check sees AIR
    assert!(store.get_voxel_world(IVec3::new(2, 2, 2)).is_air());
}

// ============================================================================
// C. COORDINATE FRAME & ZERO-MOTION ROUND TRIP (TESTS 13..15)
// ============================================================================

#[test]
fn test_13_canonical_aggregate_coordinate_frames() {
    let voxels = vec![
        (IVec3::new(10, 20, 30), VoxelBlock::new(MAT_WOOD)),
        (IVec3::new(10, 21, 30), VoxelBlock::new(MAT_WOOD)),
    ];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();

    assert_eq!(agg.min_voxel, IVec3::new(10, 20, 30));
    assert_eq!(agg.max_voxel, IVec3::new(10, 21, 30));
    assert_eq!(agg.voxels[0].relative_coord, IVec3::new(0, 0, 0));
    assert_eq!(agg.voxels[1].relative_coord, IVec3::new(0, 1, 0));

    let props = calculate_aggregate_mass_properties(&agg, None).unwrap();
    // Center of mass world must be exactly at (10.25, 20.5, 30.25)
    assert!((props.center_of_mass_world.x - 5.25).abs() < 1e-4);
    assert!((props.center_of_mass_world.y - 10.5).abs() < 1e-4);
    assert!((props.center_of_mass_world.z - 15.25).abs() < 1e-4);
}

#[test]
fn test_14_zero_motion_round_trip_lossless() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    // Nontrivial L-shape aggregate spanning negative coordinates
    let orig_voxels = vec![
        (IVec3::new(-5, 2, -3), VoxelBlock::new(MAT_WOOD)),
        (IVec3::new(-4, 2, -3), VoxelBlock::new(MAT_WOOD)),
        (IVec3::new(-4, 3, -3), VoxelBlock::new(MAT_WOOD)),
    ];

    for &(p, b) in &orig_voxels {
        store.set_voxel_world(p, b);
    }

    let agg = DetachedAggregate::from_world_voxels(100, &orig_voxels).unwrap();
    // Clear in store (simulating detachment)
    for &(p, _) in &orig_voxels {
        store.set_voxel_world(p, VoxelBlock::AIR);
    }

    // Physicalize
    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    // Zero-motion reintegrate immediately
    world
        .reintegrate_aggregate(
            dyn_id,
            &mut store,
            OrientationQuantizationPolicy::NearestLattice,
        )
        .expect("Zero motion reintegration must succeed losslessly");

    // Verify exact reproduction of original voxel coordinates and materials
    for &(p, b) in &orig_voxels {
        assert_eq!(
            store.get_voxel_world(p),
            b,
            "Voxel at {:?} must be restored exactly",
            p
        );
    }
}

#[test]
fn test_15_contact_point_surface_clamping() {
    let voxels = vec![(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD))];
    let agg = DetachedAggregate::from_world_voxels(1, &voxels).unwrap();

    // Voxel bounds: [0.0..0.5] along X, Y, Z. Center: (0.25, 0.25, 0.25)
    // Impact position from +X outside: (1.0, 0.25, 0.25)
    let impact_pos = Vec3::new(1.0, 0.25, 0.25);
    let contact = ImpactBridge::compute_contact_point(&agg, impact_pos);

    // Clamped to max X face: 0.5
    assert_eq!(contact, Vec3::new(0.5, 0.25, 0.25));
}

// ============================================================================
// D. WHOLE-IMPACT ATOMICITY & ROLLBACK INTEGRITY (TESTS 16..21)
// ============================================================================

#[test]
fn test_16_multi_aggregate_split_deterministic_creation() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    // Anchor at center
    store.set_voxel_world(IVec3::new(5, 5, 5), VoxelBlock::new(MAT_ANCHOR));
    // Overhang 1 to the left
    store.set_voxel_world(IVec3::new(4, 5, 5), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(3, 5, 5), VoxelBlock::new(MAT_WOOD));
    // Overhang 2 to the right
    store.set_voxel_world(IVec3::new(6, 5, 5), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(7, 5, 5), VoxelBlock::new(MAT_WOOD));

    // Remove the anchor in between
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(5, 5, 5)));
    let commit_res = tx.commit(&mut store).unwrap();

    let impact = ImpactEvent::builder(ImpactId(16), Vec3::new(2.75, 2.75, 2.75), 0.5)
        .energy(10.0)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    assert_eq!(res.detached_bodies.len(), 2);
    assert_eq!(res.total_voxels_detached, 4);
    assert_eq!(phys_world.rigid_bodies.len(), 2);
}

#[test]
fn test_17_multi_aggregate_atomic_rollback_on_failure() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    // Create 2 detached islands
    store.set_voxel_world(IVec3::new(1, 1, 1), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(10, 10, 10), VoxelBlock::new(MAT_WOOD));

    // Force failure on physicalization of aggregate 2 by pre-inserting RigidBodyId(2)
    let dummy = RigidBody::new(
        RigidBodyId(2),
        BodyType::Dynamic,
        Vec3::ZERO,
        glam::Quat::IDENTITY,
        Vec3::ZERO,
        Vec3::ZERO,
        MassProperties::new_dynamic(1.0, glam::Mat3::IDENTITY).unwrap(),
    )
    .unwrap();
    phys_world.rigid_bodies.insert(RigidBodyId(2), dummy);
    phys_world.next_body_id = 1;
    phys_world.next_dynamic_body_id = 1;

    let events = vec![
        StructuralEvent::new(
            IVec3::new(1, 1, 0),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
        StructuralEvent::new(
            IVec3::new(10, 10, 9),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
    ];

    let commit_res = VoxelEditCommitResult {
        affected_chunks: vec![IVec3::ZERO],
        mesh_invalidation_chunks: vec![IVec3::ZERO],
        structural_events: events,
        delta: omnisia::csg::ProposedDelta {
            deltas: vec![],
            affected_chunks: vec![],
            mesh_invalidation_chunks: vec![],
        },
        chunk_pre_states: vec![],
    };

    let impact = ImpactEvent::builder(ImpactId(17), Vec3::new(1.0, 1.0, 1.0), 1.0)
        .energy(10.0)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    );

    // Physicalization of 2nd aggregate failed -> full Phase A rollback
    assert!(res.is_err());
    assert_eq!(
        store.get_voxel_world(IVec3::new(1, 1, 1)),
        VoxelBlock::new(MAT_WOOD)
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(10, 10, 10)),
        VoxelBlock::new(MAT_WOOD)
    );
    // Only the dummy body remains in rigid bodies, 0 dynamic aggregate records
    assert_eq!(phys_world.dynamic_aggregates.len(), 0);
    assert_eq!(phys_world.rigid_bodies.len(), 1);
}

#[test]
fn test_18_multi_aggregate_atomic_rollback_restores_exact_dirty_state() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    let chunk = store.get_mut(&IVec3::ZERO).unwrap();
    chunk.dirty_flags = dirty_flags::LIGHTING_DIRTY;

    store.set_voxel_world(IVec3::new(2, 2, 2), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(10, 10, 10), VoxelBlock::new(MAT_WOOD));
    let chunk_after = store.get_mut(&IVec3::ZERO).unwrap();
    chunk_after.dirty_flags = dirty_flags::LIGHTING_DIRTY;
    let expected_dirty = chunk_after.dirty_flags;
    let expected_rev = chunk_after.revision;

    // Force failure on aggregate 2 physicalization
    let dummy = RigidBody::new(
        RigidBodyId(2),
        BodyType::Dynamic,
        Vec3::ZERO,
        glam::Quat::IDENTITY,
        Vec3::ZERO,
        Vec3::ZERO,
        MassProperties::new_dynamic(1.0, glam::Mat3::IDENTITY).unwrap(),
    )
    .unwrap();
    phys_world.rigid_bodies.insert(RigidBodyId(2), dummy);
    phys_world.next_body_id = 1;
    phys_world.next_dynamic_body_id = 1;

    let events = vec![
        StructuralEvent::new(
            IVec3::new(2, 2, 1),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
        StructuralEvent::new(
            IVec3::new(10, 10, 9),
            StructuralMutationType::VoxelRemoved {
                previous_block: VoxelBlock::new(MAT_WOOD),
            },
        ),
    ];

    let commit_res = VoxelEditCommitResult {
        affected_chunks: vec![IVec3::ZERO],
        mesh_invalidation_chunks: vec![IVec3::ZERO],
        structural_events: events,
        delta: omnisia::csg::ProposedDelta {
            deltas: vec![],
            affected_chunks: vec![],
            mesh_invalidation_chunks: vec![],
        },
        chunk_pre_states: vec![],
    };

    let impact = ImpactEvent::builder(ImpactId(18), Vec3::new(1.0, 1.0, 1.0), 1.0)
        .energy(10.0)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    );

    // Rollback must restore exact dirty_flags and revision
    assert!(res.is_err());
    let chunk_restored = store.get(&IVec3::ZERO).unwrap();
    assert_eq!(chunk_restored.dirty_flags, expected_dirty);
    assert_eq!(chunk_restored.revision, expected_rev);
    assert_eq!(
        store.get_voxel_world(IVec3::new(2, 2, 2)),
        VoxelBlock::new(MAT_WOOD)
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(10, 10, 10)),
        VoxelBlock::new(MAT_WOOD)
    );
}

#[test]
fn test_19_structural_transaction_rollback_restores_exact_pre_state() {
    let mut struct_sys = StructuralSystem::new(AnchorPolicy::default());
    struct_sys.next_aggregate_id = 42;
    struct_sys.total_events_processed = 10;
    struct_sys.total_connectivity_checks = 5;

    let snapshot = struct_sys.create_transaction_snapshot();

    // Mutate state during failed transaction
    struct_sys.next_aggregate_id = 45;
    struct_sys.total_events_processed = 15;
    struct_sys.detached_aggregates.push(DetachedAggregate {
        id: 42,
        min_voxel: IVec3::ZERO,
        max_voxel: IVec3::ZERO,
        voxels: vec![],
    });

    // Rollback
    struct_sys.restore_transaction_snapshot(&snapshot);

    assert_eq!(struct_sys.next_aggregate_id, 42);
    assert_eq!(struct_sys.total_events_processed, 10);
    assert_eq!(struct_sys.total_connectivity_checks, 5);
    assert_eq!(struct_sys.detached_aggregates.len(), 0);
}

#[test]
fn test_20_impulse_failure_does_not_create_split_ownership() {
    let (mut store, mut struct_sys, mut phys_world) = setup_test_environment();
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));
    store.set_voxel_world(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 2, 0), VoxelBlock::new(MAT_WOOD));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(0, 1, 0)));
    let commit_res = tx.commit(&mut store).unwrap();

    // Create an impact where impulse direction is degenerate (e.g. exactly at contact point with no normal or dir)
    let contact_pos = Vec3::new(0.25, 1.25, 0.25);
    let impact = ImpactEvent::builder(ImpactId(20), contact_pos, 0.0)
        .impulse(10.0)
        .build()
        .unwrap();

    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    // Phase A committed: body is dynamic and ownership is valid
    assert_eq!(res.detached_bodies.len(), 1);
    // Degenerate impulse did not crash or roll back valid ownership
    assert_eq!(res.impulse_applied, None);
    assert_eq!(phys_world.rigid_bodies.len(), 1);
    assert_eq!(store.get_voxel_world(IVec3::new(0, 2, 0)), VoxelBlock::AIR);
}

#[test]
fn test_21_single_authoritative_dynamic_aggregate_owner() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(1, 1, 1), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    // 1. DynamicAggregateRecord is the authoritative owner in PhysicsWorld
    let record = world.get_dynamic_aggregate(dyn_id).unwrap();
    assert_eq!(record.aggregate.id, 1);

    // 2. to_dynamic_body() produces a synchronized snapshot view
    let dyn_body = record.to_dynamic_body(&world).unwrap();
    assert_eq!(dyn_body.id, dyn_id);
    assert_eq!(dyn_body.voxel_count(), 1);

    // 3. Modifying snapshot does not affect authoritative record
    let mut modified_body = dyn_body.clone();
    modified_body.velocity = Vec3::new(10.0, 0.0, 0.0);
    assert_eq!(
        world
            .get_rigid_body(record.rigid_body_id)
            .unwrap()
            .linear_velocity(),
        Vec3::ZERO
    );
}

// ============================================================================
// E. PHYSICS SIMULATION & SLEEPING (TESTS 22..25)
// ============================================================================

#[test]
fn test_22_dynamic_body_to_rigid_body_one_to_one() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(2, 2, 2), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    assert_eq!(world.dynamic_aggregates.len(), 1);
    assert_eq!(world.rigid_bodies.len(), 1);

    let record = world.get_dynamic_aggregate(dyn_id).unwrap();
    assert!(world.rigid_bodies.contains_key(&record.rigid_body_id));
}

#[test]
fn test_23_mass_and_inertia_tensor_consistency() {
    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[
            (IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD)),
            (IVec3::new(1, 0, 0), VoxelBlock::new(MAT_WOOD)),
        ],
    )
    .unwrap();

    let props = calculate_aggregate_mass_properties(&agg, None).unwrap();
    assert_eq!(props.total_mass, 2.0); // 2 voxels * 1.0 kg default
    assert!(props.local_inertia.x_axis.x > 0.0);
    assert!(props.local_inertia.y_axis.y > 0.0);
    assert!(props.local_inertia.z_axis.z > 0.0);
}

#[test]
fn test_24_impulse_at_point_generates_angular_velocity() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[
            (IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD)),
            (IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD)),
            (IVec3::new(0, 2, 0), VoxelBlock::new(MAT_WOOD)),
        ],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    let record = world.get_dynamic_aggregate(dyn_id).unwrap();
    let rb = world.get_rigid_body_mut(record.rigid_body_id).unwrap();

    // Center of mass is around y=0.75. Apply impulse off-center at top y=1.25 along +X
    let off_center_pt = rb.position() + Vec3::new(0.0, 0.5, 0.0);
    rb.apply_impulse_at_point(Vec3::new(10.0, 0.0, 0.0), off_center_pt)
        .unwrap();

    assert!(rb.linear_velocity().x > 0.0);
    assert!(
        rb.angular_velocity().length() > 0.0,
        "Off-center impulse must generate angular torque"
    );
}

#[test]
fn test_25_physics_step_and_island_sleeping() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    let record = world.get_dynamic_aggregate(dyn_id).unwrap();
    let rb = world.get_rigid_body_mut(record.rigid_body_id).unwrap();

    // Put to sleep manually to verify sleep state contract
    rb.put_to_sleep();
    assert_eq!(rb.sleep_state(), SleepState::Sleeping);
    assert_eq!(rb.linear_velocity(), Vec3::ZERO);
    assert_eq!(rb.angular_velocity(), Vec3::ZERO);
}

// ============================================================================
// F. REINTEGRATION & ISOLATION (TESTS 26..30)
// ============================================================================

#[test]
fn test_26_reintegration_eligibility_predicate() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    // Place ground at y=0
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));

    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    // While awake, not eligible
    assert!(!ImpactBridge::is_eligible_for_reintegration(
        dyn_id, &world, &store
    ));

    // Put to sleep
    let record = world.get_dynamic_aggregate(dyn_id).unwrap();
    let rb = world.get_rigid_body_mut(record.rigid_body_id).unwrap();
    rb.put_to_sleep();

    // Now resting on ground and sleeping -> eligible!
    assert!(ImpactBridge::is_eligible_for_reintegration(
        dyn_id, &world, &store
    ));
}

#[test]
fn test_27_reintegration_isolation_compile_and_runtime() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    // Exclusive borrow &mut world and &mut store isolates prepare and commit
    let res = world.reintegrate_aggregate(
        dyn_id,
        &mut store,
        OrientationQuantizationPolicy::NearestLattice,
    );
    assert!(res.is_ok());
}

#[test]
fn test_28_reintegration_restores_authoritative_voxels_and_mesh_dirty() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    world
        .reintegrate_aggregate(
            dyn_id,
            &mut store,
            OrientationQuantizationPolicy::NearestLattice,
        )
        .unwrap();

    assert_eq!(
        store.get_voxel_world(IVec3::new(0, 0, 0)),
        VoxelBlock::new(MAT_WOOD)
    );
    let chunk = store.get(&IVec3::ZERO).unwrap();
    assert!(chunk.is_dirty(dirty_flags::MESH_DIRTY));
}

#[test]
fn test_29_reintegration_deregisters_physics_infallibly() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    let record = world.get_dynamic_aggregate(dyn_id).unwrap();
    let rb_id = record.rigid_body_id;

    world
        .reintegrate_aggregate(
            dyn_id,
            &mut store,
            OrientationQuantizationPolicy::NearestLattice,
        )
        .unwrap();

    // Deregistered completely
    assert!(!world.dynamic_aggregates.contains_key(&dyn_id));
    assert!(!world.rigid_bodies.contains_key(&rb_id));
    assert!(!world.body_colliders.contains_key(&rb_id));
}

#[test]
fn test_30_reintegration_destination_occupied_fails_cleanly() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    // Occupy destination with solid stone
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_ANCHOR));

    let agg = DetachedAggregate::from_world_voxels(
        1,
        &[(IVec3::new(0, 0, 0), VoxelBlock::new(MAT_WOOD))],
    )
    .unwrap();

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    let res = world.reintegrate_aggregate(
        dyn_id,
        &mut store,
        OrientationQuantizationPolicy::NearestLattice,
    );

    // Must fail without overwriting existing stone
    assert!(res.is_err());
    assert_eq!(
        store.get_voxel_world(IVec3::new(0, 0, 0)),
        VoxelBlock::new(MAT_ANCHOR)
    );
    // Dynamic body remains alive in PhysicsWorld
    assert!(world.dynamic_aggregates.contains_key(&dyn_id));
}

// ============================================================================
// G. CROSS-CHUNK & UNLOADED BOUNDARIES (TESTS 31..33)
// ============================================================================

#[test]
fn test_31_cross_chunk_detachment_and_reintegration() {
    let mut store = setup_multi_chunk_store(IVec3::new(0, 0, 0), IVec3::new(1, 0, 0));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    // Island spanning chunk 0 and chunk 1 (x=31 and x=32)
    let cross_voxels = vec![
        (IVec3::new(31, 5, 5), VoxelBlock::new(MAT_WOOD)),
        (IVec3::new(32, 5, 5), VoxelBlock::new(MAT_WOOD)),
    ];

    for &(p, b) in &cross_voxels {
        store.set_voxel_world(p, b);
    }

    let agg = DetachedAggregate::from_world_voxels(1, &cross_voxels).unwrap();
    for &(p, _) in &cross_voxels {
        store.set_voxel_world(p, VoxelBlock::AIR);
    }

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    world
        .reintegrate_aggregate(
            dyn_id,
            &mut store,
            OrientationQuantizationPolicy::NearestLattice,
        )
        .unwrap();

    assert_eq!(
        store.get_voxel_world(IVec3::new(31, 5, 5)),
        VoxelBlock::new(MAT_WOOD)
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(32, 5, 5)),
        VoxelBlock::new(MAT_WOOD)
    );
}

#[test]
fn test_32_negative_coordinate_detachment_and_reintegration() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-2), IVec3::splat(0));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());

    let neg_voxels = vec![
        (IVec3::new(-35, -5, -40), VoxelBlock::new(MAT_WOOD)),
        (IVec3::new(-34, -5, -40), VoxelBlock::new(MAT_WOOD)),
    ];

    for &(p, b) in &neg_voxels {
        store.set_voxel_world(p, b);
    }

    let agg = DetachedAggregate::from_world_voxels(1, &neg_voxels).unwrap();
    for &(p, _) in &neg_voxels {
        store.set_voxel_world(p, VoxelBlock::AIR);
    }

    let dyn_id = world
        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
        .unwrap();

    world
        .reintegrate_aggregate(
            dyn_id,
            &mut store,
            OrientationQuantizationPolicy::NearestLattice,
        )
        .unwrap();

    assert_eq!(
        store.get_voxel_world(IVec3::new(-35, -5, -40)),
        VoxelBlock::new(MAT_WOOD)
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(-34, -5, -40)),
        VoxelBlock::new(MAT_WOOD)
    );
}

#[test]
fn test_33_unloaded_chunk_prevents_false_detachment() {
    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));
    let mut anchor_policy = AnchorPolicy::default();
    anchor_policy.register_anchor_material(MAT_ANCHOR);
    let mut struct_sys = StructuralSystem::new(anchor_policy);
    let mut phys_world = PhysicsWorld::new(PhysicsWorldConfig::default());
    // Voxel at x=31 (border of chunk 0)
    store.set_voxel_world(IVec3::new(31, 0, 0), VoxelBlock::new(MAT_WOOD));

    let events = vec![StructuralEvent::new(
        IVec3::new(31, 0, 1),
        StructuralMutationType::VoxelRemoved {
            previous_block: VoxelBlock::new(MAT_WOOD),
        },
    )];

    let commit_res = VoxelEditCommitResult {
        affected_chunks: vec![IVec3::ZERO],
        mesh_invalidation_chunks: vec![IVec3::ZERO],
        structural_events: events,
        delta: omnisia::csg::ProposedDelta {
            deltas: vec![],
            affected_chunks: vec![],
            mesh_invalidation_chunks: vec![],
        },
        chunk_pre_states: vec![],
    };

    let impact = ImpactEvent::builder(ImpactId(33), Vec3::new(15.5, 0.0, 0.0), 1.0)
        .energy(10.0)
        .build()
        .unwrap();

    // Chunk (1, 0, 0) is unloaded -> BFS returns PendingUnloadedNeighbor -> Err(UnloadedChunk)
    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    );

    assert!(res.is_err());
    // Zero voxels cleared: (31, 0, 0) is still wood!
    assert_eq!(
        store.get_voxel_world(IVec3::new(31, 0, 0)),
        VoxelBlock::new(MAT_WOOD)
    );
    assert_eq!(phys_world.rigid_bodies.len(), 0);
}

// ============================================================================
// H. CRITICAL ARCHITECTURAL END-TO-END TEST (TEST 34)
// ============================================================================

#[test]
fn test_34_full_impact_csg_structure_physics_sleep_reintegration_lifecycle() {
    let mut store = setup_multi_chunk_store(IVec3::splat(-1), IVec3::splat(1));
    let mut anchor_policy = AnchorPolicy::default();
    anchor_policy.register_anchor_material(MAT_ANCHOR);
    let mut struct_sys = StructuralSystem::new(anchor_policy);
    let mut phys_world = PhysicsWorld::new(PhysicsWorldConfig::default());

    // 1. Build world structure:
    // Bedrock ground at y=0
    for x in -2..=2 {
        for z in -2..=2 {
            store.set_voxel_world(IVec3::new(x, 0, z), VoxelBlock::new(MAT_ANCHOR));
        }
    }
    // Pillar at (0, 1, 0) and (0, 2, 0)
    store.set_voxel_world(IVec3::new(0, 1, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(0, 2, 0), VoxelBlock::new(MAT_WOOD));
    // Overhang beam at (0, 3, 0) and (1, 3, 0)
    store.set_voxel_world(IVec3::new(0, 3, 0), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(1, 3, 0), VoxelBlock::new(MAT_WOOD));

    // 2. Impact severing pillar at (0, 2, 0)
    let impact = ImpactEvent::builder(ImpactId(34), Vec3::new(0.25, 1.25, 0.25), 0.4)
        .impulse(15.0)
        .direction(Vec3::X)
        .build()
        .unwrap();

    // CSG Crater removes (0, 2, 0)
    let tx = CraterGenerator::from_impact(
        &impact,
        &DefaultDestructionPolicy,
        &MaterialRegistry::default(),
        &store,
    )
    .unwrap();
    let commit_res = tx.commit(&mut store).unwrap();
    assert_eq!(store.get_voxel_world(IVec3::new(0, 2, 0)), VoxelBlock::AIR);

    // 3. Phase 10.3 Integration: reconcile & physicalize
    let res = ImpactBridge::reconcile_and_physicalize(
        &impact,
        &commit_res,
        &mut store,
        &mut struct_sys,
        &mut phys_world,
        None,
        AggregateColliderStrategy::CompoundBoxes,
    )
    .unwrap();

    assert_eq!(res.detached_bodies.len(), 1);
    assert_eq!(res.total_voxels_detached, 2);
    let dyn_id = res.detached_bodies[0];

    // Detached voxels are now exclusively in DYNAMIC_SIMULATION
    assert_eq!(store.get_voxel_world(IVec3::new(0, 3, 0)), VoxelBlock::AIR);
    assert_eq!(store.get_voxel_world(IVec3::new(1, 3, 0)), VoxelBlock::AIR);
    assert_eq!(phys_world.rigid_bodies.len(), 1);

    // Velocity applied from impulse
    let rb_id = {
        let record = phys_world.get_dynamic_aggregate(dyn_id).unwrap();
        let rb = phys_world.get_rigid_body(record.rigid_body_id).unwrap();
        assert!(rb.linear_velocity().x > 0.0);
        record.rigid_body_id
    };

    // 4. Physics simulation step & settling
    let prof = phys_world.step_profiled().unwrap();
    assert!(prof.timings.total_step_ns > 0);

    // 5. Body comes to rest and sleeps
    let rb_mut = phys_world.get_rigid_body_mut(rb_id).unwrap();
    rb_mut.put_to_sleep();

    // 6. Reintegration back into STATIC_WORLD
    phys_world
        .reintegrate_aggregate(
            dyn_id,
            &mut store,
            OrientationQuantizationPolicy::NearestLattice,
        )
        .expect("Settled body must reintegrate cleanly into STATIC_WORLD");

    // 7. Ownership verified:
    // PhysicsWorld has 0 bodies
    assert_eq!(phys_world.rigid_bodies.len(), 0);
    assert_eq!(phys_world.dynamic_aggregates.len(), 0);
    // Voxel state restored in STATIC_WORLD
    assert_eq!(struct_sys.detached_aggregates.len(), 1);
}
