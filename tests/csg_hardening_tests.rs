use glam::IVec3;
use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::coord::{chunk_and_local_to_world_voxel, world_voxel_to_chunk_and_local, CHUNK_SIZE};
use omnisia::csg::{DuplicateEditPolicy, VoxelEdit, VoxelEditError, VoxelEditTransaction};
use omnisia::material::MaterialId;
use omnisia::streaming::store::ChunkStore;
use omnisia::structure::anchor::AnchorPolicy;
use omnisia::structure::events::StructuralMutationType;
use omnisia::structure::manager::StructuralSystem;
use omnisia::voxel::VoxelBlock;

// ============================================================================
// TEST FIXTURES & HELPERS
// ============================================================================

const MAT_STONE: MaterialId = MaterialId::STONE;
const MAT_DIRT: MaterialId = MaterialId::DIRT;
const MAT_WOOD: MaterialId = MaterialId::OAK_WOOD;

fn create_store_with_chunk(coord: IVec3, fill: Option<MaterialId>) -> ChunkStore {
    let mut store = ChunkStore::new();
    let mut chunk = Chunk::new(coord);
    if let Some(mat) = fill {
        chunk.fill_material(mat);
    }
    chunk.clear_dirty(dirty_flags::ALL);
    store.insert(chunk);
    store
}

fn create_store_with_chunks(coords: &[IVec3], fill: Option<MaterialId>) -> ChunkStore {
    let mut store = ChunkStore::new();
    for &coord in coords {
        let mut chunk = Chunk::new(coord);
        if let Some(mat) = fill {
            chunk.fill_material(mat);
        }
        chunk.clear_dirty(dirty_flags::ALL);
        store.insert(chunk);
    }
    store
}

// ============================================================================
// CATEGORY A: ARBITRARY VOXEL EDITS
// ============================================================================

#[test]
fn test_a1_add_air_to_solid_success() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let pos = IVec3::new(4, 5, 6);
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::AIR);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MAT_STONE)));

    let result = tx.commit(&mut store).expect("Add on air must succeed");
    assert_eq!(result.delta.len(), 1);
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::new(MAT_STONE));
}

#[test]
fn test_a2_add_solid_to_solid_fails_precondition() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));
    let pos = IVec3::new(4, 5, 6);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MAT_DIRT)));

    let err = tx.commit(&mut store).expect_err("Add on solid must fail");
    match err {
        VoxelEditError::AddTargetNotEmpty {
            position, current, ..
        } => {
            assert_eq!(position, pos);
            assert_eq!(current, VoxelBlock::new(MAT_STONE));
        }
        other => panic!("Expected AddTargetNotEmpty, got: {:?}", other),
    }
    // Verify zero mutation
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::new(MAT_STONE));
}

#[test]
fn test_a3_add_air_with_air_fails_invalid_op() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let pos = IVec3::new(4, 5, 6);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(pos, VoxelBlock::AIR));

    let err = tx
        .commit(&mut store)
        .expect_err("Add with AIR must fail as invalid op");
    match err {
        VoxelEditError::InvalidOperation { reason } => {
            assert!(reason.contains("Cannot perform Add with VoxelBlock::AIR"));
        }
        other => panic!("Expected InvalidOperation, got: {:?}", other),
    }
}

#[test]
fn test_a4_remove_solid_to_air_success() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));
    let pos = IVec3::new(4, 5, 6);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos));

    let result = tx.commit(&mut store).expect("Remove on solid must succeed");
    assert_eq!(result.delta.len(), 1);
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::AIR);
}

#[test]
fn test_a5_remove_air_to_air_fails_precondition() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let pos = IVec3::new(4, 5, 6);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(pos));

    let err = tx
        .commit(&mut store)
        .expect_err("Remove on air must fail precondition");
    match err {
        VoxelEditError::RemoveTargetAlreadyAir { position } => {
            assert_eq!(position, pos);
        }
        other => panic!("Expected RemoveTargetAlreadyAir, got: {:?}", other),
    }
}

#[test]
fn test_a6_replace_with_matching_precondition_success() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));
    let pos = IVec3::new(4, 5, 6);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::replace(
        pos,
        VoxelBlock::new(MAT_STONE),
        VoxelBlock::new(MAT_WOOD),
    ));

    let result = tx
        .commit(&mut store)
        .expect("Replace with matching precondition must succeed");
    assert_eq!(result.delta.len(), 1);
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::new(MAT_WOOD));
}

#[test]
fn test_a7_replace_with_mismatched_precondition_fails() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));
    let pos = IVec3::new(4, 5, 6);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::replace(
        pos,
        VoxelBlock::new(MAT_DIRT), // Expected DIRT but is STONE
        VoxelBlock::new(MAT_WOOD),
    ));

    let err = tx
        .commit(&mut store)
        .expect_err("Replace with mismatched precondition must fail");
    match err {
        VoxelEditError::PreconditionMismatch {
            position,
            expected,
            actual,
        } => {
            assert_eq!(position, pos);
            assert_eq!(expected, VoxelBlock::new(MAT_DIRT));
            assert_eq!(actual, VoxelBlock::new(MAT_STONE));
        }
        other => panic!("Expected PreconditionMismatch, got: {:?}", other),
    }
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::new(MAT_STONE));
}

#[test]
fn test_a8_replace_unconditional_success() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));
    let pos = IVec3::new(4, 5, 6);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::replace_unconditional(
        pos,
        VoxelBlock::new(MAT_WOOD),
    ));

    let result = tx
        .commit(&mut store)
        .expect("Unconditional replace must succeed");
    assert_eq!(result.delta.len(), 1);
    assert_eq!(store.get_voxel_world(pos), VoxelBlock::new(MAT_WOOD));
}

#[test]
fn test_a9_mixed_transaction_add_remove_replace() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    // Setup initial blocks
    store.set_voxel_world(IVec3::new(1, 1, 1), VoxelBlock::new(MAT_STONE));
    store.set_voxel_world(IVec3::new(2, 2, 2), VoxelBlock::new(MAT_DIRT));
    // IVec3(3, 3, 3) is AIR

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(1, 1, 1)));
    tx.add_edit(VoxelEdit::replace(
        IVec3::new(2, 2, 2),
        VoxelBlock::new(MAT_DIRT),
        VoxelBlock::new(MAT_WOOD),
    ));
    tx.add_edit(VoxelEdit::add(
        IVec3::new(3, 3, 3),
        VoxelBlock::new(MAT_STONE),
    ));

    let result = tx
        .commit(&mut store)
        .expect("Mixed transaction must succeed");
    assert_eq!(result.delta.len(), 3);
    assert_eq!(store.get_voxel_world(IVec3::new(1, 1, 1)), VoxelBlock::AIR);
    assert_eq!(
        store.get_voxel_world(IVec3::new(2, 2, 2)),
        VoxelBlock::new(MAT_WOOD)
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(3, 3, 3)),
        VoxelBlock::new(MAT_STONE)
    );
}

#[test]
fn test_a10_duplicate_edit_rejection_deterministic_under_shuffled_inputs() {
    let store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));
    let pos = IVec3::new(10, 10, 10);

    // Order 1: Remove first, then Replace
    let mut tx1 =
        VoxelEditTransaction::new().with_duplicate_policy(DuplicateEditPolicy::RejectDuplicates);
    tx1.add_edit(VoxelEdit::remove(pos));
    tx1.add_edit(VoxelEdit::replace_unconditional(
        pos,
        VoxelBlock::new(MAT_WOOD),
    ));

    // Order 2: Replace first, then Remove
    let mut tx2 =
        VoxelEditTransaction::new().with_duplicate_policy(DuplicateEditPolicy::RejectDuplicates);
    tx2.add_edit(VoxelEdit::replace_unconditional(
        pos,
        VoxelBlock::new(MAT_WOOD),
    ));
    tx2.add_edit(VoxelEdit::remove(pos));

    let err1 = tx1.validate(&store).expect_err("tx1 must reject duplicate");
    let err2 = tx2.validate(&store).expect_err("tx2 must reject duplicate");

    assert_eq!(
        err1,
        VoxelEditError::ConflictingDuplicateEdit { position: pos }
    );
    assert_eq!(
        err2,
        VoxelEditError::ConflictingDuplicateEdit { position: pos }
    );
}

#[test]
fn test_a11_last_write_wins_preserves_transaction_order() {
    let mut store1 = create_store_with_chunk(IVec3::ZERO, None);
    let mut store2 = create_store_with_chunk(IVec3::ZERO, None);
    let pos = IVec3::new(5, 5, 5);

    // tx1: Add Stone, then Add Wood -> Wood must win
    let mut tx1 =
        VoxelEditTransaction::new().with_duplicate_policy(DuplicateEditPolicy::LastWriteWins);
    tx1.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MAT_STONE)));
    tx1.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MAT_WOOD)));
    let res1 = tx1.commit(&mut store1).expect("tx1 LastWriteWins succeeds");
    assert_eq!(res1.delta.len(), 1);
    assert_eq!(store1.get_voxel_world(pos), VoxelBlock::new(MAT_WOOD));

    // tx2: Add Wood, then Add Stone -> Stone must win
    let mut tx2 =
        VoxelEditTransaction::new().with_duplicate_policy(DuplicateEditPolicy::LastWriteWins);
    tx2.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MAT_WOOD)));
    tx2.add_edit(VoxelEdit::add(pos, VoxelBlock::new(MAT_STONE)));
    let res2 = tx2.commit(&mut store2).expect("tx2 LastWriteWins succeeds");
    assert_eq!(res2.delta.len(), 1);
    assert_eq!(store2.get_voxel_world(pos), VoxelBlock::new(MAT_STONE));
}

#[test]
fn test_a12_adjacent_edits_forming_cavity_and_tunnel() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));

    // Carve 3x3x3 cavity from (10, 10, 10) to (12, 12, 12)
    let mut tx = VoxelEditTransaction::new();
    for x in 10..=12 {
        for y in 10..=12 {
            for z in 10..=12 {
                tx.add_edit(VoxelEdit::remove(IVec3::new(x, y, z)));
            }
        }
    }
    // Carve tunnel from (13, 11, 11) to (20, 11, 11)
    for x in 13..=20 {
        tx.add_edit(VoxelEdit::remove(IVec3::new(x, 11, 11)));
    }

    let res = tx
        .commit(&mut store)
        .expect("Cavity + tunnel carve must succeed");
    assert_eq!(res.delta.len(), 27 + 8);
    for x in 10..=12 {
        for y in 10..=12 {
            for z in 10..=12 {
                assert_eq!(store.get_voxel_world(IVec3::new(x, y, z)), VoxelBlock::AIR);
            }
        }
    }
    for x in 13..=20 {
        assert_eq!(
            store.get_voxel_world(IVec3::new(x, 11, 11)),
            VoxelBlock::AIR
        );
    }
}

// ============================================================================
// CATEGORY B: CROSS-CHUNK BOUNDARIES
// ============================================================================

#[test]
fn test_b1_boundary_crossing_x_axis_31_to_32() {
    let chunks = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)];
    let mut store = create_store_with_chunks(&chunks, None);

    let v0 = IVec3::new(31, 10, 10);
    let v1 = IVec3::new(32, 10, 10);

    let (c0, l0) = world_voxel_to_chunk_and_local(v0);
    let (c1, l1) = world_voxel_to_chunk_and_local(v1);
    assert_eq!(c0, IVec3::new(0, 0, 0));
    assert_eq!(l0, IVec3::new(31, 10, 10));
    assert_eq!(c1, IVec3::new(1, 0, 0));
    assert_eq!(l1, IVec3::new(0, 10, 10));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(v0, VoxelBlock::new(MAT_STONE)));
    tx.add_edit(VoxelEdit::add(v1, VoxelBlock::new(MAT_DIRT)));

    let res = tx
        .commit(&mut store)
        .expect("X boundary cross must succeed");
    assert_eq!(res.affected_chunks.len(), 2);
    assert_eq!(store.get_voxel_world(v0), VoxelBlock::new(MAT_STONE));
    assert_eq!(store.get_voxel_world(v1), VoxelBlock::new(MAT_DIRT));
}

#[test]
fn test_b2_boundary_crossing_y_axis_31_to_32() {
    let chunks = [IVec3::new(0, 0, 0), IVec3::new(0, 1, 0)];
    let mut store = create_store_with_chunks(&chunks, None);

    let v0 = IVec3::new(10, 31, 10);
    let v1 = IVec3::new(10, 32, 10);

    let (c0, l0) = world_voxel_to_chunk_and_local(v0);
    let (c1, l1) = world_voxel_to_chunk_and_local(v1);
    assert_eq!(c0, IVec3::new(0, 0, 0));
    assert_eq!(l0, IVec3::new(10, 31, 10));
    assert_eq!(c1, IVec3::new(0, 1, 0));
    assert_eq!(l1, IVec3::new(10, 0, 10));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(v0, VoxelBlock::new(MAT_STONE)));
    tx.add_edit(VoxelEdit::add(v1, VoxelBlock::new(MAT_DIRT)));

    let res = tx
        .commit(&mut store)
        .expect("Y boundary cross must succeed");
    assert_eq!(res.affected_chunks.len(), 2);
    assert_eq!(store.get_voxel_world(v0), VoxelBlock::new(MAT_STONE));
    assert_eq!(store.get_voxel_world(v1), VoxelBlock::new(MAT_DIRT));
}

#[test]
fn test_b3_boundary_crossing_z_axis_31_to_32() {
    let chunks = [IVec3::new(0, 0, 0), IVec3::new(0, 0, 1)];
    let mut store = create_store_with_chunks(&chunks, None);

    let v0 = IVec3::new(10, 10, 31);
    let v1 = IVec3::new(10, 10, 32);

    let (c0, l0) = world_voxel_to_chunk_and_local(v0);
    let (c1, l1) = world_voxel_to_chunk_and_local(v1);
    assert_eq!(c0, IVec3::new(0, 0, 0));
    assert_eq!(l0, IVec3::new(10, 10, 31));
    assert_eq!(c1, IVec3::new(0, 0, 1));
    assert_eq!(l1, IVec3::new(10, 10, 0));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(v0, VoxelBlock::new(MAT_STONE)));
    tx.add_edit(VoxelEdit::add(v1, VoxelBlock::new(MAT_DIRT)));

    let res = tx
        .commit(&mut store)
        .expect("Z boundary cross must succeed");
    assert_eq!(res.affected_chunks.len(), 2);
    assert_eq!(store.get_voxel_world(v0), VoxelBlock::new(MAT_STONE));
    assert_eq!(store.get_voxel_world(v1), VoxelBlock::new(MAT_DIRT));
}

#[test]
fn test_b4_boundary_crossing_edge_xy() {
    let chunks = [
        IVec3::new(0, 0, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(1, 1, 0),
    ];
    let mut store = create_store_with_chunks(&chunks, None);

    let v_corner = IVec3::new(31, 31, 15);
    let v_diag = IVec3::new(32, 32, 15);

    let (c0, l0) = world_voxel_to_chunk_and_local(v_corner);
    let (c1, l1) = world_voxel_to_chunk_and_local(v_diag);
    assert_eq!(c0, IVec3::new(0, 0, 0));
    assert_eq!(l0, IVec3::new(31, 31, 15));
    assert_eq!(c1, IVec3::new(1, 1, 0));
    assert_eq!(l1, IVec3::new(0, 0, 15));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(v_corner, VoxelBlock::new(MAT_STONE)));
    tx.add_edit(VoxelEdit::add(v_diag, VoxelBlock::new(MAT_DIRT)));

    let res = tx.commit(&mut store).expect("Edge crossing must succeed");
    assert_eq!(res.affected_chunks.len(), 2);
    assert_eq!(store.get_voxel_world(v_corner), VoxelBlock::new(MAT_STONE));
    assert_eq!(store.get_voxel_world(v_diag), VoxelBlock::new(MAT_DIRT));
}

#[test]
fn test_b5_boundary_crossing_corner_xyz() {
    let chunks = [
        IVec3::new(0, 0, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(1, 1, 0),
        IVec3::new(1, 0, 1),
        IVec3::new(0, 1, 1),
        IVec3::new(1, 1, 1),
    ];
    let mut store = create_store_with_chunks(&chunks, None);

    let v0 = IVec3::new(31, 31, 31);
    let v1 = IVec3::new(32, 32, 32);

    let (c0, l0) = world_voxel_to_chunk_and_local(v0);
    let (c1, l1) = world_voxel_to_chunk_and_local(v1);
    assert_eq!(c0, IVec3::new(0, 0, 0));
    assert_eq!(l0, IVec3::new(31, 31, 31));
    assert_eq!(c1, IVec3::new(1, 1, 1));
    assert_eq!(l1, IVec3::new(0, 0, 0));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(v0, VoxelBlock::new(MAT_STONE)));
    tx.add_edit(VoxelEdit::add(v1, VoxelBlock::new(MAT_DIRT)));

    let res = tx.commit(&mut store).expect("Corner crossing must succeed");
    assert_eq!(res.affected_chunks.len(), 2);
}

#[test]
fn test_b6_multi_chunk_spanning_transaction_8_chunks() {
    let mut chunks = Vec::new();
    for cx in 0..2 {
        for cy in 0..2 {
            for cz in 0..2 {
                chunks.push(IVec3::new(cx, cy, cz));
            }
        }
    }
    let mut store = create_store_with_chunks(&chunks, None);

    let mut tx = VoxelEditTransaction::new();
    for &c in &chunks {
        let v = c * CHUNK_SIZE + IVec3::new(15, 15, 15);
        tx.add_edit(VoxelEdit::add(v, VoxelBlock::new(MAT_STONE)));
    }

    let res = tx
        .commit(&mut store)
        .expect("8-chunk transaction must succeed");
    assert_eq!(res.affected_chunks.len(), 8);
    for &c in &chunks {
        let v = c * CHUNK_SIZE + IVec3::new(15, 15, 15);
        assert_eq!(store.get_voxel_world(v), VoxelBlock::new(MAT_STONE));
    }
}

// ============================================================================
// CATEGORY C: NEGATIVE COORDINATES
// ============================================================================

#[test]
fn test_c1_negative_coordinates_boundary_0_to_minus_1() {
    let chunks = [IVec3::new(-1, 0, 0), IVec3::new(0, 0, 0)];
    let mut store = create_store_with_chunks(&chunks, None);

    let v_pos = IVec3::new(0, 10, 10);
    let v_neg = IVec3::new(-1, 10, 10);

    let (c_pos, l_pos) = world_voxel_to_chunk_and_local(v_pos);
    let (c_neg, l_neg) = world_voxel_to_chunk_and_local(v_neg);

    assert_eq!(c_pos, IVec3::new(0, 0, 0));
    assert_eq!(l_pos, IVec3::new(0, 10, 10));
    assert_eq!(c_neg, IVec3::new(-1, 0, 0));
    assert_eq!(l_neg, IVec3::new(31, 10, 10));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(v_pos, VoxelBlock::new(MAT_STONE)));
    tx.add_edit(VoxelEdit::add(v_neg, VoxelBlock::new(MAT_DIRT)));

    let res = tx
        .commit(&mut store)
        .expect("0 to -1 boundary must succeed");
    assert_eq!(res.affected_chunks.len(), 2);
    assert_eq!(store.get_voxel_world(v_pos), VoxelBlock::new(MAT_STONE));
    assert_eq!(store.get_voxel_world(v_neg), VoxelBlock::new(MAT_DIRT));
}

#[test]
fn test_c2_negative_coordinates_boundary_minus_32_to_minus_33() {
    let chunks = [IVec3::new(-2, 0, 0), IVec3::new(-1, 0, 0)];
    let mut store = create_store_with_chunks(&chunks, None);

    let v_32 = IVec3::new(-32, 10, 10);
    let v_33 = IVec3::new(-33, 10, 10);

    let (c_32, l_32) = world_voxel_to_chunk_and_local(v_32);
    let (c_33, l_33) = world_voxel_to_chunk_and_local(v_33);

    assert_eq!(c_32, IVec3::new(-1, 0, 0));
    assert_eq!(l_32, IVec3::new(0, 10, 10));
    assert_eq!(c_33, IVec3::new(-2, 0, 0));
    assert_eq!(l_33, IVec3::new(31, 10, 10));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(v_32, VoxelBlock::new(MAT_STONE)));
    tx.add_edit(VoxelEdit::add(v_33, VoxelBlock::new(MAT_DIRT)));

    let res = tx
        .commit(&mut store)
        .expect("-32 to -33 boundary must succeed");
    assert_eq!(res.affected_chunks.len(), 2);
    assert_eq!(store.get_voxel_world(v_32), VoxelBlock::new(MAT_STONE));
    assert_eq!(store.get_voxel_world(v_33), VoxelBlock::new(MAT_DIRT));
}

#[test]
fn test_c3_negative_coordinates_all_octants_roundtrip() {
    let test_points = [
        IVec3::new(0, 0, 0),
        IVec3::new(-1, -1, -1),
        IVec3::new(-32, -32, -32),
        IVec3::new(-33, -33, -33),
        IVec3::new(31, -33, 0),
        IVec3::new(-33, 0, 31),
        IVec3::new(-64, 63, -65),
        IVec3::new(100, -100, 200),
    ];

    for &p in &test_points {
        let (c, l) = world_voxel_to_chunk_and_local(p);
        assert!(
            l.x >= 0 && l.x < 32 && l.y >= 0 && l.y < 32 && l.z >= 0 && l.z < 32,
            "Local coordinate out of bounds [0..31]: {:?}",
            l
        );
        let roundtrip = chunk_and_local_to_world_voxel(c, l);
        assert_eq!(
            roundtrip, p,
            "Lossless round-trip failed: {} != {}",
            roundtrip, p
        );
    }
}

#[test]
fn test_c4_negative_coordinates_crater_and_arbitrary_edits() {
    let chunks = [
        IVec3::new(-2, -2, -2),
        IVec3::new(-1, -2, -2),
        IVec3::new(-2, -1, -2),
        IVec3::new(-2, -2, -1),
        IVec3::new(-1, -1, -1),
    ];
    let mut store = create_store_with_chunks(&chunks, Some(MAT_STONE));

    // Arbitrary edit across negative chunks
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(-32, -32, -32)));
    tx.add_edit(VoxelEdit::remove(IVec3::new(-33, -33, -33)));
    tx.add_edit(VoxelEdit::replace(
        IVec3::new(-31, -31, -31),
        VoxelBlock::new(MAT_STONE),
        VoxelBlock::new(MAT_WOOD),
    ));

    let res = tx
        .commit(&mut store)
        .expect("Negative coordinate multi-chunk edits must succeed");
    assert_eq!(res.delta.len(), 3);
    assert_eq!(
        store.get_voxel_world(IVec3::new(-32, -32, -32)),
        VoxelBlock::AIR
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(-33, -33, -33)),
        VoxelBlock::AIR
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(-31, -31, -31)),
        VoxelBlock::new(MAT_WOOD)
    );
}

// ============================================================================
// CATEGORY D: STRUCTURAL CONSISTENCY
// ============================================================================

#[test]
fn test_d1_structural_solid_to_solid_replacement_emits_no_detachment_seeds() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::replace(
        IVec3::new(10, 10, 10),
        VoxelBlock::new(MAT_STONE),
        VoxelBlock::new(MAT_WOOD),
    ));

    let res = tx.commit(&mut store).expect("Replace succeeds");
    assert_eq!(res.structural_events.len(), 1);
    match &res.structural_events[0].mutation {
        StructuralMutationType::VoxelReplaced { .. } => {}
        other => panic!("Expected VoxelReplaced, got: {:?}", other),
    }

    let seeds = StructuralSystem::collect_candidate_seeds(&res.structural_events, &store);
    assert!(
        seeds.is_empty(),
        "Solid to solid replace must NOT produce candidate seeds for detachment"
    );
}

#[test]
fn test_d2_structural_air_to_solid_placement_emits_no_detachment_seeds() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(10, 10, 10),
        VoxelBlock::new(MAT_STONE),
    ));

    let res = tx.commit(&mut store).expect("Add succeeds");
    assert_eq!(res.structural_events.len(), 1);
    match &res.structural_events[0].mutation {
        StructuralMutationType::VoxelPlaced { .. } => {}
        other => panic!("Expected VoxelPlaced, got: {:?}", other),
    }

    let seeds = StructuralSystem::collect_candidate_seeds(&res.structural_events, &store);
    assert!(
        seeds.is_empty(),
        "Air to solid placement must NOT produce candidate seeds for detachment"
    );
}

#[test]
fn test_d3_structural_solid_to_air_removal_emits_valid_seeds() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(10, 10, 10)));

    let res = tx.commit(&mut store).expect("Remove succeeds");
    assert_eq!(res.structural_events.len(), 1);
    match &res.structural_events[0].mutation {
        StructuralMutationType::VoxelRemoved { .. } => {}
        other => panic!("Expected VoxelRemoved, got: {:?}", other),
    }

    let seeds = StructuralSystem::collect_candidate_seeds(&res.structural_events, &store);
    assert_eq!(
        seeds.len(),
        6,
        "Single interior removal in solid block must yield 6 adjacent solid neighbor seeds"
    );
}

#[test]
fn test_d4_structural_column_removal_detaches_single_component() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let mut anchor_policy = AnchorPolicy::default();
    anchor_policy.register_anchor_material(MAT_STONE);
    let mut struct_sys = StructuralSystem::new(anchor_policy);

    // Bedrock anchor at y=0, wood pillar at y=1..3 and wood block at y=4
    store.set_voxel_world(IVec3::new(5, 0, 5), VoxelBlock::new(MAT_STONE));
    for y in 1..=4 {
        store.set_voxel_world(IVec3::new(5, y, 5), VoxelBlock::new(MAT_WOOD));
    }

    // Remove pillar middle (y=2)
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(5, 2, 5)));

    let res = tx.commit(&mut store).expect("Pillar removal succeeds");
    let aggregates = struct_sys.reconcile_events(&res.structural_events, &mut store);

    assert_eq!(
        aggregates.len(),
        1,
        "Must produce exactly 1 detached aggregate"
    );
    assert!(
        aggregates[0]
            .iter_world_voxels()
            .any(|(pos, _)| pos == IVec3::new(5, 4, 5)),
        "Top block must be detached"
    );
}

#[test]
fn test_d5_structural_multiple_removals_deduplicate_seeds_deterministically() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));

    let mut tx = VoxelEditTransaction::new();
    // Remove 2 adjacent voxels: (10, 10, 10) and (11, 10, 10)
    tx.add_edit(VoxelEdit::remove(IVec3::new(10, 10, 10)));
    tx.add_edit(VoxelEdit::remove(IVec3::new(11, 10, 10)));

    let res = tx.commit(&mut store).expect("Removals succeed");
    let seeds = StructuralSystem::collect_candidate_seeds(&res.structural_events, &store);

    // 6 + 6 - 2 shared = 10 unique solid neighbors
    assert_eq!(
        seeds.len(),
        10,
        "Adjacent removals must deduplicate candidate seeds"
    );
}

#[test]
fn test_d6_structural_boundary_crossing_component_detachment() {
    let chunks = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)];
    let mut store = create_store_with_chunks(&chunks, None);
    let mut anchor_policy = AnchorPolicy::default();
    anchor_policy.register_anchor_material(MAT_STONE);
    let mut struct_sys = StructuralSystem::new(anchor_policy);

    // Bedrock anchor at (31, 0, 5)
    store.set_voxel_world(IVec3::new(31, 0, 5), VoxelBlock::new(MAT_STONE));
    // Bridge across border: (31, 5, 5) -> (32, 5, 5)
    store.set_voxel_world(IVec3::new(31, 5, 5), VoxelBlock::new(MAT_WOOD));
    store.set_voxel_world(IVec3::new(32, 5, 5), VoxelBlock::new(MAT_WOOD));

    // Connect anchor to bridge via (31, 1..4, 5) of wood
    for y in 1..=4 {
        store.set_voxel_world(IVec3::new(31, y, 5), VoxelBlock::new(MAT_WOOD));
    }

    // Cut connection at y=2
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::remove(IVec3::new(31, 2, 5)));

    let res = tx.commit(&mut store).expect("Cut succeeds");
    let aggregates = struct_sys.reconcile_events(&res.structural_events, &mut store);

    assert_eq!(
        aggregates.len(),
        1,
        "Must extract 1 detached cross-chunk aggregate"
    );
    assert!(aggregates[0]
        .iter_world_voxels()
        .any(|(pos, _)| pos == IVec3::new(31, 5, 5)));
    assert!(aggregates[0]
        .iter_world_voxels()
        .any(|(pos, _)| pos == IVec3::new(32, 5, 5)));
}

// ============================================================================
// CATEGORY E: PERSISTENCE & REVISION INTERACTION
// ============================================================================

#[test]
fn test_e1_persistence_save_dirty_marked_on_commit() {
    let chunks = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)];
    let mut store = create_store_with_chunks(&chunks, None);

    // Initial state: dirty flags cleared
    assert_eq!(store.get(&IVec3::new(0, 0, 0)).unwrap().dirty_flags, 0);
    assert_eq!(store.get(&IVec3::new(1, 0, 0)).unwrap().dirty_flags, 0);

    // Edit on border: (31, 5, 5) in chunk (0, 0, 0)
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(31, 5, 5),
        VoxelBlock::new(MAT_STONE),
    ));
    tx.commit(&mut store).expect("Commit succeeds");

    let chunk0 = store.get(&IVec3::new(0, 0, 0)).unwrap();
    let chunk1 = store.get(&IVec3::new(1, 0, 0)).unwrap();

    // Directly mutated chunk receives SAVE_DIRTY, VOXEL_DIRTY, MESH_DIRTY, STRUCTURAL_DIRTY
    assert!(chunk0.dirty_flags & dirty_flags::SAVE_DIRTY != 0);
    assert!(chunk0.dirty_flags & dirty_flags::VOXEL_DIRTY != 0);
    assert!(chunk0.dirty_flags & dirty_flags::MESH_DIRTY != 0);

    // Neighbor chunk (1, 0, 0) receives only MESH_DIRTY, NOT SAVE_DIRTY
    assert_eq!(chunk1.dirty_flags, dirty_flags::MESH_DIRTY);
    assert_eq!(chunk1.dirty_flags & dirty_flags::SAVE_DIRTY, 0);
}

#[test]
fn test_e2_persistence_revision_incremented_per_voxel_edit() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let rev_initial = store.get(&IVec3::ZERO).unwrap().revision;

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(1, 1, 1),
        VoxelBlock::new(MAT_STONE),
    ));
    tx.add_edit(VoxelEdit::add(
        IVec3::new(2, 2, 2),
        VoxelBlock::new(MAT_DIRT),
    ));
    tx.commit(&mut store).expect("Commit succeeds");

    let rev_after = store.get(&IVec3::ZERO).unwrap().revision;
    assert_eq!(
        rev_after,
        rev_initial + 2,
        "Each voxel change increments chunk revision by 1"
    );
}

#[test]
fn test_e3_persistence_failed_validation_causes_zero_save_dirty_or_revision() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let rev_initial = store.get(&IVec3::ZERO).unwrap().revision;
    let flags_initial = store.get(&IVec3::ZERO).unwrap().dirty_flags;

    let mut tx = VoxelEditTransaction::new();
    // Attempt Remove on air (will fail validation)
    tx.add_edit(VoxelEdit::remove(IVec3::new(5, 5, 5)));

    let err = tx.commit(&mut store);
    assert!(err.is_err());

    let chunk = store.get(&IVec3::ZERO).unwrap();
    assert_eq!(
        chunk.revision, rev_initial,
        "Revision must remain unchanged"
    );
    assert_eq!(
        chunk.dirty_flags, flags_initial,
        "Dirty flags must remain unchanged"
    );
}

#[test]
fn test_e4_persistence_stale_async_save_job_contract() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);

    let mut tx1 = VoxelEditTransaction::new();
    tx1.add_edit(VoxelEdit::add(
        IVec3::new(1, 1, 1),
        VoxelBlock::new(MAT_STONE),
    ));
    tx1.commit(&mut store).unwrap();
    let saved_rev = store.get(&IVec3::ZERO).unwrap().revision;

    // Concurrently a second edit arrives before async save completes
    let mut tx2 = VoxelEditTransaction::new();
    tx2.add_edit(VoxelEdit::add(
        IVec3::new(2, 2, 2),
        VoxelBlock::new(MAT_DIRT),
    ));
    tx2.commit(&mut store).unwrap();
    let current_rev = store.get(&IVec3::ZERO).unwrap().revision;
    assert!(current_rev > saved_rev);

    // Stale save job completion attempts to clear SAVE_DIRTY with saved_rev
    let chunk = store.get_mut(&IVec3::ZERO).unwrap();
    let cleared = chunk.clear_dirty_if_revision_matched(dirty_flags::SAVE_DIRTY, saved_rev);

    assert!(
        !cleared,
        "Stale save job with old revision must NOT clear SAVE_DIRTY"
    );
    assert!(
        chunk.dirty_flags & dirty_flags::SAVE_DIRTY != 0,
        "SAVE_DIRTY must remain set for newer revision"
    );
}

#[test]
fn test_e5_persistence_stale_async_mesh_job_contract() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);

    let mut tx1 = VoxelEditTransaction::new();
    tx1.add_edit(VoxelEdit::add(
        IVec3::new(1, 1, 1),
        VoxelBlock::new(MAT_STONE),
    ));
    tx1.commit(&mut store).unwrap();
    let meshed_rev = store.get(&IVec3::ZERO).unwrap().revision;

    // Another edit occurs
    let mut tx2 = VoxelEditTransaction::new();
    tx2.add_edit(VoxelEdit::add(
        IVec3::new(2, 2, 2),
        VoxelBlock::new(MAT_DIRT),
    ));
    tx2.commit(&mut store).unwrap();

    let chunk = store.get_mut(&IVec3::ZERO).unwrap();
    let cleared = chunk.clear_dirty_if_revision_matched(dirty_flags::MESH_DIRTY, meshed_rev);

    assert!(
        !cleared,
        "Stale mesh job with old revision must NOT clear MESH_DIRTY"
    );
    assert!(
        chunk.dirty_flags & dirty_flags::MESH_DIRTY != 0,
        "MESH_DIRTY must remain set"
    );
}

// ============================================================================
// CATEGORY F: DETERMINISTIC REPLAY
// ============================================================================

#[test]
fn test_f1_deterministic_replay_identical_world_state() {
    let mut store1 = create_store_with_chunk(IVec3::ZERO, None);
    let mut store2 = create_store_with_chunk(IVec3::ZERO, None);

    let edits = vec![
        VoxelEdit::add(IVec3::new(1, 2, 3), VoxelBlock::new(MAT_STONE)),
        VoxelEdit::add(IVec3::new(4, 5, 6), VoxelBlock::new(MAT_DIRT)),
        VoxelEdit::add(IVec3::new(7, 8, 9), VoxelBlock::new(MAT_WOOD)),
    ];

    let mut tx1 = VoxelEditTransaction::new();
    tx1.add_edits(edits.clone());
    let res1 = tx1.commit(&mut store1).unwrap();

    let mut tx2 = VoxelEditTransaction::new();
    tx2.add_edits(edits);
    let res2 = tx2.commit(&mut store2).unwrap();

    assert_eq!(res1.delta, res2.delta);
    assert_eq!(res1.affected_chunks, res2.affected_chunks);
    assert_eq!(res1.structural_events, res2.structural_events);

    let c1 = store1.get(&IVec3::ZERO).unwrap();
    let c2 = store2.get(&IVec3::ZERO).unwrap();
    assert_eq!(c1.voxels.as_ref(), c2.voxels.as_ref());
    assert_eq!(c1.dirty_flags, c2.dirty_flags);
    assert_eq!(c1.revision, c2.revision);
    assert_eq!(c1.non_air_count, c2.non_air_count);
}

#[test]
fn test_f2_deterministic_replay_shuffled_input_edits_produce_identical_delta() {
    let mut store1 = create_store_with_chunk(IVec3::ZERO, None);
    let mut store2 = create_store_with_chunk(IVec3::ZERO, None);

    let mut edits = Vec::new();
    for i in 0..15 {
        edits.push(VoxelEdit::add(
            IVec3::new(i * 2, i, 10),
            VoxelBlock::new(MAT_STONE),
        ));
    }

    let mut shuffled = edits.clone();
    // Deterministic pseudo-shuffle
    shuffled.reverse();
    shuffled.swap(2, 8);
    shuffled.swap(4, 11);

    let mut tx1 = VoxelEditTransaction::new();
    tx1.add_edits(edits);
    let res1 = tx1.commit(&mut store1).unwrap();

    let mut tx2 = VoxelEditTransaction::new();
    tx2.add_edits(shuffled);
    let res2 = tx2.commit(&mut store2).unwrap();

    // Canonical sorting by (x, y, z) guarantees identical delta ordering
    assert_eq!(res1.delta.deltas, res2.delta.deltas);
    assert_eq!(res1.affected_chunks, res2.affected_chunks);
    assert_eq!(res1.structural_events, res2.structural_events);

    let c1 = store1.get(&IVec3::ZERO).unwrap();
    let c2 = store2.get(&IVec3::ZERO).unwrap();
    assert_eq!(c1.voxels.as_ref(), c2.voxels.as_ref());
}

#[test]
fn test_f3_deterministic_replay_cross_chunk_and_negative_coords() {
    let chunks = [IVec3::new(-1, -1, 0), IVec3::new(0, 0, 0)];
    let mut store1 = create_store_with_chunks(&chunks, None);
    let mut store2 = create_store_with_chunks(&chunks, None);

    let edits = vec![
        VoxelEdit::add(IVec3::new(-1, -1, 5), VoxelBlock::new(MAT_STONE)),
        VoxelEdit::add(IVec3::new(0, 0, 5), VoxelBlock::new(MAT_DIRT)),
        VoxelEdit::add(IVec3::new(-32, -32, 5), VoxelBlock::new(MAT_WOOD)),
    ];

    let mut tx1 = VoxelEditTransaction::new();
    tx1.add_edits(edits.clone());
    let res1 = tx1.commit(&mut store1).unwrap();

    let mut tx2 = VoxelEditTransaction::new();
    let mut rev_edits = edits;
    rev_edits.reverse();
    tx2.add_edits(rev_edits);
    let res2 = tx2.commit(&mut store2).unwrap();

    assert_eq!(res1.delta.deltas, res2.delta.deltas);
    for &c in &chunks {
        let c1 = store1.get(&c).unwrap();
        let c2 = store2.get(&c).unwrap();
        assert_eq!(c1.voxels.as_ref(), c2.voxels.as_ref());
        assert_eq!(c1.revision, c2.revision);
        assert_eq!(c1.dirty_flags, c2.dirty_flags);
    }
}

// ============================================================================
// CATEGORY G: CHUNK INVALIDATION SYMMETRY
// ============================================================================

#[test]
fn test_g1_invalidation_six_faces_resident_neighbors() {
    let center = IVec3::ZERO;
    let neighbors = [
        (IVec3::new(-1, 0, 0), IVec3::new(0, 16, 16)), // -X
        (IVec3::new(1, 0, 0), IVec3::new(31, 16, 16)), // +X
        (IVec3::new(0, -1, 0), IVec3::new(16, 0, 16)), // -Y
        (IVec3::new(0, 1, 0), IVec3::new(16, 31, 16)), // +Y
        (IVec3::new(0, 0, -1), IVec3::new(16, 16, 0)), // -Z
        (IVec3::new(0, 0, 1), IVec3::new(16, 16, 31)), // +Z
    ];

    for (n_coord, voxel_pos) in neighbors {
        let mut store = create_store_with_chunks(&[center, n_coord], None);

        let mut tx = VoxelEditTransaction::new();
        tx.add_edit(VoxelEdit::add(voxel_pos, VoxelBlock::new(MAT_STONE)));
        let res = tx.commit(&mut store).expect("Face boundary edit succeeds");

        assert!(
            res.mesh_invalidation_chunks.contains(&n_coord),
            "Mesh invalidation chunks must contain neighbor {:?}",
            n_coord
        );
        let n_chunk = store.get(&n_coord).unwrap();
        assert_eq!(
            n_chunk.dirty_flags & dirty_flags::MESH_DIRTY,
            dirty_flags::MESH_DIRTY,
            "Neighbor {:?} must receive MESH_DIRTY",
            n_coord
        );
    }
}

#[test]
fn test_g2_invalidation_edge_and_corner_neighbors() {
    let center = IVec3::ZERO;
    let n_x = IVec3::new(1, 0, 0);
    let n_y = IVec3::new(0, 1, 0);
    let n_z = IVec3::new(0, 0, 1);
    let mut store = create_store_with_chunks(&[center, n_x, n_y, n_z], None);

    // Corner voxel at (31, 31, 31) in chunk (0, 0, 0) touches +X, +Y, +Z faces
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(31, 31, 31),
        VoxelBlock::new(MAT_STONE),
    ));
    let res = tx.commit(&mut store).expect("Corner edit succeeds");

    assert!(res.mesh_invalidation_chunks.contains(&n_x));
    assert!(res.mesh_invalidation_chunks.contains(&n_y));
    assert!(res.mesh_invalidation_chunks.contains(&n_z));

    assert_eq!(
        store.get(&n_x).unwrap().dirty_flags & dirty_flags::MESH_DIRTY,
        dirty_flags::MESH_DIRTY
    );
    assert_eq!(
        store.get(&n_y).unwrap().dirty_flags & dirty_flags::MESH_DIRTY,
        dirty_flags::MESH_DIRTY
    );
    assert_eq!(
        store.get(&n_z).unwrap().dirty_flags & dirty_flags::MESH_DIRTY,
        dirty_flags::MESH_DIRTY
    );
}

#[test]
fn test_g3_invalidation_unloaded_neighbor_not_fabricated_or_mutated() {
    // Only chunk (0, 0, 0) is resident
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    assert_eq!(store.resident_count(), 1);

    // Voxel at boundary x=0 touches unloaded neighbor (-1, 0, 0)
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(0, 16, 16),
        VoxelBlock::new(MAT_STONE),
    ));

    let res = tx.commit(&mut store).expect("Boundary edit succeeds");

    // Neighbor (-1, 0, 0) is in mesh_invalidation_chunks proposal
    assert!(res.mesh_invalidation_chunks.contains(&IVec3::new(-1, 0, 0)));

    // But MUST NOT be fabricated in store
    assert!(!store.is_chunk_resident(&IVec3::new(-1, 0, 0)));
    assert_eq!(
        store.resident_count(),
        1,
        "Unloaded neighbor must NOT be fabricated"
    );
}

#[test]
fn test_g4_invalidation_negative_boundary_neighbors() {
    let c_neg = IVec3::new(-1, 0, 0);
    let c_pos = IVec3::new(0, 0, 0);
    let mut store = create_store_with_chunks(&[c_neg, c_pos], None);

    // Voxel at (-1, 16, 16) has chunk (-1, 0, 0) and local x=31 (+X face)
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(-1, 16, 16),
        VoxelBlock::new(MAT_STONE),
    ));
    let res = tx
        .commit(&mut store)
        .expect("Negative boundary edit succeeds");

    assert!(res.mesh_invalidation_chunks.contains(&c_pos));
    let chunk_pos = store.get(&c_pos).unwrap();
    assert_eq!(
        chunk_pos.dirty_flags & dirty_flags::MESH_DIRTY,
        dirty_flags::MESH_DIRTY
    );
}

// ============================================================================
// CATEGORY H: TRANSACTIONAL REVERT
// ============================================================================

#[test]
fn test_h1_transaction_validation_failure_before_equals_after() {
    let mut store = create_store_with_chunk(IVec3::ZERO, Some(MAT_STONE));
    let c_before = store.get(&IVec3::ZERO).unwrap().clone();

    let mut tx = VoxelEditTransaction::new();
    // Intentionally fail: Add on existing solid
    tx.add_edit(VoxelEdit::add(
        IVec3::new(5, 5, 5),
        VoxelBlock::new(MAT_DIRT),
    ));

    let err = tx.commit(&mut store);
    assert!(err.is_err());

    let c_after = store.get(&IVec3::ZERO).unwrap();
    assert_eq!(c_before.voxels.as_ref(), c_after.voxels.as_ref());
    assert_eq!(c_before.dirty_flags, c_after.dirty_flags);
    assert_eq!(c_before.revision, c_after.revision);
    assert_eq!(c_before.non_air_count, c_after.non_air_count);
}

#[test]
fn test_h2_transaction_revert_crucial_multi_chunk_guardrail() {
    // Guardrail 13: Exact pattern
    // 1. Prepare multiple resident chunks
    let chunks = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)];
    let mut store = create_store_with_chunks(&chunks, None);

    // Initial setup with some voxels
    store.set_voxel_world(IVec3::new(10, 10, 10), VoxelBlock::new(MAT_STONE));
    store.set_voxel_world(IVec3::new(31, 15, 15), VoxelBlock::new(MAT_STONE));
    store.set_voxel_world(IVec3::new(32, 15, 15), VoxelBlock::new(MAT_DIRT));
    // Clear dirty flags to have known baseline
    for &c in &chunks {
        store.get_mut(&c).unwrap().clear_dirty(dirty_flags::ALL);
    }

    // 2. Capture their pre-state
    let pre_c0 = store.get(&IVec3::new(0, 0, 0)).unwrap().clone();
    let pre_c1 = store.get(&IVec3::new(1, 0, 0)).unwrap().clone();

    // 3. Commit multi-chunk transaction touching directly mutated chunks and boundary neighbors
    let mut tx = VoxelEditTransaction::new();
    // Boundary-crossing edits
    tx.add_edit(VoxelEdit::remove(IVec3::new(31, 15, 15)));
    tx.add_edit(VoxelEdit::replace(
        IVec3::new(32, 15, 15),
        VoxelBlock::new(MAT_DIRT),
        VoxelBlock::new(MAT_WOOD),
    ));
    // Internal edit
    tx.add_edit(VoxelEdit::add(
        IVec3::new(5, 5, 5),
        VoxelBlock::new(MAT_STONE),
    ));

    let commit_res = tx.commit(&mut store).expect("Commit must succeed");

    // 4. Verify mutation occurred
    assert_eq!(
        store.get_voxel_world(IVec3::new(31, 15, 15)),
        VoxelBlock::AIR
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(32, 15, 15)),
        VoxelBlock::new(MAT_WOOD)
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(5, 5, 5)),
        VoxelBlock::new(MAT_STONE)
    );
    assert!(store.get(&IVec3::new(0, 0, 0)).unwrap().revision > pre_c0.revision);
    assert!(store.get(&IVec3::new(1, 0, 0)).unwrap().revision > pre_c1.revision);

    // 5. Call revert()
    commit_res.revert(&mut store).expect("Revert must succeed");

    // 6. Verify exact equality with the pre-commit state
    let post_c0 = store.get(&IVec3::new(0, 0, 0)).unwrap();
    let post_c1 = store.get(&IVec3::new(1, 0, 0)).unwrap();

    assert_eq!(
        post_c0.voxels.as_ref(),
        pre_c0.voxels.as_ref(),
        "Chunk 0 voxels must match pre-commit exactly"
    );
    assert_eq!(
        post_c1.voxels.as_ref(),
        pre_c1.voxels.as_ref(),
        "Chunk 1 voxels must match pre-commit exactly"
    );
    assert_eq!(
        post_c0.non_air_count, pre_c0.non_air_count,
        "Chunk 0 non_air_count must match pre-commit"
    );
    assert_eq!(
        post_c1.non_air_count, pre_c1.non_air_count,
        "Chunk 1 non_air_count must match pre-commit"
    );
    assert_eq!(
        post_c0.dirty_flags, pre_c0.dirty_flags,
        "Chunk 0 dirty_flags must match pre-commit"
    );
    assert_eq!(
        post_c1.dirty_flags, pre_c1.dirty_flags,
        "Chunk 1 dirty_flags must match pre-commit"
    );
    assert_eq!(
        post_c0.revision, pre_c0.revision,
        "Chunk 0 revision must match pre-commit"
    );
    assert_eq!(
        post_c1.revision, pre_c1.revision,
        "Chunk 1 revision must match pre-commit"
    );
}

#[test]
fn test_h3_transaction_revert_preflight_chunk_not_resident() {
    // Guardrail 14: Revert preflight safety
    let chunks = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)];
    let mut store = create_store_with_chunks(&chunks, None);

    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(5, 5, 5),
        VoxelBlock::new(MAT_STONE),
    ));
    tx.add_edit(VoxelEdit::add(
        IVec3::new(35, 5, 5),
        VoxelBlock::new(MAT_DIRT),
    ));

    let commit_res = tx.commit(&mut store).expect("Commit succeeds");

    // 2. Make chunk (1, 0, 0) non-resident via store.remove()
    store.remove(&IVec3::new(1, 0, 0));
    assert!(!store.is_chunk_resident(&IVec3::new(1, 0, 0)));

    // 3. Call revert()
    let err = commit_res
        .revert(&mut store)
        .expect_err("Revert must fail preflight");

    // 4. Expect ChunkNotResident
    match err {
        VoxelEditError::ChunkNotResident { chunk_coord } => {
            assert_eq!(chunk_coord, IVec3::new(1, 0, 0));
        }
        other => panic!("Expected ChunkNotResident, got: {:?}", other),
    }

    // 5. Verify no partial revert occurred on chunk (0, 0, 0)
    // The voxel added to chunk (0, 0, 0) at (5, 5, 5) must still be STONE (not partially reverted)
    assert_eq!(
        store.get_voxel_world(IVec3::new(5, 5, 5)),
        VoxelBlock::new(MAT_STONE),
        "Chunk 0 must NOT have suffered partial rollback"
    );
}

#[test]
fn test_h4_transaction_revert_unchanged_authority_contract() {
    // Guardrail 5: Contract test showing revert operates under unchanged-authority assumption
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let mut tx = VoxelEditTransaction::new();
    tx.add_edit(VoxelEdit::add(
        IVec3::new(12, 12, 12),
        VoxelBlock::new(MAT_STONE),
    ));

    let res = tx.commit(&mut store).unwrap();
    assert_eq!(
        store.get_voxel_world(IVec3::new(12, 12, 12)),
        VoxelBlock::new(MAT_STONE)
    );

    // Under unchanged authority (same ChunkStore, no interleaved modifications), revert is guaranteed
    let revert_res = res.revert(&mut store);
    assert!(
        revert_res.is_ok(),
        "Revert under unchanged authority must succeed"
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(12, 12, 12)),
        VoxelBlock::AIR
    );
}

#[test]
fn test_h5_transaction_empty_is_noop() {
    let mut store = create_store_with_chunk(IVec3::ZERO, None);
    let pre_chunk = store.get(&IVec3::ZERO).unwrap().clone();

    let tx = VoxelEditTransaction::new();
    let res = tx.commit(&mut store).expect("Empty commit succeeds");

    assert!(res.delta.is_empty());
    assert!(res.affected_chunks.is_empty());
    assert!(res.mesh_invalidation_chunks.is_empty());
    assert!(res.structural_events.is_empty());
    assert!(res.chunk_pre_states.is_empty());

    let post_chunk = store.get(&IVec3::ZERO).unwrap();
    assert_eq!(post_chunk.revision, pre_chunk.revision);
    assert_eq!(post_chunk.dirty_flags, pre_chunk.dirty_flags);
}
