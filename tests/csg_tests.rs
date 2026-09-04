use glam::{IVec3, Vec3};
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::csg::{
    CraterGenerator, DefaultDestructionPolicy, DestructionPolicy, MaterialDestructionPolicy,
    VoxelEdit, VoxelEditError, VoxelEditOperation, VoxelEditTransaction,
};
use omnisia::impact::{ImpactEvent, ImpactId, ImpactSource, ImpactSourceKind};
use omnisia::material::{MaterialId, MaterialRegistry};
use omnisia::streaming::store::ChunkStore;
use omnisia::structure::events::StructuralMutationType;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;

fn create_test_store_with_chunk(coord: IVec3, fill: Option<MaterialId>) -> ChunkStore {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(coord);
    if let Some(mat) = fill {
        chunk.fill_material(mat);
    }
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
    store
}

// ============================================================================
// 1. ADD / REMOVE / REPLACE BASIC SEMANTICS
// ============================================================================

#[test]
fn test_add_voxel_success() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, None);
    let pos = IVec3::new(5, 5, 5);
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::AIR);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MaterialId::STONE)));

    let result = tx.commit(&mut store).expect("Add on air must succeed");
    assert_eq!(result.delta.len(), 1);
    assert_eq!(
        store.get_voxel_world(pos),
        VoxelBlock::new(MaterialId::STONE)
    );
}

#[test]
fn test_remove_voxel_success() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));
    let pos = IVec3::new(5, 5, 5);
    assert!(!store.get_voxel_world(pos).is_air());

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos));

    let result = tx.commit(&mut store).expect("Remove on solid must succeed");
    assert_eq!(result.delta.len(), 1);
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::AIR);
}

#[test]
fn test_replace_voxel_success() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));
    let pos = IVec3::new(5, 5, 5);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::replace(
        pos,
        VoxelBlock::new(MaterialId::STONE),
        VoxelBlock::new(MaterialId::DIRT),
    ));

    let result = tx
        .commit(&mut store)
        .expect("Replace with matching precondition must succeed");
    assert_eq!(result.delta.len(), 1);
    assert_eq!(
        store.get_voxel_world(pos),
        VoxelBlock::new(MaterialId::DIRT)
    );
}

// ============================================================================
// 2. PRECONDITION VALIDATION & REJECTION
// ============================================================================

#[test]
fn test_invalid_add_on_solid_rejected() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));
    let pos = IVec3::new(5, 5, 5);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MaterialId::DIRT)));

    let res = tx.commit(&mut store);
    assert!(
        matches!(res, Err(VoxelEditError::AddTargetNotEmpty { .. })),
        "Adding to a solid voxel must fail with AddTargetNotEmpty"
    );
    assert_eq!(
        store.get_voxel_world(pos),
        VoxelBlock::new(MaterialId::STONE),
        "World state must remain unchanged"
    );
}

#[test]
fn test_invalid_remove_on_air_rejected() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, None);
    let pos = IVec3::new(5, 5, 5);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos));

    let res = tx.commit(&mut store);
    assert!(
        matches!(res, Err(VoxelEditError::RemoveTargetAlreadyAir { .. })),
        "Removing air must fail with RemoveTargetAlreadyAir"
    );
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::AIR);
}

#[test]
fn test_invalid_replace_precondition_mismatch() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::DIRT));
    let pos = IVec3::new(5, 5, 5);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::replace(
        pos,
        VoxelBlock::new(MaterialId::STONE), // Precondition expects STONE, actual is DIRT
        VoxelBlock::new(MaterialId::GLASS),
    ));

    let res = tx.commit(&mut store);
    assert!(
        matches!(res, Err(VoxelEditError::PreconditionMismatch { .. })),
        "Replace with mismatching precondition must fail"
    );
    assert_eq!(
        store.get_voxel_world(pos),
        VoxelBlock::new(MaterialId::DIRT),
        "World state must remain unchanged"
    );
}

// ============================================================================
// 3. TRANSACTION VALIDATION & NON-MUTATION
// ============================================================================

#[test]
fn test_transaction_validate_is_non_mutating() {
    let store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));
    let pos = IVec3::new(3, 3, 3);
    let rev_before = store.get(&IVec3::ZERO).unwrap().revision;

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos));

    let delta = tx
        .validate(&store)
        .expect("Validation on resident chunk must succeed");
    assert_eq!(delta.len(), 1);

    let rev_after = store.get(&IVec3::ZERO).unwrap().revision;
    assert_eq!(
        rev_before, rev_after,
        "validate() must not increment chunk revision"
    );
    assert_eq!(
        store.get_voxel_world(pos),
        VoxelBlock::new(MaterialId::STONE),
        "validate() must not mutate voxel block"
    );
    assert_eq!(
        store.get(&IVec3::ZERO).unwrap().dirty_flags,
        0,
        "validate() must not set any dirty flags"
    );
}

// ============================================================================
// 4. TRANSACTION ATOMICITY & REVERSION
// ============================================================================

#[test]
fn test_transaction_atomic_failure_reverts_cleanly() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, None);
    let pos_valid1 = IVec3::new(1, 1, 1);
    let pos_valid2 = IVec3::new(2, 2, 2);
    let pos_invalid = IVec3::new(3, 3, 3);

    // Set pos_invalid to air, so Remove will fail
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        pos_valid1,
        VoxelBlock::new(MaterialId::STONE),
    )); // Valid (air -> stone)
    tx.add_edit(VoxelEdit::add(
        pos_valid2,
        VoxelBlock::new(MaterialId::DIRT),
    )); // Valid (air -> dirt)
    tx.add_edit(VoxelEdit::remove(pos_invalid)); // INVALID: target is already air!

    let res = tx.commit(&mut store);
    assert!(res.is_err(), "Transaction with one invalid edit must fail");

    // CRITICAL ATOMICITY CHECK: valid1 and valid2 must NOT have been committed!
    assert_eq!(
        store.get_voxel_world(pos_valid1),
        VoxelBlock::AIR,
        "Atomic failure: valid edit 1 must not be committed"
    );
    assert_eq!(
        store.get_voxel_world(pos_valid2),
        VoxelBlock::AIR,
        "Atomic failure: valid edit 2 must not be committed"
    );
    assert_eq!(store.get_voxel_world(pos_invalid), VoxelBlock::AIR);
    assert_eq!(
        store.get(&IVec3::ZERO).unwrap().dirty_flags,
        0,
        "No dirty flags should be marked on failed transaction"
    );
}

#[test]
fn test_transaction_successful_commit() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, None);
    let pos1 = IVec3::new(1, 1, 1);
    let pos2 = IVec3::new(2, 2, 2);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(pos1, VoxelBlock::new(MaterialId::STONE)));
    tx.add_edit(VoxelEdit::add(pos2, VoxelBlock::new(MaterialId::DIRT)));

    let result = tx.commit(&mut store).expect("Commit must succeed");
    assert_eq!(result.affected_chunks, vec![IVec3::ZERO]);
    assert_eq!(
        store.get_voxel_world(pos1),
        VoxelBlock::new(MaterialId::STONE)
    );
    assert_eq!(
        store.get_voxel_world(pos2),
        VoxelBlock::new(MaterialId::DIRT)
    );
    assert!(
        store.get(&IVec3::ZERO).unwrap().is_dirty(
            dirty_flags::VOXEL_DIRTY
                | dirty_flags::MESH_DIRTY
                | dirty_flags::SAVE_DIRTY
                | dirty_flags::STRUCTURAL_DIRTY
        ),
        "Chunk must be marked dirty on successful commit"
    );
}

// ============================================================================
// 5. DUPLICATE EDITS & DETERMINISTIC ORDERING
// ============================================================================

#[test]
fn test_duplicate_edit_rejection_deterministic() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));
    let pos = IVec3::new(5, 5, 5);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos));
    tx.add_edit(VoxelEdit::replace_unconditional(
        pos,
        VoxelBlock::new(MaterialId::DIRT),
    ));

    let res = tx.commit(&mut store);
    assert!(
        matches!(res, Err(VoxelEditError::ConflictingDuplicateEdit { position }) if position == pos),
        "Duplicate edits targeting the same voxel must be rejected deterministically"
    );
    assert_eq!(
        store.get_voxel_world(pos),
        VoxelBlock::new(MaterialId::STONE),
        "World state must remain unchanged"
    );
}

#[test]
fn test_deterministic_ordering_insertion_invariance() {
    let mut store1 = create_test_store_with_chunk(IVec3::ZERO, None);
    let mut store2 = create_test_store_with_chunk(IVec3::ZERO, None);

    let pos_a = IVec3::new(2, 5, 7);
    let pos_b = IVec3::new(10, 1, 3);
    let pos_c = IVec3::new(15, 8, 20);

    let mut tx1 = VoxelEditTransaction::new();
    tx1.add_edit(VoxelEdit::add(pos_a, VoxelBlock::new(MaterialId::STONE)));
    tx1.add_edit(VoxelEdit::add(pos_b, VoxelBlock::new(MaterialId::DIRT)));
    tx1.add_edit(VoxelEdit::add(pos_c, VoxelBlock::new(MaterialId::GLASS)));

    // Reverse insertion order in tx2
    let mut tx2 = VoxelEditTransaction::new();
    tx2.add_edit(VoxelEdit::add(pos_c, VoxelBlock::new(MaterialId::GLASS)));
    tx2.add_edit(VoxelEdit::add(pos_b, VoxelBlock::new(MaterialId::DIRT)));
    tx2.add_edit(VoxelEdit::add(pos_a, VoxelBlock::new(MaterialId::STONE)));

    let delta1 = tx1.validate(&store1).unwrap();
    let delta2 = tx2.validate(&store2).unwrap();

    assert_eq!(
        delta1, delta2,
        "Validation and deltas must be invariant to insertion order"
    );

    let res1 = tx1.commit(&mut store1).unwrap();
    let res2 = tx2.commit(&mut store2).unwrap();

    assert_eq!(
        res1.delta, res2.delta,
        "Commit deltas must be strictly identical"
    );
    assert_eq!(
        res1.structural_events, res2.structural_events,
        "Structural events must be identical"
    );
}

// ============================================================================
// 6. CROSS-CHUNK ATOMICITY & COORDINATE CORRECTNESS
// ============================================================================

#[test]
fn test_cross_chunk_successful_commit() {
    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::new(0, 0, 0)));
    store.insert(Chunk::new(IVec3::new(1, 0, 0)));
    store.insert(Chunk::new(IVec3::new(-1, 0, 0)));

    let pos_chunk0 = IVec3::new(5, 5, 5);
    let pos_chunk1 = IVec3::new(35, 5, 5); // x=35 -> chunk 1, local x=3
    let pos_chunk_neg = IVec3::new(-5, 5, 5); // x=-5 -> chunk -1, local x=27

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        pos_chunk0,
        VoxelBlock::new(MaterialId::STONE),
    ));
    tx.add_edit(VoxelEdit::add(
        pos_chunk1,
        VoxelBlock::new(MaterialId::DIRT),
    ));
    tx.add_edit(VoxelEdit::add(
        pos_chunk_neg,
        VoxelBlock::new(MaterialId::GLASS),
    ));

    let res = tx
        .commit(&mut store)
        .expect("Cross-chunk transaction must commit");
    assert_eq!(res.affected_chunks.len(), 3);
    assert_eq!(
        store.get_voxel_world(pos_chunk0),
        VoxelBlock::new(MaterialId::STONE)
    );
    assert_eq!(
        store.get_voxel_world(pos_chunk1),
        VoxelBlock::new(MaterialId::DIRT)
    );
    assert_eq!(
        store.get_voxel_world(pos_chunk_neg),
        VoxelBlock::new(MaterialId::GLASS)
    );
}

#[test]
fn test_cross_chunk_atomic_failure_zero_state_change() {
    let mut store = ChunkStore::new();
    let mut chunk0 = Chunk::new(IVec3::new(0, 0, 0));
    chunk0.clear_dirty(dirty_flags::ALL);
    store.insert(chunk0);
    let mut chunk1 = Chunk::new(IVec3::new(1, 0, 0));
    chunk1.clear_dirty(dirty_flags::ALL);
    store.insert(chunk1);
    // NOTE: Chunk (-1, 0, 0) is deliberately NOT resident in memory!

    let pos_chunk0 = IVec3::new(5, 5, 5);
    let pos_chunk1 = IVec3::new(35, 5, 5);
    let pos_unloaded = IVec3::new(-5, 5, 5); // Chunk (-1, 0, 0) is unloaded

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        pos_chunk0,
        VoxelBlock::new(MaterialId::STONE),
    ));
    tx.add_edit(VoxelEdit::add(
        pos_chunk1,
        VoxelBlock::new(MaterialId::DIRT),
    ));
    tx.add_edit(VoxelEdit::add(
        pos_unloaded,
        VoxelBlock::new(MaterialId::GLASS),
    ));

    let res = tx.commit(&mut store);
    assert!(
        matches!(
            res,
            Err(VoxelEditError::ChunkNotResident { chunk_coord }) if chunk_coord == IVec3::new(-1, 0, 0)
        ),
        "Transaction touching an unloaded chunk must fail"
    );

    // Verify neither chunk 0 nor chunk 1 was touched
    assert_eq!(store.get_voxel_world(pos_chunk0), VoxelBlock::AIR);
    assert_eq!(store.get_voxel_world(pos_chunk1), VoxelBlock::AIR);
    assert_eq!(store.get(&IVec3::new(0, 0, 0)).unwrap().dirty_flags, 0);
    assert_eq!(store.get(&IVec3::new(1, 0, 0)).unwrap().dirty_flags, 0);
}

#[test]
fn test_negative_coordinates_euclidean_correctness() {
    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::new(-1, -1, -1)));
    store.insert(Chunk::new(IVec3::new(-2, -1, -1)));

    let pos_neg1 = IVec3::new(-1, -1, -1); // Chunk (-1, -1, -1), local (31, 31, 31)
    let pos_neg32 = IVec3::new(-32, -1, -1); // Chunk (-1, -1, -1), local (0, 31, 31)
    let pos_neg33 = IVec3::new(-33, -1, -1); // Chunk (-2, -1, -1), local (31, 31, 31)

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(pos_neg1, VoxelBlock::new(MaterialId::STONE)));
    tx.add_edit(VoxelEdit::add(pos_neg32, VoxelBlock::new(MaterialId::DIRT)));
    tx.add_edit(VoxelEdit::add(
        pos_neg33,
        VoxelBlock::new(MaterialId::GLASS),
    ));

    tx.commit(&mut store)
        .expect("Euclidean negative coordinates must commit cleanly");

    assert_eq!(
        store.get_voxel_world(pos_neg1),
        VoxelBlock::new(MaterialId::STONE)
    );
    assert_eq!(
        store.get_voxel_world(pos_neg32),
        VoxelBlock::new(MaterialId::DIRT)
    );
    assert_eq!(
        store.get_voxel_world(pos_neg33),
        VoxelBlock::new(MaterialId::GLASS)
    );
}

#[test]
fn test_chunk_boundary_crossings_correctness() {
    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::new(0, 0, 0)));
    store.insert(Chunk::new(IVec3::new(1, 0, 0)));

    let pos_border_left = IVec3::new(31, 10, 10); // Chunk 0, local x=31
    let pos_border_right = IVec3::new(32, 10, 10); // Chunk 1, local x=0

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        pos_border_left,
        VoxelBlock::new(MaterialId::STONE),
    ));
    tx.add_edit(VoxelEdit::add(
        pos_border_right,
        VoxelBlock::new(MaterialId::DIRT),
    ));

    let res = tx.commit(&mut store).unwrap();
    assert_eq!(
        res.affected_chunks,
        vec![IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]
    );
}

// ============================================================================
// 7. CRATER GENERATOR & GEOMETRY
// ============================================================================

#[test]
fn test_spherical_crater_generation_bounded() {
    let materials = MaterialRegistry::new();
    let policy = DefaultDestructionPolicy;
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));

    // Center crater at world (8.0, 8.0, 8.0) meters with radius 2.0 meters (4 voxels radius)
    let center = Vec3::new(8.0, 8.0, 8.0);
    let radius = 2.0;

    let tx = CraterGenerator::generate(center, radius, &policy, &materials, &store)
        .expect("Crater generation in resident chunk must succeed");
    assert!(!tx.is_empty(), "Crater must propose voxel removals");

    // All proposed edits must be Remove
    for edit in tx.edits() {
        assert_eq!(edit.operation, VoxelEditOperation::Remove);
    }

    // Commit the crater
    let res = tx.commit(&mut store).expect("Crater commit must succeed");
    assert_eq!(res.affected_chunks, vec![IVec3::ZERO]);

    // Check that center voxel is now AIR
    let center_voxel = IVec3::new(16, 16, 16); // 8.0m / 0.5m = 16
    assert_eq!(store.get_voxel_world(center_voxel), VoxelBlock::AIR);

    // Check that a far voxel remains STONE
    let far_voxel = IVec3::new(0, 0, 0);
    assert_eq!(
        store.get_voxel_world(far_voxel),
        VoxelBlock::new(MaterialId::STONE)
    );
}

#[test]
fn test_zero_radius_crater_produces_empty_transaction() {
    let materials = MaterialRegistry::new();
    let policy = DefaultDestructionPolicy;
    let store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));

    let tx_zero = CraterGenerator::generate(Vec3::splat(8.0), 0.0, &policy, &materials, &store)
        .expect("Zero radius crater should succeed");
    assert_eq!(tx_zero.len(), 0);

    let tx_neg = CraterGenerator::generate(Vec3::splat(8.0), -5.0, &policy, &materials, &store)
        .expect("Negative radius crater should succeed");
    assert_eq!(tx_neg.len(), 0);
}

#[test]
fn test_crater_generation_determinism_and_replay() {
    let materials = MaterialRegistry::new();
    let policy = DefaultDestructionPolicy;
    let store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));

    let center = Vec3::new(5.0, 7.5, 9.0);
    let radius = 3.25;

    let tx1 = CraterGenerator::generate(center, radius, &policy, &materials, &store).unwrap();
    let tx2 = CraterGenerator::generate(center, radius, &policy, &materials, &store).unwrap();

    assert_eq!(tx1.edits(), tx2.edits(), "Crater edits must be identical");
}

// ============================================================================
// 8. MATERIAL-AWARE POLICY & INDESTRUCTIBLE VOXELS
// ============================================================================

#[test]
fn test_material_aware_destruction_policy() {
    let materials = MaterialRegistry::new();
    let default_policy = DefaultDestructionPolicy;

    let stone_block = VoxelBlock::new(MaterialId::STONE);
    let air_block = VoxelBlock::AIR;

    assert!(default_policy.is_destructible(&stone_block, &materials));
    assert!(!default_policy.is_destructible(&air_block, &materials));
}

#[test]
fn test_indestructible_voxel_preservation() {
    let materials = MaterialRegistry::new();
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));

    let indestructible_pos = IVec3::new(16, 16, 16);
    store.set_voxel_world(
        indestructible_pos,
        VoxelBlock::new(MaterialId::AG_CORE_CASING),
    );

    let policy = MaterialDestructionPolicy::new().with_indestructible(MaterialId::AG_CORE_CASING);

    let center = Vec3::new(8.0, 8.0, 8.0);
    let radius = 2.0;

    let tx = CraterGenerator::generate(center, radius, &policy, &materials, &store).unwrap();

    // Verify that the indestructible voxel is NOT in the removal list
    for edit in tx.edits() {
        assert_ne!(
            edit.position, indestructible_pos,
            "Indestructible voxel must not be removed"
        );
    }

    tx.commit(&mut store).unwrap();

    // The indestructible block must be preserved intact!
    assert_eq!(
        store.get_voxel_world(indestructible_pos),
        VoxelBlock::new(MaterialId::AG_CORE_CASING)
    );
    // Adjacent stone block was removed
    let adj_pos = IVec3::new(16, 17, 16);
    assert_eq!(store.get_voxel_world(adj_pos), VoxelBlock::AIR);
}

// ============================================================================
// 9. MESH & STRUCTURAL INVALIDATION
// ============================================================================

#[test]
fn test_mesh_invalidation_dirty_flags() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));
    let pos = IVec3::new(10, 10, 10);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos));

    let res = tx.commit(&mut store).unwrap();
    assert_eq!(res.affected_chunks, vec![IVec3::ZERO]);

    let chunk = store.get(&IVec3::ZERO).unwrap();
    assert!(chunk.is_dirty(dirty_flags::MESH_DIRTY));
    assert!(chunk.is_dirty(dirty_flags::VOXEL_DIRTY));
    assert!(chunk.is_dirty(dirty_flags::SAVE_DIRTY));
    assert!(chunk.is_dirty(dirty_flags::STRUCTURAL_DIRTY));
}

#[test]
fn test_neighbor_chunk_mesh_invalidation_on_boundaries() {
    let mut store = ChunkStore::new();
    let mut chunk0 = Chunk::new(IVec3::new(0, 0, 0));
    chunk0.clear_dirty(dirty_flags::ALL);
    store.insert(chunk0);

    let mut chunk_neg = Chunk::new(IVec3::new(-1, 0, 0));
    chunk_neg.clear_dirty(dirty_flags::ALL);
    store.insert(chunk_neg);

    // Edit at local x=0 in chunk (0, 0, 0)
    let border_pos = IVec3::new(0, 5, 5);
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        border_pos,
        VoxelBlock::new(MaterialId::STONE),
    ));

    let res = tx.commit(&mut store).unwrap();

    // res.mesh_invalidation_chunks must contain both chunk 0 and neighbor chunk -1
    assert!(res.mesh_invalidation_chunks.contains(&IVec3::new(0, 0, 0)));
    assert!(res.mesh_invalidation_chunks.contains(&IVec3::new(-1, 0, 0)));

    // Neighbor chunk in store must have MESH_DIRTY marked
    let neighbor = store.get(&IVec3::new(-1, 0, 0)).unwrap();
    assert!(
        neighbor.is_dirty(dirty_flags::MESH_DIRTY),
        "Resident neighbor chunk must be marked MESH_DIRTY on boundary edit"
    );
}

#[test]
fn test_structural_events_emission_without_bfs() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));
    let pos_remove = IVec3::new(5, 5, 5);
    let pos_add = IVec3::new(6, 6, 6);
    store.set_voxel_world(pos_add, VoxelBlock::AIR);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos_remove));
    tx.add_edit(VoxelEdit::add(pos_add, VoxelBlock::new(MaterialId::GLASS)));

    let res = tx.commit(&mut store).unwrap();
    assert_eq!(res.structural_events.len(), 2);

    let remove_ev = res
        .structural_events
        .iter()
        .find(|e| e.world_voxel == pos_remove)
        .unwrap();
    assert!(matches!(
        remove_ev.mutation,
        StructuralMutationType::VoxelRemoved { .. }
    ));
    assert!(remove_ev.can_cause_detachment());

    let add_ev = res
        .structural_events
        .iter()
        .find(|e| e.world_voxel == pos_add)
        .unwrap();
    assert!(matches!(
        add_ev.mutation,
        StructuralMutationType::VoxelPlaced { .. }
    ));
    assert!(!add_ev.can_cause_detachment());
}

#[test]
fn test_failed_transaction_emits_no_invalidation() {
    let mut store = create_test_store_with_chunk(IVec3::ZERO, None);
    let pos = IVec3::new(5, 5, 5);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos)); // Invalid: already air

    assert!(tx.commit(&mut store).is_err());
    assert_eq!(
        store.get(&IVec3::ZERO).unwrap().dirty_flags,
        0,
        "Failed commit must not mark dirty flags"
    );
}

// ============================================================================
// 10. IMPACT INTEGRATION & FIREWALL TESTS
// ============================================================================

#[test]
fn test_impact_event_to_crater_transaction_integration() {
    let materials = MaterialRegistry::new();
    let policy = DefaultDestructionPolicy;
    let mut store = create_test_store_with_chunk(IVec3::ZERO, Some(MaterialId::STONE));

    let impact = ImpactEvent::builder(ImpactId(101), Vec3::new(8.0, 8.0, 8.0), 2.5)
        .source(ImpactSource::new(ImpactSourceKind::Projectile, 42))
        .energy(1000.0)
        .build()
        .expect("Valid impact event");

    let tx = CraterGenerator::from_impact(&impact, &policy, &materials, &store).unwrap();
    assert!(!tx.is_empty());

    let res = tx.commit(&mut store).unwrap();
    assert_eq!(res.affected_chunks, vec![IVec3::ZERO]);
    assert_eq!(
        store.get_voxel_world(IVec3::new(16, 16, 16)),
        VoxelBlock::AIR
    );
}

#[test]
fn test_physics_state_unmutated_by_csg() {
    let mut world = World::new();
    let initial_bodies_count = world.physics.bodies.len();

    // Fill chunk (0, 0, 0)
    if let Some(chunk) = world.store.get_mut(&IVec3::ZERO) {
        chunk.fill_material(MaterialId::STONE);
    } else {
        world.store.insert(Chunk::new(IVec3::ZERO));
        world
            .store
            .get_mut(&IVec3::ZERO)
            .unwrap()
            .fill_material(MaterialId::STONE);
    }

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(5, 5, 5)));
    tx.commit(&mut world.store).unwrap();

    // SCOPE FIREWALL VERIFICATION:
    assert_eq!(
        world.physics.bodies.len(),
        initial_bodies_count,
        "CSG must NOT spawn or alter physics bodies (Phase 10.3 responsibility)"
    );
}

#[test]
fn test_structural_graph_unmutated_by_csg() {
    let mut world = World::new();

    if let Some(chunk) = world.store.get_mut(&IVec3::ZERO) {
        chunk.fill_material(MaterialId::STONE);
    } else {
        world.store.insert(Chunk::new(IVec3::ZERO));
        world
            .store
            .get_mut(&IVec3::ZERO)
            .unwrap()
            .fill_material(MaterialId::STONE);
    }

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(10, 10, 10)));
    let res = tx.commit(&mut world.store).unwrap();

    // Structural events are returned as notifications, but NOT sent to world.structure.process_event
    assert!(!res.structural_events.is_empty());
    // World ownership audit verifies zero dynamic voxels spawned
    let audit = world.audit_world_ownership();
    assert_eq!(
        audit.total_dynamic_voxels, 0,
        "Zero dynamic voxels detached during Phase 10.2"
    );
    assert_eq!(
        audit.active_bodies_count, 0,
        "Zero dynamic bodies active in Phase 10.2"
    );
}
