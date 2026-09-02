use glam::IVec3;
use omnisia::chunk::Chunk;
use omnisia::material::MaterialRegistry;
use omnisia::modding::registry::BlockRegistry;
use omnisia::modding::resource_id::ResourceId;
use omnisia::modding::runtime::ContentRuntime;
use omnisia::streaming::store::ChunkStore;
use omnisia::structure::adjacency::is_face_adjacent;
use omnisia::structure::anchor::AnchorPolicy;
use omnisia::structure::connectivity::{
    check_structural_connectivity, ConnectivityConfig, ConnectivityStatus,
};
use omnisia::structure::events::{StructuralEvent, StructuralMutationType};
use omnisia::structure::manager::StructuralSystem;
use omnisia::voxel::VoxelBlock;
use omnisia::world::World;

fn get_test_registries() -> (MaterialRegistry, BlockRegistry) {
    let resolved = ContentRuntime::build_runtime("content/core", "mods")
        .expect("Core Content harus berhasil dimuat untuk test suite");
    (resolved.materials, resolved.blocks)
}

// ============================================================================
// 1. BASIC CONNECTIVITY & ANCHOR SEMANTICS
// ============================================================================

#[test]
fn test_structural_basic_connectivity_to_anchor() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);

    let stone_id = materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    // Verifikasi kebijakan data-driven: stone adalah anchor, wood bukan anchor
    assert!(anchor_policy.is_anchor_material(stone_id));
    assert!(!anchor_policy.is_anchor_material(wood_id));

    let mut store = ChunkStore::new();
    let chunk = Chunk::new(IVec3::ZERO);
    store.insert(chunk);

    // Bangun pilar: Anchor Stone di (0, 0, 0), Wood di (0, 1, 0), Wood di (0, 2, 0), Wood di (0, 3, 0)
    store.set_voxel_world(IVec3::new(0, 0, 0), VoxelBlock::new(stone_id));
    store.set_voxel_world(IVec3::new(0, 1, 0), VoxelBlock::new(wood_id));
    store.set_voxel_world(IVec3::new(0, 2, 0), VoxelBlock::new(wood_id));
    store.set_voxel_world(IVec3::new(0, 3, 0), VoxelBlock::new(wood_id));

    let config = ConnectivityConfig::default();

    // Periksa konektivitas ujung atas pilar kayu (0, 3, 0)
    let status =
        check_structural_connectivity(IVec3::new(0, 3, 0), &store, &anchor_policy, &config, None);

    assert_eq!(
        status,
        ConnectivityStatus::ConnectedToAnchor,
        "Pilar kayu harus terhubung ke anchor stone di dasar!"
    );
}

#[test]
fn test_detached_aggregate_extraction_after_break() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);
    let mut system = StructuralSystem::new(anchor_policy);

    let stone_id = materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));

    // Anchor Stone di (5, 5, 5) -> Wood di (5, 6, 5) -> Wood di (5, 7, 5)
    store.set_voxel_world(IVec3::new(5, 5, 5), VoxelBlock::new(stone_id));
    store.set_voxel_world(IVec3::new(5, 6, 5), VoxelBlock::new(wood_id));
    store.set_voxel_world(IVec3::new(5, 7, 5), VoxelBlock::new(wood_id));

    // Pemain menghancurkan balok penghubung di (5, 6, 5)
    let prev = store.get_voxel_world(IVec3::new(5, 6, 5));
    store.set_voxel_world(IVec3::new(5, 6, 5), VoxelBlock::AIR);

    let event = StructuralEvent::new(
        IVec3::new(5, 6, 5),
        StructuralMutationType::VoxelRemoved {
            previous_block: prev,
        },
    );

    let detached = system.process_event(&event, &mut store);

    // Gugusan di (5, 7, 5) kini putus dari anchor!
    assert_eq!(
        detached.len(),
        1,
        "Tepat satu aggregate lepas harus diekstraksi!"
    );
    let agg = &detached[0];
    assert_eq!(agg.voxel_count(), 1);
    assert_eq!(agg.world_coord_of(&agg.voxels[0]), IVec3::new(5, 7, 5));
    assert_eq!(agg.voxels[0].block.material(), wood_id);

    // Verifikasi Guardrail 5: Voxel yang terlepas telah dihapus dari ChunkStore otoritatif (No Double Ownership)
    assert!(
        store.get_voxel_world(IVec3::new(5, 7, 5)).is_air(),
        "Voxel yang diekstraksi harus diubah menjadi AIR pada chunk otoritatif!"
    );
}

#[test]
fn test_multiple_independent_structures_do_not_merge() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);
    let mut system = StructuralSystem::new(anchor_policy);

    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));

    // Dua struktur terpisah di udara tanpa anchor:
    // Struktur A di (2, 10, 2) & (2, 11, 2)
    // Struktur B di (15, 10, 15) & (15, 11, 15)
    store.set_voxel_world(IVec3::new(2, 10, 2), VoxelBlock::new(wood_id));
    store.set_voxel_world(IVec3::new(2, 11, 2), VoxelBlock::new(wood_id));

    store.set_voxel_world(IVec3::new(15, 10, 15), VoxelBlock::new(wood_id));
    store.set_voxel_world(IVec3::new(15, 11, 15), VoxelBlock::new(wood_id));

    // Trigger mutasi di sebelah struktur A
    let event_a = StructuralEvent::new(
        IVec3::new(2, 9, 2),
        StructuralMutationType::VoxelRemoved {
            previous_block: VoxelBlock::new(wood_id),
        },
    );
    let detached_a = system.process_event(&event_a, &mut store);

    assert_eq!(detached_a.len(), 1);
    assert_eq!(detached_a[0].voxel_count(), 2);

    // Struktur B harus belum terpengaruh
    assert_eq!(
        store.get_voxel_world(IVec3::new(15, 10, 15)).material(),
        wood_id
    );
    assert_eq!(
        store.get_voxel_world(IVec3::new(15, 11, 15)).material(),
        wood_id
    );
}

// ============================================================================
// 2. CHUNK BOUNDARY CONTINUITY & CHAIN A <-> B <-> C
// ============================================================================

#[test]
fn test_chunk_boundary_structural_continuity_multi_chunk_chain() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);
    let mut system = StructuralSystem::new(anchor_policy);

    let stone_id = materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    // Tiga chunk berurutan: Chunk 0 (0,0,0) -> Chunk 1 (1,0,0) -> Chunk 2 (2,0,0)
    store.insert(Chunk::new(IVec3::new(0, 0, 0)));
    store.insert(Chunk::new(IVec3::new(1, 0, 0)));
    store.insert(Chunk::new(IVec3::new(2, 0, 0)));

    // Anchor di Chunk 0 pada x = 28
    store.set_voxel_world(IVec3::new(28, 10, 10), VoxelBlock::new(stone_id));

    // Balok kayu menyambung melintasi perbatasan Chunk 0 ke Chunk 1 lalu Chunk 2:
    // x = 29..=31 (di Chunk 0)
    // x = 32..=63 (di Chunk 1)
    // x = 64..=70 (di Chunk 2)
    for x in 29..=70 {
        store.set_voxel_world(IVec3::new(x, 10, 10), VoxelBlock::new(wood_id));
    }

    let config = ConnectivityConfig::default();

    // 1. Verifikasi ujung balok di Chunk 2 (x = 70) terhubung ke anchor di Chunk 0
    let status = check_structural_connectivity(
        IVec3::new(70, 10, 10),
        &store,
        &system.anchor_policy,
        &config,
        None,
    );
    assert_eq!(
        status,
        ConnectivityStatus::ConnectedToAnchor,
        "Struktur melintasi 3 chunk (Chain A <-> B <-> C) harus tetap satu kesatuan terhubung!"
    );

    // 2. Putuskan balok di perbatasan Chunk 0 (x = 30)
    let prev = store.get_voxel_world(IVec3::new(30, 10, 10));
    store.set_voxel_world(IVec3::new(30, 10, 10), VoxelBlock::AIR);

    let event = StructuralEvent::new(
        IVec3::new(30, 10, 10),
        StructuralMutationType::VoxelRemoved {
            previous_block: prev,
        },
    );

    let detached = system.process_event(&event, &mut store);

    // Sisi kanan (x = 31 di Chunk 0, x = 32..63 di Chunk 1, x = 64..70 di Chunk 2) harus terlepas sebagai SATU aggregate!
    assert_eq!(detached.len(), 1);
    let agg = &detached[0];

    // Panjang voxel terlepas: dari 31 sampai 70 = 40 voxel
    assert_eq!(
        agg.voxel_count(),
        40,
        "Seluruh gugusan melintasi 3 chunk harus diekstraksi ke dalam satu DetachedAggregate!"
    );
    assert_eq!(agg.min_voxel, IVec3::new(31, 10, 10));
    assert_eq!(agg.max_voxel, IVec3::new(70, 10, 10));

    // Verifikasi chunk otoritatif kini kosong dari voxel yang lepas
    for x in 31..=70 {
        assert!(store.get_voxel_world(IVec3::new(x, 10, 10)).is_air());
    }
}

#[test]
fn test_negative_coordinates_structural_ownership() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);
    let mut system = StructuralSystem::new(anchor_policy);

    let stone_id = materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    // Dua chunk pada koordinat negatif: Chunk -2 (-2, 0, 0) dan Chunk -1 (-1, 0, 0)
    store.insert(Chunk::new(IVec3::new(-2, 0, 0)));
    store.insert(Chunk::new(IVec3::new(-1, 0, 0)));

    // Anchor di Chunk -2 (x = -40)
    store.set_voxel_world(IVec3::new(-40, 5, 5), VoxelBlock::new(stone_id));

    // Kayu dari x = -39 hingga -30 (melintasi batas x = -33 -> -32)
    // x = -39..=-33 berada di Chunk -2
    // x = -32..=-30 berada di Chunk -1
    for x in -39..=-30 {
        store.set_voxel_world(IVec3::new(x, 5, 5), VoxelBlock::new(wood_id));
    }

    let config = ConnectivityConfig::default();

    // Verifikasi konektivitas di x = -30 (Chunk -1) menuju anchor di x = -40 (Chunk -2)
    let status = check_structural_connectivity(
        IVec3::new(-30, 5, 5),
        &store,
        &system.anchor_policy,
        &config,
        None,
    );
    assert_eq!(
        status,
        ConnectivityStatus::ConnectedToAnchor,
        "Konektivitas melintasi batas koordinat negatif (-33 -> -32) harus benar!"
    );

    // Putuskan di x = -35
    let prev = store.get_voxel_world(IVec3::new(-35, 5, 5));
    store.set_voxel_world(IVec3::new(-35, 5, 5), VoxelBlock::AIR);

    let event = StructuralEvent::new(
        IVec3::new(-35, 5, 5),
        StructuralMutationType::VoxelRemoved {
            previous_block: prev,
        },
    );

    let detached = system.process_event(&event, &mut store);
    assert_eq!(detached.len(), 1);
    let agg = &detached[0];

    // Voxel lepas dari x = -34 hingga -30 = 5 voxel
    assert_eq!(agg.voxel_count(), 5);
    assert_eq!(agg.min_voxel, IVec3::new(-34, 5, 5));
    assert_eq!(agg.max_voxel, IVec3::new(-30, 5, 5));
}

// ============================================================================
// 3. 6-CONNECTED ADJACENCY & UNLOADED CHUNK GUARDS
// ============================================================================

#[test]
fn test_diagonal_non_connectivity_6_way() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);

    let stone_id = materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));

    // Anchor stone di (10, 10, 10)
    store.set_voxel_world(IVec3::new(10, 10, 10), VoxelBlock::new(stone_id));

    // Balok kayu bersentuhan secara diagonal saja (rusuk: +X, +Y -> (11, 11, 10))
    store.set_voxel_world(IVec3::new(11, 11, 10), VoxelBlock::new(wood_id));

    assert!(!is_face_adjacent(
        IVec3::new(10, 10, 10),
        IVec3::new(11, 11, 10)
    ));

    let config = ConnectivityConfig::default();
    let status = check_structural_connectivity(
        IVec3::new(11, 11, 10),
        &store,
        &anchor_policy,
        &config,
        None,
    );

    // Karena hanya diagonal, TIDAK TERHUBUNG ke anchor!
    match status {
        ConnectivityStatus::Detached { component_voxels } => {
            assert_eq!(component_voxels.len(), 1);
        }
        _ => panic!("Sentuhan diagonal 6-way tidak boleh dianggap terhubung ke anchor!"),
    }
}

#[test]
fn test_unloaded_neighbor_does_not_falsely_detach() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);

    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    // HANYA muat Chunk (0, 0, 0). Chunk (1, 0, 0) TIDAK DIMUAT (Unloaded)
    store.insert(Chunk::new(IVec3::ZERO));

    // Balok kayu berada di x = 31 (ujung timur chunk 0, menempel pada chunk 1 yang belum dimuat)
    store.set_voxel_world(IVec3::new(31, 15, 15), VoxelBlock::new(wood_id));

    let config = ConnectivityConfig::default();
    let status = check_structural_connectivity(
        IVec3::new(31, 15, 15),
        &store,
        &anchor_policy,
        &config,
        None,
    );

    // GUARDRAIL 3: Unloaded neighbor TIDAK BOLEH menjadi Air dan TIDAK BOLEH Detached!
    match status {
        ConnectivityStatus::PendingUnloadedNeighbor { unloaded_chunk, .. } => {
            assert_eq!(unloaded_chunk, IVec3::new(1, 0, 0));
        }
        other => panic!(
            "Unloaded neighbor harus menghasilkan PendingUnloadedNeighbor, tapi menghasilkan {:?}",
            other
        ),
    }
}

#[test]
fn test_search_budget_exceeded_does_not_falsely_detach() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);

    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    store.insert(Chunk::new(IVec3::ZERO));

    // Buat gugusan kayu 100 voxel di dalam interior chunk (menghindari perbatasan agar murni menguji budget limit)
    for x in 5..25 {
        for y in 5..10 {
            store.set_voxel_world(IVec3::new(x, y, 10), VoxelBlock::new(wood_id));
        }
    }

    // Set budget ketat hanya 15 voxel
    let config = ConnectivityConfig {
        max_voxels_budget: 15,
    };

    let status =
        check_structural_connectivity(IVec3::new(10, 5, 10), &store, &anchor_policy, &config, None);

    // GUARDRAIL 2: Budget habis TIDAK BOLEH menghasilkan Detached!
    match status {
        ConnectivityStatus::IndeterminateBudgetExceeded { visited_count } => {
            assert!(visited_count >= 15);
        }
        other => panic!(
            "Budget habis harus menghasilkan IndeterminateBudgetExceeded, tapi menghasilkan {:?}",
            other
        ),
    }
}

// ============================================================================
// 4. EVENT LOCALITY & DATA-DRIVEN ANCHOR TESTS
// ============================================================================

#[test]
fn test_event_driven_locality_no_global_search() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);
    let mut system = StructuralSystem::new(anchor_policy);

    let stone_id = materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    let mut store = ChunkStore::new();
    // Muat 9 chunk (lingkungan 3x3)
    for cz in -1..=1 {
        for cx in -1..=1 {
            let mut chunk = Chunk::new(IVec3::new(cx, 0, cz));
            // Isi lantai dasar dengan stone anchor
            for z in 0..32 {
                for x in 0..32 {
                    chunk.set_voxel(x, 0, z, VoxelBlock::new(stone_id));
                }
            }
            store.insert(chunk);
        }
    }

    // Bangun tiang kayu 5 blok di (5, 1..=5, 5)
    for y in 1..=5 {
        store.set_voxel_world(IVec3::new(5, y, 5), VoxelBlock::new(wood_id));
    }

    let initial_inspected = system.total_voxels_inspected;

    // Hapus balok di (5, 4, 5)
    let prev = store.get_voxel_world(IVec3::new(5, 4, 5));
    store.set_voxel_world(IVec3::new(5, 4, 5), VoxelBlock::AIR);

    let event = StructuralEvent::new(
        IVec3::new(5, 4, 5),
        StructuralMutationType::VoxelRemoved {
            previous_block: prev,
        },
    );

    system.process_event(&event, &mut store);

    let voxels_scanned = system.total_voxels_inspected - initial_inspected;

    // GUARDRAIL 10: Mutasi lokal tidak boleh memicu scan global (3x3 chunk = ~295,000 voxel!)
    // Pencarian harus terlokalisir secara ketat (< 50 voxel)
    assert!(
        voxels_scanned < 50,
        "Pencarian harus terlokalisir: hanya memeriksa {} voxel (bukan ratusan ribu)!",
        voxels_scanned
    );
}

#[test]
fn test_anchor_semantics_data_driven_block_registry() {
    let (materials, blocks) = get_test_registries();
    let anchor_policy = AnchorPolicy::from_registries(&materials, &blocks);

    let stone_id = materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let deepslate_id = materials
        .resolve_material_id(&ResourceId::core("deepslate").unwrap())
        .unwrap();
    let dirt_id = materials
        .resolve_material_id(&ResourceId::core("dirt").unwrap())
        .unwrap();
    let grass_id = materials
        .resolve_material_id(&ResourceId::core("grass").unwrap())
        .unwrap();

    // Sesuai content JSON: stone_block & deepslate_block memiliki structural_anchor = true
    assert!(
        anchor_policy.is_anchor_material(stone_id),
        "Stone harus menjadi anchor data-driven!"
    );
    assert!(
        anchor_policy.is_anchor_material(deepslate_id),
        "Deepslate harus menjadi anchor data-driven!"
    );

    // Dirt dan Grass BUKAN anchor
    assert!(
        !anchor_policy.is_anchor_material(dirt_id),
        "Dirt BUKAN anchor!"
    );
    assert!(
        !anchor_policy.is_anchor_material(grass_id),
        "Grass BUKAN anchor!"
    );
}

#[test]
fn test_world_production_live_set_voxel_world_integration() {
    let mut world = World::new();

    let stone_id = world
        .materials
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let wood_id = world
        .materials
        .resolve_material_id(&ResourceId::core("wood_oak").unwrap())
        .unwrap();

    // Pastikan chunk origin ada di store
    world.store.insert(Chunk::new(IVec3::ZERO));

    // Tempatkan tiang melalui production API World::set_voxel_world
    world.set_voxel_world(IVec3::new(10, 10, 10), VoxelBlock::new(stone_id));
    world.set_voxel_world(IVec3::new(10, 11, 10), VoxelBlock::new(wood_id));
    world.set_voxel_world(IVec3::new(10, 12, 10), VoxelBlock::new(wood_id));

    // Putuskan balok kayu di y = 11
    let detached = world.set_voxel_world(IVec3::new(10, 11, 10), VoxelBlock::AIR);

    // GUARDRAIL 1: StructuralEvent terintegrasi langsung di World::set_voxel_world
    assert_eq!(
        detached.len(),
        1,
        "World::set_voxel_world harus secara langsung menghasilkan detached aggregate!"
    );
    assert_eq!(detached[0].voxel_count(), 1);
    assert_eq!(
        detached[0].world_coord_of(&detached[0].voxels[0]),
        IVec3::new(10, 12, 10)
    );
    assert!(world.get_voxel_world(IVec3::new(10, 12, 10)).is_air());
}
