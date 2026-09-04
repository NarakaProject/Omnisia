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

    // ========================================================================
    // BENCHMARK 22: DynamicBody 30 Hz Physics Tick Throughput
    // ========================================================================
    {
        use omnisia::physics::PhysicsRuntime;
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::aggregate::DetachedAggregate;

        let mut store = ChunkStore::new();
        // Muat chunk 3x3 horizontal
        for cz in -1..=1 {
            for cx in -1..=1 {
                let mut chunk = Chunk::new(IVec3::new(cx, 0, cz));
                for x in 0..32 {
                    for z in 0..32 {
                        chunk.set_voxel(x, 0, z, VoxelBlock::new(MaterialId::STONE));
                    }
                }
                store.insert(chunk);
            }
        }

        let mut runtime = PhysicsRuntime::default();
        let body_count = 100;

        // Buat 100 DynamicBody dengan aggregate 8 voxel (2x2x2)
        for i in 0..body_count {
            let mut voxels = Vec::new();
            let bx = (i % 10) * 3 - 15;
            let bz = (i / 10) * 3 - 15;
            for dy in 0..2 {
                for dz in 0..2 {
                    for dx in 0..2 {
                        voxels.push((
                            IVec3::new(bx + dx, 15 + dy, bz + dz),
                            VoxelBlock::new(MaterialId::DIRT),
                        ));
                    }
                }
            }
            let agg = DetachedAggregate::from_world_voxels(i as u64, &voxels).unwrap();
            runtime.spawn_from_detached_aggregate(agg);
        }

        let ticks = 500;
        let start = Instant::now();
        for _ in 0..ticks {
            runtime.tick(1.0 / 30.0, &store);
        }
        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / ticks as f64;
        let us_per_body_step = us_per_tick / body_count as f64;

        println!(
            "[BENCHMARK 22] DynamicBody 30 Hz Physics Tick (100 bodies): {:.2} µs/tick ({:.2} µs/body-step, {:.1} ticks/sec)",
            us_per_tick, us_per_body_step, 1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 23: Two-Phase Reintegration Throughput
    // ========================================================================
    {
        use omnisia::physics::{DynamicBodyState, PhysicsRuntime};
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::aggregate::DetachedAggregate;

        let iterations = 10_000;
        let start = Instant::now();

        for i in 0..iterations {
            let mut store = ChunkStore::new();
            store.insert(Chunk::new(IVec3::ZERO));
            // Dasar lantai
            store.set_voxel_world(IVec3::new(10, 0, 10), VoxelBlock::new(MaterialId::STONE));

            let mut runtime = PhysicsRuntime::default();
            let voxels = vec![
                (IVec3::new(10, 1, 10), VoxelBlock::new(MaterialId::DIRT)),
                (IVec3::new(10, 2, 10), VoxelBlock::new(MaterialId::DIRT)),
            ];
            let agg = DetachedAggregate::from_world_voxels(i as u64, &voxels).unwrap();
            let body_id = runtime.spawn_from_detached_aggregate(agg);
            runtime
                .get_body_mut(body_id)
                .unwrap()
                .set_state(DynamicBodyState::Settled);

            let reintegrated = runtime.process_settled_reintegration(&mut store);
            std::hint::black_box(reintegrated);
        }

        let elapsed = start.elapsed();
        let us_per_reintegration = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 23] Two-Phase Dynamic Reintegration (Prepare+Commit): {:.2} µs/op ({:.1} ops/sec)",
            us_per_reintegration, 1_000_000.0 / us_per_reintegration
        );
    }

    // ========================================================================
    // BENCHMARK 24: Player Fixed 30Hz Simulation Tick
    // ========================================================================
    {
        use omnisia::player::{PlayerController, PlayerInput};
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let mut player = PlayerController::new(Vec3::new(8.0, 0.5, 8.0));
        player.state.grounded = true;
        player.set_input(PlayerInput::from_raw(
            true, false, false, false, true, false, false,
        ));

        let iterations = 100_000;
        let start = Instant::now();

        for _ in 0..iterations {
            player.step_simulation(1.0 / 30.0, &store, 0.0);
            std::hint::black_box(&player.state);
        }

        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 24] Player Fixed 30Hz Simulation Tick: {:.3} µs/tick ({:.1} ticks/sec)",
            us_per_tick,
            1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 25: Player Swept Capsule Collision Query
    // ========================================================================
    {
        use omnisia::player::{resolve_swept_step, Capsule};
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        // Buat dinding vertikal di x = 16 (vx = 16)
        for vy in 0..10 {
            for vz in 0..32 {
                chunk.set_voxel(16, vy, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let mut capsule = Capsule::new(Vec3::new(10.0, 0.5, 10.0), 0.30, 1.8);
        let mut velocity = Vec3::new(15.0, -9.81, 5.0);
        let delta = velocity * (1.0 / 30.0);

        let iterations = 100_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let stats = resolve_swept_step(&mut capsule, &mut velocity, delta, &store);
            std::hint::black_box(stats);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 25] Player Swept Capsule Collision Query: {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 26: Player Update in Populated DynamicBody Scene (8C.2 & 8C.4)
    // ========================================================================
    {
        use omnisia::player::PlayerController;
        use omnisia::structure::aggregate::DetachedAggregate;
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        world.store.insert(chunk);

        // Spawn 50 dynamic bodies in the vicinity
        for i in 0..50 {
            let voxels = vec![(
                IVec3::new((i % 16) * 2, 2 + (i / 16), 4),
                VoxelBlock::new(MaterialId::STONE),
            )];
            let agg = DetachedAggregate::from_world_voxels(100 + i as u64, &voxels).unwrap();
            world.physics.spawn_from_detached_aggregate(agg);
        }

        let mut player = PlayerController::new(Vec3::new(4.0, 0.5, 4.0));
        let iterations = 10_000;
        let start = Instant::now();

        for _ in 0..iterations {
            world.update_player(&mut player, 1.0 / 30.0, 0.0);
            std::hint::black_box(&player.state);
        }

        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 26] Player Update with 50 DynamicBodies: {:.3} µs/tick ({:.1} ticks/sec)",
            us_per_tick,
            1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 27: Structural Mutation Event Dispatch Under Active Simulation (8C.4)
    // ========================================================================
    {
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..16 {
            for vz in 0..16 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        world.store.insert(chunk);

        let iterations = 2_000;
        let start = Instant::now();

        for i in 0..iterations {
            // Tempatkan lalu hancurkan balok
            let pos = IVec3::new(5, 1, 5);
            world.set_voxel_world(pos, VoxelBlock::new(MaterialId::STONE));
            let detached = world.set_voxel_world(pos, VoxelBlock::AIR);
            std::hint::black_box(detached);
            std::hint::black_box(i);
        }

        let elapsed = start.elapsed();
        let us_per_mutation = elapsed.as_micros() as f64 / (iterations * 2) as f64;
        println!(
            "[BENCHMARK 27] Structural Mutation Event Dispatch: {:.3} µs/mutation ({:.1} mutations/sec)",
            us_per_mutation,
            1_000_000.0 / us_per_mutation
        );
    }

    // ========================================================================
    // BENCHMARK 28: Settled DynamicBody Two-Phase Reintegration (8C.6)
    // ========================================================================
    {
        use omnisia::structure::aggregate::DetachedAggregate;
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        let chunk = Chunk::new(IVec3::ZERO);
        world.store.insert(chunk);

        let iterations = 10_000;
        let start = Instant::now();

        for i in 0..iterations {
            let voxels = vec![(IVec3::new(4, 5, 4), VoxelBlock::new(MaterialId::STONE))];
            let agg = DetachedAggregate::from_world_voxels(1000 + i as u64, &voxels).unwrap();
            let body_id = world.physics.spawn_from_detached_aggregate(agg);
            world.physics.get_body_mut(body_id).unwrap().state =
                omnisia::physics::DynamicBodyState::Settled;

            let res = world
                .physics
                .process_settled_reintegration(&mut world.store);
            std::hint::black_box(res);
            // Bersihkan kembali untuk iterasi berikutnya
            world
                .store
                .set_voxel_world(IVec3::new(4, 5, 4), VoxelBlock::AIR);
        }

        let elapsed = start.elapsed();
        let us_per_reintegration = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 28] Two-Phase Reintegration Commit: {:.3} µs/op ({:.1} ops/sec)",
            us_per_reintegration,
            1_000_000.0 / us_per_reintegration
        );
    }

    // ========================================================================
    // BENCHMARK 29: World Ownership Audit Routine (8C.5)
    // ========================================================================
    {
        use omnisia::world::World;

        let mut world = World::with_seed(WorldSeed(42));
        for cx in 0..3 {
            for cz in 0..3 {
                let mut chunk = Chunk::new(IVec3::new(cx, 0, cz));
                for vx in 0..32 {
                    for vz in 0..32 {
                        chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
                    }
                }
                world.store.insert(chunk);
            }
        }

        let iterations = 5_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let report = world.audit_world_ownership();
            std::hint::black_box(report);
        }

        let elapsed = start.elapsed();
        let us_per_audit = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 29] World Ownership Audit: {:.3} µs/audit ({:.1} audits/sec)",
            us_per_audit,
            1_000_000.0 / us_per_audit
        );
    }

    // ========================================================================
    // BENCHMARK 30: Auto-Step Traversal Solver (8D.1)
    // ========================================================================
    {
        use omnisia::player::collider::Capsule;
        use omnisia::player::collision::resolve_swept_step_with_stepup;

        let mut store = omnisia::streaming::store::ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        // Undakan 1-voxel di x >= 8, y = 1
        for vx in 8..16 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let iterations = 20_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut capsule = Capsule::new(Vec3::new(3.7, 0.5, 5.0), 1.8, 0.30);
            let mut velocity = Vec3::new(5.0, 0.0, 0.0);
            let delta = velocity * (1.0 / 30.0);
            let stats = resolve_swept_step_with_stepup(
                &mut capsule,
                &mut velocity,
                delta,
                0.55,
                true,
                &store,
                None,
            );
            std::hint::black_box(stats);
            std::hint::black_box(capsule);
        }

        let elapsed = start.elapsed();
        let us_per_step = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 30] Auto-Step Traversal Solver: {:.3} µs/step ({:.1} steps/sec)",
            us_per_step,
            1_000_000.0 / us_per_step
        );
    }

    // ========================================================================
    // BENCHMARK 31: Airborne Glide Physics Simulation (8D.2)
    // ========================================================================
    {
        use omnisia::player::{PlayerController, PlayerInput};

        let mut store = omnisia::streaming::store::ChunkStore::new();
        for cy in 0..=3 {
            let mut chunk = Chunk::new(IVec3::new(0, cy, 0));
            if cy == 0 {
                for vx in 0..32 {
                    for vz in 0..32 {
                        chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
                    }
                }
            }
            store.insert(chunk);
        }

        let mut player = PlayerController::new(Vec3::new(4.0, 50.0, 4.0));
        player.state.grounded = false;
        player.state.airborne_origin = omnisia::player::AirborneOrigin::SprintJump;
        player.state.velocity.y = -1.0;
        let input = PlayerInput {
            move_forward: 1.0,
            sprint: true,
            ..Default::default()
        };
        player.set_input(input);

        let iterations = 20_000;
        let start = Instant::now();

        for _ in 0..iterations {
            player.step_simulation(1.0 / 30.0, &store, 0.0);
            // Jaga ketinggian agar tetap melayang di udara selama iterasi
            if player.state.position.y < 20.0 {
                player.state.position.y = 50.0;
                player.state.grounded = false;
                player.state.airborne_origin = omnisia::player::AirborneOrigin::SprintJump;
                player.state.velocity.y = -1.0;
            }
            std::hint::black_box(player.state.velocity);
        }

        let elapsed = start.elapsed();
        let us_per_tick = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 31] Airborne Glide Simulation: {:.3} µs/tick ({:.1} ticks/sec)",
            us_per_tick,
            1_000_000.0 / us_per_tick
        );
    }

    // ========================================================================
    // BENCHMARK 32 (BM-G1): Ground Detection Flat Ground (8D.4)
    // Target: < 2.0 µs/query
    // ========================================================================
    {
        use omnisia::player::collision::check_ground_support;
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let feet_pos = Vec3::new(4.25, 0.5, 4.25);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 32] Ground Detection Flat Ground (BM-G1): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 33 (BM-G2): Ground Detection Edge Contact (8D.4)
    // Target: < 5.0 µs/query
    // ========================================================================
    {
        use omnisia::player::collision::check_ground_support;
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        chunk.set_voxel(4, 1, 4, VoxelBlock::new(MaterialId::STONE));
        store.insert(chunk);

        // Posisi menjorok di tepi (d = 0.15m)
        let feet_pos = Vec3::new(2.65, 0.96, 2.25);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 33] Ground Detection Edge Contact (BM-G2): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 34 (BM-G3): Ground Detection Uneven Terrain / Slope (8D.4)
    // Target: < 5.0 µs/query
    // ========================================================================
    {
        use omnisia::player::collision::check_ground_support;
        use omnisia::streaming::store::ChunkStore;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..10 {
            for vz in 0..10 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
                if vx >= 5 {
                    chunk.set_voxel(vx, 1, vz, VoxelBlock::new(MaterialId::STONE));
                }
            }
        }
        store.insert(chunk);

        // Posisi di lereng undakan
        let feet_pos = Vec3::new(2.55, 1.00, 2.25);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res = check_ground_support(feet_pos, 0.30, 0.05, &store);
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 34] Ground Detection Uneven/Slope (BM-G3): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 35 (BM-G4): Ground Detection DynamicBody Contact (8D.4)
    // Target: < 10.0 µs/query
    // ========================================================================
    {
        use omnisia::physics::{DynamicBodyState, PhysicsRuntime};
        use omnisia::player::collision::check_ground_support_with_physics;
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::aggregate::DetachedAggregate;

        let mut store = ChunkStore::new();
        let mut chunk = Chunk::new(IVec3::ZERO);
        for vx in 0..32 {
            for vz in 0..32 {
                chunk.set_voxel(vx, 0, vz, VoxelBlock::new(MaterialId::STONE));
            }
        }
        store.insert(chunk);

        let mut physics = PhysicsRuntime::default();
        let mut voxels = Vec::new();
        for vx in 0..4 {
            for vz in 0..4 {
                voxels.push((IVec3::new(vx, 1, vz), VoxelBlock::new(MaterialId::STONE)));
            }
        }
        let agg = DetachedAggregate::from_world_voxels(999, &voxels).unwrap();
        let body_id = physics.spawn_from_detached_aggregate(agg);
        let body_mut = physics.get_body_mut(body_id).unwrap();
        body_mut.state = DynamicBodyState::Settled;
        body_mut.is_grounded = true;

        let feet_pos = Vec3::new(1.0, 1.0, 1.0);
        let iterations = 50_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let res =
                check_ground_support_with_physics(feet_pos, 0.30, 0.05, &store, Some(&physics));
            std::hint::black_box(res);
        }

        let elapsed = start.elapsed();
        let us_per_query = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 35] Ground Detection DynamicBody Contact (BM-G4): {:.3} µs/query ({:.1} queries/sec)",
            us_per_query,
            1_000_000.0 / us_per_query
        );
    }

    // ========================================================================
    // BENCHMARK 36: Broadphase Update & AABB Query (Phase 9.1)
    // 100 Separated Bodies across Positive and Negative Space
    // ========================================================================
    {
        use omnisia::physics::{
            Aabb, BodyType, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
        };

        let mut broadphase = SpatialHashBroadphase::new(4.0);
        let count = 100;
        for i in 0..count {
            let base_x = (i as f32 - 50.0) * 10.0;
            let aabb = Aabb::try_new(
                Vec3::new(base_x, -10.0, -10.0),
                Vec3::new(base_x + 2.0, -8.0, -8.0),
            )
            .unwrap();
            broadphase
                .insert(BroadphaseProxy::new(
                    RigidBodyId(i as u64 + 1),
                    BodyType::Dynamic,
                    aabb,
                ))
                .unwrap();
        }

        let iterations = 10_000;
        let start = Instant::now();

        for iter in 0..iterations {
            let id = RigidBodyId((iter % count) as u64 + 1);
            let base_x = ((iter % count) as f32 - 50.0) * 10.0 + 0.1;
            let new_aabb = Aabb::try_new(
                Vec3::new(base_x, -10.0, -10.0),
                Vec3::new(base_x + 2.0, -8.0, -8.0),
            )
            .unwrap();
            let _ = broadphase.update(id, new_aabb);

            let query_box = Aabb::try_new(
                Vec3::new(base_x - 1.0, -11.0, -11.0),
                Vec3::new(base_x + 3.0, -7.0, -7.0),
            )
            .unwrap();
            let hits = broadphase.query_aabb(&query_box);
            std::hint::black_box(hits);
        }

        let elapsed = start.elapsed();
        let us_per_op = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 36] Broadphase Update & Query (100 separated bodies, +/- coords): {:.3} µs/op ({:.1} ops/sec)",
            us_per_op,
            1_000_000.0 / us_per_op
        );
    }

    // ========================================================================
    // BENCHMARK 37: Broadphase Candidate Pair Generation (Phase 9.1)
    // Dense Cluster of 200 Bodies & Medium 500 Bodies
    // ========================================================================
    {
        use omnisia::physics::{
            Aabb, BodyType, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
        };

        let mut broadphase = SpatialHashBroadphase::new(4.0);
        let count = 200;
        for i in 0..count {
            let x = ((i * 7) % 20) as f32 * 0.8;
            let y = ((i * 11) % 15) as f32 * 0.8;
            let z = ((i * 13) % 20) as f32 * 0.8;
            let aabb =
                Aabb::try_new(Vec3::new(x, y, z), Vec3::new(x + 2.0, y + 2.0, z + 2.0)).unwrap();
            let btype = if i % 5 == 0 {
                BodyType::Static
            } else {
                BodyType::Dynamic
            };
            broadphase
                .insert(BroadphaseProxy::new(RigidBodyId(i as u64 + 1), btype, aabb))
                .unwrap();
        }

        let iterations = 1_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let pairs = broadphase.generate_candidate_pairs();
            std::hint::black_box(pairs);
        }

        let elapsed = start.elapsed();
        let us_per_gen = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 37] Broadphase Candidate Pair Generation (200 dense bodies): {:.3} µs/run ({:.1} runs/sec)",
            us_per_gen,
            1_000_000.0 / us_per_gen
        );
    }

    // ========================================================================
    // BENCHMARK 38: Broadphase Large Multi-Cell Spanning AABBs (Phase 9.1)
    // 50 Large Bodies (12m x 12m) Spanning 3x3x3 Cells with Deduplication
    // ========================================================================
    {
        use omnisia::physics::{
            Aabb, BodyType, BroadphaseProxy, RigidBodyId, SpatialHashBroadphase,
        };

        let mut broadphase = SpatialHashBroadphase::new(4.0);
        let count = 50;
        for i in 0..count {
            let x = (i as f32 * 4.0) % 40.0;
            let y = (i as f32 * 2.0) % 20.0;
            let z = (i as f32 * 3.0) % 40.0;
            // AABB 12m melintasi 3 hingga 4 sel di setiap sumbu
            let aabb =
                Aabb::try_new(Vec3::new(x, y, z), Vec3::new(x + 12.0, y + 12.0, z + 12.0)).unwrap();
            broadphase
                .insert(BroadphaseProxy::new(
                    RigidBodyId(i as u64 + 1),
                    BodyType::Dynamic,
                    aabb,
                ))
                .unwrap();
        }

        let iterations = 1_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let pairs = broadphase.generate_candidate_pairs();
            std::hint::black_box(pairs);
        }

        let elapsed = start.elapsed();
        let us_per_gen = elapsed.as_micros() as f64 / iterations as f64;
        println!(
            "[BENCHMARK 38] Broadphase Large Multi-Cell Spanning & Deduplication (50 bodies): {:.3} µs/run ({:.1} runs/sec)",
            us_per_gen,
            1_000_000.0 / us_per_gen
        );
    }

    // ========================================================================
    // BENCHMARK 39: RigidBody State Construction & Access (Phase 9.2)
    // 1,000 Bodies In-Memory Construction, Storage & State Access
    // ========================================================================
    {
        use glam::Quat;
        use omnisia::physics::{MassProperties, RigidBody, RigidBodyId};
        use std::collections::BTreeMap;

        let iterations = 1_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut bodies = BTreeMap::new();
            for i in 1..=1000 {
                let id = RigidBodyId(i);
                let pos = Vec3::new(i as f32 * 0.5, 10.0, -5.0);
                let rot = Quat::from_rotation_y(0.01 * i as f32);
                let mass = 10.0 + (i % 50) as f32;
                let mass_props = MassProperties::from_sphere(mass, 0.5).unwrap();
                let body = RigidBody::new(
                    id,
                    omnisia::physics::BodyType::Dynamic,
                    pos,
                    rot,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    mass_props,
                )
                .unwrap();
                bodies.insert(id, body);
            }

            // Akses state badan
            let mut checksum = 0.0f32;
            for body in bodies.values() {
                checksum += body.position().x + body.mass_properties().inverse_mass;
            }
            std::hint::black_box(checksum);
        }

        let elapsed = start.elapsed();
        let us_per_batch = elapsed.as_micros() as f64 / iterations as f64;
        let us_per_body = us_per_batch / 1000.0;
        println!(
            "[BENCHMARK 39] RigidBody State Construction & Access (1,000 bodies): {:.3} µs/batch ({:.3} µs/body, {:.1} bodies/sec)",
            us_per_batch,
            us_per_body,
            1_000_000.0 / us_per_body
        );
    }

    // ========================================================================
    // BENCHMARK 40: Shape AABB Computation & Transform Composition (Phase 9.3)
    // 1,000 Shapes (Sphere, Rotated Box, Rotated Capsule) & Offset Colliders
    // ========================================================================
    {
        use glam::Quat;
        use omnisia::physics::{
            BoxShape, Capsule, Collider, ColliderId, RigidBodyId, Shape, Sphere, Transform,
        };

        let iterations = 1_000;
        let start = Instant::now();

        for iter in 0..iterations {
            let mut aabb_accum = Vec3::ZERO;
            for i in 1..=1000 {
                let angle = (iter + i) as f32 * 0.01;
                let body_transform = Transform::new(
                    Vec3::new(i as f32, 10.0, -5.0),
                    Quat::from_rotation_y(angle),
                )
                .unwrap();

                let local_transform =
                    Transform::from_translation(Vec3::new(0.5, 0.2, -0.3)).unwrap();

                let shape = match i % 3 {
                    0 => Shape::Sphere(Sphere::new(1.2).unwrap()),
                    1 => Shape::Box(BoxShape::new(Vec3::new(1.0, 2.0, 0.5)).unwrap()),
                    _ => Shape::Capsule(Capsule::new(0.6, 1.5).unwrap()),
                };

                let collider = Collider::new(
                    ColliderId(i as u64),
                    RigidBodyId(i as u64),
                    shape,
                    local_transform,
                );
                let aabb = collider.compute_world_aabb(&body_transform).unwrap();
                aabb_accum += aabb.min + aabb.max;
            }
            std::hint::black_box(aabb_accum);
        }

        let elapsed = start.elapsed();
        let us_per_batch = elapsed.as_micros() as f64 / iterations as f64;
        let us_per_op = us_per_batch / 1000.0;
        println!(
            "[BENCHMARK 40] Shape AABB & Collider Transform (1,000 shapes): {:.3} µs/batch ({:.3} µs/op, {:.1} ops/sec)",
            us_per_batch,
            us_per_op,
            1_000_000.0 / us_per_op
        );
    }

    // ========================================================================
    // BENCHMARK 41: Narrowphase Pair Collision Evaluation (Phase 9.4)
    // 5,000 Evaluations (Sphere/Sphere, Sphere/Box, Capsule/Capsule, Box/Box, Box/Capsule)
    // ========================================================================
    {
        use glam::Quat;
        use omnisia::physics::{
            collide, BoxShape, Capsule, Collider, ColliderId, RigidBodyId, Shape, Sphere, Transform,
        };

        let sphere_a = Collider::new(
            ColliderId(1),
            RigidBodyId(1),
            Shape::Sphere(Sphere::new(1.0).unwrap()),
            Transform::IDENTITY,
        );
        let sphere_b = Collider::new(
            ColliderId(2),
            RigidBodyId(2),
            Shape::Sphere(Sphere::new(1.0).unwrap()),
            Transform::IDENTITY,
        );

        let box_a = Collider::new(
            ColliderId(3),
            RigidBodyId(3),
            Shape::Box(BoxShape::new(Vec3::ONE).unwrap()),
            Transform::IDENTITY,
        );
        let box_b = Collider::new(
            ColliderId(4),
            RigidBodyId(4),
            Shape::Box(BoxShape::new(Vec3::new(1.5, 0.8, 1.2)).unwrap()),
            Transform::IDENTITY,
        );

        let cap_a = Collider::new(
            ColliderId(5),
            RigidBodyId(5),
            Shape::Capsule(Capsule::new(0.5, 1.0).unwrap()),
            Transform::IDENTITY,
        );
        let cap_b = Collider::new(
            ColliderId(6),
            RigidBodyId(6),
            Shape::Capsule(Capsule::new(0.6, 1.2).unwrap()),
            Transform::IDENTITY,
        );

        let iterations = 200;
        let start = Instant::now();
        let mut total_contacts = 0usize;

        for iter in 0..iterations {
            let offset = (iter as f32 * 0.001) % 0.5;

            // 1,000 Sphere/Sphere
            for i in 0..1000 {
                let dist = 1.0 + (i as f32 * 0.002) + offset;
                let t_a = Transform::IDENTITY;
                let t_b = Transform::from_translation(Vec3::new(dist, 0.0, 0.0)).unwrap();
                if let Ok(Some(_)) = collide(&sphere_a, &t_a, &sphere_b, &t_b) {
                    total_contacts += 1;
                }
            }

            // 1,000 Sphere/Box
            for i in 0..1000 {
                let dist = 1.0 + (i as f32 * 0.002) + offset;
                let t_s = Transform::from_translation(Vec3::new(dist, 0.0, 0.0)).unwrap();
                let t_b =
                    Transform::new(Vec3::ZERO, Quat::from_rotation_y(0.1 * i as f32)).unwrap();
                if let Ok(Some(_)) = collide(&sphere_a, &t_s, &box_a, &t_b) {
                    total_contacts += 1;
                }
            }

            // 1,000 Capsule/Capsule
            for i in 0..1000 {
                let dist = 0.5 + (i as f32 * 0.002) + offset;
                let t_a = Transform::IDENTITY;
                let t_b = Transform::new(
                    Vec3::new(dist, 0.0, 0.0),
                    Quat::from_rotation_z(0.05 * i as f32),
                )
                .unwrap();
                if let Ok(Some(_)) = collide(&cap_a, &t_a, &cap_b, &t_b) {
                    total_contacts += 1;
                }
            }

            // 1,000 Box/Box (SAT 15 axes)
            for i in 0..1000 {
                let dist = 1.2 + (i as f32 * 0.002) + offset;
                let t_a = Transform::IDENTITY;
                let t_b = Transform::new(
                    Vec3::new(dist, 0.0, 0.0),
                    Quat::from_rotation_y(0.02 * i as f32),
                )
                .unwrap();
                if let Ok(Some(_)) = collide(&box_a, &t_a, &box_b, &t_b) {
                    total_contacts += 1;
                }
            }

            // 1,000 Box/Capsule
            for i in 0..1000 {
                let dist = 0.8 + (i as f32 * 0.002) + offset;
                let t_b = Transform::IDENTITY;
                let t_c = Transform::new(
                    Vec3::new(dist, 0.0, 0.0),
                    Quat::from_rotation_x(0.03 * i as f32),
                )
                .unwrap();
                if let Ok(Some(_)) = collide(&box_a, &t_b, &cap_a, &t_c) {
                    total_contacts += 1;
                }
            }
        }

        std::hint::black_box(total_contacts);
        let elapsed = start.elapsed();
        let total_evals = (iterations * 5000) as f64;
        let us_per_eval = elapsed.as_micros() as f64 / total_evals;
        let evals_per_sec = 1_000_000.0 / us_per_eval;

        println!(
            "[BENCHMARK 41] Narrowphase Pair Collision Evaluation (5,000 pairs/batch): {:.3} µs/eval ({:.1} evals/sec, contacts: {})",
            us_per_eval,
            evals_per_sec,
            total_contacts
        );
    }

    // ========================================================================
    // BENCHMARK 42: Sequential Impulse Contact Solver (Phase 9.5)
    // 100, 500, 1,000 Contacts @ 10 Iterations
    // ========================================================================
    {
        use glam::{Mat3, Quat};
        use omnisia::physics::{
            solve_contacts, ColliderId, Contact, RigidBody, RigidBodyId, SolverConfig,
        };
        use std::collections::BTreeMap;

        let contact_counts = [100, 500, 1000];
        let solver_config = SolverConfig {
            iterations: 10,
            beta: 0.2,
            penetration_slop: 0.001,
            ..Default::default()
        };
        let dt = 1.0 / 30.0;

        for &num_contacts in &contact_counts {
            // Setup dynamic body A and static floor body B with contacts
            let mut bodies = BTreeMap::new();
            let body_a_id = RigidBodyId(1);
            let body_b_id = RigidBodyId(2);

            let inertia = Mat3::from_diagonal(Vec3::ONE);
            let body_a = RigidBody::new_dynamic(
                body_a_id,
                Vec3::new(0.0, 1.0, 0.0),
                Quat::IDENTITY,
                2.0,
                inertia,
            )
            .unwrap();
            let body_b = RigidBody::new_static(body_b_id, Vec3::ZERO, Quat::IDENTITY).unwrap();

            bodies.insert(body_a_id, body_a);
            bodies.insert(body_b_id, body_b);

            let mut contacts = Vec::with_capacity(num_contacts);
            for i in 0..num_contacts {
                let x = (i as f32) * 0.01;
                let contact = Contact::new(
                    ColliderId((i * 2 + 1) as u64),
                    ColliderId((i * 2 + 2) as u64),
                    body_a_id,
                    body_b_id,
                    Vec3::new(x, 0.5, 0.0),
                    Vec3::NEG_Y,
                    0.02,
                );
                contacts.push(contact);
            }

            let num_runs = 500;
            let start = Instant::now();

            for _ in 0..num_runs {
                // Reset velocity to simulate arriving at contact each step
                bodies
                    .get_mut(&body_a_id)
                    .unwrap()
                    .set_linear_velocity(Vec3::new(0.0, -2.0, 0.0))
                    .unwrap();
                solve_contacts(&mut bodies, &contacts, dt, &solver_config).unwrap();
            }

            let elapsed = start.elapsed();
            let us_total = elapsed.as_micros() as f64 / num_runs as f64;
            let us_per_contact = us_total / num_contacts as f64;
            let us_per_contact_iter = us_per_contact / solver_config.iterations as f64;

            println!(
                "[BENCHMARK 42] Contact Solver ({} contacts, {} iters): {:.3} µs/batch ({:.3} µs/contact, {:.4} µs/contact/iter)",
                num_contacts,
                solver_config.iterations,
                us_total,
                us_per_contact,
                us_per_contact_iter,
            );
        }
    }

    // ========================================================================
    // BENCHMARK 43: Linear + Angular Integration (Phase 9.6)
    // 100, 500, 1,000 Bodies (Linear-Only & Mixed Linear+Angular)
    // ========================================================================
    {
        use glam::{Mat3, Quat};
        use omnisia::physics::{integrate_bodies, RigidBody, RigidBodyId};
        use std::collections::BTreeMap;

        let body_counts = [100, 500, 1000];
        let dt = 1.0 / 30.0;
        let gravity = Vec3::new(0.0, -9.81, 0.0);

        // Sub-benchmark A: Linear-only workload (angular velocity = 0)
        for &count in &body_counts {
            let mut bodies = BTreeMap::new();
            let inertia = Mat3::from_diagonal(Vec3::ONE);

            for i in 0..count {
                let id = RigidBodyId(i as u64 + 1);
                let pos = Vec3::new((i as f32) * 0.5, 10.0, 0.0);
                let mut body =
                    RigidBody::new_dynamic(id, pos, Quat::IDENTITY, 2.0, inertia).unwrap();
                body.set_linear_velocity(Vec3::new(1.0, 0.0, 0.0)).unwrap();
                bodies.insert(id, body);
            }

            let num_runs = 1000;
            let start = Instant::now();

            for _ in 0..num_runs {
                integrate_bodies(&mut bodies, dt, gravity).unwrap();
            }

            let elapsed = start.elapsed();
            let us_total = elapsed.as_micros() as f64 / num_runs as f64;
            let us_per_body = us_total / count as f64;

            println!(
                "[BENCHMARK 43A] Linear-Only Integration ({} bodies): {:.3} µs/batch ({:.4} µs/body)",
                count, us_total, us_per_body,
            );
        }

        // Sub-benchmark B: Mixed Linear + Angular workload
        for &count in &body_counts {
            let mut bodies = BTreeMap::new();
            let inertia = Mat3::from_diagonal(Vec3::ONE);

            for i in 0..count {
                let id = RigidBodyId(i as u64 + 1);
                let pos = Vec3::new((i as f32) * 0.5, 10.0, 0.0);
                let mut body =
                    RigidBody::new_dynamic(id, pos, Quat::IDENTITY, 2.0, inertia).unwrap();
                body.set_linear_velocity(Vec3::new(1.0, 0.0, 0.0)).unwrap();
                body.set_angular_velocity(Vec3::new(0.0, 1.5, 0.0)).unwrap();
                bodies.insert(id, body);
            }

            let num_runs = 1000;
            let start = Instant::now();

            for _ in 0..num_runs {
                integrate_bodies(&mut bodies, dt, gravity).unwrap();
            }

            let elapsed = start.elapsed();
            let us_total = elapsed.as_micros() as f64 / num_runs as f64;
            let us_per_body = us_total / count as f64;

            println!(
                "[BENCHMARK 43B] Mixed Linear + Angular Integration ({} bodies): {:.3} µs/batch ({:.4} µs/body)",
                count, us_total, us_per_body,
            );
        }
    }

    // ========================================================================
    // BENCHMARK 44: Friction + Restitution Solver (Phase 9.7)
    // 100, 500, 1,000 Contacts @ 10 Iterations (Normal, Restitution, Friction, Combined)
    // ========================================================================
    {
        use glam::{Mat3, Quat};
        use omnisia::physics::{
            solve_contacts, ColliderId, Contact, RigidBody, RigidBodyId, SolverConfig,
        };
        use std::collections::BTreeMap;

        let contact_counts = [100, 500, 1000];
        let solver_config = SolverConfig {
            iterations: 10,
            beta: 0.2,
            penetration_slop: 0.001,
            restitution_velocity_threshold: 0.1,
        };
        let dt = 1.0 / 30.0;

        let configurations = [
            ("Normal-only (e=0, mu=0)", 0.0, 0.0),
            ("Restitution-only (e=0.5, mu=0)", 0.5, 0.0),
            ("Friction-only (e=0, mu=0.5)", 0.0, 0.5),
            ("Combined (e=0.5, mu=0.5)", 0.5, 0.5),
        ];

        for &(cfg_name, restitution, friction) in &configurations {
            for &num_contacts in &contact_counts {
                let mut bodies_template = BTreeMap::new();
                let body_a_id = RigidBodyId(1);
                let body_b_id = RigidBodyId(2);

                let inertia = Mat3::from_diagonal(Vec3::ONE);
                let mut body_a = RigidBody::new_dynamic(
                    body_a_id,
                    Vec3::new(0.0, 1.0, 0.0),
                    Quat::IDENTITY,
                    2.0,
                    inertia,
                )
                .unwrap();
                body_a
                    .set_linear_velocity(Vec3::new(3.0, -4.0, 3.0))
                    .unwrap();
                bodies_template.insert(body_a_id, body_a);

                let body_b = RigidBody::new_static(body_b_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
                bodies_template.insert(body_b_id, body_b);

                let mut contacts = Vec::with_capacity(num_contacts);
                for i in 0..num_contacts {
                    let point = Vec3::new((i as f32) * 0.01, 0.0, ((i % 10) as f32) * 0.01);
                    let mut c = Contact::new(
                        ColliderId(i as u64 * 2 + 1),
                        ColliderId(i as u64 * 2 + 2),
                        body_a_id,
                        body_b_id,
                        point,
                        Vec3::NEG_Y,
                        0.005,
                    );
                    c.restitution = restitution;
                    c.friction = friction;
                    contacts.push(c);
                }

                let num_runs = 100;
                let start = Instant::now();

                for _ in 0..num_runs {
                    let mut b = bodies_template.clone();
                    solve_contacts(&mut b, &contacts, dt, &solver_config).unwrap();
                }

                let elapsed = start.elapsed();
                let us_total = elapsed.as_micros() as f64 / num_runs as f64;
                let us_per_contact = us_total / num_contacts as f64;
                let us_per_contact_iter = us_per_contact / solver_config.iterations as f64;

                println!(
                    "[BENCHMARK 44] {} ({} contacts, {} iters): {:.3} µs/batch ({:.4} µs/contact, {:.5} µs/contact/iter)",
                    cfg_name,
                    num_contacts,
                    solver_config.iterations,
                    us_total,
                    us_per_contact,
                    us_per_contact_iter
                );
            }
        }
    }

    // ========================================================================
    // BENCHMARK 45: Physics Island & Sleeping Management (Phase 9.8)
    // 100, 500, 1,000 Bodies: All Active vs 90% Sleeping
    // ========================================================================
    {
        use glam::{Mat3, Quat, Vec3};
        use omnisia::physics::{
            PhysicsWorld, PhysicsWorldConfig, RigidBody, RigidBodyId, SleepConfig,
        };

        println!("------------------------------------------------------------");
        println!(" [BENCHMARK 45] Physics Island & Sleeping Management (9.8)  ");
        println!("------------------------------------------------------------");

        let body_counts = [100, 500, 1000];

        for &count in &body_counts {
            // Setup PhysicsWorld dengan 1 static floor dan N dynamic boxes
            let mut world = PhysicsWorld::new(PhysicsWorldConfig {
                world_gravity: Vec3::ZERO,
                sleep_config: SleepConfig {
                    linear_velocity_threshold: 0.05,
                    angular_velocity_threshold: 0.05,
                    sleep_duration: 0.5,
                },
                ..Default::default()
            });

            let floor_id = RigidBodyId(0);
            let floor_body = RigidBody::new_static(floor_id, Vec3::ZERO, Quat::IDENTITY).unwrap();
            world.add_rigid_body(floor_body, None).unwrap();

            let inertia = Mat3::from_diagonal(Vec3::splat(1.0));
            let mut dynamic_ids = Vec::with_capacity(count);

            for i in 1..=count {
                let id = RigidBodyId(i as u64);
                let x = ((i - 1) % 25) as f32 * 2.0;
                let z = ((i - 1) / 25) as f32 * 2.0;
                let pos = Vec3::new(x, 1.0, z);
                let mut body =
                    RigidBody::new_dynamic(id, pos, Quat::IDENTITY, 1.0, inertia).unwrap();
                body.set_linear_velocity(Vec3::new(1.0, 0.0, 0.0)).unwrap();
                world.add_rigid_body(body, None).unwrap();
                dynamic_ids.push(id);
            }

            // 1. Ukur Waktu Konstruksi Pulau (build_islands)
            let islands_runs = 200;
            let start_islands = Instant::now();
            for _ in 0..islands_runs {
                let islands = world.build_islands(&[]).unwrap();
                std::hint::black_box(islands);
            }
            let us_island = start_islands.elapsed().as_micros() as f64 / islands_runs as f64;
            let us_island_per_body = us_island / count as f64;

            // 2. Ukur Waktu Full Step: All Active (100% aktif)
            let step_runs = 100;
            let start_active = Instant::now();
            for _ in 0..step_runs {
                let _ = world.step().unwrap();
            }
            let us_active = start_active.elapsed().as_micros() as f64 / step_runs as f64;
            let us_active_per_body = us_active / count as f64;

            // 3. Konfigurasikan 90% Sleeping, 10% Active
            let sleep_count = (count * 9) / 10;
            for &id in dynamic_ids.iter().take(sleep_count) {
                if let Some(b) = world.get_rigid_body_mut(id) {
                    b.put_to_sleep();
                }
            }

            // 4. Ukur Waktu Full Step: 90% Sleeping
            let start_sleeping = Instant::now();
            for _ in 0..step_runs {
                let _ = world.step().unwrap();
            }
            let us_sleeping = start_sleeping.elapsed().as_micros() as f64 / step_runs as f64;
            let us_sleeping_per_body = us_sleeping / count as f64;

            let speedup = if us_sleeping > 0.0 {
                us_active / us_sleeping
            } else {
                1.0
            };

            println!(
                "[BM45] Island Build ({} bodies): {:.3} µs/build ({:.4} µs/body)",
                count, us_island, us_island_per_body
            );
            println!(
                "[BM45] Step 100% Active ({} bodies): {:.3} µs/step ({:.4} µs/body)",
                count, us_active, us_active_per_body
            );
            println!(
                "[BM45] Step 90% Sleeping ({} bodies): {:.3} µs/step ({:.4} µs/body)",
                count, us_sleeping, us_sleeping_per_body
            );
            println!(
                "[BM45] Sleeping Speedup Factor ({} bodies): {:.2}x",
                count, speedup
            );
        }
    }

    // ========================================================================
    // BENCHMARK 46: Dynamic ↔ Static vs Dynamic ↔ Dynamic Solver (Phase 9.9)
    // 100, 500, 1,000 Contacts @ 10 Iterations (Combined e=0.5, mu=0.5)
    // ========================================================================
    {
        use glam::{Mat3, Quat, Vec3};
        use omnisia::physics::{
            solve_contacts, ColliderId, Contact, RigidBody, RigidBodyId, SolverConfig,
        };
        use std::collections::BTreeMap;

        println!("------------------------------------------------------------");
        println!(" [BENCHMARK 46] Dynamic ↔ Static vs Dynamic ↔ Dynamic (9.9) ");
        println!("------------------------------------------------------------");

        let contact_counts = [100, 500, 1000];
        let solver_config = SolverConfig {
            iterations: 10,
            beta: 0.2,
            penetration_slop: 0.001,
            restitution_velocity_threshold: 0.1,
        };
        let dt = 1.0 / 30.0;
        let num_runs = 100;
        let inertia = Mat3::from_diagonal(Vec3::ONE);

        for &num_contacts in &contact_counts {
            // 1. Scenario A: Dynamic ↔ Static
            let mut bodies_ds = BTreeMap::new();
            let mut contacts_ds = Vec::with_capacity(num_contacts);
            for i in 0..num_contacts {
                let dyn_id = RigidBodyId((i * 2 + 1) as u64);
                let static_id = RigidBodyId((i * 2 + 2) as u64);

                let mut dyn_b = RigidBody::new_dynamic(
                    dyn_id,
                    Vec3::new(i as f32 * 0.1, 1.0, 0.0),
                    Quat::IDENTITY,
                    2.0,
                    inertia,
                )
                .unwrap();
                dyn_b
                    .set_linear_velocity(Vec3::new(2.0, -3.0, 1.0))
                    .unwrap();
                dyn_b
                    .set_angular_velocity(Vec3::new(0.5, 0.0, 0.5))
                    .unwrap();
                bodies_ds.insert(dyn_id, dyn_b);

                let static_b = RigidBody::new_static(
                    static_id,
                    Vec3::new(i as f32 * 0.1, 0.0, 0.0),
                    Quat::IDENTITY,
                )
                .unwrap();
                bodies_ds.insert(static_id, static_b);

                let mut c = Contact::new(
                    ColliderId((i * 2 + 1) as u64),
                    ColliderId((i * 2 + 2) as u64),
                    dyn_id,
                    static_id,
                    Vec3::new(i as f32 * 0.1, 0.5, 0.0),
                    Vec3::NEG_Y,
                    0.005,
                );
                c.restitution = 0.5;
                c.friction = 0.5;
                contacts_ds.push(c);
            }

            let start_ds = Instant::now();
            for _ in 0..num_runs {
                let mut b = bodies_ds.clone();
                solve_contacts(&mut b, &contacts_ds, dt, &solver_config).unwrap();
            }
            let us_ds_total = start_ds.elapsed().as_micros() as f64 / num_runs as f64;
            let us_ds_per_contact = us_ds_total / num_contacts as f64;
            let us_ds_per_iter = us_ds_per_contact / solver_config.iterations as f64;

            // 2. Scenario B: Dynamic ↔ Dynamic
            let mut bodies_dd = BTreeMap::new();
            let mut contacts_dd = Vec::with_capacity(num_contacts);
            for i in 0..num_contacts {
                let dyn_a_id = RigidBodyId((i * 2 + 1) as u64);
                let dyn_b_id = RigidBodyId((i * 2 + 2) as u64);

                let mut dyn_a = RigidBody::new_dynamic(
                    dyn_a_id,
                    Vec3::new(i as f32 * 0.1, 1.0, 0.0),
                    Quat::IDENTITY,
                    2.0,
                    inertia,
                )
                .unwrap();
                dyn_a
                    .set_linear_velocity(Vec3::new(2.0, -3.0, 1.0))
                    .unwrap();
                dyn_a
                    .set_angular_velocity(Vec3::new(0.5, 0.0, 0.5))
                    .unwrap();
                bodies_dd.insert(dyn_a_id, dyn_a);

                let mut dyn_b = RigidBody::new_dynamic(
                    dyn_b_id,
                    Vec3::new(i as f32 * 0.1, 0.0, 0.0),
                    Quat::IDENTITY,
                    3.0,
                    inertia,
                )
                .unwrap();
                dyn_b
                    .set_linear_velocity(Vec3::new(-1.0, 2.0, -0.5))
                    .unwrap();
                dyn_b
                    .set_angular_velocity(Vec3::new(0.0, 0.5, 0.0))
                    .unwrap();
                bodies_dd.insert(dyn_b_id, dyn_b);

                let mut c = Contact::new(
                    ColliderId((i * 2 + 1) as u64),
                    ColliderId((i * 2 + 2) as u64),
                    dyn_a_id,
                    dyn_b_id,
                    Vec3::new(i as f32 * 0.1, 0.5, 0.0),
                    Vec3::NEG_Y,
                    0.005,
                );
                c.restitution = 0.5;
                c.friction = 0.5;
                contacts_dd.push(c);
            }

            let start_dd = Instant::now();
            for _ in 0..num_runs {
                let mut b = bodies_dd.clone();
                solve_contacts(&mut b, &contacts_dd, dt, &solver_config).unwrap();
            }
            let us_dd_total = start_dd.elapsed().as_micros() as f64 / num_runs as f64;
            let us_dd_per_contact = us_dd_total / num_contacts as f64;
            let us_dd_per_iter = us_dd_per_contact / solver_config.iterations as f64;

            let ratio = if us_ds_total > 0.0 {
                us_dd_total / us_ds_total
            } else {
                1.0
            };

            println!(
                "[BM46] Dynamic ↔ Static ({} contacts): {:.3} µs/batch ({:.4} µs/contact, {:.5} µs/contact/iter)",
                num_contacts, us_ds_total, us_ds_per_contact, us_ds_per_iter
            );
            println!(
                "[BM46] Dynamic ↔ Dynamic ({} contacts): {:.3} µs/batch ({:.4} µs/contact, {:.5} µs/contact/iter)",
                num_contacts, us_dd_total, us_dd_per_contact, us_dd_per_iter
            );
            println!(
                "[BM46] Ratio Dynamic-Dynamic / Dynamic-Static: {:.2}x",
                ratio
            );
        }
    }

    // ========================================================================
    // BENCHMARK 47: Player ↔ DynamicBody Interaction (Phase 9.10)
    // 10, 100, 500 DynamicBodies @ 30 Hz Fixed Step
    // ========================================================================
    {
        use glam::{Mat3, Quat, Vec3};
        use omnisia::physics::{
            BoxShape, Collider, ColliderId, PhysicsWorld, PhysicsWorldConfig, PlayerBridgeConfig,
            PlayerRigidBodyBridge, RigidBody, RigidBodyId, Shape, Transform,
        };
        use omnisia::player::PlayerController;

        println!("------------------------------------------------------------");
        println!(" [BENCHMARK 47] Player ↔ DynamicBody Interaction (9.10)     ");
        println!("------------------------------------------------------------");

        let body_counts = [10, 100, 500];
        let dt = 1.0 / 30.0;
        let num_runs = 100;
        let inertia = Mat3::from_diagonal(Vec3::ONE);

        for &count in &body_counts {
            let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
            let mut bridge = PlayerRigidBodyBridge::new(PlayerBridgeConfig::default());

            // Buat N dynamic boxes terdistribusi di grid sekitar origin
            let grid_side = (count as f32).sqrt().ceil() as usize;
            for i in 0..count {
                let id = RigidBodyId((i + 1) as u64);
                let gx = (i % grid_side) as f32 * 1.5 - (grid_side as f32 * 0.75);
                let gz = (i / grid_side) as f32 * 1.5 - (grid_side as f32 * 0.75);
                let pos = Vec3::new(gx, 0.5, gz);

                let body = RigidBody::new_dynamic(id, pos, Quat::IDENTITY, 2.0, inertia).unwrap();
                world.add_rigid_body(body, None).unwrap();

                let col = Collider::new(
                    ColliderId((i + 1) as u64),
                    id,
                    Shape::Box(BoxShape::new(Vec3::splat(0.5)).unwrap()),
                    Transform::IDENTITY,
                );
                world.add_collider(col).unwrap();
            }

            let mut player = PlayerController::new(Vec3::new(0.0, 1.0, 0.0));
            player.state.velocity = Vec3::new(3.0, 0.0, 0.0); // Jalan normal 3.0 m/s

            let start = Instant::now();
            for _ in 0..num_runs {
                bridge.step(&mut player, &mut world, None, dt, 0.0);
            }
            let us_step = start.elapsed().as_micros() as f64 / num_runs as f64;

            println!(
                "[BM47] Player ↔ DynamicBody Step ({} bodies): {:.3} µs/step",
                count, us_step
            );
        }
    }

    // 48. Benchmark Structural Aggregate ↔ RigidBody Integration (9.11)
    {
        use omnisia::chunk::Chunk;
        use omnisia::physics::{
            AggregateColliderStrategy, OrientationQuantizationPolicy, PhysicsWorld,
            PhysicsWorldConfig,
        };
        use omnisia::streaming::store::ChunkStore;
        use omnisia::structure::aggregate::DetachedAggregate;
        use omnisia::voxel::VoxelBlock;

        println!("------------------------------------------------------------");
        println!(" [BENCHMARK 48] Structural Aggregate ↔ RigidBody (9.11)      ");
        println!("------------------------------------------------------------");

        // Helper pembangun aggregate sintetis
        fn make_benchmark_aggregate(id: u64, size: IVec3, offset: IVec3) -> DetachedAggregate {
            let mut voxels = Vec::new();
            for x in 0..size.x {
                for y in 0..size.y {
                    for z in 0..size.z {
                        voxels.push((
                            offset + IVec3::new(x, y, z),
                            VoxelBlock::new(MaterialId::STONE),
                        ));
                    }
                }
            }
            DetachedAggregate::from_world_voxels(id, &voxels).unwrap()
        }

        let aggregate_configs = [
            ("Small (2 voxels)", IVec3::new(2, 1, 1)),
            ("Medium (8 voxels)", IVec3::new(2, 2, 2)),
            ("Large (64 voxels)", IVec3::new(4, 4, 4)),
        ];

        let counts = [10, 100, 500];

        for (desc, size) in &aggregate_configs {
            println!("  Topology: {}", desc);

            for &count in &counts {
                let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
                let grid_side = (count as f32).sqrt().ceil() as usize;

                // 1. Ukur Waktu Physicalization
                let start_phys = Instant::now();
                let mut dyn_ids = Vec::with_capacity(count);
                for i in 0..count {
                    let gx = (i % grid_side) as i32 * 6;
                    let gz = (i / grid_side) as i32 * 6;
                    let agg =
                        make_benchmark_aggregate((i + 1) as u64, *size, IVec3::new(gx, 20, gz));
                    let id = world
                        .physicalize_aggregate(agg, None, AggregateColliderStrategy::CompoundBoxes)
                        .unwrap();
                    dyn_ids.push(id);
                }
                let us_phys = start_phys.elapsed().as_micros() as f64 / count as f64;

                // 2. Ukur Waktu Physics Step (30 Hz fixed timestep)
                let num_steps = 20;
                let start_step = Instant::now();
                for _ in 0..num_steps {
                    world.step().unwrap();
                }
                let us_step = start_step.elapsed().as_micros() as f64 / num_steps as f64;

                // 3. Ukur Waktu Sleeping Step (tidurkan seluruh badan)
                for id in &dyn_ids {
                    if let Some(rec) = world.get_dynamic_aggregate(*id) {
                        if let Some(b) = world.get_rigid_body_mut(rec.rigid_body_id) {
                            b.put_to_sleep();
                        }
                    }
                }
                let start_sleep = Instant::now();
                for _ in 0..num_steps {
                    world.step().unwrap();
                }
                let us_sleep = start_sleep.elapsed().as_micros() as f64 / num_steps as f64;

                // 4. Ukur Waktu Reintegrasi ke ChunkStore
                let mut store = ChunkStore::new();
                for cx in -2..=((grid_side as i32 * 6) / 32 + 2) {
                    for cz in -2..=((grid_side as i32 * 6) / 32 + 2) {
                        store.insert(Chunk::new(IVec3::new(cx, 0, cz)));
                    }
                }
                let start_reint = Instant::now();
                let mut reint_count = 0;
                for id in &dyn_ids {
                    if world
                        .reintegrate_aggregate(
                            *id,
                            &mut store,
                            OrientationQuantizationPolicy::NearestLattice,
                        )
                        .is_ok()
                    {
                        reint_count += 1;
                    }
                }
                let us_reint = if reint_count > 0 {
                    start_reint.elapsed().as_micros() as f64 / reint_count as f64
                } else {
                    0.0
                };

                println!(
                    "    [BM48] N={:3} -> Phys: {:.2} µs/agg | Step: {:.2} µs/step | Sleep: {:.2} µs/step | Reint: {:.2} µs/agg (ok: {})",
                    count, us_phys, us_step, us_sleep, us_reint, reint_count
                );
            }
        }
    }

    // 49. Benchmark RigidBody Stress & Scaling (Phase 9.12)
    {
        use glam::Quat;
        use omnisia::physics::world::{PhysicsWorld, PhysicsWorldConfig};
        use omnisia::physics::{
            BodyType, BoxShape, Collider, ColliderId, MassProperties, RigidBody, RigidBodyId,
            Shape, Transform,
        };

        println!("------------------------------------------------------------");
        println!("[BENCHMARK 49] RigidBody Stress & Scaling (Phase 9.12)");

        // 1. Sparse Active Scaling (100 to 5000 bodies)
        let body_counts = [100, 500, 1000, 2500, 5000];
        for &count in &body_counts {
            let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
            world.config.world_gravity = Vec3::ZERO;
            let spacing = 15.0f32;
            let grid_side = (count as f32).cbrt().ceil() as i32;

            for i in 0..count {
                let id = RigidBodyId((i + 1) as u64);
                let gx = (i % grid_side) as f32 * spacing;
                let gy = ((i / grid_side) % grid_side) as f32 * spacing;
                let gz = (i / (grid_side * grid_side)) as f32 * spacing;

                let body = RigidBody::new(
                    id,
                    BodyType::Dynamic,
                    Vec3::new(gx, gy, gz),
                    Quat::IDENTITY,
                    Vec3::new(0.01, 0.0, 0.0),
                    Vec3::ZERO,
                    MassProperties::from_box(1.0, Vec3::splat(1.0)).unwrap(),
                )
                .unwrap();
                world.add_rigid_body(body, None).unwrap();
                let col = Collider::new(
                    ColliderId((i + 1) as u64),
                    id,
                    Shape::Box(BoxShape::new(Vec3::splat(0.5)).unwrap()),
                    Transform::IDENTITY,
                );
                world.add_collider(col).unwrap();
            }

            // Warm-up 2 steps
            world.step().unwrap();
            world.step().unwrap();

            // Profiled step
            let prof = world.step_profiled().unwrap();
            let total_ms = prof.timings.total_step_ns as f64 / 1_000_000.0;
            let bp_us = prof.timings.broadphase_candidates_ns as f64 / 1_000.0;
            let island_us = prof.timings.island_build_ns as f64 / 1_000.0;
            let integ_us = prof.timings.transform_integration_ns as f64 / 1_000.0;

            println!(
                "    [BM49] N={:4} Active -> Total: {:.3} ms | BP: {:.1} µs | Island: {:.1} µs | Integ: {:.1} µs",
                count, total_ms, bp_us, island_us, integ_us
            );
        }

        // 2. Sleeping Speedup at N=5000
        {
            let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
            world.config.world_gravity = Vec3::ZERO;
            let count = 5000;
            let spacing = 15.0f32;
            let grid_side = (count as f32).cbrt().ceil() as i32;

            for i in 0..count {
                let id = RigidBodyId((i + 1) as u64);
                let gx = (i % grid_side) as f32 * spacing;
                let gy = ((i / grid_side) % grid_side) as f32 * spacing;
                let gz = (i / (grid_side * grid_side)) as f32 * spacing;

                let mut body = RigidBody::new(
                    id,
                    BodyType::Dynamic,
                    Vec3::new(gx, gy, gz),
                    Quat::IDENTITY,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    MassProperties::from_box(1.0, Vec3::splat(1.0)).unwrap(),
                )
                .unwrap();
                body.put_to_sleep();
                world.add_rigid_body(body, None).unwrap();
                let col = Collider::new(
                    ColliderId((i + 1) as u64),
                    id,
                    Shape::Box(BoxShape::new(Vec3::splat(0.5)).unwrap()),
                    Transform::IDENTITY,
                );
                world.add_collider(col).unwrap();
            }

            let prof = world.step_profiled().unwrap();
            let sleep_ms = prof.timings.total_step_ns as f64 / 1_000_000.0;
            println!(
                "    [BM49] N=5000 Sleeping -> Total: {:.3} ms (Bypassed integration & solver)",
                sleep_ms
            );
        }

        // ====================================================================
        // [BENCHMARK 50] Impact Foundation & Deterministic Pipeline (Phase 10.1)
        // ====================================================================
        {
            use omnisia::impact::{
                AffectedVolume, DeterministicImpactPipeline, ImpactEvent, ImpactId, ImpactSource,
            };

            println!("[BENCHMARK 50] Impact Foundation & Deterministic Pipeline (Phase 10.1)");

            // 1. ImpactEvent Construction & Validation (10,000 events)
            let n_events = 10_000;
            let start = Instant::now();
            let mut dummy_sum = 0.0f32;
            for i in 0..n_events {
                let event = ImpactEvent::builder(
                    ImpactId(i as u64),
                    Vec3::new(i as f32 * 0.1, 10.0, -i as f32 * 0.1),
                    3.5,
                )
                .source(ImpactSource::projectile((i % 100) as u64))
                .direction(Vec3::new(0.0, -1.0, 0.0))
                .energy(5000.0)
                .build()
                .unwrap();
                dummy_sum += event.radius;
            }
            let dur = start.elapsed();
            let ns_per_event = dur.as_nanos() as f64 / n_events as f64;
            println!(
                "    [BM50] ImpactEvent Construction: {:.2} ns/event (Total: {:?}, dummy: {:.0})",
                ns_per_event, dur, dummy_sum
            );

            // 2. AffectedVolume Spatial Query (10,000 queries)
            let start = Instant::now();
            let mut total_chunks = 0usize;
            for i in 0..n_events {
                let center = Vec3::new((i % 50) as f32 * 10.0, 5.0, (i / 50) as f32 * 10.0);
                let vol = AffectedVolume::from_sphere(center, 4.0).unwrap();
                total_chunks += vol.chunk_count();
            }
            let dur = start.elapsed();
            let ns_per_query = dur.as_nanos() as f64 / n_events as f64;
            println!(
                "    [BM50] AffectedVolume Query: {:.2} ns/query (Total: {:?}, total_chunks: {})",
                ns_per_query, dur, total_chunks
            );

            // 3. Deterministic Impact Pipeline Processing (1,000 shuffled events)
            let n_pipe = 1_000;
            let mut pipeline = DeterministicImpactPipeline::new();
            for i in 0..n_pipe {
                // Submit in reverse / interleaved order
                let id = if i % 2 == 0 { n_pipe - i } else { i };
                let event = ImpactEvent::builder(
                    ImpactId(id as u64),
                    Vec3::new((i % 20) as f32 * 8.0, 0.0, (i / 20) as f32 * 8.0),
                    2.5,
                )
                .source(ImpactSource::environment(1))
                .energy(1000.0)
                .build()
                .unwrap();
                pipeline.submit(event);
            }

            let start = Instant::now();
            let processed = pipeline.process();
            let dur = start.elapsed();
            let us_total = dur.as_nanos() as f64 / 1_000.0;
            let ns_per_processed = dur.as_nanos() as f64 / n_pipe as f64;
            println!(
                "    [BM50] Pipeline Sort & Process (1,000 events): {:.2} µs ({:.1} ns/event, output: {})",
                us_total, ns_per_processed, processed.len()
            );
        }
    }

    println!("============================================================");
    println!("             BENCHMARK SUITE COMPLETE                       ");
    println!("============================================================");
}
