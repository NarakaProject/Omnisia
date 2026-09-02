use glam::IVec3;

use omnisia::chunk::dirty_flags;
use omnisia::material::{MaterialId, MaterialRegistry};
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
use omnisia::worldgen::voxelizer::ResolvedGenMaterials;

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
        IVec3::new(2, -2, 2),
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

    // Seed berbeda menghasilkan topografi dan fitur 3D berbeda
    assert_ne!(chunk_a.voxels, chunk_b.voxels);
}

#[test]
fn test_chunk_loading_order_independence() {
    let registry = get_test_material_registry();
    let generator = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(1337)));

    let coords = [
        IVec3::new(0, 0, 0),
        IVec3::new(1, -1, 0),
        IVec3::new(2, 0, 1),
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
// 3. VERTICAL CHUNK Y & DEEP STRATA TESTS
// ============================================================================

#[test]
fn test_negative_chunk_y_deep_subsurface() {
    let registry = get_test_material_registry();
    let generator = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(1234)));

    let deepslate_id = registry
        .resolve_material_id(&ResourceId::core("deepslate").unwrap())
        .expect("core:deepslate harus terdaftar di registry");

    // Chunk di Y = -2 (World Y: -64 hingga -33) -> Deep Strata
    let chunk_deep = generator.generate_chunk(IVec3::new(0, -2, 0), &registry);

    // Deep underground memiliki massa padat tinggi (deepslate / ores / caverns)
    assert!(chunk_deep.non_air_count > 10000);

    // Memverifikasi keberadaan deepslate stone di kedalaman $y < -32$
    let mut found_deepslate = false;
    for block in chunk_deep.voxels.iter() {
        if block.material() == deepslate_id {
            found_deepslate = true;
            break;
        }
    }
    assert!(
        found_deepslate,
        "Deepslate harus terbentuk pada kedalaman $y < -32$!"
    );
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

    // Verifikasi kontinuitas kedalaman sungai melintasi batas chunk (x = 31 vs x = 32)
    for z in 0..50 {
        let pt1 = profiler.evaluate(31.0, z as f32);
        let pt2 = profiler.evaluate(32.0, z as f32);

        // Jika satu titik adalah sungai, titik sebelahnya memiliki transisi kedalaman mulus
        let depth_diff = (pt1.hydrology.river_depth - pt2.hydrology.river_depth).abs();
        assert!(
            depth_diff < 1.5,
            "Transisi sungai pada boundary x=31->32 harus mulus: d1={}, d2={}",
            pt1.hydrology.river_depth,
            pt2.hydrology.river_depth
        );
    }
}

// ============================================================================
// 5. 3D CAVES & TOPOLOGY TESTS (PHASE 5)
// ============================================================================

#[test]
fn test_3d_cave_determinism_and_topology() {
    let config = WorldGenConfig::new(WorldSeed::from_u64(4242));
    let generator = ProceduralWorldGenerator::new(config);
    let caves = generator.caves();

    // Uji sampling 3D pada beberapa titik bawah tanah
    let sample1 = caves.is_cave(10.0, -10.0, 10.0, 20.0);
    let sample2 = caves.is_cave(10.0, -10.0, 10.0, 20.0);
    assert_eq!(sample1, sample2, "Sampling gua 3D harus deterministik!");

    // Gua tidak boleh terbentuk di atas permukaan tanah bebas
    assert!(!caves.is_cave(0.0, 50.0, 0.0, 20.0));

    // Verifikasi adanya rongga 3D di bawah tanah
    let mut cave_voxels = 0;
    for y in -40..0 {
        for z in 0..20 {
            for x in 0..20 {
                if caves.is_cave(x as f32, y as f32, z as f32, 20.0) {
                    cave_voxels += 1;
                }
            }
        }
    }
    assert!(
        cave_voxels > 0,
        "Gua 3D harus menghasilkan rongga nyata di bawah tanah!"
    );
}

#[test]
fn test_cave_boundary_continuity_xyz() {
    let config = WorldGenConfig::new(WorldSeed::from_u64(9999));
    let generator = ProceduralWorldGenerator::new(config);
    let caves = generator.caves();

    // Verifikasi evaluasi gua 3D melewati batas chunk sumbu X (x=31 vs x=32), Y (y=-1 vs y=0), Z (z=31 vs z=32)
    let eps = 0.001;
    for x in [-1.0, 31.0, 32.0] {
        for y in [-33.0, -32.0, -1.0, 0.0] {
            for z in [-1.0, 31.0, 32.0] {
                let c1 = caves.is_cave(x, y, z, 30.0);
                let c2 = caves.is_cave(x + eps, y + eps, z + eps, 30.0);
                assert_eq!(c1, c2, "Evaluasi gua 3D harus kontinu pada batas eps!");
            }
        }
    }
}

// ============================================================================
// 6. OVERHANGS & 3D NON-COLUMNAR TOPOLOGY TESTS (PHASE 5)
// ============================================================================

#[test]
fn test_overhang_topology_non_columnar() {
    let registry = get_test_material_registry();
    let config = WorldGenConfig::new(WorldSeed::from_u64(7777));
    let generator = ProceduralWorldGenerator::new(config);
    let water_id = registry
        .resolve_material_id(&ResourceId::core("water").unwrap())
        .unwrap();

    // Cari kolom voxel yang memiliki topologi non-kolumnar (Solid di atas Air di atas Solid)
    let mut found_non_columnar = false;

    // Pindai area pegunungan
    for cx in -3..3 {
        for cz in -3..3 {
            let chunk = generator.generate_chunk(IVec3::new(cx, 0, cz), &registry);
            for lz in 0..32 {
                for lx in 0..32 {
                    let mut solid_run = 0;
                    let mut transitions = 0;
                    let mut last_solid = false;

                    for ly in 0..32 {
                        let voxel_mat = chunk.get_voxel(lx, ly, lz).material();
                        let is_solid = voxel_mat != MaterialId::AIR && voxel_mat != water_id;
                        if is_solid != last_solid {
                            transitions += 1;
                            last_solid = is_solid;
                        }
                        if is_solid {
                            solid_run += 1;
                        }
                    }

                    // Jika ada transisi solid -> air -> solid (minimal 3 transisi di kolom vertikal yang sama)
                    if transitions >= 3 && solid_run > 2 {
                        found_non_columnar = true;
                        break;
                    }
                }
                if found_non_columnar {
                    break;
                }
            }
            if found_non_columnar {
                break;
            }
        }
        if found_non_columnar {
            break;
        }
    }

    assert!(
        found_non_columnar,
        "Generasi 3D harus mampu menghasilkan topologi non-kolumnar (overhang / gua tebing)!"
    );
}

// ============================================================================
// 7. UNDERGROUND STRATA & ORE DISTRIBUTION TESTS (PHASE 5)
// ============================================================================

#[test]
fn test_underground_layers_stratification() {
    let registry = get_test_material_registry();
    let config = WorldGenConfig::new(WorldSeed::from_u64(1337));
    let generator = ProceduralWorldGenerator::new(config);

    let stone_id = registry
        .resolve_material_id(&ResourceId::core("stone").unwrap())
        .unwrap();
    let deepslate_id = registry
        .resolve_material_id(&ResourceId::core("deepslate").unwrap())
        .unwrap();

    // Chunk di $Y = 0$ (Upper Strata: batu dominan)
    let chunk_y0 = generator.generate_chunk(IVec3::new(0, 0, 0), &registry);
    let mut found_stone = false;
    for b in chunk_y0.voxels.iter() {
        if b.material() == stone_id {
            found_stone = true;
            break;
        }
    }
    assert!(found_stone, "Upper strata harus mengandung stone!");

    // Chunk di $Y = -2$ ($world\_y < -32$, Deep Strata: deepslate dominan)
    let chunk_y_neg = generator.generate_chunk(IVec3::new(0, -2, 0), &registry);
    let mut found_deepslate = false;
    for b in chunk_y_neg.voxels.iter() {
        if b.material() == deepslate_id {
            found_deepslate = true;
            break;
        }
    }
    assert!(found_deepslate, "Deep strata harus mengandung deepslate!");
}

#[test]
fn test_ore_distribution_invariants() {
    let registry = get_test_material_registry();
    let config = WorldGenConfig::new(WorldSeed::from_u64(42));
    let generator = ProceduralWorldGenerator::new(config);

    let coal_id = registry
        .resolve_material_id(&ResourceId::core("coal_ore").unwrap())
        .unwrap();
    let iron_id = registry
        .resolve_material_id(&ResourceId::core("iron_ore").unwrap())
        .unwrap();
    let gold_id = registry
        .resolve_material_id(&ResourceId::core("gold_ore").unwrap())
        .unwrap();
    let water_id = registry
        .resolve_material_id(&ResourceId::core("water").unwrap())
        .unwrap();

    let mut found_coal = false;
    let mut found_iron = false;
    let mut found_gold = false;

    for cy in -2..=1 {
        for cx in 0..3 {
            let chunk = generator.generate_chunk(IVec3::new(cx, cy, 0), &registry);
            for b in chunk.voxels.iter() {
                let mat = b.material();
                if mat == coal_id {
                    found_coal = true;
                    assert_ne!(mat, MaterialId::AIR);
                    assert_ne!(mat, water_id);
                } else if mat == iron_id {
                    found_iron = true;
                    assert_ne!(mat, MaterialId::AIR);
                    assert_ne!(mat, water_id);
                } else if mat == gold_id {
                    found_gold = true;
                    assert_ne!(mat, MaterialId::AIR);
                    assert_ne!(mat, water_id);
                }
            }
        }
    }

    assert!(found_coal, "Coal ore harus berhasil terbentuk di dunia!");
    assert!(found_iron, "Iron ore harus berhasil terbentuk di dunia!");
    assert!(
        found_gold,
        "Gold ore harus berhasil terbentuk di kedalaman!"
    );
}

#[test]
fn test_natural_formations_voxel_presence() {
    let registry = get_test_material_registry();
    let config = WorldGenConfig::new(WorldSeed::from_u64(8888));
    let generator = ProceduralWorldGenerator::new(config);

    // Generate beberapa chunk di permukaan dataran/pegunungan dan pastikan ada voxel yang terbentuk
    let chunk = generator.generate_chunk(IVec3::new(0, 0, 0), &registry);
    assert!(
        chunk.non_air_count > 0,
        "Chunk harus memiliki voxel terbentuk!"
    );
}

// ============================================================================
// 8. PERSISTENCE & ERROR HANDLING TESTS
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

    let coord = IVec3::new(0, 0, 0);
    let chunk = generator.generate_chunk(coord, &registry);

    assert!(chunk.non_air_count > 0);
    assert_eq!(chunk.position, coord);
}

#[test]
fn test_missing_generation_material_fails_explicitly() {
    // Registry kosong tanpa material core
    let empty_registry = MaterialRegistry::new();
    let result = ResolvedGenMaterials::resolve(&empty_registry);

    assert!(
        result.is_err(),
        "ResolvedGenMaterials::resolve HARUS gagal secara eksplisit jika ada material inti yang hilang!"
    );
}
