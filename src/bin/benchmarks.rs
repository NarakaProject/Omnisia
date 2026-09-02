use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use glam::IVec3;
use omnisia::chunk::Chunk;
use omnisia::coord::{canonical_linear_index, CHUNK_VOLUME};
use omnisia::material::MaterialId;
use omnisia::mesh::ao::calculate_face_ao;
use omnisia::mesh::culled::generate_culled_mesh;
use omnisia::mesh::greedy::generate_greedy_mesh;
use omnisia::mesh::types::{FaceDirection, MeshData};
use omnisia::modding::discovery::ModDiscovery;
use omnisia::modding::resource_id::ResourceId;
use omnisia::modding::runtime::ContentRuntime;
use omnisia::storage::{decompress_and_deserialize_chunk, serialize_and_compress_chunk};
use omnisia::voxel::VoxelBlock;

fn main() {
    println!("============================================================");
    println!("     OMNISIA ENGINE ARCHITECTURE BENCHMARK SUITE           ");
    println!("     Phase 2.5: Core Boundary + Safe Override Layer        ");
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

    // Siapkan 1 Chunk Terrain Sintetis (Bukit dengan rumput, tanah, batu)
    let mut terrain_chunk = Chunk::new(IVec3::ZERO);
    for z in 0..32 {
        for x in 0..32 {
            let height = 12 + (((x as f32 * 0.2).sin() + (z as f32 * 0.2).cos()) * 4.0) as usize;
            for y in 0..=height.min(31) {
                let mat = if y < height.saturating_sub(4) {
                    MaterialId::STONE
                } else if y < height {
                    MaterialId::DIRT
                } else {
                    MaterialId::GRASS
                };
                terrain_chunk.set_voxel(x, y, z, VoxelBlock::new(mat));
            }
        }
    }

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
        println!(
            "[BENCHMARK 4] Greedy Meshing 32³: {:.3} ms/chunk (Vertices: {}, Indices: {}, Quads: {})",
            ms_per_chunk,
            greedy_mesh.vertex_count(),
            greedy_mesh.index_count(),
            greedy_mesh.quad_count()
        );
        println!(
            "  -> Greedy Quad Reduction Ratio: {:.2}x vs Culled",
            culled_mesh.quad_count() as f64 / greedy_mesh.quad_count().max(1) as f64
        );
    }

    // 5. Benchmark AO Calculation
    {
        let start = Instant::now();
        let iterations = 500_000;
        let mut sum_ao = 0.0f32;
        for i in 0..iterations {
            let x = i % 30 + 1;
            let y = (i / 30) % 30 + 1;
            let z = (i / 900) % 30 + 1;
            let ao = calculate_face_ao(&terrain_chunk, x, y, z, FaceDirection::PosY);
            sum_ao += ao[0];
        }
        let elapsed = start.elapsed();
        let ns_per_ao = elapsed.as_nanos() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 5] AO Calculation: {:.2} ns/face (Total: {:?}, sum: {:.1})",
            ns_per_ao, elapsed, sum_ao
        );
    }

    // 6. Benchmark 100 Chunks Meshing (Rayon Parallel)
    {
        let chunks: Vec<Chunk> = (0..100)
            .map(|i| {
                let mut c = Chunk::new(IVec3::new(i % 10, 0, i / 10));
                for z in 0..32 {
                    for x in 0..32 {
                        let h = 8 + ((x + z + i as usize) % 10);
                        for y in 0..h {
                            c.set_voxel(x, y, z, VoxelBlock::new(MaterialId::STONE));
                        }
                    }
                }
                c
            })
            .collect();

        let start = Instant::now();
        use rayon::prelude::*;
        let results: Vec<MeshData> = chunks
            .par_iter()
            .map(|c| {
                let mut m = MeshData::new();
                generate_culled_mesh(c, &registry, &mut m);
                m
            })
            .collect();

        let elapsed = start.elapsed();
        let total_verts: usize = results.iter().map(|m| m.vertex_count()).sum();
        println!(
            "[BENCHMARK 6] 100 Chunk Parallel Meshing (Rayon): {:?} ({:.2} ms/100 chunks, Total Vertices: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_verts
        );
    }

    // 7. Benchmark 1,000 Chunks Synthetic Meshing (Rayon Parallel)
    {
        let chunks: Vec<Chunk> = (0..1000)
            .map(|i| {
                let mut c = Chunk::new(IVec3::new(i % 10, i / 100, (i / 10) % 10));
                for z in 0..32 {
                    for x in 0..32 {
                        let h = 4 + ((x + z + i as usize) % 8);
                        for y in 0..h {
                            c.set_voxel(x, y, z, VoxelBlock::new(MaterialId::DIRT));
                        }
                    }
                }
                c
            })
            .collect();

        let start = Instant::now();
        use rayon::prelude::*;
        let results: Vec<MeshData> = chunks
            .par_iter()
            .map(|c| {
                let mut m = MeshData::new();
                generate_culled_mesh(c, &registry, &mut m);
                m
            })
            .collect();

        let elapsed = start.elapsed();
        let total_quads: usize = results.iter().map(|m| m.quad_count()).sum();
        println!(
            "[BENCHMARK 7] 1,000 Chunk Synthetic Meshing (Rayon): {:?} ({:.2} ms/1,000 chunks, Total Quads: {})",
            elapsed,
            elapsed.as_secs_f64() * 1000.0,
            total_quads
        );
    }

    // 8 & 9 & 10. Chunk Serialization & Zstd Compression/Decompression
    {
        let start_comp = Instant::now();
        let compressed = serialize_and_compress_chunk(&terrain_chunk).expect("Kompresi Zstd gagal");
        let comp_time = start_comp.elapsed();

        let start_decomp = Instant::now();
        let decompressed =
            decompress_and_deserialize_chunk(&compressed).expect("Dekompresi Zstd gagal");
        let decomp_time = start_decomp.elapsed();

        let raw_size = CHUNK_VOLUME * std::mem::size_of::<VoxelBlock>(); // 128 KiB
        let comp_size = compressed.len();
        let ratio = raw_size as f64 / comp_size as f64;

        println!(
            "[BENCHMARK 8 & 9] Chunk Zstd Compress: {:?} | Raw: {} bytes -> Compressed: {} bytes ({:.1}x ratio)",
            comp_time, raw_size, comp_size, ratio
        );
        println!(
            "[BENCHMARK 10] Chunk Zstd Decompress: {:?} (Valid non_air: {})",
            decomp_time, decompressed.non_air_count
        );
    }

    // 11. Benchmark Connectivity Traversal (Coarse / Sub-Cluster BFS)
    {
        let start = Instant::now();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut count = 0;

        queue.push_back(IVec3::new(16, 10, 16));
        visited.insert(IVec3::new(16, 10, 16));

        while let Some(curr) = queue.pop_front() {
            count += 1;
            for offset in [
                IVec3::new(1, 0, 0),
                IVec3::new(-1, 0, 0),
                IVec3::new(0, 1, 0),
                IVec3::new(0, -1, 0),
                IVec3::new(0, 0, 1),
                IVec3::new(0, 0, -1),
            ] {
                let next = curr + offset;
                if next.x >= 0
                    && next.x < 32
                    && next.y >= 0
                    && next.y < 32
                    && next.z >= 0
                    && next.z < 32
                    && !terrain_chunk
                        .get_voxel(next.x as usize, next.y as usize, next.z as usize)
                        .is_air()
                    && visited.insert(next)
                {
                    queue.push_back(next);
                }
            }
        }
        let elapsed = start.elapsed();
        println!(
            "[BENCHMARK 11] Localized Connectivity BFS ({} voxels traversed): {:?}",
            count, elapsed
        );
    }

    // 12. Benchmark Mod Discovery & Manifest Parsing
    {
        let start = Instant::now();
        let iterations = 1_000;
        let mut discovered_count = 0;
        for _ in 0..iterations {
            let (discovered, _) = ModDiscovery::discover_from_dir("mods");
            discovered_count = discovered.len();
        }
        let elapsed = start.elapsed();
        let us_per_discovery = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 12] Mod Discovery & Manifest Parsing: {:.2} µs/run (Mods found: {}, Total: {:?})",
            us_per_discovery, discovered_count, elapsed
        );
    }

    // 13. Benchmark Registry Runtime Index O(1) vs ResourceId String Lookup
    {
        let iterations = 10_000_000;
        let res_id = ResourceId::parse("core:stone").unwrap();
        let mat_id = registry.resolve_material_id(&res_id).unwrap();

        // A. Runtime Index O(1) Lookup (Voxel Hot Path)
        let start_index = Instant::now();
        let mut sum_density = 0.0f32;
        for _ in 0..iterations {
            if let Some(def) = registry.get(mat_id) {
                sum_density += def.density_kg_m3;
            }
        }
        let elapsed_index = start_index.elapsed();
        let ns_per_index_lookup = elapsed_index.as_nanos() as f64 / iterations as f64;

        // B. Persistent ResourceId Hash Lookup (Load/Tooling Path)
        let start_res = Instant::now();
        let mut sum_density_res = 0.0f32;
        for _ in 0..iterations {
            if let Some(def) = registry.get_by_resource_id(&res_id) {
                sum_density_res += def.density_kg_m3;
            }
        }
        let elapsed_res = start_res.elapsed();
        let ns_per_res_lookup = elapsed_res.as_nanos() as f64 / iterations as f64;

        println!(
            "[BENCHMARK 13] Registry Voxel Hot Path Lookup (MaterialId): {:.2} ns/op (Zero overhead, sum: {})",
            ns_per_index_lookup, sum_density as usize
        );
        println!(
            "  -> Comparison: ResourceId Hash Lookup: {:.2} ns/op ({:.1}x difference, sum: {})",
            ns_per_res_lookup,
            ns_per_res_lookup / ns_per_index_lookup.max(0.01),
            sum_density_res as usize
        );
    }

    println!("============================================================");
    println!("             BENCHMARK SUITE COMPLETE                       ");
    println!("============================================================");
}
