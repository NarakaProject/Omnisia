use glam::IVec3;

use omnisia::chunk::{dirty_flags, Chunk};
use omnisia::material::{MaterialId, MaterialRegistry};
use omnisia::modding::runtime::ContentRuntime;
use omnisia::storage::{
    decompress_and_deserialize_chunk, serialize_and_compress_chunk, MemoryCompressedRegionStore,
    RegionStore,
};
use omnisia::streaming::jobs::{ChunkJobRequest, JobPriority, JobType};
use omnisia::streaming::memory::{MemoryBudget, MemoryUsage};
use omnisia::streaming::residency::{ResidencyState, ResidencyStateMachine};
use omnisia::streaming::scheduler::ChunkScheduler;
use omnisia::voxel::VoxelBlock;

fn get_test_material_registry() -> MaterialRegistry {
    ContentRuntime::build_runtime("content/core", "mods")
        .expect("Core Content harus berhasil dimuat untuk test suite")
        .materials
}

// ============================================================================
// 1. LIFECYCLE STATE MACHINE TESTS
// ============================================================================

#[test]
fn test_chunk_lifecycle_state_transitions() {
    // Valid transitions
    assert!(ResidencyStateMachine::is_valid_transition(
        ResidencyState::Unloaded,
        ResidencyState::Queued
    ));
    assert!(ResidencyStateMachine::is_valid_transition(
        ResidencyState::Queued,
        ResidencyState::Loading
    ));
    assert!(ResidencyStateMachine::is_valid_transition(
        ResidencyState::Loading,
        ResidencyState::Resident
    ));
    assert!(ResidencyStateMachine::is_valid_transition(
        ResidencyState::Resident,
        ResidencyState::Saving
    ));
    assert!(ResidencyStateMachine::is_valid_transition(
        ResidencyState::Saving,
        ResidencyState::Resident
    ));
    assert!(ResidencyStateMachine::is_valid_transition(
        ResidencyState::Resident,
        ResidencyState::Evicting
    ));
    assert!(ResidencyStateMachine::is_valid_transition(
        ResidencyState::Evicting,
        ResidencyState::Unloaded
    ));

    // Invalid transitions (harus ditolak tegas)
    assert!(!ResidencyStateMachine::is_valid_transition(
        ResidencyState::Unloaded,
        ResidencyState::Saving
    ));
    assert!(!ResidencyStateMachine::is_valid_transition(
        ResidencyState::Loading,
        ResidencyState::Evicting
    ));
    assert!(!ResidencyStateMachine::is_valid_transition(
        ResidencyState::Queued,
        ResidencyState::Saving
    ));
}

// ============================================================================
// 2. STALE JOB PROTECTION & REVISION TESTS
// ============================================================================

#[test]
fn test_stale_job_result_rejection() {
    let mut chunk = Chunk::new(IVec3::ZERO);
    assert_eq!(chunk.revision, 0);

    // Mutasi chunk menaikkan revisi
    chunk.set_voxel(0, 0, 0, VoxelBlock::new(MaterialId::STONE));
    assert_eq!(chunk.revision, 1);

    chunk.set_voxel(1, 1, 1, VoxelBlock::new(MaterialId::DIRT));
    assert_eq!(chunk.revision, 2);

    // Simulasi hasil meshing/saving lama dengan revisi 1 (sudah basi)
    let stale_revision = 1;
    let is_cleared = chunk.clear_dirty_if_revision_matched(dirty_flags::SAVE_DIRTY, stale_revision);
    assert!(
        !is_cleared,
        "Stale revision tidak boleh membersihkan dirty flag!"
    );
    assert!(chunk.is_dirty(dirty_flags::SAVE_DIRTY));

    // Hasil dengan revisi 2 (akurat) berhasil membersihkan dirty flag
    let valid_revision = 2;
    let is_cleared_valid =
        chunk.clear_dirty_if_revision_matched(dirty_flags::SAVE_DIRTY, valid_revision);
    assert!(is_cleared_valid);
    assert!(!chunk.is_dirty(dirty_flags::SAVE_DIRTY));
}

#[test]
fn test_dirty_chunk_mutation_during_save_race() {
    let mut chunk = Chunk::new(IVec3::ZERO);
    chunk.set_voxel(5, 5, 5, VoxelBlock::new(MaterialId::STONE));
    let save_started_revision = chunk.revision; // misal revision 1

    // Saat proses saving berlangsung di background worker, user memutasi voxel lain
    chunk.set_voxel(6, 6, 6, VoxelBlock::new(MaterialId::GOLD_ACCENT));
    assert_eq!(chunk.revision, save_started_revision + 1);

    // Save job selesai dengan save_started_revision (1)
    let cleared =
        chunk.clear_dirty_if_revision_matched(dirty_flags::SAVE_DIRTY, save_started_revision);
    assert!(!cleared);
    assert!(
        chunk.is_dirty(dirty_flags::SAVE_DIRTY),
        "SAVE_DIRTY harus tetap aktif karena ada mutasi baru saat proses save berlangsung!"
    );
}

// ============================================================================
// 3. PERSISTENCE VIA STABLE RESOURCE ID PALETTE TESTS
// ============================================================================

#[test]
fn test_save_load_stable_resource_id_palette_roundtrip() {
    let registry = get_test_material_registry();
    let mut chunk = Chunk::new(IVec3::new(-3, 2, -7));

    chunk.set_voxel(0, 0, 0, VoxelBlock::new(MaterialId::STONE));
    chunk.set_voxel(15, 10, 15, VoxelBlock::new(MaterialId::AG_CORE_CASING));
    chunk.set_voxel(31, 31, 31, VoxelBlock::new(MaterialId::GOLD_ACCENT));

    // 1. Serialisasi ke Zstd byte stream
    let compressed = serialize_and_compress_chunk(&chunk, &registry)
        .expect("Serialisasi chunk palette harus berhasil");
    assert!(!compressed.is_empty());

    // 2. Dekompresi dan deserialisasi
    let loaded = decompress_and_deserialize_chunk(&compressed, &registry)
        .expect("Dekompresi chunk palette harus berhasil");

    assert_eq!(loaded.position, chunk.position);
    assert_eq!(loaded.non_air_count, chunk.non_air_count);
    assert_eq!(loaded.get_voxel(0, 0, 0), chunk.get_voxel(0, 0, 0));
    assert_eq!(loaded.get_voxel(15, 10, 15), chunk.get_voxel(15, 10, 15));
    assert_eq!(loaded.get_voxel(31, 31, 31), chunk.get_voxel(31, 31, 31));
}

// ============================================================================
// 4. SCHEDULER PRIORITY, COALESCING, & TIE-BREAK TESTS
// ============================================================================

#[test]
fn test_duplicate_request_coalescing() {
    let mut scheduler = ChunkScheduler::new(2);
    let target_coord = IVec3::new(10, 0, 10);

    // Kirim 5 request identik
    for _ in 0..5 {
        scheduler.request_job(
            target_coord,
            JobType::LoadChunk,
            JobPriority::High,
            0,
            100.0,
        );
    }

    // Harus terkoalesi menjadi tepat 1 job dalam antrean
    assert_eq!(scheduler.pending_job_count(), 1);
}

#[test]
fn test_scheduler_deterministic_priority_and_tie_break() {
    let mut queue = std::collections::BinaryHeap::new();

    let req_low = ChunkJobRequest::new(
        1,
        IVec3::new(1, 0, 0),
        JobType::LoadChunk,
        JobPriority::Low,
        0,
        500.0,
    );
    let req_high = ChunkJobRequest::new(
        2,
        IVec3::new(2, 0, 0),
        JobType::LoadChunk,
        JobPriority::High,
        0,
        100.0,
    );
    let req_critical = ChunkJobRequest::new(
        3,
        IVec3::new(0, 0, 0),
        JobType::LoadChunk,
        JobPriority::Critical,
        0,
        10.0,
    );
    let req_high_closer = ChunkJobRequest::new(
        4,
        IVec3::new(3, 0, 0),
        JobType::LoadChunk,
        JobPriority::High,
        0,
        20.0,
    );

    queue.push(req_low);
    queue.push(req_high);
    queue.push(req_critical);
    queue.push(req_high_closer);

    // 1. Critical harus keluar pertama
    assert_eq!(queue.pop().unwrap().job_id, 3);
    // 2. High dengan jarak lebih dekat (20.0) harus keluar sebelum High (100.0)
    assert_eq!(queue.pop().unwrap().job_id, 4);
    // 3. High berikutnya (100.0)
    assert_eq!(queue.pop().unwrap().job_id, 2);
    // 4. Low keluar terakhir
    assert_eq!(queue.pop().unwrap().job_id, 1);
}

// ============================================================================
// 5. NEGATIVE COORDINATES & REGION STORE TESTS
// ============================================================================

#[test]
fn test_negative_coordinates_streaming_and_storage() {
    let registry = get_test_material_registry();
    let store = MemoryCompressedRegionStore::new();

    let test_coords = [
        IVec3::new(0, 0, 0),
        IVec3::new(-1, 0, 0),
        IVec3::new(0, -1, 0),
        IVec3::new(0, 0, -1),
        IVec3::new(-32, -32, -32),
        IVec3::new(-33, 10, -100),
    ];

    for &coord in &test_coords {
        let mut chunk = Chunk::new(coord);
        chunk.set_voxel(0, 0, 0, VoxelBlock::new(MaterialId::DIRT));
        store.save_chunk(&chunk, &registry).expect("Save gagal");
        assert!(store.has_chunk(coord));

        let loaded = store
            .load_chunk(coord, &registry)
            .expect("Load gagal")
            .unwrap();
        assert_eq!(loaded.position, coord);
        assert_eq!(loaded.get_voxel(0, 0, 0).material(), MaterialId::DIRT);
    }
}

// ============================================================================
// 6. MEMORY BUDGET & EVICTION PROTECTION TESTS
// ============================================================================

#[test]
fn test_memory_budget_enforcement() {
    let budget = MemoryBudget::with_chunk_limit(10);
    let usage_under = MemoryUsage::new(8, 0);
    assert!(!budget.is_over_budget(&usage_under));
    assert_eq!(budget.excess_chunks(&usage_under), 0);

    let usage_over = MemoryUsage::new(15, 0);
    assert!(budget.is_over_budget(&usage_over));
    assert_eq!(budget.excess_chunks(&usage_over), 5);
}

// ============================================================================
// 7. DISTANT LOD CONTRACT TESTS
// ============================================================================

#[test]
fn test_distant_lod_contract_rebuildable() {
    use omnisia::lod::{DistantRepresentation, HierarchicalLodStore, LodLevel};

    let mut lod_store = HierarchicalLodStore::new();
    let mut chunk = Chunk::new(IVec3::new(0, 0, 0));
    chunk.fill_material(MaterialId::STONE);

    // Build LOD1 & LOD2
    lod_store.build_from_chunk(&chunk, LodLevel::Lod1);
    lod_store.build_from_chunk(&chunk, LodLevel::Lod2);

    assert_eq!(lod_store.len(), 2);
    let sample = lod_store
        .get_aggregated_sample(IVec3::new(10, 10, 10), LodLevel::Lod1)
        .unwrap();
    assert_eq!(sample.dominant_material, MaterialId::STONE);
    assert_eq!(sample.occupancy_ratio, 255);

    // Invariant: LOD data is rebuildable from scratch
    lod_store.clear();
    assert!(lod_store.is_empty());
}
