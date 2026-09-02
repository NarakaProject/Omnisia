use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use glam::{IVec3, Vec3};
use omnisia::camera::Camera;
use omnisia::chunk::Chunk;
use omnisia::coord::{canonical_linear_index, CHUNK_VOLUME};
use omnisia::material::MaterialId;
use omnisia::mesh::ao::calculate_face_ao;
use omnisia::mesh::culled::generate_culled_mesh;
use omnisia::mesh::greedy::generate_greedy_mesh;
use omnisia::mesh::types::{FaceDirection, MeshData};
use omnisia::modding::discovery::ModDiscovery;
use omnisia::modding::runtime::ContentRuntime;
use omnisia::storage::{decompress_and_deserialize_chunk, serialize_and_compress_chunk};
use omnisia::streaming::generator::ChunkGenerator;
use omnisia::voxel::VoxelBlock;
use omnisia::worldgen::config::WorldGenConfig;
use omnisia::worldgen::noise::sample_fbm_3d;
use omnisia::worldgen::pipeline::ProceduralWorldGenerator;
use omnisia::worldgen::seed::WorldSeed;

fn main() {
    println!("============================================================");
    println!("     OMNISIA ENGINE ARCHITECTURE BENCHMARK SUITE           ");
    println!("     Phase 6: Vegetation & Performance Stabilization       ");
    println!("     Target Baseline: MacBook Pro 2018 (Intel x86_64)      ");
    println!("============================================================");

    let resolved = ContentRuntime::build_runtime("content/core", "mods")
        .expect("Gagal memuat Core Content & Mods untuk benchmark");
    let registry = resolved.materials;

    // 1. Benchmark Chunk Indexing
    {
        let start = Instant::now();
        let iterations = 10_000_000;
        let mut sum = 0usize;
        for i in 0..iterations {
            let x = (i % 32) as usize;
            let y = ((i / 32) % 32) as usize;
            let z = ((i / 1024) % 32) as usize;
            sum = sum.wrapping_add(canonical_linear_index(x, y, z));
        }
        let elapsed = start.elapsed();
        let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 1] Chunk Indexing: {:.2} ns/op (Total: {:?}, Iterations: {}, sum: {})",
            ns_per_op, elapsed, iterations, sum
        );
    }

    // 2. Benchmark Chunk Fill & Mutation
    {
        let mut chunk = Chunk::new(IVec3::ZERO);
        let start = Instant::now();
        let iterations = 10_000;
        for _ in 0..iterations {
            chunk.fill_material(MaterialId::STONE);
            chunk.fill_material(MaterialId::AIR);
        }
        let elapsed = start.elapsed();
        let us_per_chunk_fill = elapsed.as_micros() as f64 / (iterations * 2) as f64;
        println!(
            "[BENCHMARK 2] Chunk Fill (32k voxels): {:.2} µs/chunk (Total: {:?})",
            us_per_chunk_fill, elapsed
        );
    }

    // Siapkan 1 Chunk Terrain Prosedural Nyata (Phase 6 dengan 3D features & vegetasi)
    let worldgen = ProceduralWorldGenerator::new(WorldGenConfig::new(WorldSeed::from_u64(1337)));
    let terrain_chunk = worldgen.generate_chunk(IVec3::new(0, 0, 0), &registry);

    // 3. Benchmark Culled Face Meshing 32³
    let mut culled_mesh = MeshData::new();
    {
        let start = Instant::now();
        let iterations = 500;
        for _ in 0..iterations {
            generate_culled_mesh(&terrain_chunk, &registry, &mut culled_mesh);
        }
        let elapsed = start.elapsed();
        let ms_per_chunk = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
        println!(
            "[BENCHMARK 3] Culled Meshing 32³: {:.3} ms/chunk (Vertices: {}, Indices: {}, Quads: {})",
            ms_per_chunk,
            culled_mesh.vertex_count(),
            culled_mesh.index_count(),
            culled_mesh.quad_count()
        );
    }

    // 4. Benchmark Greedy Meshing
    let mut greedy_mesh = MeshData::new();
    {
        let start = Instant::now();
        let iterations = 500;
        for _ in 0..iterations {
            generate_greedy_mesh(&terrain_chunk, &registry, &mut greedy_mesh);
        }
        let elapsed = start.elapsed();
        let ms_per_chunk = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
        let reduction_ratio =
            culled_mesh.quad_count() as f64 / greedy_mesh.quad_count().max(1) as f64;
        println!(
            "[BENCHMARK 4] Greedy Meshing 32³: {:.3} ms/chunk (Vertices: {}, Indices: {}, Quads: {})",
            ms_per_chunk,
            greedy_mesh.vertex_count(),
            greedy_mesh.index_count(),
            greedy_mesh.quad_count()
        );
        println!(
            "  -> Greedy Quad Reduction Ratio: {:.2}x vs Culled",
            reduction_ratio
        );
    }

    // 5. Benchmark Ambient Occlusion (AO) per face
    {
        let start = Instant::now();
        let iterations = 500_000;
        let mut sum_ao = 0.0f32;
        for i in 0..iterations {
            let x = (i % 30) + 1;
            let y = ((i / 30) % 30) + 1;
            let z = ((i / 900) % 30) + 1;
            let ao = calculate_face_ao(&terrain_chunk, x, y, z, FaceDirection::PosY);
            sum_ao += ao[0] as f32;
        }
        let elapsed = start.elapsed();
        let ns_per_face = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 5] AO Calculation: {:.2} ns/face (Total: {:?}, sum: {:.1})",
            ns_per_face, elapsed, sum_ao
        );
    }

    // 6. Benchmark Rayon Parallel Meshing Across 100 Real Chunks
    {
        use rayon::prelude::*;
        let start = Instant::now();
        let chunks: Vec<Chunk> = (0..100)
            .into_par_iter()
            .map(|i| {
                let coord = IVec3::new(i % 10, 0, i / 10);
                worldgen.generate_chunk(coord, &registry)
            })
            .collect();

        let meshes: Vec<MeshData> = chunks
            .par_iter()
            .map(|c| {
                let mut m = MeshData::new();
                generate_greedy_mesh(c, &registry, &mut m);
                m
            })
            .collect();

        let elapsed = start.elapsed();
        let total_verts: usize = meshes.iter().map(|m| m.vertex_count()).sum();
        println!(
            "[BENCHMARK 6] 100 Procedural Chunk Parallel Meshing (Rayon): {:?} ({:.2} ms/100 chunks, Total Vertices: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_verts
        );
    }

    // 7. Benchmark 1,000 Chunks Parallel Meshing (Synthetic Stress Test)
    {
        use rayon::prelude::*;
        let mut sample_chunk = Chunk::new(IVec3::ZERO);
        for z in 0..32 {
            for x in 0..32 {
                for y in 0..16 {
                    sample_chunk.set_voxel(x, y, z, VoxelBlock::new(MaterialId::DIRT));
                }
            }
        }
        let chunk_ref = &sample_chunk;

        let start = Instant::now();
        let count = 1000;
        let total_quads: usize = (0..count)
            .into_par_iter()
            .map(|_| {
                let mut m = MeshData::new();
                generate_greedy_mesh(chunk_ref, &registry, &mut m);
                m.quad_count()
            })
            .sum();
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 7] 1,000 Chunk Synthetic Meshing (Rayon): {:?} ({:.2} ms/1000 chunks, Total Quads: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_quads
        );
    }

    // 8 & 9. Benchmark Zstd Palette Chunk Compression
    let compressed_bytes = {
        let start = Instant::now();
        let compressed =
            serialize_and_compress_chunk(&terrain_chunk, &registry).expect("Kompresi chunk gagal");
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 8 & 9] Chunk Palette Zstd Compress: {:?} | Raw: {} bytes -> Compressed: {} bytes ({:.1}x ratio)",
            elapsed,
            CHUNK_VOLUME * std::mem::size_of::<VoxelBlock>(),
            compressed.len(),
            (CHUNK_VOLUME * std::mem::size_of::<VoxelBlock>()) as f64 / compressed.len() as f64
        );
        compressed
    };

    // 10. Benchmark Zstd Palette Chunk Decompression
    {
        let start = Instant::now();
        let loaded = decompress_and_deserialize_chunk(&compressed_bytes, &registry)
            .expect("Dekompresi chunk gagal");
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 10] Chunk Palette Zstd Decompress: {:?} (Valid non_air: {})",
            elapsed, loaded.non_air_count
        );
    }

    // 11. Benchmark Connectivity BFS
    {
        let start = Instant::now();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut traversed = 0;

        let start_coord = IVec3::new(0, 0, 0);
        queue.push_back(start_coord);
        visited.insert(start_coord);

        let neighbors = [
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Y,
            IVec3::NEG_Y,
            IVec3::Z,
            IVec3::NEG_Z,
        ];

        while let Some(current) = queue.pop_front() {
            traversed += 1;
            for offset in &neighbors {
                let next = current + *offset;
                if next.x >= 0
                    && next.x < 32
                    && next.y >= 0
                    && next.y < 32
                    && next.z >= 0
                    && next.z < 32
                    && !visited.contains(&next)
                {
                    let block =
                        terrain_chunk.get_voxel(next.x as usize, next.y as usize, next.z as usize);
                    if !block.is_air() {
                        visited.insert(next);
                        queue.push_back(next);
                    }
                }
            }
        }
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 11] Localized Connectivity BFS ({} voxels traversed): {:?}",
            traversed, elapsed
        );
    }

    // 12. Benchmark Mod Discovery & Manifest Parsing
    {
        let start = Instant::now();
        let iterations = 1000;
        let mut count = 0;
        for _ in 0..iterations {
            let (discovered, _) = ModDiscovery::discover_from_dir("mods");
            count = discovered.len();
        }
        let elapsed = start.elapsed();
        let us_per_run = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 12] Mod Discovery & Manifest Parsing: {:.2} µs/run (Mods found: {}, Total: {:?})",
            us_per_run, count, elapsed
        );
    }

    // 13. Benchmark MaterialRegistry Hot Path Lookup (MaterialId)
    {
        let start_index = Instant::now();
        let iterations = 10_000_000;
        let mut sum_density = 0.0f32;
        let mat_id = MaterialId::STONE;
        for _ in 0..iterations {
            if let Some(def) = registry.get(mat_id) {
                sum_density += def.density_kg_m3;
            }
        }
        let elapsed_index = start_index.elapsed();
        let ns_per_index_lookup = elapsed_index.as_nanos() as f64 / iterations as f64;

        println!(
            "[BENCHMARK 13] Registry Voxel Hot Path Lookup (MaterialId): {:.2} ns/op (Zero overhead, sum: {})",
            ns_per_index_lookup, sum_density as usize
        );
    }

    // 14. Benchmark Noise 3D fBm Sampling Throughput (1,000,000 samples)
    {
        let start = Instant::now();
        let iterations = 1_000_000;
        let mut sum = 0.0f32;
        for i in 0..iterations {
            let x = (i % 100) as f32;
            let y = ((i / 100) % 100) as f32;
            let z = (i / 10000) as f32;
            sum += sample_fbm_3d(x, y, z, 1337, 3, 0.5, 2.0, 0.02);
        }
        let elapsed = start.elapsed();
        let ns_per_noise = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 14] Noise 3D fBm Sampling (1M samples): {:.2} ns/sample (Total: {:?}, sum: {:.1})",
            ns_per_noise, elapsed, sum
        );
    }

    // 15. Benchmark 3D Cave Density & Worm Tunnel Sampling (100,000 points)
    {
        let caves = worldgen.caves();
        let start = Instant::now();
        let iterations = 100_000;
        let mut cave_count = 0;
        for i in 0..iterations {
            let x = (i % 100) as f32;
            let y = ((i / 100) % 100) as f32 - 50.0;
            let z = (i / 1000) as f32;
            if caves.is_cave(x, y, z, 20.0) {
                cave_count += 1;
            }
        }
        let elapsed = start.elapsed();
        let ns_per_point = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 15] 3D Cave & Worm Tunnel Sampling (100k points): {:.2} ns/point (Total: {:?}, caves found: {})",
            ns_per_point, elapsed, cave_count
        );
    }

    // 16. Benchmark 3D Overhang & Feature Evaluation (100,000 points)
    {
        let overhangs = worldgen.overhangs();
        let start = Instant::now();
        let iterations = 100_000;
        let mut sum_density = 0.0f32;
        for i in 0..iterations {
            let x = (i % 100) as f32;
            let y = ((i / 100) % 100) as f32;
            let z = (i / 1000) as f32;
            sum_density += overhangs.sample_density(
                x,
                y,
                z,
                25.0,
                omnisia::worldgen::biome::BiomeType::Mountains,
            );
        }
        let elapsed = start.elapsed();
        let ns_per_point = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 16] 3D Overhang & Feature Evaluation (100k points): {:.2} ns/point (Total: {:?}, sum density: {:.1})",
            ns_per_point, elapsed, sum_density
        );
    }

    // 17. Benchmark Phase 6 Procedural Chunk Generation (3D Features + Canonical Vegetation Stamping)
    {
        let start = Instant::now();
        let iterations = 200;
        for i in 0..iterations {
            let coord = IVec3::new(i % 10, 0, i / 10);
            let _ = worldgen.generate_chunk(coord, &registry);
        }
        let elapsed = start.elapsed();
        let ms_per_chunk = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
        println!(
            "[BENCHMARK 17] Phase 6 Procedural Chunk Generation (Vegetation + 3D): {:.3} ms/chunk ({:.1} chunks/sec)",
            ms_per_chunk,
            1000.0 / ms_per_chunk
        );
    }

    // 18. Benchmark 100 Procedural Chunks Parallel Generation (Rayon)
    {
        use rayon::prelude::*;
        let start = Instant::now();
        let coords: Vec<IVec3> = (0..100).map(|i| IVec3::new(i % 10, 0, i / 10)).collect();
        let chunks: Vec<Chunk> = coords
            .par_iter()
            .map(|&c| worldgen.generate_chunk(c, &registry))
            .collect();
        let elapsed = start.elapsed();
        let total_voxels: usize = chunks.iter().map(|c| c.non_air_count as usize).sum();
        println!(
            "[BENCHMARK 18] 100 Procedural Chunks Parallel Generation (Rayon): {:?} ({:.2} ms total, Total Solid Voxels: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_voxels
        );
    }

    // 19. Benchmark Frustum Extraction & 1,000 Chunk AABB Intersections
    {
        let camera = Camera::new(Vec3::new(0.0, 30.0, 0.0), -90.0, -10.0);
        let start = Instant::now();
        let frustum = camera.extract_frustum(16.0 / 9.0);
        let iterations = 100_000;
        let mut visible_count = 0;
        for i in 0..iterations {
            let coord = IVec3::new((i % 40) - 20, (i / 40) % 5, (i / 200) - 20);
            if frustum.intersects_chunk(coord) {
                visible_count += 1;
            }
        }
        let elapsed = start.elapsed();
        let ns_per_cull = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 19] Frustum Culling Intersection: {:.2} ns/chunk (Total: {:?}, visible: {}/{})",
            ns_per_cull, elapsed, visible_count, iterations
        );
    }

    // 20. Benchmark Event-Driven Localized Structural Connectivity (Gate A)
    {
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::anchor::AnchorPolicy;
        use omnisia::structure::connectivity::{check_structural_connectivity, ConnectivityConfig};

        let anchor_policy = AnchorPolicy::from_registries(&registry, &resolved.blocks);
        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        let stone_id = registry
            .resolve_material_id(&omnisia::modding::resource_id::ResourceId::core("stone").unwrap())
            .unwrap();
        let wood_id = registry
            .resolve_material_id(
                &omnisia::modding::resource_id::ResourceId::core("wood_oak").unwrap(),
            )
            .unwrap();

        // Fondasi stone anchor di dasar (y = 0)
        for z in 0..32 {
            for x in 0..32 {
                chunk.set_voxel(x, 0, z, VoxelBlock::new(stone_id));
            }
        }
        // Tiang kayu 15 voxel ke atas
        for y in 1..=15 {
            chunk.set_voxel(16, y, 16, VoxelBlock::new(wood_id));
        }
        store.insert(chunk);

        let config = ConnectivityConfig::default();
        let iterations = 10_000;
        let mut inspected_total = 0;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = check_structural_connectivity(
                IVec3::new(16, 15, 16),
                &store,
                &anchor_policy,
                &config,
                Some(&mut inspected_total),
            );
        }
        let elapsed = start.elapsed();
        let us_per_check = elapsed.as_micros() as f64 / iterations as f64;
        let avg_voxels = inspected_total as f64 / iterations as f64;
        println!(
            "[BENCHMARK 20] Localized Structural Connectivity: {:.2} µs/check (Avg voxels scanned: {:.1}, Total: {:?})",
            us_per_check, avg_voxels, elapsed
        );
    }

    // 21. Benchmark Detached Aggregate Extraction Throughput (Gate A)
    {
        use omnisia::structure::aggregate::DetachedAggregate;

        let wood_id = registry
            .resolve_material_id(
                &omnisia::modding::resource_id::ResourceId::core("wood_oak").unwrap(),
            )
            .unwrap();
        // Buat komponen 125 voxel (5x5x5)
        let mut voxels = Vec::new();
        for z in 0..5 {
            for y in 0..5 {
                for x in 0..5 {
                    voxels.push((IVec3::new(x, y, z), VoxelBlock::new(wood_id)));
                }
            }
        }

        let iterations = 20_000;
        let start = Instant::now();
        for i in 0..iterations {
            let agg = DetachedAggregate::from_world_voxels(i as u64, &voxels).unwrap();
            std::hint::black_box(agg);
        }
        let elapsed = start.elapsed();
        let us_per_extraction = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 21] Detached Aggregate Extraction (125 voxels): {:.2} µs/op ({:.1} extractions/sec)",
            us_per_extraction, 1_000_000.0 / us_per_extraction
        );
    }

    println!("============================================================");
    println!("             BENCHMARK SUITE COMPLETE                       ");
    println!("============================================================");
}
