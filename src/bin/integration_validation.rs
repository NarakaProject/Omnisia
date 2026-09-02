use std::time::Instant;

use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::material::MaterialId;
use omnisia::physics::DynamicBodyState;
use omnisia::player::PlayerController;
use omnisia::storage::{MemoryCompressedRegionStore, RegionStore};
use omnisia::structure::aggregate::DetachedAggregate;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;
use omnisia::worldgen::seed::WorldSeed;

fn main() {
    println!("================================================================================");
    println!("           OMNISIA — PHASE 8C INTEGRATION LAYER VALIDATION                      ");
    println!("================================================================================");

    let start_all = Instant::now();

    // ------------------------------------------------------------------------
    // STAGE 1: PLAYER <-> STATIC WORLD AUTHORITATIVE INTERACTION (8C.1)
    // ------------------------------------------------------------------------
    print!("Stage 1: Player <-> Static World Integration ... ");
    let mut world = World::with_seed(WorldSeed(101));
    let mut chunk = Chunk::new(IVec3::ZERO);
    for vx in 0..16 {
        for vz in 0..16 {
            chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
        }
    }
    world.store.insert(chunk);

    let mut player = PlayerController::new(Vec3::new(4.0, 3.0, 4.0));
    assert!(!player.state.grounded);

    // Jatuh dan mendarat di lantai statis (surface y = 0.5m)
    for _ in 0..60 {
        world.update_player(&mut player, 1.0 / 30.0, 0.0);
        if player.state.grounded {
            break;
        }
    }
    assert!(player.state.grounded);
    assert!((player.state.position.y - 0.5).abs() < 1e-3);
    println!("PASS (Grounded at y = {:.2}m)", player.state.position.y);

    // ------------------------------------------------------------------------
    // STAGE 2: PLAYER <-> DYNAMICBODY INTERACTION (8C.2)
    // ------------------------------------------------------------------------
    print!("Stage 2: Player <-> DynamicBody Interaction ... ");
    // Buat platform AntiGravity di y = 5.0m
    let mut voxels = Vec::new();
    for vx in 0..4 {
        for vz in 0..4 {
            voxels.push((IVec3::new(vx, 10, vz), VoxelBlock::new(MaterialId::STONE)));
        }
    }
    let agg = DetachedAggregate::from_world_voxels(201, &voxels).unwrap();
    let body_id = world.physics.spawn_from_detached_aggregate(agg);
    let body_mut = world.physics.get_body_mut(body_id).unwrap();
    body_mut.gravity_scale = 0.0;

    let mut dyn_player = PlayerController::new(Vec3::new(1.0, 6.0, 1.0));
    for _ in 0..30 {
        world.update_player(&mut dyn_player, 1.0 / 30.0, 0.0);
        if dyn_player.state.grounded {
            break;
        }
    }
    assert!(dyn_player.state.grounded);
    assert!((dyn_player.state.position.y - 5.5).abs() < 1e-3);
    println!(
        "PASS (Supported on DynamicBody at y = {:.2}m)",
        dyn_player.state.position.y
    );

    // ------------------------------------------------------------------------
    // STAGE 3: DYNAMICBODY <-> STATIC WORLD SWEPT COLLISION (8C.3)
    // ------------------------------------------------------------------------
    print!("Stage 3: DynamicBody <-> Static World Collision ... ");
    let voxels_fall = vec![(IVec3::new(8, 8, 8), VoxelBlock::new(MaterialId::STONE))];
    let agg_fall = DetachedAggregate::from_world_voxels(202, &voxels_fall).unwrap();
    let fall_id = world.physics.spawn_from_detached_aggregate(agg_fall);

    for _ in 0..60 {
        world.physics.tick(1.0 / 30.0, &world.store);
        if let Some(b) = world.physics.get_body(fall_id) {
            if b.is_grounded {
                break;
            }
        }
    }
    let body_fall = world.physics.get_body(fall_id).unwrap();
    assert!(body_fall.is_grounded);
    assert!((body_fall.position.y - 0.5).abs() < 1e-3);
    println!(
        "PASS (DynamicBody landed and snapped at y = {:.2}m)",
        body_fall.position.y
    );

    // ------------------------------------------------------------------------
    // STAGE 4: RUNTIME STRUCTURAL MUTATION & DYNAMIC BODY EMERGENCE (8C.4)
    // ------------------------------------------------------------------------
    print!("Stage 4: Runtime Structural Mutation & Detachment ... ");
    // Bangun pilar dengan overhang di (10, 1..=3, 10)
    for vy in 1..=3 {
        world
            .store
            .set_voxel_world(IVec3::new(10, vy, 10), VoxelBlock::new(MaterialId::STONE));
    }
    world
        .store
        .set_voxel_world(IVec3::new(10, 4, 10), VoxelBlock::new(MaterialId::STONE));
    world
        .store
        .set_voxel_world(IVec3::new(11, 4, 10), VoxelBlock::new(MaterialId::STONE));

    let prev_bodies = world.physics.body_count();
    let detached = world.set_voxel_world(IVec3::new(10, 1, 10), VoxelBlock::AIR);
    assert!(!detached.is_empty());
    assert_eq!(world.physics.body_count(), prev_bodies + detached.len());
    println!(
        "PASS ({} aggregates detached into DynamicBodies)",
        detached.len()
    );

    // ------------------------------------------------------------------------
    // STAGE 5: SYSTEM-WIDE OWNERSHIP INTEGRITY AUDIT (8C.5)
    // ------------------------------------------------------------------------
    print!("Stage 5: Voxel Ownership Audit & Zero Duplicate Detection ... ");
    let audit = world.audit_world_ownership();
    assert_eq!(audit.duplicate_detections, 0);
    assert!(audit.total_dynamic_voxels > 0);
    assert!(audit.total_static_voxels > 0);
    println!(
        "PASS (Static: {}, Dynamic: {}, Duplicates: 0)",
        audit.total_static_voxels, audit.total_dynamic_voxels
    );

    // ------------------------------------------------------------------------
    // STAGE 6: PERSISTENCE & TWO-PHASE REINTEGRATION (8C.6)
    // ------------------------------------------------------------------------
    print!("Stage 6: Two-Phase Reintegration & Persistence Palette Roundtrip ... ");
    let mut chunk_edge = Chunk::new(IVec3::new(1, 0, 0));
    chunk_edge.dirty_flags = 0;
    world.store.insert(chunk_edge);

    // Badan dinamis tepat di perbatasan x = 32 (local_x = 0 chunk 1,0,0)
    let voxels_reint = vec![(IVec3::new(32, 2, 2), VoxelBlock::new(MaterialId::OAK_WOOD))];
    let agg_reint = DetachedAggregate::from_world_voxels(203, &voxels_reint).unwrap();
    let reint_id = world.physics.spawn_from_detached_aggregate(agg_reint);
    world.physics.get_body_mut(reint_id).unwrap().state = DynamicBodyState::Settled;

    let reintegrated = world
        .physics
        .process_settled_reintegration(&mut world.store);
    assert_eq!(reintegrated.len(), 1);

    // Verifikasi propagasi dirty flags ke tetangga chunk (0,0,0) pada batas x = 31
    let neighbor = world.store.get(&IVec3::ZERO).unwrap();
    assert!(neighbor.dirty_flags & dirty_flags::MESH_DIRTY != 0);

    // Simpan ke storage dan muat kembali
    let storage = MemoryCompressedRegionStore::new();
    let chunk_target = world.store.get(&IVec3::new(1, 0, 0)).unwrap();
    storage.save_chunk(chunk_target, &world.materials).unwrap();
    let loaded = storage
        .load_chunk(IVec3::new(1, 0, 0), &world.materials)
        .unwrap()
        .unwrap();

    let block_loaded = loaded.get_voxel(0, 2, 2);
    assert_eq!(block_loaded.material, MaterialId::OAK_WOOD);
    println!("PASS (Settled body reintegrated and saved/loaded with 100% fidelity)");

    println!("================================================================================");
    println!("           PHASE 8C INTEGRATION VALIDATION: ALL STAGES PASSED!                  ");
    println!("           Elapsed: {:.2?}", start_all.elapsed());
    println!("================================================================================");
}
