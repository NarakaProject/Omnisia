use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::csg::edit::VoxelEdit;
use omnisia::csg::transaction::VoxelEditTransaction;
use omnisia::interaction::{
    calculate_yield, can_gather, execute_gather_transaction, handle_player_gather,
    resolve_resource, validate_gather_action, CollectionResult, GatheringError,
    InteractionCooldown, ResourceDefinition, VoxelHit, DEFAULT_INTERACTION_REACH,
};
use omnisia::material::MaterialId;
use omnisia::mesh::types::FaceDirection;
use omnisia::modding::resource_id::ResourceId;
use omnisia::player::PlayerController;
use omnisia::streaming::store::ChunkStore;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;

fn add_empty_chunk(store: &mut ChunkStore, coord: IVec3) {
    let mut chunk = Chunk::new(coord);
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
}

// ============================================================================
// 1. RESOURCE IDENTITY TESTS
// ============================================================================

#[test]
fn test_resource_definition_resolves_correctly() {
    let world = World::new();
    let stone_res_id = ResourceId::core("stone").unwrap();
    let stone_mat = world
        .materials
        .resolve_material_id(&stone_res_id)
        .expect("core:stone material should be registered");

    let res_def = resolve_resource(&world.resources, stone_mat)
        .expect("core:stone should resolve to ResourceDefinition");

    assert_eq!(res_def.resource_id, stone_res_id);
    assert!(res_def.harvestable);
    assert_eq!(res_def.base_yield, 1);
    assert_eq!(
        res_def.source_block,
        Some(ResourceId::core("stone_block").unwrap())
    );

    // Resolusi via ResourceId persisten
    let by_id = world
        .resources
        .get_by_resource_id(&stone_res_id)
        .expect("Should resolve by persistent ResourceId");
    assert_eq!(by_id.resource_id, stone_res_id);
}

#[test]
fn test_resource_identity_is_stable() {
    let world = World::new();
    let iron_res_id = ResourceId::core("iron_ore").unwrap();
    let iron_mat = world
        .materials
        .resolve_material_id(&iron_res_id)
        .expect("core:iron_ore material should be registered");

    let def1 = world
        .resources
        .get_by_material(iron_mat)
        .expect("First lookup should find iron_ore");
    let def2 = world
        .resources
        .get_by_material(iron_mat)
        .expect("Second lookup should find iron_ore");

    assert_eq!(def1, def2);
    assert_eq!(def1.resource_id.to_string(), "core:iron_ore");
}

#[test]
fn test_resource_mapping_is_data_driven() {
    let world = World::new();

    // Verifikasi bahwa mapping berasal dari data BlockRegistry yang memuat file JSON
    let stone_block = world
        .blocks
        .get_by_resource_id(&ResourceId::core("stone_block").unwrap())
        .expect("stone_block must be in BlockRegistry");
    assert!(stone_block.components.harvestable.is_some());

    let iron_block = world
        .blocks
        .get_by_resource_id(&ResourceId::core("iron_ore_block").unwrap())
        .expect("iron_ore_block must be in BlockRegistry");
    assert!(iron_block.components.harvestable.is_some());
}

#[test]
fn test_unmapped_block_returns_no_resource() {
    let world = World::new();
    let casing_res_id = ResourceId::core("ag_core_casing").unwrap();
    let casing_mat = world
        .materials
        .resolve_material_id(&casing_res_id)
        .expect("ag_core_casing material should be registered");

    // ag_core_casing_block tidak memiliki harvestable component
    assert!(!world.resources.is_harvestable(casing_mat));
    assert!(world.resources.get_by_material(casing_mat).is_none());
}

// ============================================================================
// 2. HARVESTABILITY TESTS
// ============================================================================

#[test]
fn test_harvestable_resource_can_be_gathered() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_res_id = ResourceId::core("stone").unwrap();
    let stone_mat = world.materials.resolve_material_id(&stone_res_id).unwrap();

    let target = IVec3::new(2, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(1.0, 0.25, 0.25),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect("Harvestable stone should be valid to gather");

    assert_eq!(def.resource_id, stone_res_id);
    assert_eq!(edit.position, target);
}

#[test]
fn test_air_cannot_be_gathered() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let target = IVec3::new(2, 0, 0);
    // Voxel target dibiarkan AIR

    let hit = VoxelHit {
        voxel_coord: target,
        material: MaterialId::AIR,
        hit_point: Vec3::new(1.0, 0.25, 0.25),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err =
        can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).expect_err("Gathering air must fail");

    assert_eq!(err, GatheringError::TargetIsAir { coord: target });
    assert!(world.store.get_voxel_world(target).is_air());
}

#[test]
fn test_non_harvestable_block_cannot_be_gathered() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let casing_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("ag_core_casing").unwrap())
        .unwrap();

    let target = IVec3::new(3, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(casing_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: casing_mat,
        hit_point: Vec3::new(1.5, 0.25, 0.25),
        distance: 1.5,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect_err("Non-harvestable block must fail to gather");

    assert!(matches!(err, GatheringError::NotHarvestable { .. }));
    // Voxel tetap utuh
    assert_eq!(world.store.get_voxel_world(target).material(), casing_mat);
}

#[test]
fn test_unloaded_target_cannot_be_gathered() {
    let world = World::new();
    // Chunk (10, 0, 10) tidak resident
    let target = IVec3::new(320, 0, 320);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(160.0, 0.25, 160.0),
        distance: 2.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err =
        can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).expect_err("Unloaded target must fail");

    assert_eq!(err, GatheringError::TargetNotResident { coord: target });
}

// ============================================================================
// 3. YIELD & DETERMINISM TESTS
// ============================================================================

#[test]
fn test_yield_quantity_is_correct() {
    let world = World::new();
    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let def = world.resources.get_by_material(stone_mat).unwrap();
    let yield_val = calculate_yield(def);
    assert_eq!(yield_val, 1);
}

#[test]
fn test_repeated_equivalent_gathers_produce_deterministic_yield() {
    let world = World::new();
    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let def = world.resources.get_by_material(stone_mat).unwrap();

    for _ in 0..100 {
        assert_eq!(calculate_yield(def), 1);
    }
}

#[test]
fn test_no_uncontrolled_randomness_exists() {
    let def = ResourceDefinition::new(ResourceId::core("custom_gem").unwrap(), 5);
    let mut outputs = Vec::new();
    for _ in 0..50 {
        outputs.push(calculate_yield(&def));
    }
    assert!(outputs.iter().all(|&y| y == 5));
}

// ============================================================================
// 4. REACH & TARGETING TESTS
// ============================================================================

#[test]
fn test_in_range_target_gathers() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(2, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(1.0, 0.25, 0.25),
        distance: 4.99,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    assert!(can_gather(&world, &hit, 5.0).is_ok());
}

#[test]
fn test_beyond_reach_target_fails() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(12, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(6.0, 0.25, 0.25),
        distance: 5.01,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let err = can_gather(&world, &hit, 5.0).expect_err("Beyond reach must fail");
    assert!(matches!(err, GatheringError::ExceedsReach { .. }));
}

#[test]
fn test_target_uses_phase_11_1_interaction_semantics() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    // Tempatkan balok tepat di depan pandangan pemain
    let target = IVec3::new(2, 2, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    // Eye position standing: (0.0, 1.62, 0.0). Voxel target ada di (2, 2, 0) = [1.0, 1.0, 0.0]..[1.5, 1.5, 0.5]
    // Arahkan sedikit ke atas dan kanan
    let eye = player.eye_position();
    let voxel_center = Vec3::new(1.25, 1.25, 0.25);
    let look_dir = (voxel_center - eye).normalize();

    let (def, tx) = validate_gather_action(&world, &player, look_dir)
        .expect("Raycast based gather validation should succeed");

    assert_eq!(def.resource_id, ResourceId::core("stone").unwrap());
    assert_eq!(tx.len(), 1);
    assert_eq!(tx.edits()[0].position, target);
}

// ============================================================================
// 5. MUTATION & ATOMICITY TESTS
// ============================================================================

#[test]
fn test_successful_gathering_removes_voxel() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(3, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(1.5, 0.25, 0.25),
        distance: 1.5,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let result = execute_gather_transaction(&mut world, target, &def, &tx)
        .expect("Gathering execution must succeed");

    // Voxel harus terhapus menjadi AIR
    assert!(world.store.get_voxel_world(target).is_air());

    // CollectionResult harus dihasilkan secara tepat
    assert_eq!(result.collection.source_coord, target);
    assert_eq!(
        result.collection.resource_id,
        ResourceId::core("stone").unwrap()
    );
    assert_eq!(result.collection.quantity, 1);
}

#[test]
fn test_failed_gathering_leaves_voxel_unchanged() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(3, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(1.5, 0.25, 0.25),
        distance: 5.5, // Exceeds reach
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let res = can_gather(&world, &hit, 5.0);
    assert!(res.is_err());

    // Voxel tidak boleh termutasi
    assert_eq!(world.store.get_voxel_world(target).material(), stone_mat);
}

#[test]
fn test_atomicity_preflight_failure_produces_zero_mutation_and_zero_collection() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(4, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let initial_rev = world.store.get(&IVec3::ZERO).unwrap().revision;

    // Buat transaksi dengan target yang valid dan target di chunk yang tidak resident
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(target));
    tx.add_edit(VoxelEdit::remove(IVec3::new(100, 100, 100))); // Unloaded chunk!

    let def = ResourceDefinition::new(ResourceId::core("stone").unwrap(), 1);

    let exec_res = execute_gather_transaction(&mut world, target, &def, &tx);
    assert!(
        exec_res.is_err(),
        "Transaction with unresident edit must fail preflight"
    );

    // Integritas atomik: target awal TIDAK BOLEH termutasi, revisi TIDAK berubah
    assert_eq!(world.store.get_voxel_world(target).material(), stone_mat);
    assert_eq!(world.store.get(&IVec3::ZERO).unwrap().revision, initial_rev);
}

// ============================================================================
// 6. COORDINATE TESTS (NEGATIVE & CHUNK BOUNDARY)
// ============================================================================

#[test]
fn test_negative_world_coordinates() {
    let mut world = World::new();
    let neg_chunk = IVec3::new(-1, 0, -1);
    add_empty_chunk(&mut world.store, neg_chunk);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(-5, 2, -10);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(-2.5, 1.25, -5.0),
        distance: 2.0,
        face: FaceDirection::NegZ,
        normal: Vec3::new(0.0, 0.0, -1.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH)
        .expect("Negative coordinate gather should validate");
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let result = execute_gather_transaction(&mut world, target, &def, &tx)
        .expect("Negative coordinate gather should execute");

    assert!(world.store.get_voxel_world(target).is_air());
    assert_eq!(result.collection.source_coord, target);
}

#[test]
fn test_chunk_boundary_coordinates() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::new(0, 0, 0));
    add_empty_chunk(&mut world.store, IVec3::new(1, 0, 0));

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    // Voxel 31 adalah perbatasan X chunk 0
    let target = IVec3::new(31, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(15.5, 0.25, 0.25),
        distance: 1.0,
        face: FaceDirection::PosX,
        normal: Vec3::new(1.0, 0.0, 0.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let result = execute_gather_transaction(&mut world, target, &def, &tx).unwrap();
    assert!(world.store.get_voxel_world(target).is_air());
    assert_eq!(result.collection.source_coord, target);

    // Verifikasi neighbor chunk (1, 0, 0) terinvalidation
    assert!(world.store.dirty_mesh_chunks.contains(&IVec3::new(0, 0, 0)));
    assert!(world.store.dirty_mesh_chunks.contains(&IVec3::new(1, 0, 0)));
}

// ============================================================================
// 7. STRUCTURAL & REMESH INTEGRATION TESTS
// ============================================================================

#[test]
fn test_gathering_preserves_structural_connectivity() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    // Buat pilar vertikal:
    // Bedrock anchor pada Y = 0
    // Tiang batu (harvestable stone) pada Y = 1
    // Tiang kayu (wood) pada Y = 2
    world.set_voxel_world(IVec3::new(5, 0, 5), VoxelBlock::new(MaterialId(255))); // Bedrock
    world.set_voxel_world(IVec3::new(5, 1, 5), VoxelBlock::new(stone_mat));
    world.set_voxel_world(IVec3::new(5, 2, 5), VoxelBlock::new(wood_mat));

    let hit = VoxelHit {
        voxel_coord: IVec3::new(5, 1, 5),
        material: stone_mat,
        hit_point: Vec3::new(2.5, 0.75, 2.5),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let result = execute_gather_transaction(&mut world, IVec3::new(5, 1, 5), &def, &tx).unwrap();

    // Stone terhapus
    assert!(world.store.get_voxel_world(IVec3::new(5, 1, 5)).is_air());

    // Wood di Y=2 kehilangan tumpuan dan terlepas menjadi DynamicBody di PhysicsRuntime
    assert_eq!(result.mutation.newly_detached_aggregates.len(), 1);
    assert_eq!(world.physics.body_count(), 1);
}

// ============================================================================
// 8. COOLDOWN & DEBOUNCE TESTS
// ============================================================================

#[test]
fn test_cooldown_debounce_rate_limiting() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    world
        .store
        .set_voxel_world(IVec3::new(2, 2, 0), VoxelBlock::new(stone_mat));
    world
        .store
        .set_voxel_world(IVec3::new(3, 2, 0), VoxelBlock::new(stone_mat));

    let player = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let eye = player.eye_position();
    let look_dir = (Vec3::new(1.25, 1.25, 0.25) - eye).normalize();

    let mut cooldown = InteractionCooldown::new(0.20);
    assert!(cooldown.can_act());

    // Aksi 1: Berhasil
    let res1 = handle_player_gather(&mut world, &player, look_dir, &mut cooldown);
    assert!(res1.is_ok());
    assert!(!cooldown.can_act());

    // Aksi 2 seketika: Ditolak oleh CooldownActive
    let res2 = handle_player_gather(&mut world, &player, look_dir, &mut cooldown);
    assert!(matches!(res2, Err(GatheringError::CooldownActive { .. })));

    // Majukan waktu sebesar 0.25s
    cooldown.tick(0.25);
    assert!(cooldown.can_act());

    // Aksi 3 setelah cooldown: Diizinkan (walaupun mungkin miss atau air sekarang)
    assert!(cooldown.can_act());
}

// ============================================================================
// 9. ARCHITECTURAL FIREWALL TESTS
// ============================================================================

#[test]
fn test_firewall_no_inventory_or_item_stacks() {
    // Memverifikasi CollectionResult adalah struktur murni semantic tanpa slot inventory
    let result = CollectionResult {
        source_coord: IVec3::new(1, 2, 3),
        resource_id: ResourceId::core("stone").unwrap(),
        quantity: 1,
    };
    assert_eq!(result.quantity, 1);
    assert_eq!(result.resource_id.namespace.as_str(), "core");
}

#[test]
fn test_firewall_no_tool_requirements_in_baseline() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let iron_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("iron_ore").unwrap())
        .unwrap();

    let target = IVec3::new(1, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(iron_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: iron_mat,
        hit_point: Vec3::new(0.5, 0.25, 0.25),
        distance: 0.5,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    // Gathering dasar tidak memerlukan tool / pickaxe
    let res = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH);
    assert!(res.is_ok());
}

#[test]
fn test_firewall_no_dropped_item_entities() {
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(1, 0, 0);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(0.5, 0.25, 0.25),
        distance: 0.5,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let _result = execute_gather_transaction(&mut world, target, &def, &tx).unwrap();

    // PhysicsRuntime tidak memiliki entitas drop item (hanya detached bodies jika ada)
    assert_eq!(world.physics.body_count(), 0);
}

#[test]
fn test_negative_chunk_boundary() {
    let mut world = World::new();
    // Chunk (-1, 0, 0) spans world voxels [-32..-1, 0..31, 0..31]
    // Chunk (-2, 0, 0) spans world voxels [-64..-33, 0..31, 0..31]
    add_empty_chunk(&mut world.store, IVec3::new(-1, 0, 0));
    add_empty_chunk(&mut world.store, IVec3::new(-2, 0, 0));

    let stone_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();

    let target = IVec3::new(-32, 5, 5); // Border voxel in chunk (-1, 0, 0) adjacent to (-2, 0, 0)
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(stone_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: stone_mat,
        hit_point: Vec3::new(-16.0, 2.75, 2.75),
        distance: 1.0,
        face: FaceDirection::NegX,
        normal: Vec3::new(-1.0, 0.0, 0.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let result = execute_gather_transaction(&mut world, target, &def, &tx).unwrap();
    assert!(world.store.get_voxel_world(target).is_air());
    assert_eq!(result.collection.source_coord, target);

    // Both host and neighbor dirty
    assert!(world
        .store
        .dirty_mesh_chunks
        .contains(&IVec3::new(-1, 0, 0)));
    assert!(world
        .store
        .dirty_mesh_chunks
        .contains(&IVec3::new(-2, 0, 0)));
}

#[test]
fn test_determinism_repeated_gathering() {
    let stone_res_id = ResourceId::core("stone").unwrap();

    let run_once = || {
        let mut world = World::new();
        add_empty_chunk(&mut world.store, IVec3::ZERO);

        let stone_mat = world.materials.resolve_material_id(&stone_res_id).unwrap();

        let target = IVec3::new(5, 5, 5);
        world
            .store
            .set_voxel_world(target, VoxelBlock::new(stone_mat));

        let hit = VoxelHit {
            voxel_coord: target,
            material: stone_mat,
            hit_point: Vec3::new(2.5, 2.75, 2.75),
            distance: 2.0,
            face: FaceDirection::NegX,
            normal: Vec3::new(-1.0, 0.0, 0.0),
        };

        let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
        let mut tx = VoxelEditTransaction::new();
        tx.add_edit(edit);

        let result = execute_gather_transaction(&mut world, target, &def, &tx).unwrap();
        (result.collection, result.mutation.commit_result.delta.len())
    };

    let (first_col, first_deltas) = run_once();

    for _ in 0..100 {
        let (col, deltas) = run_once();
        assert_eq!(col, first_col);
        assert_eq!(deltas, first_deltas);
    }
}

#[test]
fn test_firewall_no_renderer_dependency() {
    // Memverifikasi seluruh subsistem gathering dapat berjalan murni headless
    let mut world = World::new();
    add_empty_chunk(&mut world.store, IVec3::ZERO);

    let iron_mat = world
        .materials
        .resolve_material_id(&ResourceId::core("iron_ore").unwrap())
        .unwrap();

    let target = IVec3::new(3, 3, 3);
    world
        .store
        .set_voxel_world(target, VoxelBlock::new(iron_mat));

    let hit = VoxelHit {
        voxel_coord: target,
        material: iron_mat,
        hit_point: Vec3::new(1.5, 1.75, 1.75),
        distance: 1.5,
        face: FaceDirection::PosZ,
        normal: Vec3::new(0.0, 0.0, 1.0),
    };

    let (def, edit) = can_gather(&world, &hit, DEFAULT_INTERACTION_REACH).unwrap();
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(edit);

    let result = execute_gather_transaction(&mut world, target, &def, &tx).unwrap();
    assert_eq!(result.collection.quantity, 1);
    assert!(world.store.get_voxel_world(target).is_air());
}
