use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::csg::transaction::VoxelEditTransaction;
use omnisia::interaction::{
    can_place, can_remove, execute_interaction_transaction, handle_player_interaction,
    InteractionAction, InteractionCooldown, InteractionMutationError, VoxelHit,
    DEFAULT_INTERACTION_REACH,
};
use omnisia::material::MaterialId;
use omnisia::mesh::types::FaceDirection;
use omnisia::modding::ResourceId;
use omnisia::player::PlayerController;
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;

const TEST_STONE: MaterialId = MaterialId(1);
const TEST_DIRT: MaterialId = MaterialId(2);

fn create_test_store() -> ChunkStore {
    ChunkStore::new()
}

fn add_empty_chunk(store: &mut ChunkStore, coord: IVec3) {
    let mut chunk = Chunk::new(coord);
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
}

// ============================================================================
// 1. VOXEL REMOVAL TESTS
// ============================================================================

#[test]
fn test_voxel_removal_success() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    let target = IVec3::new(4, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(2.0, 0.25, 0.25),
        distance: 1.75,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let edit = can_remove(&world.store, &hit, DEFAULT_INTERACTION_REACH)
        .expect("Valid removal should succeed");
    assert_eq!(edit.position, target);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let result =
        execute_interaction_transaction(&mut world, &tx).expect("Commit should execute cleanly");
    assert_eq!(result.commit_result.delta.len(), 1);

    // Target voxel harus berubah menjadi AIR
    assert!(world.store.get_voxel_world(target).is_air());
}

#[test]
fn test_voxel_removal_air_fails() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(4, 0, 0);
    // Voxel dibiarkan AIR

    let hit = VoxelHit {
        voxel_coord: target,
        material: MaterialId(0),
        hit_point: Vec3::new(2.0, 0.25, 0.25),
        distance: 1.75,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err = can_remove(&store, &hit, DEFAULT_INTERACTION_REACH).unwrap_err();
    assert_eq!(
        err,
        InteractionMutationError::RemovalTargetIsAir { coord: target }
    );
}

#[test]
fn test_voxel_removal_reach_boundaries() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(10, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // 1. Jarak di luar reach (5.01m > 5.0m)
    let hit_beyond = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(5.0, 0.25, 0.25),
        distance: 5.01,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };
    let err = can_remove(&store, &hit_beyond, 5.0).unwrap_err();
    assert_eq!(
        err,
        InteractionMutationError::ExceedsReach {
            distance: 5.01,
            max_reach: 5.0
        }
    );

    // 2. Jarak tepat pada batas reach (5.0m == 5.0m) -> SUKSES (inklusif)
    let hit_exact = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(5.0, 0.25, 0.25),
        distance: 5.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };
    assert!(can_remove(&store, &hit_exact, 5.0).is_ok());
}

#[test]
fn test_voxel_removal_non_resident_fails() {
    let store = create_test_store();
    // Tidak ada chunk resident sama sekali

    let hit = VoxelHit {
        voxel_coord: IVec3::new(4, 0, 0),
        material: TEST_STONE,
        hit_point: Vec3::new(2.0, 0.25, 0.25),
        distance: 1.75,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err = can_remove(&store, &hit, 5.0).unwrap_err();
    assert_eq!(
        err,
        InteractionMutationError::TargetNotResident {
            coord: IVec3::new(4, 0, 0)
        }
    );
}

#[test]
fn test_voxel_removal_negative_coordinates() {
    let mut world = World::new();
    // Chunk (-1, -1, -1)
    world.store.insert(Chunk::new(IVec3::new(-1, -1, -1)));

    let target = IVec3::new(-4, -2, -6);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(-1.5, -0.75, -2.75),
        distance: 1.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };

    let edit =
        can_remove(&world.store, &hit, 5.0).expect("Removal in negative coords should succeed");
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let res = execute_interaction_transaction(&mut world, &tx).unwrap();
    assert_eq!(res.commit_result.delta.len(), 1);
    assert!(world.store.get_voxel_world(target).is_air());
}

// ============================================================================
// 2. VOXEL PLACEMENT TESTS
// ============================================================================

#[test]
fn test_voxel_placement_success() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    let target = IVec3::new(4, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    // Player di posisi yang tidak mengganggu penempatan
    let player = PlayerController::new(Vec3::new(0.0, 10.0, 0.0));

    // Ray menabrak sisi -X dari voxel (4, 0, 0)
    // Sisi normal = (-1, 0, 0), kandidat penempatan = (3, 0, 0)
    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(2.0, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let edit =
        can_place(&world.store, &hit, TEST_DIRT, &player, 5.0).expect("Placement should be valid");
    assert_eq!(edit.position, IVec3::new(3, 0, 0));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    execute_interaction_transaction(&mut world, &tx).unwrap();

    let placed_block = world.store.get_voxel_world(IVec3::new(3, 0, 0));
    assert_eq!(placed_block.material(), TEST_DIRT);
}

#[test]
fn test_voxel_placement_into_occupied_fails() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(4, 0, 0);
    let neighbor = IVec3::new(3, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));
    store.set_voxel_world(neighbor, VoxelBlock::new(TEST_DIRT)); // Sudah terisi solid

    let player = PlayerController::new(Vec3::new(0.0, 10.0, 0.0));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(2.0, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err = can_place(&store, &hit, TEST_STONE, &player, 5.0).unwrap_err();
    assert_eq!(
        err,
        InteractionMutationError::PlacementOccupied {
            coord: neighbor,
            current: VoxelBlock::new(TEST_DIRT)
        }
    );
}

#[test]
fn test_voxel_placement_air_material_fails() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(4, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let player = PlayerController::new(Vec3::new(0.0, 10.0, 0.0));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(2.0, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err = can_place(&store, &hit, MaterialId(0), &player, 5.0).unwrap_err();
    match err {
        InteractionMutationError::InvalidMaterial(_) => {}
        _ => panic!("Expected InvalidMaterial error"),
    }
}

#[test]
fn test_voxel_placement_cross_chunk_boundary() {
    let mut world = World::new();
    // Chunk 0: voxels [0..31]
    // Chunk 1: voxels [32..63]
    world.store.insert(Chunk::new(IVec3::ZERO));
    world.store.insert(Chunk::new(IVec3::new(1, 0, 0)));

    // Blok target di ujung paling kanan Chunk 0 (voxel 31)
    let target = IVec3::new(31, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let player = PlayerController::new(Vec3::new(0.0, 10.0, 0.0));

    // Menabrak sisi +X (normal [1, 0, 0])
    // Kandidat penempatan adalah voxel 32 (voxel 0 dari Chunk 1!)
    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(16.0, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };

    let edit = can_place(&world.store, &hit, TEST_DIRT, &player, 5.0)
        .expect("Cross-chunk placement should succeed");
    assert_eq!(edit.position, IVec3::new(32, 0, 0));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let res = execute_interaction_transaction(&mut world, &tx).unwrap();

    // Chunk 1 termutasi
    assert_eq!(res.commit_result.affected_chunks, vec![IVec3::new(1, 0, 0)]);
    assert_eq!(
        world.store.get_voxel_world(IVec3::new(32, 0, 0)).material(),
        TEST_DIRT
    );
}

#[test]
fn test_voxel_placement_unloaded_destination_fails() {
    let mut store = create_test_store();
    // Hanya Chunk 0 yang resident
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(31, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let player = PlayerController::new(Vec3::new(0.0, 10.0, 0.0));

    // Menabrak sisi +X menuju Chunk 1 (belum resident!)
    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(16.0, 0.25, 0.25),
        distance: 2.0,
        face: FaceDirection::PosX,
        normal: Vec3::X,
    };

    let err = can_place(&store, &hit, TEST_DIRT, &player, 5.0).unwrap_err();
    assert_eq!(
        err,
        InteractionMutationError::DestinationNotResident {
            coord: IVec3::new(32, 0, 0)
        }
    );
}

// ============================================================================
// 3. PLAYER CAPSULE OVERLAP GUARD TESTS
// ============================================================================

#[test]
fn test_placement_capsule_overlap_standing_fails() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    // Lantai pada Y = -1
    let floor = IVec3::new(0, -1, 0);
    store.set_voxel_world(floor, VoxelBlock::new(TEST_STONE));

    // Pemain berdiri di (0.25, 0.0, 0.25)
    // Kapsul berdiri: base = (0.25, 0.0, 0.25), radius = 0.30, height = 1.80
    // Menempati rentang Y: [0.0, 1.80], X: [-0.05, 0.55], Z: [-0.05, 0.55]
    let player = PlayerController::new(Vec3::new(0.25, 0.0, 0.25));

    // Balok pendukung di samping pemain pada (1, 0, 0)
    let side_block = IVec3::new(1, 0, 0);
    store.set_voxel_world(side_block, VoxelBlock::new(TEST_STONE));

    // Mencoba menempatkan balok pada sisi -X dari balok samping
    // Kandidat penempatan = (1 - 1, 0, 0) = (0, 0, 0)!
    // Voxel (0, 0, 0) berada pada bounds [0.0..0.5] pada X, Y, Z
    // Ini tepat berada di dalam ruang kaki/badan pemain!
    let hit = VoxelHit {
        voxel_coord: side_block,
        material: TEST_STONE,
        hit_point: Vec3::new(0.5, 0.25, 0.25),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err = can_place(&store, &hit, TEST_DIRT, &player, 5.0).unwrap_err();
    assert_eq!(
        err,
        InteractionMutationError::PlayerCapsuleOverlap {
            coord: IVec3::new(0, 0, 0)
        },
        "Penempatan voxel di dalam kapsul pemain harus ditolak secara tegas!"
    );
}

#[test]
fn test_placement_capsule_crouching_allows_head_space() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let side_block = IVec3::new(1, 2, 0);
    store.set_voxel_world(side_block, VoxelBlock::new(TEST_STONE));

    // Pemain berdiri di (0.25, 0.0, 0.25)
    let standing_player = PlayerController::new(Vec3::new(0.25, 0.0, 0.25));

    // Pemain jongkok di (0.25, 0.0, 0.25)
    // Tinggi jongkok = 1.20m (puncak kepala di Y = 1.20m)
    let mut crouching_player = PlayerController::new(Vec3::new(0.25, 0.0, 0.25));
    crouching_player.state.crouching = true;

    // Kandidat penempatan pada voxel (0, 2, 0) -> Y in [1.0, 1.5]
    // Untuk standing player (height 1.80m, puncak Y = 1.80m), Y in [1.0, 1.5] beririsan!
    // Untuk crouching player (height 1.20m, puncak Y = 1.20m), apakah beririsan?
    // Pada Y = 1.20m, belahan bola atas berpusat di 1.20 - 0.30 = 0.90m.
    // Puncak bola tepat di Y = 1.20m. Voxel [1.0..1.5] memiliki rentang Y [1.0, 1.5] yang overlap di [1.0..1.20].
    // Mari uji voxel pada Y = 3 -> Y in [1.5, 2.0]:
    // Standing player: puncak di 1.80m -> OVERLAP dengan [1.5, 2.0]!
    // Crouching player: puncak di 1.20m -> TIDAK OVERLAP dengan [1.5, 2.0] (1.20 < 1.5)!

    let side_block_high = IVec3::new(1, 3, 0);
    store.set_voxel_world(side_block_high, VoxelBlock::new(TEST_STONE));

    let hit_high = VoxelHit {
        voxel_coord: side_block_high,
        material: TEST_STONE,
        hit_point: Vec3::new(0.5, 1.75, 0.25),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    // 1. Standing player: tinggi 1.80m menabrak voxel Y in [1.5, 2.0] -> FAIL
    let err_standing = can_place(&store, &hit_high, TEST_DIRT, &standing_player, 5.0).unwrap_err();
    assert_eq!(
        err_standing,
        InteractionMutationError::PlayerCapsuleOverlap {
            coord: IVec3::new(0, 3, 0)
        }
    );

    // 2. Crouching player: tinggi 1.20m berada di bawah 1.5m -> SUCCEED
    let res_crouching = can_place(&store, &hit_high, TEST_DIRT, &crouching_player, 5.0);
    assert!(
        res_crouching.is_ok(),
        "Pemain jongkok memiliki headroom bebas dan harus diizinkan menempatkan balok di atas kepalanya"
    );
}

#[test]
fn test_placement_capsule_tangent_boundary_succeeds() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    // Pemain di (0.25, 0.0, 0.25). Radius = 0.30m
    // Batas kapsul di sumbu X: [0.25 - 0.30, 0.25 + 0.30] = [-0.05, 0.55]
    // Voxel (2, 0, 0) ada di X: [1.0, 1.5] -> Jarak horizontal > 0.45m -> Tidak beririsan!
    let player = PlayerController::new(Vec3::new(0.25, 0.0, 0.25));

    let anchor = IVec3::new(3, 0, 0);
    store.set_voxel_world(anchor, VoxelBlock::new(TEST_STONE));

    let hit = VoxelHit {
        voxel_coord: anchor,
        material: TEST_STONE,
        hit_point: Vec3::new(1.5, 0.25, 0.25),
        distance: 1.5,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let res = can_place(&store, &hit, TEST_DIRT, &player, 5.0);
    assert!(
        res.is_ok(),
        "Penempatan di luar radius kapsul harus berhasil"
    );
    assert_eq!(res.unwrap().position, IVec3::new(2, 0, 0));
}

// ============================================================================
// 4. ATOMICITY & TRANSACTION INTEGRITY
// ============================================================================

#[test]
fn test_atomicity_failed_validation_leaves_world_unchanged() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    let target = IVec3::new(4, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(TEST_STONE));
    let initial_revision = world.store.get(&IVec3::ZERO).unwrap().revision;

    // Buat proposal invalid (misal Add pada balok yang sudah solid)
    let mut invalid_tx = VoxelEditTransaction::new();
    invalid_tx.add_edit(omnisia::csg::VoxelEdit::add(
        target,
        VoxelBlock::new(TEST_DIRT),
    ));

    let err = execute_interaction_transaction(&mut world, &invalid_tx).unwrap_err();
    match err {
        InteractionMutationError::TransactionError(_) => {}
        _ => panic!("Expected TransactionError"),
    }

    // Dunia harus 100% utuh tanpa perubahan apa pun
    assert_eq!(
        world.store.get_voxel_world(target),
        VoxelBlock::new(TEST_STONE)
    );
    assert_eq!(
        world.store.get(&IVec3::ZERO).unwrap().revision,
        initial_revision,
        "Revisi chunk tidak boleh berubah bila validasi gagal"
    );
}

// ============================================================================
// 5. STRUCTURE & PHYSICS INTEGRATION
// ============================================================================

#[test]
fn test_removal_structural_detachment_into_physics() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    let stone_id = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = world
        .materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    // Buat pilar vertikal:
    // Bedrock anchor pada Y = 0
    // Tiang batu pada Y = 1
    // Tiang kayu pada Y = 2
    world.set_voxel_world(IVec3::new(5, 0, 5), VoxelBlock::new(MaterialId(255))); // Bedrock
    world.set_voxel_world(IVec3::new(5, 1, 5), VoxelBlock::new(stone_id));
    world.set_voxel_world(IVec3::new(5, 2, 5), VoxelBlock::new(wood_id));

    assert_eq!(world.physics.bodies.len(), 0);

    // Sekarang hapus tiang tumpuan di Y = 1 menggunakan transaksi interaksi
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(omnisia::csg::VoxelEdit::remove(IVec3::new(5, 1, 5)));

    let result = execute_interaction_transaction(&mut world, &tx).unwrap();

    // Verifikasi gugusan lepas diekstraksi
    assert_eq!(
        result.newly_detached_aggregates.len(),
        1,
        "Voxel kayu di Y=2 harus terlepas menjadi detached aggregate!"
    );

    // Dan terdaftar langsung di PhysicsWorld sebagai DynamicBody
    assert_eq!(
        world.physics.bodies.len(),
        1,
        "DynamicBody harus terdaftar langsung di runtime fisika!"
    );
}

// ============================================================================
// 6. REMESHING & NEIGHBOR INVALIDATION
// ============================================================================

#[test]
fn test_remesh_dirty_flags_and_neighbor_invalidation() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));
    world.store.insert(Chunk::new(IVec3::new(-1, 0, 0)));

    // Tempatkan voxel pada border X = 0 (local.x == 0)
    world.set_voxel_world(IVec3::new(0, 5, 5), VoxelBlock::new(TEST_STONE));
    if let Some(c) = world.store.get_mut(&IVec3::ZERO) {
        c.clear_dirty(dirty_flags::ALL);
    }
    if let Some(c) = world.store.get_mut(&IVec3::new(-1, 0, 0)) {
        c.clear_dirty(dirty_flags::ALL);
    }
    world.store.dirty_mesh_chunks.clear();

    // Hapus voxel pada batas chunk X = 0
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(omnisia::csg::VoxelEdit::remove(IVec3::new(0, 5, 5)));

    execute_interaction_transaction(&mut world, &tx).unwrap();

    // Chunk (0, 0, 0) dan neighbor (-1, 0, 0) keduanya harus ditandai MESH_DIRTY
    let chunk_0 = world.store.get(&IVec3::ZERO).unwrap();
    let chunk_neg = world.store.get(&IVec3::new(-1, 0, 0)).unwrap();

    assert!(chunk_0.is_dirty(dirty_flags::MESH_DIRTY));
    assert!(chunk_neg.is_dirty(dirty_flags::MESH_DIRTY));
    assert!(world.store.dirty_mesh_chunks.contains(&IVec3::ZERO));
    assert!(world
        .store
        .dirty_mesh_chunks
        .contains(&IVec3::new(-1, 0, 0)));
}

// ============================================================================
// 7. DETERMINISM TESTS
// ============================================================================

#[test]
fn test_determinism_repeated_mutations() {
    let mut store = create_test_store();
    add_empty_chunk(&mut store, IVec3::ZERO);

    let target = IVec3::new(4, 0, 0);
    store.set_voxel_world(target, VoxelBlock::new(TEST_STONE));

    let hit = VoxelHit {
        voxel_coord: target,
        material: TEST_STONE,
        hit_point: Vec3::new(2.0, 0.25, 0.25),
        distance: 1.75,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    // 100 repetisi validasi harus menghasilkan proposal identik
    let first = can_remove(&store, &hit, 5.0).unwrap();
    for _ in 0..100 {
        let repeat = can_remove(&store, &hit, 5.0).unwrap();
        assert_eq!(first, repeat);
    }
}

// ============================================================================
// 8. COOLDOWN & DEBOUNCE TESTS
// ============================================================================

#[test]
fn test_cooldown_debounce_rate_limiting() {
    let mut world = World::new();
    world.store.insert(Chunk::new(IVec3::ZERO));

    // Buat baris balok di depan mata pemain (Y = 3 -> bounds [1.5, 2.0], eye_height = 1.62m)
    for x in 1..=5 {
        world
            .store
            .set_voxel_world(IVec3::new(x, 3, 0), VoxelBlock::new(TEST_STONE));
    }

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let look_dir = Vec3::X; // Menghadap langsung ke baris balok

    let mut cooldown = InteractionCooldown::new(0.20);
    assert!(cooldown.can_act());

    // 1. Aksi pertama berhasil
    let res1 = handle_player_interaction(
        &mut world,
        &player,
        look_dir,
        InteractionAction::RemoveVoxel,
        &mut cooldown,
    );
    assert!(res1.is_ok(), "Aksi pertama harus berhasil");
    assert!(!cooldown.can_act(), "Cooldown harus terkunci setelah aksi");

    // 2. Aksi kedua seketika (tanpa jeda waktu dt) harus ditolak
    let res2 = handle_player_interaction(
        &mut world,
        &player,
        look_dir,
        InteractionAction::RemoveVoxel,
        &mut cooldown,
    );
    assert!(
        matches!(res2, Err(InteractionMutationError::CooldownActive { .. })),
        "Spam aksi berulang dalam 1 frame harus ditolak oleh cooldown debounce"
    );

    // 3. Majukan waktu sebesar 0.10s (setengah cooldown) -> masih terkunci
    cooldown.tick(0.10);
    assert!(!cooldown.can_act());

    // 4. Majukan waktu lagi sebesar 0.11s (total 0.21s > 0.20s) -> diizinkan kembali
    cooldown.tick(0.11);
    assert!(cooldown.can_act());

    let res3 = handle_player_interaction(
        &mut world,
        &player,
        look_dir,
        InteractionAction::RemoveVoxel,
        &mut cooldown,
    );
    assert!(
        res3.is_ok(),
        "Aksi kedua setelah cooldown berlalu harus berhasil"
    );
}
