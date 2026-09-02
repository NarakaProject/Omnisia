use glam::IVec3;

use omnisia::chunk::dirty_flags;
use omnisia::material::MaterialRegistry;
use omnisia::modding::resource_id::ResourceId;
use omnisia::modding::runtime::ContentRuntime;
use omnisia::storage::{MemoryCompressedRegionStore, RegionStore};
use omnisia::streaming::generator::ChunkGenerator;
use omnisia::voxel::VoxelBlock;
use omnisia::worldgen::biome::{BiomeClassifier, BiomeType};
use omnisia::worldgen::climate::ClimateSample;
use omnisia::worldgen::config::WorldGenConfig;
use omnisia::worldgen::pipeline::ProceduralWorldGenerator;
use omnisia::worldgen::seed::{GeneratorVersion, WorldSeed};

fn get_test_material_registry() -> MaterialRegistry {
    ContentRuntime::build_runtime("content/core", "mods")
        .expect("Core Content harus berhasil dimuat untuk test suite")
        .materials
}

// ============================================================================
// 1. DETERMINISM & SEED TESTS
// ============================================================================

#[test]
fn test_seed_determinism() {
    let registry = get_test_material_registry();
    let seed = WorldSeed::from_string("omnisia-test-seed-42");
    let config = WorldGenConfig::new(seed);
    let generator1 = ProceduralWorldGenerator::new(config);
    let generator2 = ProceduralWorldGenerator::new(config);

    let test_coords = [
        IVec3::new(0, 0, 0),
        IVec3::new(5, 0, -3),
        IVec3::new(-10, 0, 20),
        IVec3::new(-1, -1, -1),
    ];

    for coord in test_coords {
        let chunk1 = generator1.generate_chunk(coord, &registry);
        let chunk2 = generator2.generate_chunk(coord, &registry);

        assert_eq!(chunk1.position, chunk2.position);
        assert_eq!(chunk1.non_air_count, chunk2.non_air_count);
        assert_eq!(
            chunk1.voxels, chunk2.voxels,
            "Voxel data harus 100% identik byte-for-byte!"
        );
    }
}

#[test]
fn test_different_seeds_produce_different_terrain() {
    let registry = get_test_material_registry();
    let gen_a = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(11111)));
    let gen_b = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(99999)));

    let coord = IVec3::new(2, 0, 2);
    let chunk_a = gen_a.generate_chunk(coord, &registry);
    let chunk_b = gen_b.generate_chunk(coord, &registry);

    // Seed berbeda menghasilkan topografi berbeda
    assert_ne!(chunk_a.voxels, chunk_b.voxels);
}

#[test]
fn test_chunk_loading_order_independence() {
    let registry = get_test_material_registry();
    let generator = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(1337)));

    let coords = [
        IVec3::new(0, 0, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(2, 0, 0),
    ];

    // Urutan A -> B -> C
    let c_a1 = generator.generate_chunk(coords[0], &registry);
    let c_b1 = generator.generate_chunk(coords[1], &registry);
    let c_c1 = generator.generate_chunk(coords[2], &registry);

    // Urutan C -> A -> B
    let c_c2 = generator.generate_chunk(coords[2], &registry);
    let c_a2 = generator.generate_chunk(coords[0], &registry);
    let c_b2 = generator.generate_chunk(coords[1], &registry);

    assert_eq!(c_a1.voxels, c_a2.voxels);
    assert_eq!(c_b1.voxels, c_b2.voxels);
    assert_eq!(c_c1.voxels, c_c2.voxels);
}

// ============================================================================
// 2. BOUNDARY CONTINUITY & MATHEMATICAL SEAMLESS TESTS
// ============================================================================

#[test]
fn test_border_continuity_across_chunks() {
    let config = WorldGenConfig::new(WorldSeed::from_u64(42));
    let generator = ProceduralWorldGenerator::new(config);
    let profiler = generator.profiler();

    // 1. Verifikasi kontinuitas limit (C0 Continuity): h(x + eps) -> h(x)
    let eps = 0.001;
    for i in -20..20 {
        let x = i as f32 * 16.0;
        let z = i as f32 * 16.0;
        let h1 = profiler.evaluate(x, z).surface_height_y;
        let h2 = profiler.evaluate(x + eps, z).surface_height_y;
        let h3 = profiler.evaluate(x, z + eps).surface_height_y;

        assert!(
            (h2 - h1).abs() < 0.01,
            "Fungsi terrain harus kontinu matematis: |h(x+eps) - h(x)| < 0.01"
        );
        assert!(
            (h3 - h1).abs() < 0.01,
            "Fungsi terrain harus kontinu matematis: |h(z+eps) - h(z)| < 0.01"
        );
    }

    // 2. Verifikasi kontinuitas perbatasan chunk (tidak ada patahan diskontinu buatan pada boundary x=31 -> x=32)
    for z in -50..50 {
        let wz = z as f32;
        let h_west_side = profiler.evaluate(31.0, wz).surface_height_y;
        let h_mid = profiler.evaluate(31.5, wz).surface_height_y;
        let h_east_side = profiler.evaluate(32.0, wz).surface_height_y;

        let diff1 = (h_mid - h_west_side).abs();
        let diff2 = (h_east_side - h_mid).abs();
        assert!(
            diff1 < 2.5 && diff2 < 2.5,
            "Perbatasan X harus mulus: h(31)={}, h(31.5)={}, h(32)={}",
            h_west_side,
            h_mid,
            h_east_side
        );
    }

    // Verifikasi perbatasan Z: chunk (0,0) sisi utara (lz=31) vs chunk (0,1) sisi selatan (lz=32)
    for x in -50..50 {
        let wx = x as f32;
        let h_south_side = profiler.evaluate(wx, 31.0).surface_height_y;
        let h_mid = profiler.evaluate(wx, 31.5).surface_height_y;
        let h_north_side = profiler.evaluate(wx, 32.0).surface_height_y;

        let diff1 = (h_mid - h_south_side).abs();
        let diff2 = (h_north_side - h_mid).abs();
        assert!(
            diff1 < 2.5 && diff2 < 2.5,
            "Perbatasan Z harus mulus: h(31)={}, h(31.5)={}, h(32)={}",
            h_south_side,
            h_mid,
            h_north_side
        );
    }
}

#[test]
fn test_negative_coordinates_worldgen_continuity() {
    let config = WorldGenConfig::new(WorldSeed::from_u64(777));
    let generator = ProceduralWorldGenerator::new(config);
    let profiler = generator.profiler();

    // Perbatasan melewati titik origin (x = -1 vs x = 0)
    for z in -20..20 {
        let wz = z as f32;
        let h_neg = profiler.evaluate(-1.0, wz).surface_height_y;
        let h_mid = profiler.evaluate(-0.5, wz).surface_height_y;
        let h_pos = profiler.evaluate(0.0, wz).surface_height_y;

        assert!((h_mid - h_neg).abs() < 2.5);
        assert!((h_pos - h_mid).abs() < 2.5);
    }
}

// ============================================================================
// 3. VERTICAL CHUNK Y & SEA LEVEL TESTS
// ============================================================================

#[test]
fn test_negative_chunk_y_deep_subsurface() {
    let registry = get_test_material_registry();
    let generator = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(1234)));

    let stone_id = registry
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .expect("core:stone harus terdaftar di registry");

    // Chunk di Y = -2 (World Y: -64 hingga -33)
    let chunk_deep = generator.generate_chunk(IVec3::new(0, -2, 0), &registry);

    // Deep underground harus padat penuh (32768 voxels batu) tanpa udara/rumput
    assert_eq!(chunk_deep.non_air_count, 32768);
    for block in chunk_deep.voxels.iter() {
        assert_eq!(block.material(), stone_id);
    }
}

#[test]
fn test_sea_level_consistency_in_world_coordinates() {
    let registry = get_test_material_registry();
    let mut config = WorldGenConfig::new(WorldSeed::from_u64(555));
    config.sea_level = 16;
    let generator = ProceduralWorldGenerator::new(config);

    // Cari titik lautan di mana surface_height < sea_level
    let profiler = generator.profiler();
    let pt = profiler.evaluate(-500.0, -500.0);

    if pt.surface_height_y < 16.0 {
        let chunk = generator.generate_chunk(IVec3::new(-16, 0, -16), &registry);
        // Air harus terisi sampai ketinggian sea_level (16)
        let water_id = registry
            .resolve_material_id(&ResourceId::core("water").unwrap())
            .unwrap();

        let mut found_water = false;
        for block in chunk.voxels.iter() {
            if block.material() == water_id {
                found_water = true;
                break;
            }
        }
        assert!(found_water, "Air harus terbentuk di area lautan!");
    }
}

// ============================================================================
// 4. BIOMES & HYDROLOGY TESTS
// ============================================================================

#[test]
fn test_biome_classification_determinism() {
    let climate_cold = ClimateSample {
        continentalness: 0.8,
        temperature: -0.6,
        moisture: 0.0,
        erosion: 0.2,
        peaks_valleys: 0.9,
    };
    let biome_cold = BiomeClassifier::classify(&climate_cold, 60.0, 16.0);
    assert_eq!(biome_cold, BiomeType::SnowPeaks);

    let climate_desert = ClimateSample {
        continentalness: 0.3,
        temperature: 0.7,
        moisture: -0.5,
        erosion: -0.2,
        peaks_valleys: 0.1,
    };
    let biome_desert = BiomeClassifier::classify(&climate_desert, 20.0, 16.0);
    assert_eq!(biome_desert, BiomeType::Desert);
}

#[test]
fn test_river_continuity_across_boundaries() {
    let config = WorldGenConfig::new(WorldSeed::from_u64(888));
    let generator = ProceduralWorldGenerator::new(config);
    let profiler = generator.profiler();

    // Uji kelanjutan lembah sungai sepanjang garis transversal
    let mut river_samples = 0;
    for x in 0..100 {
        let pt = profiler.evaluate(x as f32, 50.0);
        if pt.hydrology.is_river {
            river_samples += 1;
        }
    }
    // Sungai harus eksis dan kontinu dalam rentang teruji
    assert!(
        river_samples > 0,
        "Sungai harus terdeteksi di koordinat teruji"
    );
}

// ============================================================================
// 5. PERSISTENCE PRECEDENCE & MUTATION TESTS
// ============================================================================

#[test]
fn test_persistence_precedence_and_mutation_preservation() {
    let registry = get_test_material_registry();
    let config = WorldGenConfig::new(WorldSeed::from_u64(1337));
    let generator = ProceduralWorldGenerator::new(config);
    let store = MemoryCompressedRegionStore::new();

    let coord = IVec3::new(3, 0, 3);

    // 1. Generate chunk awal
    let mut chunk = generator.generate_chunk(coord, &registry);
    let initial_voxel = chunk.get_voxel(10, 10, 10).material();

    // 2. Pemain memutasi voxel
    let gold_id = registry
        .resolve_material_id(&ResourceId::core("gold_accent").unwrap())
        .unwrap();
    assert_ne!(initial_voxel, gold_id);
    chunk.set_voxel(10, 10, 10, VoxelBlock::new(gold_id));
    chunk.mark_dirty(dirty_flags::SAVE_DIRTY);

    // 3. Simpan ke RegionStore disk
    store.save_chunk(&chunk, &registry).expect("Save gagal");

    // 4. Simulasi reload: disk load diprioritaskan di atas generator
    let loaded = store
        .load_chunk(coord, &registry)
        .expect("Load gagal")
        .unwrap();

    assert_eq!(
        loaded.get_voxel(10, 10, 10).material(),
        gold_id,
        "Mutasi pemain harus tetap awet dan mengalahkan initial procedural state!"
    );
}

#[test]
fn test_generator_version_identity() {
    let config_v1 = WorldGenConfig {
        seed: WorldSeed::from_u64(100),
        generator_version: GeneratorVersion(1),
        ..Default::default()
    };
    let config_v2 = WorldGenConfig {
        seed: WorldSeed::from_u64(100),
        generator_version: GeneratorVersion(2),
        ..Default::default()
    };

    assert_ne!(config_v1.identity(), config_v2.identity());
    assert_ne!(config_v1.config_hash(), config_v2.config_hash());
}

#[test]
fn test_generator_does_not_depend_on_neighbor_residency() {
    let registry = get_test_material_registry();
    let generator = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(999)));

    let coord = IVec3::new(15, 0, 25);

    // Generate chunk secara terisolasi tanpa ada chunk lain di memori
    let standalone_chunk = generator.generate_chunk(coord, &registry);

    // Generate kembali
    let duplicate_chunk = generator.generate_chunk(coord, &registry);

    assert_eq!(standalone_chunk.voxels, duplicate_chunk.voxels);
}

#[test]
fn test_deterministic_golden_snapshot() {
    let registry = get_test_material_registry();
    let config = WorldGenConfig::new(WorldSeed::from_u64(42));
    let generator = ProceduralWorldGenerator::new(config);

    let stone_id = registry
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .expect("core:stone harus terdaftar di registry");

    let coord = IVec3::new(0, 0, 0);
    let chunk = generator.generate_chunk(coord, &registry);

    // Golden invariant snapshot
    assert!(chunk.non_air_count > 0);
    assert_eq!(chunk.position, coord);

    // Snapshot material di lapisan batu bawah
    assert_eq!(chunk.get_voxel(16, 2, 16).material(), stone_id);
}
